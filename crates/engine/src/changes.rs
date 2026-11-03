//! What a build changed: a snapshot of the app folder is taken when a run starts, and the folder
//! is compared with it afterwards, file by file, with a line diff for text. Nothing here runs the
//! app or reads the original; it only looks at the project's own files.
//!
//! The snapshot is `changes-baseline.json` in the project folder (versioned; a file of another
//! version is treated as no baseline). Everything is capped, so a huge folder can't hurt.

use crate::workspace::Workspace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub const BASELINE_FILE: &str = "changes-baseline.json";
const VERSION: u32 = 1;
const MAX_FILES: usize = 3000;
const MAX_DEPTH: usize = 12;
/// A text file larger than this is compared by hash only.
const MAX_TEXT_BYTES: u64 = 200 * 1024;
/// Files above this aren't read at all (compared by size).
const MAX_HASH_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_TEXT: usize = 12 * 1024 * 1024;
/// Most bytes read in one snapshot; files after that are compared by size only.
const MAX_TOTAL_READ: u64 = 256 * 1024 * 1024;
/// Most lines of one file's diff, and most lines of diff cells (old lines × new lines).
const MAX_DIFF_LINES: usize = 600;
const MAX_CELLS: usize = 4_000_000;
const CONTEXT: usize = 3;
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next", "__pycache__", ".venv", "venv", ".idea", ".vscode"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub size: u64,
    /// Lowercase hex SHA-256 (empty for a file too large to read).
    pub sha256: String,
    /// The file's text, when it's a small UTF-8 text file.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub files: BTreeMap<String, Entry>,
    /// The walk stopped at the file cap.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Added,
    Modified,
    Deleted,
}

/// One line of a diff: `op` is `+`, `-`, ` ` (context) or `@` (lines left out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub op: char,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub path: String,
    pub kind: Kind,
    pub added: usize,
    pub removed: usize,
    /// Not shown as text (binary, or too large).
    pub binary: bool,
    pub lines: Vec<DiffLine>,
    /// More lines than are listed.
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// A snapshot exists to compare with.
    pub has_baseline: bool,
    pub changes: Vec<Change>,
    pub truncated: bool,
}

fn hex_sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Snapshot the project's app folder (and nothing of Axiom's own records).
pub fn take(ws: &Workspace) -> Snapshot {
    let base = ws.app_dir();
    let mut snap = Snapshot { version: VERSION, ..Snapshot::default() };
    let mut text_total = 0usize;
    let mut read_total = 0u64;
    let mut stack = vec![(base.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            let path = e.path();
            if ws.is_own_record(&path) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            let name = e.file_name().to_string_lossy().to_string();
            if meta.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if snap.files.len() >= MAX_FILES {
                snap.truncated = true;
                return snap;
            }
            let Some(rel) = relative(&base, &path) else { continue };
            let size = meta.len();
            let (sha256, text) = if size > MAX_HASH_BYTES || read_total.saturating_add(size) > MAX_TOTAL_READ {
                (String::new(), None)
            } else {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        read_total = read_total.saturating_add(size);
                        let text = (size <= MAX_TEXT_BYTES && text_total + bytes.len() <= MAX_TOTAL_TEXT && !bytes.contains(&0))
                            .then(|| String::from_utf8(bytes.clone()).ok())
                            .flatten();
                        if let Some(t) = &text {
                            text_total += t.len();
                        }
                        (hex_sha(&bytes), text)
                    }
                    Err(_) => continue,
                }
            };
            snap.files.insert(rel, Entry { size, sha256, text });
        }
    }
    snap
}

fn relative(base: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Take a snapshot now and keep it as the baseline (a run is about to start). Best effort: a
/// folder that can't be written just has no baseline.
pub fn mark(ws: &Workspace) {
    let snap = take(ws);
    if let Ok(bytes) = serde_json::to_vec(&snap) {
        let _ = craft_anything_platform::fs::write_atomic(&ws.root.join(BASELINE_FILE), &bytes);
    }
}

/// The saved baseline, when there is one of a version this build reads.
pub fn load(ws: &Workspace) -> Option<Snapshot> {
    let bytes = std::fs::read(ws.root.join(BASELINE_FILE)).ok()?;
    let snap: Snapshot = serde_json::from_slice(&bytes).ok()?;
    (snap.version == VERSION).then_some(snap)
}

/// What differs between the baseline and the folder now. With `path`, only that file's lines are
/// listed (the others give counts only).
pub fn report(ws: &Workspace, path: Option<&str>) -> Report {
    let Some(old) = load(ws) else { return Report::default() };
    let now = take(ws);
    let mut out = Report { has_baseline: true, changes: compare(&old, &now, path), truncated: old.truncated || now.truncated };
    out.changes.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The differences between two snapshots.
pub fn compare(old: &Snapshot, now: &Snapshot, only: Option<&str>) -> Vec<Change> {
    let mut changes = Vec::new();
    for (path, new) in &now.files {
        let with_lines = only.is_none_or(|p| p == path);
        match old.files.get(path) {
            None => changes.push(change(path, Kind::Added, None, Some(new), with_lines)),
            Some(was) if was.sha256 != new.sha256 || was.size != new.size => changes.push(change(path, Kind::Modified, Some(was), Some(new), with_lines)),
            Some(_) => {}
        }
    }
    for (path, was) in &old.files {
        if !now.files.contains_key(path) {
            changes.push(change(path, Kind::Deleted, Some(was), None, only.is_none_or(|p| p == path)));
        }
    }
    changes
}

fn change(path: &str, kind: Kind, was: Option<&Entry>, now: Option<&Entry>, with_lines: bool) -> Change {
    let old_text = was.and_then(|e| e.text.as_deref());
    let new_text = now.and_then(|e| e.text.as_deref());
    let binary = (was.is_some() && old_text.is_none()) || (now.is_some() && new_text.is_none());
    let (a, b) = (old_text.unwrap_or(""), new_text.unwrap_or(""));
    if binary {
        return Change { path: path.to_string(), kind, added: 0, removed: 0, binary: true, lines: Vec::new(), truncated: false };
    }
    let (ops, added, removed) = diff_lines(a, b);
    let (lines, truncated) = if with_lines { with_context(&ops) } else { (Vec::new(), false) };
    Change { path: path.to_string(), kind, added, removed, binary: false, lines, truncated }
}

/// A line diff: every line as (op, text), and how many were added and removed.
pub fn diff_lines(a: &str, b: &str) -> (Vec<DiffLine>, usize, usize) {
    let old: Vec<&str> = a.lines().collect();
    let new: Vec<&str> = b.lines().collect();
    let mut head = 0;
    while head < old.len() && head < new.len() && old.get(head) == new.get(head) {
        head += 1;
    }
    let mut tail = 0;
    while tail < old.len() - head && tail < new.len() - head && old.get(old.len() - 1 - tail) == new.get(new.len() - 1 - tail) {
        tail += 1;
    }
    let mid_old = old.get(head..old.len() - tail).unwrap_or_default();
    let mid_new = new.get(head..new.len() - tail).unwrap_or_default();
    let mut out: Vec<DiffLine> = old.iter().take(head).map(|l| line(' ', l)).collect();
    let (mut added, mut removed) = (0, 0);
    if mid_old.len().saturating_mul(mid_new.len()) > MAX_CELLS {
        // Too big to match line by line: the middle is shown as replaced.
        out.extend(mid_old.iter().map(|l| line('-', l)));
        out.extend(mid_new.iter().map(|l| line('+', l)));
        removed = mid_old.len();
        added = mid_new.len();
    } else {
        for (op, l) in lcs(mid_old, mid_new) {
            match op {
                '+' => added += 1,
                '-' => removed += 1,
                _ => {}
            }
            out.push(line(op, l));
        }
    }
    out.extend(old.iter().skip(old.len() - tail).map(|l| line(' ', l)));
    (out, added, removed)
}

fn line(op: char, text: &str) -> DiffLine {
    // A very long line is cut; the diff is for reading.
    DiffLine { op, text: text.chars().take(400).collect() }
}

/// Longest-common-subsequence diff of two short line lists.
fn lcs<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(char, &'a str)> {
    let (n, m) = (a.len(), b.len());
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let v = if a.get(i) == b.get(j) {
                table.get((i + 1) * width + j + 1).copied().unwrap_or(0) + 1
            } else {
                table.get((i + 1) * width + j).copied().unwrap_or(0).max(table.get(i * width + j + 1).copied().unwrap_or(0))
            };
            if let Some(cell) = table.get_mut(i * width + j) {
                *cell = v;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n || j < m {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x == y => {
                out.push((' ', *x));
                i += 1;
                j += 1;
            }
            (Some(x), Some(_)) if table.get((i + 1) * width + j).copied().unwrap_or(0) >= table.get(i * width + j + 1).copied().unwrap_or(0) => {
                out.push(('-', *x));
                i += 1;
            }
            (_, Some(y)) => {
                out.push(('+', *y));
                j += 1;
            }
            (Some(x), None) => {
                out.push(('-', *x));
                i += 1;
            }
            (None, None) => break,
        }
    }
    out
}

/// Keep the changed lines with a few lines of context; a gap becomes one `@` line.
fn with_context(ops: &[DiffLine]) -> (Vec<DiffLine>, bool) {
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, l)| l.op != ' ').map(|(i, _)| i).collect();
    if changed.is_empty() {
        return (Vec::new(), false);
    }
    let mut keep = vec![false; ops.len()];
    for i in changed {
        let from = i.saturating_sub(CONTEXT);
        let to = (i + CONTEXT + 1).min(ops.len());
        for k in keep.iter_mut().take(to).skip(from) {
            *k = true;
        }
    }
    let mut out = Vec::new();
    let mut gap = false;
    for (l, k) in ops.iter().zip(keep) {
        if k {
            if gap && !out.is_empty() {
                out.push(DiffLine { op: '@', text: String::new() });
            }
            gap = false;
            out.push(l.clone());
        } else {
            gap = true;
        }
        if out.len() >= MAX_DIFF_LINES {
            return (out, true);
        }
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(files: &[(&str, Option<&str>)]) -> Snapshot {
        Snapshot {
            version: VERSION,
            files: files
                .iter()
                .map(|(p, t)| {
                    (
                        p.to_string(),
                        Entry { size: t.map_or(9, |t| t.len() as u64), sha256: hex_sha(t.unwrap_or("binary").as_bytes()), text: t.map(str::to_string) },
                    )
                })
                .collect(),
            truncated: false,
        }
    }

    #[test]
    fn a_line_diff_counts_added_and_removed_lines() {
        let (ops, added, removed) = diff_lines("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!((added, removed), (2, 1));
        let shown: String = ops.iter().map(|l| format!("{}{}\n", l.op, l.text)).collect();
        assert_eq!(shown, " a\n-b\n+B\n c\n+d\n");
        assert_eq!(diff_lines("same\n", "same\n").1, 0);
        assert_eq!(diff_lines("", "x\ny\n").1, 2);
    }

    #[test]
    fn snapshots_compare_into_added_modified_and_deleted() {
        let old = snap(&[("keep.txt", Some("k")), ("edit.rs", Some("fn a() {}\n")), ("gone.md", Some("bye\n")), ("img.png", None)]);
        let now = snap(&[("keep.txt", Some("k")), ("edit.rs", Some("fn a() {}\nfn b() {}\n")), ("new.rs", Some("x\n"))]);
        let mut c = compare(&old, &now, None);
        c.sort_by(|a, b| a.path.cmp(&b.path));
        let kinds: Vec<(&str, Kind)> = c.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(kinds, [("edit.rs", Kind::Modified), ("gone.md", Kind::Deleted), ("img.png", Kind::Deleted), ("new.rs", Kind::Added)]);
        assert_eq!((c[0].added, c[0].removed), (1, 0));
        assert!(c[2].binary && c[2].lines.is_empty());
        // Asking for one file lists lines for that one only.
        let one = compare(&old, &now, Some("edit.rs"));
        assert!(one.iter().find(|c| c.path == "edit.rs").is_some_and(|c| !c.lines.is_empty()));
        assert!(one.iter().find(|c| c.path == "new.rs").is_some_and(|c| c.lines.is_empty() && c.added == 1));
    }

    #[test]
    fn context_keeps_a_few_lines_around_each_change_and_marks_gaps() {
        let old: String = (0..40).map(|i| format!("line {i}\n")).collect();
        let new = old.replace("line 5\n", "five\n").replace("line 30\n", "thirty\n");
        let (ops, ..) = diff_lines(&old, &new);
        let (lines, truncated) = with_context(&ops);
        assert!(!truncated);
        assert!(lines.iter().any(|l| l.op == '@'), "the two changes are far apart");
        assert!(lines.len() < 25, "{}", lines.len());
        assert!(lines.iter().any(|l| l.op == '+' && l.text == "thirty"));
    }

    #[test]
    fn a_huge_change_is_shown_as_replaced_not_computed() {
        let old: String = (0..2500).map(|i| format!("a{i}\n")).collect();
        let new: String = (0..2500).map(|i| format!("b{i}\n")).collect();
        let (_, added, removed) = diff_lines(&old, &new);
        assert_eq!((added, removed), (2500, 2500));
    }

    #[test]
    fn a_real_folder_is_snapshotted_and_compared() {
        let d = tempfile::tempdir().unwrap();
        let ws = crate::workspace::Workspace::create(d.path(), "T", "make a thing", crate::workspace::TargetRef::None).unwrap();
        let app = ws.app_dir();
        std::fs::create_dir_all(app.join("src")).unwrap();
        std::fs::create_dir_all(app.join("node_modules")).unwrap();
        std::fs::write(app.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(app.join("node_modules/x.js"), "skipped").unwrap();
        assert!(!report(&ws, None).has_baseline, "no baseline until a run starts");
        mark(&ws);
        std::fs::write(app.join("src/main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        std::fs::write(app.join("README.md"), "# T\n").unwrap();
        let r = report(&ws, None);
        assert!(r.has_baseline);
        let names: Vec<&str> = r.changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(names, ["README.md", "src/main.rs"], "the skipped folder and Axiom's own files are left out");
        // A baseline of another version is ignored.
        std::fs::write(ws.root.join(BASELINE_FILE), r#"{"version":99,"files":{}}"#).unwrap();
        assert!(!report(&ws, None).has_baseline);
    }
}
