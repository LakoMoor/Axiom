//! Files packed inside one program file (`formats::packed`): list them and take them out
//! into a new folder. Nothing is executed.

use super::artifact_tools::{create_absent_dir, safe_components, write_new_file};
use super::native_common::{DEFAULT_LIMIT, page, params_json, spec};
use super::static_input::{builder, format_err, load};
use crate::tool::{Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset, parse_params, schema_of};
use axiom_evidence::{Authority, Confidence, Coverage, CoverageStatus, ErrorCode, Limitation, Location, ToolError, sha256_hex};
use axiom_formats::packed::{self, PackedEntry, PackedFormat, READERS};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

/// Largest packed file read (2 GiB: single-file apps are often large).
const MAX_INPUT: u64 = 2 << 30;
const DEFAULT_MAX_OUTPUT: u64 = 2 << 30;
const DEFAULT_MAX_FILES: usize = 50_000;

pub fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "core_packed_files_list",
            "Files packed inside a file",
            "List the files carried inside one program file, recognised from its bytes (the reader used is in the result; `format` picks one when a file could be read several ways). Results give each member's path, size, how it's stored and what the file says it is, plus the facts of that kind of file, paged with offset/limit. `contains` keeps paths holding that text. For ordinary archives and app bundles use core_archive_show. Nothing is executed.",
            "See which files a single-file app carries inside it.",
            Toolset::Formats,
            Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<ListParams>,
            core_packed_files_list,
        ),
        spec(
            "core_packed_files_unpack",
            "Take out packed files",
            "Write files carried inside one program file into a new folder (output_directory must not exist yet; it is created). `paths` picks exact members, `contains` those holding some text; with neither, all of them. Never overwrites; a member whose path would leave the folder is skipped and named. Bounded by max_output_bytes and max_output_files. Writes manifest.json with every member's SHA-256 (under another name, given in the result, if a packed file took that one). Nothing is executed.",
            "Unpack the files a single-file app carries, into a folder you can look through.",
            Toolset::Formats,
            Effects::MUTATES_SESSION.union(Effects::WRITES_FILESYSTEM).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<ExtractParams>,
            core_packed_files_unpack,
        ),
    ]
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    /// Absolute path of the file; the active target when omitted.
    #[serde(default)]
    pub path: Option<String>,
    /// Read the file as this kind (a reader id) instead of recognising it.
    #[serde(default)]
    pub format: Option<String>,
    /// Keep only members whose path contains this text.
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractParams {
    /// Absolute path of the file; the active target when omitted.
    #[serde(default)]
    pub path: Option<String>,
    /// A folder that doesn't exist yet; its parent must.
    pub output_directory: String,
    /// Read the file as this kind (a reader id) instead of recognising it.
    #[serde(default)]
    pub format: Option<String>,
    /// Exact member paths to take out.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    /// Take out members whose path contains this text.
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default = "default_max_output")]
    pub max_output_bytes: u64,
    #[serde(default = "default_max_files")]
    pub max_output_files: usize,
}

fn default_max_output() -> u64 {
    DEFAULT_MAX_OUTPUT
}

fn default_max_files() -> usize {
    DEFAULT_MAX_FILES
}

fn reader_for(format: Option<&str>, data: &[u8]) -> Result<&'static dyn PackedFormat, ToolError> {
    let known = || READERS.iter().map(|r| r.id()).collect::<Vec<_>>().join(", ");
    match format {
        Some(id) => {
            packed::by_id(id).ok_or_else(|| ToolError::invalid(format!("No reader called {id:?}")).with_remediation(format!("Use one of: {}.", known())))
        }
        None => packed::detect(data).ok_or_else(|| {
            ToolError::unsupported("This file doesn't carry files in any way we recognise.")
                .with_remediation(format!("Run core_target_triage to see what it is, or core_archive_show for ordinary archives. Readers: {}.", known()))
        }),
    }
}

fn core_packed_files_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: ListParams = parse_params(params)?;
    let input = load(ctx, p.path.as_deref(), MAX_INPUT)?;
    let reader = reader_for(p.format.as_deref(), &input.bytes)?;
    ctx.report(None, format!("Reading {} as {}", input.name, reader.title()));
    let listing = reader.list(&input.bytes).map_err(|e| format_err(e, reader.title()))?;
    ctx.cancel.check()?;
    let total = listing.entries.len();
    let kept: Vec<&PackedEntry> = listing.entries.iter().filter(|e| p.contains.as_deref().is_none_or(|c| e.path.contains(c))).collect();
    let (entries, cov) = page(&kept, p.offset, p.limit);
    let mut lims: Vec<Limitation> = listing.notes.iter().map(|n| Limitation::new("reader_note", n.clone())).collect();
    let mut coverage = cov;
    if listing.truncated {
        lims.push(Limitation::new("listing_cut_short", "A limit stopped the listing before its end, so more files may be inside."));
        coverage.status = CoverageStatus::Partial;
    }
    // When nothing could be listed, the reader's reason matters more than the count.
    let summary = match listing.notes.first().filter(|_| total == 0) {
        Some(why) => format!("{}: no files listed. {why}", reader.title()),
        None => format!("{}: {total} file(s) inside, {} shown.", reader.title(), entries.len()),
    };
    let ev = builder(ctx, "core_packed_files_list", input.subject.clone(), params_json(&p))
        .result(json!({"path": input.path, "format": reader.id(), "title": reader.title(), "facts": listing.facts, "total": total, "matching": kept.len(), "entries": entries}))
        .summary(summary)
        .confidence(Confidence::Observed)
        .authority(Authority::OriginalFiles)
        .limitations(lims)
        .coverage(coverage)
        .location(Location::new("path", input.path.display().to_string()))
        .build()?;
    Ok(ToolOutput::Evidence(Box::new(ev)))
}

/// What taking files out produced.
#[derive(Debug, Default, Serialize)]
struct Extracted {
    written: Vec<Value>,
    skipped: Vec<Value>,
    bytes: u64,
    stopped_at_limit: bool,
}

/// Write `chosen` members under `root`. A member that can't be read or placed is skipped and named.
fn write_members(reader: &dyn PackedFormat, data: &[u8], chosen: &[&PackedEntry], root: &Path, max_bytes: u64, max_files: usize) -> Extracted {
    let mut out = Extracted::default();
    for e in chosen {
        if out.written.len() >= max_files || out.bytes >= max_bytes {
            out.stopped_at_limit = true;
            break;
        }
        let parts = match safe_components(&e.path) {
            Ok(parts) => parts,
            Err(why) => {
                out.skipped.push(json!({"path": e.path, "why": why}));
                continue;
            }
        };
        let bytes = match reader.read(data, e, max_bytes - out.bytes) {
            Ok(b) => b,
            Err(err) => {
                out.skipped.push(json!({"path": e.path, "why": err.to_string()}));
                continue;
            }
        };
        match write_new_file(root, &parts, &bytes) {
            Ok(_) => {
                out.bytes += bytes.len() as u64;
                out.written.push(json!({"path": parts.join("/"), "size": bytes.len(), "sha256": sha256_hex(&bytes)}));
            }
            Err(why) => out.skipped.push(json!({"path": e.path, "why": why})),
        }
    }
    out
}

/// Write the manifest under the first free name: a packed file may already be called manifest.json.
fn write_manifest(root: &Path, text: &[u8]) -> Result<std::path::PathBuf, ToolError> {
    for n in 0..1000 {
        let name = if n == 0 { "manifest.json".to_string() } else { format!("manifest-{n}.json") };
        let path = root.join(&name);
        if std::fs::symlink_metadata(&path).is_err() {
            return write_new_file(root, &[name], text).map_err(|why| ToolError::io(path.display(), std::io::Error::other(why)));
        }
    }
    Err(ToolError::new(ErrorCode::OverLimit, "Every manifest name in this folder is taken by a packed file."))
}

fn core_packed_files_unpack(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: ExtractParams = parse_params(params)?;
    if p.max_output_bytes == 0 || p.max_output_files == 0 {
        return Err(ToolError::invalid("max_output_bytes and max_output_files must be above zero"));
    }
    let input = load(ctx, p.path.as_deref(), MAX_INPUT)?;
    let reader = reader_for(p.format.as_deref(), &input.bytes)?;
    let listing = reader.list(&input.bytes).map_err(|e| format_err(e, reader.title()))?;
    let chosen: Vec<&PackedEntry> = listing
        .entries
        .iter()
        .filter(|e| p.paths.as_ref().is_none_or(|w| w.contains(&e.path)) && p.contains.as_deref().is_none_or(|c| e.path.contains(c)))
        .collect();
    if chosen.is_empty() {
        return Err(ToolError::new(ErrorCode::NotFound, "No packed file matches.").with_remediation("List them with core_packed_files_list."));
    }
    let root = create_absent_dir(&p.output_directory)?;
    ctx.report(None, format!("Taking {} file(s) out of {}", chosen.len(), input.name));
    ctx.cancel.check()?;
    let out = write_members(reader, &input.bytes, &chosen, &root, p.max_output_bytes, p.max_output_files);
    let manifest = json!({"version": 1, "source_sha256": sha256_hex(&input.bytes), "format": reader.id(), "files": out.written, "skipped": out.skipped});
    let text = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
    let manifest_path = write_manifest(&root, &text)?;
    let mut lims = Vec::new();
    if out.stopped_at_limit {
        lims.push(Limitation::new("stopped_at_limit", "max_output_bytes or max_output_files stopped this before every file was out."));
    }
    if listing.truncated {
        lims.push(Limitation::new("listing_cut_short", "A limit stopped the listing before its end, so more files may be inside."));
    }
    let done = out.written.len() as u64;
    let coverage = Coverage {
        status: if out.stopped_at_limit || !out.skipped.is_empty() || listing.truncated { CoverageStatus::Partial } else { CoverageStatus::Complete },
        examined: Some(done),
        total: Some(chosen.len() as u64),
        next_offset: None,
    };
    let summary =
        format!("Took {done} of {} file(s) ({} bytes) out of {} into {}; {} skipped.", chosen.len(), out.bytes, input.name, root.display(), out.skipped.len());
    let ev = builder(ctx, "core_packed_files_unpack", input.subject.clone(), params_json(&p))
        .result(
            json!({"output_directory": root, "format": reader.id(), "written": done, "bytes": out.bytes, "skipped": out.skipped, "manifest": manifest_path}),
        )
        .summary(summary)
        .confidence(Confidence::Observed)
        .authority(Authority::OriginalFiles)
        .limitations(lims)
        .coverage(coverage)
        .location(Location::new("path", root.display().to_string()))
        .build()?;
    Ok(ToolOutput::Evidence(Box::new(ev)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_formats::FormatError;
    use axiom_formats::packed::{Compression, PackedListing};

    struct Plain;
    impl PackedFormat for Plain {
        fn id(&self) -> &'static str {
            "plain"
        }
        fn title(&self) -> &'static str {
            "Plain"
        }
        fn detect(&self, _: &[u8]) -> bool {
            true
        }
        fn list(&self, _: &[u8]) -> Result<PackedListing, FormatError> {
            Err(FormatError::Unsupported("the writer gets its members directly".into()))
        }
    }

    fn member(path: &str, offset: u64, stored_size: u64) -> PackedEntry {
        PackedEntry { path: path.into(), offset, stored_size, size: None, compression: Compression::None, kind: None }
    }

    #[test]
    fn tools_that_return_a_files_own_text_say_so() {
        let r = crate::Registry::with_all_tools();
        for id in [
            "core_packed_files_list",
            "core_packed_files_unpack",
            "formats_bytecode_show",
            "formats_ai_weights_show",
            "app_folder_explain",
            "native_library_calls_list",
            "native_offered_symbols_list",
            "native_publisher_show",
            "app_builds_line_up",
            "native_source_folder_write",
            "android_components_list",
            "web_standin_build",
            "web_standin_show",
        ] {
            assert!(r.get(id).unwrap().effects.contains(Effects::CARRIES_TARGET_CONTENT), "{id}");
        }
    }

    #[test]
    fn members_are_written_and_escapes_are_named() {
        let dir = tempfile::tempdir().unwrap();
        let (a, up, past) = (member("sub/a.txt", 0, 5), member("../evil", 0, 5), member("past", 99, 5));
        let out = write_members(&Plain, b"hello", &[&a, &up, &past], dir.path(), 1 << 20, 10);
        assert_eq!(std::fs::read(dir.path().join("sub/a.txt")).unwrap(), b"hello");
        assert_eq!(out.written.len(), 1);
        assert_eq!(out.skipped.len(), 2);
        assert!(!dir.path().parent().unwrap().join("evil").exists());
        assert!(!out.stopped_at_limit);
    }

    #[test]
    fn output_limits_stop_the_work_and_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (member("a", 0, 5), member("b", 0, 5));
        let out = write_members(&Plain, b"hello", &[&a, &b], dir.path(), 1 << 20, 1);
        assert_eq!(out.written.len(), 1);
        assert!(out.stopped_at_limit);
        let dir2 = tempfile::tempdir().unwrap();
        let tight = write_members(&Plain, b"hello", &[&a, &b], dir2.path(), 7, 10);
        assert_eq!(tight.written.len(), 1, "the second no longer fits in what's left");
        assert_eq!(tight.skipped.len(), 1);
    }
}
