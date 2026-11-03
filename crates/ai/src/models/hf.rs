//! Hugging Face, read-only: search for models in the GGUF format and list a repository's files
//! with the SHA-256 the hub publishes for each, so a download can be checked. Nothing here needs
//! an account. The parsing is separate from the calls, so it's tested on sample answers.

use crate::http::{get_json, quick_agent};
use axiom_evidence::{ErrorCode, ToolError};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const API: &str = "https://huggingface.co/api/models";
const SITE: &str = "https://huggingface.co";
/// Most hits one search returns.
pub const MAX_HITS: usize = 30;

/// A repository found by a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// `owner/name`.
    pub repo: String,
    pub downloads: u64,
    pub likes: u64,
}

/// One GGUF file in a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GgufFile {
    pub name: String,
    pub size_bytes: u64,
    /// The hub's SHA-256 of the file (lowercase hex), when it lists one.
    pub sha256: Option<String>,
    /// The quantisation named in the file name (`Q4_K_M`), when there is one.
    pub quant: Option<String>,
}

/// Whether `repo` looks like `owner/name` with only the characters the hub allows.
pub fn valid_repo(repo: &str) -> bool {
    let ok = |s: &str| !s.is_empty() && s.len() <= 96 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) && !s.starts_with('.');
    let mut parts = repo.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if ok(a) && ok(b))
}

/// Whether `file` is a plain `.gguf` file name (no folders, no way out of the models folder).
pub fn valid_file(file: &str) -> bool {
    file.len() <= 200 && file.ends_with(".gguf") && !file.contains(['/', '\\', ':']) && !file.starts_with('.') && !file.contains("..")
}

/// Percent-encode a query value.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The address a search asks for.
pub fn search_url(query: &str, limit: usize) -> String {
    format!("{API}?search={}&filter=gguf&sort=downloads&direction=-1&limit={}", encode(query.trim()), limit.clamp(1, MAX_HITS))
}

/// The address that lists a repository's files.
pub fn tree_url(repo: &str) -> String {
    format!("{API}/{repo}/tree/main")
}

/// The address a file downloads from.
pub fn file_url(repo: &str, file: &str) -> String {
    format!("{SITE}/{repo}/resolve/main/{file}?download=true")
}

/// The hits in a search answer; entries that aren't well formed are skipped.
pub fn parse_search(v: &Value) -> Vec<Hit> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let repo = m.get("id").or_else(|| m.get("modelId")).and_then(Value::as_str)?;
            valid_repo(repo).then(|| Hit {
                repo: repo.to_string(),
                downloads: m.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                likes: m.get("likes").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .take(MAX_HITS)
        .collect()
}

/// The quantisation in a GGUF file name (`…-Q4_K_M.gguf`, `…-UD-Q4_K_XL.gguf`, `…-IQ3_XS.gguf`, `…-F16.gguf`).
pub fn quant_of(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".gguf")?;
    stem.split(['-', '.'])
        .rev()
        .find(|t| {
            let u = t.to_ascii_uppercase();
            let q = u.starts_with('Q') || u.starts_with("IQ");
            (q && u.chars().skip(1).take(1).any(|c| c.is_ascii_digit() || c == 'Q')) || matches!(u.as_str(), "F16" | "BF16" | "F32")
        })
        .map(str::to_ascii_uppercase)
}

/// The single-file GGUF models in a repository listing. Split files ("-00001-of-00003") and
/// multimodal projector files are left out: one file runs by itself, the others need more.
pub fn parse_tree(v: &Value) -> Vec<GgufFile> {
    let mut files: Vec<GgufFile> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let name = f.get("path").and_then(Value::as_str)?;
            if f.get("type").and_then(Value::as_str).is_some_and(|t| t != "file") || !valid_file(name) {
                return None;
            }
            let lower = name.to_ascii_lowercase();
            if lower.contains("-of-0") || lower.contains("mmproj") {
                return None;
            }
            let size = f.pointer("/lfs/size").and_then(Value::as_u64).or_else(|| f.get("size").and_then(Value::as_u64))?;
            let sha256 = f
                .pointer("/lfs/oid")
                .and_then(Value::as_str)
                .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(str::to_ascii_lowercase);
            Some(GgufFile { name: name.to_string(), size_bytes: size, sha256, quant: quant_of(name) })
        })
        .collect();
    files.sort_by_key(|f| f.size_bytes);
    files
}

fn agent() -> axiom_net::Agent {
    quick_agent(Duration::from_secs(20))
}

/// Search the hub for GGUF models, most downloaded first.
pub fn search(query: &str, limit: usize) -> Result<Vec<Hit>, ToolError> {
    if query.trim().is_empty() {
        return Err(ToolError::invalid("Type a model name to search for.").with_remediation("For example: qwen, llama, gemma, or coder."));
    }
    let v = get_json(&agent(), "Hugging Face", &search_url(query, limit), &[])?;
    Ok(parse_search(&v))
}

/// The single-file GGUF models in `repo`, smallest first.
pub fn files(repo: &str) -> Result<Vec<GgufFile>, ToolError> {
    if !valid_repo(repo) {
        return Err(ToolError::new(ErrorCode::BadInput, format!("{repo:?} isn't a Hugging Face repository name."))
            .with_remediation("Use the form owner/name, as in unsloth/Qwen3-8B-GGUF."));
    }
    let v = get_json(&agent(), "Hugging Face", &tree_url(repo), &[])?;
    Ok(parse_tree(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_are_checked_before_they_reach_a_url_or_a_path() {
        assert!(valid_repo("unsloth/Qwen3-8B-GGUF") && valid_repo("a/b.c_d-e"));
        for bad in ["", "x", "a/b/c", "../x", "a/../b", "a/ b", "/a", "a/", "a/b?x=1", ".a/b", "a/b#c"] {
            assert!(!valid_repo(bad), "{bad:?}");
        }
        assert!(valid_file("Qwen3-8B-Q4_K_M.gguf"));
        for bad in ["a.bin", "../a.gguf", "dir/a.gguf", "a\\b.gguf", ".hidden.gguf", "C:a.gguf", "a..gguf"] {
            assert!(!valid_file(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_search_url_encodes_the_query_and_bounds_the_count() {
        let u = search_url("qwen 2.5 & co", 500);
        assert!(u.contains("search=qwen%202.5%20%26%20co") && u.ends_with("limit=30"), "{u}");
        assert!(u.contains("filter=gguf") && u.contains("sort=downloads"));
        assert_eq!(file_url("a/b", "c.gguf"), "https://huggingface.co/a/b/resolve/main/c.gguf?download=true");
    }

    #[test]
    fn a_search_answer_is_read_and_junk_is_skipped() {
        let v = json!([
            {"id": "unsloth/Qwen3-8B-GGUF", "downloads": 1200, "likes": 40},
            {"id": "not a repo", "downloads": 5},
            {"downloads": 7},
            {"modelId": "a/b"},
            "text",
        ]);
        let hits = parse_search(&v);
        assert_eq!(hits.iter().map(|h| h.repo.as_str()).collect::<Vec<_>>(), ["unsloth/Qwen3-8B-GGUF", "a/b"]);
        assert_eq!((hits[0].downloads, hits[0].likes, hits[1].downloads), (1200, 40, 0));
        assert!(parse_search(&json!({"error": "x"})).is_empty());
    }

    #[test]
    fn quantisations_are_read_from_names() {
        assert_eq!(quant_of("Qwen3-8B-Q4_K_M.gguf").as_deref(), Some("Q4_K_M"));
        assert_eq!(quant_of("gemma-3-4b-it-UD-Q4_K_XL.gguf").as_deref(), Some("Q4_K_XL"));
        assert_eq!(quant_of("model-IQ3_XS.gguf").as_deref(), Some("IQ3_XS"));
        assert_eq!(quant_of("model-f16.gguf").as_deref(), Some("F16"));
        assert_eq!(quant_of("model.Q8_0.gguf").as_deref(), Some("Q8_0"));
        assert_eq!(quant_of("README.md"), None);
        assert_eq!(quant_of("plain.gguf"), None);
    }

    #[test]
    fn a_listing_keeps_single_files_with_their_published_hashes() {
        let h = "a".repeat(64);
        let v = json!([
            {"type": "file", "path": "m-Q8_0.gguf", "size": 300, "lfs": {"oid": h, "size": 8_000_000_000u64}},
            {"type": "file", "path": "m-Q4_K_M.gguf", "size": 100, "lfs": {"oid": "short", "size": 4_000_000_000u64}},
            {"type": "file", "path": "m-Q4_K_M-00001-of-00003.gguf", "size": 1, "lfs": {"size": 1}},
            {"type": "file", "path": "mmproj-F16.gguf", "size": 5},
            {"type": "file", "path": "README.md", "size": 5},
            {"type": "directory", "path": "sub"},
            {"type": "file", "path": "../x.gguf", "size": 5},
            {"type": "file", "path": "small.gguf"},
        ]);
        let f = parse_tree(&v);
        assert_eq!(f.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["m-Q4_K_M.gguf", "m-Q8_0.gguf"], "smallest first");
        assert_eq!(f[0].sha256, None, "a malformed hash is dropped, not trusted");
        assert_eq!(f[1].sha256.as_deref(), Some(h.as_str()));
        assert_eq!(f[1].size_bytes, 8_000_000_000);
    }

    #[test]
    fn a_blank_search_is_asked_to_say_something() {
        assert!(search("  ", 5).is_err());
        assert!(files("nope").is_err());
    }
}
