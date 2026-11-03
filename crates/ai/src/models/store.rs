//! The models on this computer: GGUF files in one folder of the engine store, each with a small
//! `<file>.json` beside it that remembers where it came from. A download is written aside and
//! moved into place only when complete and, when the hub published a SHA-256, only when it matches.

use super::hf;
use axiom_engines::install::{download_file, download_verified};
use axiom_evidence::{ErrorCode, ToolError};
use axiom_platform::process::Cancel;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The most one model file may be (a safety net against a wrong address, not a promise).
const MAX_BYTES: u64 = 80 << 30;
const META_VERSION: u32 = 1;

/// The folder models live in, under the engine store.
pub fn dir(store: &Path) -> PathBuf {
    store.join("models")
}

/// A model that's here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Installed {
    pub file: String,
    pub size_bytes: u64,
    /// The repository it came from, when it was downloaded here.
    pub repo: Option<String>,
    pub sha256: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    version: u32,
    repo: String,
    sha256: Option<String>,
}

fn meta_path(models: &Path, file: &str) -> PathBuf {
    models.join(format!("{file}.json"))
}

/// The path of a model file in `models`, refusing any name that isn't a plain `.gguf` file name.
pub fn path_of(models: &Path, file: &str) -> Result<PathBuf, ToolError> {
    if !hf::valid_file(file) {
        return Err(ToolError::new(ErrorCode::BadInput, format!("{file:?} isn't a model file name."))
            .with_remediation("Use the file name as models_installed_list shows it (it ends in .gguf)."));
    }
    Ok(models.join(file))
}

/// Every model in `models`, smallest first. A missing folder is an empty list.
pub fn list(models: &Path) -> Vec<Installed> {
    let mut v: Vec<Installed> = std::fs::read_dir(models)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_str()?.to_string();
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            if !hf::valid_file(&file) {
                return None;
            }
            let m: Option<Meta> = std::fs::read(meta_path(models, &file)).ok().and_then(|b| serde_json::from_slice(&b).ok());
            Some(Installed { size_bytes: meta.len(), repo: m.as_ref().map(|m| m.repo.clone()), sha256: m.and_then(|m| m.sha256), path: e.path(), file })
        })
        .collect();
    v.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes).then_with(|| a.file.cmp(&b.file)));
    v
}

/// Download `file` from `repo` into `models`. With `sha256` (the hub's own) the bytes are checked as
/// they arrive and a mismatch deletes the file. `progress` gets (bytes so far, total).
pub fn download(
    models: &Path,
    repo: &str,
    file: &str,
    sha256: Option<&str>,
    cancel: Option<&Cancel>,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<Installed, ToolError> {
    if !hf::valid_repo(repo) {
        return Err(ToolError::new(ErrorCode::BadInput, format!("{repo:?} isn't a Hugging Face repository name.")).with_remediation("Use the form owner/name."));
    }
    let dest = path_of(models, file)?;
    if dest.exists() {
        return Err(ToolError::new(ErrorCode::BadInput, format!("{file} is already downloaded.")).with_remediation("Remove it first to download it again."));
    }
    std::fs::create_dir_all(models).map_err(|e| ToolError::io(models.display(), e))?;
    let url = hf::file_url(repo, file);
    match sha256 {
        Some(h) => download_verified(&url, &dest, h, MAX_BYTES, cancel, progress)?,
        None => download_file(&url, &dest, MAX_BYTES, cancel, progress)?,
    }
    let meta = Meta { version: META_VERSION, repo: repo.to_string(), sha256: sha256.map(str::to_string) };
    // The note is a convenience: a model without it still runs, so a failed write isn't an error.
    if let Ok(bytes) = serde_json::to_vec_pretty(&meta) {
        let _ = std::fs::write(meta_path(models, file), bytes);
    }
    let size_bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(Installed { file: file.to_string(), size_bytes, repo: Some(repo.to_string()), sha256: sha256.map(str::to_string), path: dest })
}

/// Delete a model and its note. Whether there was one.
pub fn remove(models: &Path, file: &str) -> Result<bool, ToolError> {
    let path = path_of(models, file)?;
    let _ = std::fs::remove_file(meta_path(models, file));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(ToolError::io(path.display(), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_reads_files_and_notes_and_skips_everything_else() {
        let d = tempfile::tempdir().unwrap();
        let m = dir(d.path());
        assert!(list(&m).is_empty(), "no folder, no models");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("big.gguf"), vec![0u8; 300]).unwrap();
        std::fs::write(m.join("small.gguf"), vec![0u8; 100]).unwrap();
        std::fs::write(m.join("small.gguf.json"), r#"{"version":1,"repo":"a/b","sha256":null}"#).unwrap();
        std::fs::write(m.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(m.join("dir.gguf")).unwrap();
        let v = list(&m);
        assert_eq!(v.iter().map(|i| i.file.as_str()).collect::<Vec<_>>(), ["small.gguf", "big.gguf"]);
        assert_eq!((v[0].size_bytes, v[0].repo.as_deref()), (100, Some("a/b")));
        assert_eq!(v[1].repo, None);
    }

    #[test]
    fn a_name_can_never_leave_the_folder() {
        let d = tempfile::tempdir().unwrap();
        for bad in ["../x.gguf", "a/b.gguf", "x.bin", ""] {
            assert!(path_of(d.path(), bad).is_err(), "{bad:?}");
            assert!(remove(d.path(), bad).is_err(), "{bad:?}");
        }
        assert_eq!(path_of(d.path(), "ok.gguf").unwrap(), d.path().join("ok.gguf"));
    }

    #[test]
    fn removing_takes_the_note_too_and_a_second_time_says_none_was_there() {
        let d = tempfile::tempdir().unwrap();
        let m = dir(d.path());
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("a.gguf"), "x").unwrap();
        std::fs::write(m.join("a.gguf.json"), "{}").unwrap();
        assert!(remove(&m, "a.gguf").unwrap());
        assert!(!m.join("a.gguf.json").exists());
        assert!(!remove(&m, "a.gguf").unwrap());
    }

    #[test]
    fn bad_names_and_existing_files_are_refused_before_any_network_use() {
        let d = tempfile::tempdir().unwrap();
        let m = dir(d.path());
        assert!(download(&m, "nope", "a.gguf", None, None, &|_, _| {}).is_err());
        assert!(download(&m, "a/b", "../a.gguf", None, None, &|_, _| {}).is_err());
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("a.gguf"), "x").unwrap();
        let e = download(&m, "a/b", "a.gguf", None, None, &|_, _| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadInput);
    }
}
