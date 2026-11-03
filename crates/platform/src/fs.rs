//! File safety: atomic writes (temp file, fsync, rename), optionally private (0600 on Unix).

use axiom_evidence::ToolError;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path` atomically, keeping the existing file's permissions when it exists.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    write_impl(path, bytes, false)
}

/// Write `bytes` to `path` atomically with owner-only permissions (0600 on Unix). Used for
/// configuration files that may hold paths or tokens, and for the key fallback file.
pub fn write_atomic_private(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    write_impl(path, bytes, true)
}

/// Read a text file the app keeps for itself (settings, keys). In a browser page there is no disk:
/// those files live in the page's local storage, under the path they would have had.
pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    #[cfg(target_arch = "wasm32")]
    {
        browser::get(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(path)
    }
}

/// `is_absolute` that also holds for `/web/report.md` in a browser page, where the standard library
/// (which has no idea what the system is) wants a drive prefix and so calls no path absolute.
pub trait AbsolutePath {
    fn is_absolute_path(&self) -> bool;
}

impl AbsolutePath for Path {
    fn is_absolute_path(&self) -> bool {
        if cfg!(target_arch = "wasm32") { self.has_root() } else { self.is_absolute() }
    }
}

impl AbsolutePath for std::path::PathBuf {
    fn is_absolute_path(&self) -> bool {
        self.as_path().is_absolute_path()
    }
}

/// What [`metadata`] says about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    pub is_dir: bool,
    pub len: u64,
}

/// Facts about a file or folder. In a browser page only the files given to the page are there.
pub fn metadata(path: &Path) -> std::io::Result<Meta> {
    #[cfg(target_arch = "wasm32")]
    {
        pagefiles::meta(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::metadata(path).map(|m| Meta { is_dir: m.is_dir(), len: m.len() })
    }
}

/// A file's bytes. In a browser page: a file the person gave the page (see [`stage_file`]).
pub fn read(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    {
        pagefiles::read(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path)
    }
}

/// Keep a file the person gave the page (picked or dropped) in memory and return the path it can
/// be opened by. Browser only: a page has no disk to put it on.
#[cfg(target_arch = "wasm32")]
pub fn stage_file(name: &str, bytes: Vec<u8>) -> std::path::PathBuf {
    pagefiles::stage(name, bytes)
}

/// The files given to a browser page, kept in memory under `/web/`.
#[cfg(target_arch = "wasm32")]
mod pagefiles {
    use crate::sync::Mutex;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, LazyLock};

    static FILES: LazyLock<Mutex<BTreeMap<PathBuf, Arc<[u8]>>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

    fn not_found(path: &Path) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} isn't one of the files given to this page", path.display()))
    }

    pub fn stage(name: &str, bytes: Vec<u8>) -> PathBuf {
        let clean: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
        let clean = if clean.trim().is_empty() { "file".to_string() } else { clean };
        let path = PathBuf::from("/web").join(clean);
        if let Ok(mut files) = FILES.lock() {
            files.insert(path.clone(), Arc::from(bytes));
        }
        path
    }

    pub fn read(path: &Path) -> std::io::Result<Vec<u8>> {
        FILES.lock().ok().and_then(|f| f.get(path).map(|b| b.to_vec())).ok_or_else(|| not_found(path))
    }

    pub fn meta(path: &Path) -> std::io::Result<super::Meta> {
        let files = FILES.lock().map_err(|_| not_found(path))?;
        match files.get(path) {
            Some(b) => Ok(super::Meta { is_dir: false, len: b.len() as u64 }),
            None if path == Path::new("/web") => Ok(super::Meta { is_dir: true, len: 0 }),
            None => Err(not_found(path)),
        }
    }
}

/// The page's local storage standing in for a disk (browser only).
#[cfg(target_arch = "wasm32")]
mod browser {
    use axiom_evidence::{ErrorCode, ToolError};
    use std::path::Path;

    fn key(path: &Path) -> String {
        format!("axiom-file:{}", path.display())
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn get(path: &Path) -> std::io::Result<String> {
        let missing = || std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} isn't in this page's storage", path.display()));
        storage().ok_or_else(missing)?.get_item(&key(path)).ok().flatten().ok_or_else(missing)
    }

    pub fn set(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            ToolError::new(ErrorCode::FeatureMissing, format!("{} holds binary data, which this page can't keep.", path.display()))
                .with_remediation("Use the desktop app to work with files like this.")
        })?;
        let store = storage().ok_or_else(|| {
            ToolError::new(ErrorCode::FileError, "This page can't keep settings: the browser blocks its storage.")
                .with_remediation("Allow site data for this page (private windows often block it) and try again.")
        })?;
        store.set_item(&key(path), text).map_err(|_| {
            ToolError::new(ErrorCode::FileError, format!("Couldn't keep {} in the browser's storage.", path.display()))
                .with_remediation("The storage may be full or blocked; clear some site data and try again.")
        })
    }
}

fn write_impl(path: &Path, bytes: &[u8], private: bool) -> Result<(), ToolError> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = private;
        return browser::set(path, bytes);
    }
    #[cfg(not(target_arch = "wasm32"))]
    write_to_disk(path, bytes, private)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_to_disk(path: &Path, bytes: &[u8], private: bool) -> Result<(), ToolError> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        Some(_) => std::path::PathBuf::from("."),
        None => return Err(ToolError::invalid(format!("{} has no parent folder", path.display()))),
    };
    std::fs::create_dir_all(&dir).map_err(|e| ToolError::io(dir.display(), e))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.tmp-{}-{n}", std::process::id()));
    let existing_perms = std::fs::metadata(path).ok().map(|m| m.permissions());
    let result = (|| {
        let mut f = std::fs::File::create(&tmp).map_err(|e| ToolError::io(tmp.display(), e))?;
        if private {
            set_private(&tmp)?;
        } else if let Some(p) = existing_perms {
            std::fs::set_permissions(&tmp, p).map_err(|e| ToolError::io(tmp.display(), e))?;
        }
        f.write_all(bytes).map_err(|e| ToolError::io(tmp.display(), e))?;
        f.sync_all().map_err(|e| ToolError::io(tmp.display(), e))?;
        drop(f);
        rename_retrying(&tmp, path).map_err(|e| ToolError::io(path.display(), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// On Windows a file another program has open for a moment (an antivirus scan, the search
/// indexer) can't be replaced: try again briefly before giving up.
fn rename_retrying(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut tries = 0;
    loop {
        match std::fs::rename(from, to) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            r => return r,
        }
    }
}

#[cfg(unix)]
fn set_private(path: &Path) -> Result<(), ToolError> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| ToolError::io(path.display(), e))
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> Result<(), ToolError> {
    // Windows: files under the user's profile are already private to the user by default ACLs.
    Ok(())
}

/// Copy `from` to `to` only if `to` does not exist yet (the first backup is never overwritten).
/// Returns true when a copy was made, false when a backup already existed.
pub fn backup_once(from: &Path, to: &Path) -> Result<bool, ToolError> {
    if to.exists() {
        return Ok(false);
    }
    let bytes = std::fs::read(from).map_err(|e| ToolError::io(from.display(), e))?;
    let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(ToolError::io(to.display(), e)),
    };
    set_private(to)?;
    f.write_all(&bytes).map_err(|e| ToolError::io(to.display(), e))?;
    f.sync_all().map_err(|e| ToolError::io(to.display(), e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_and_private_is_0600() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub").join("a.txt");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        let q = dir.path().join("secret");
        write_atomic_private(&q, b"k").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&q).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // no temp files left behind
        assert_eq!(std::fs::read_dir(dir.path().join("sub")).unwrap().count(), 1);
    }

    #[test]
    fn absolute_paths_are_told_from_relative_ones_and_metadata_and_read_agree() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.bin");
        std::fs::write(&file, b"abc").unwrap();
        assert!(file.is_absolute_path() && file.as_path().is_absolute_path());
        assert!(!Path::new("x.bin").is_absolute_path());
        assert_eq!(metadata(&file).unwrap(), Meta { is_dir: false, len: 3 });
        assert!(metadata(dir.path()).unwrap().is_dir);
        assert_eq!(read(&file).unwrap(), b"abc");
        assert!(read(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn backup_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("a.bak");
        std::fs::write(&a, "first").unwrap();
        assert!(backup_once(&a, &b).unwrap());
        std::fs::write(&a, "second").unwrap();
        assert!(!backup_once(&a, &b).unwrap());
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "first");
    }
}
