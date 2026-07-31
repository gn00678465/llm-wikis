//! Full-content mutation snapshots (spec §12, plan Task 7).
//!
//! [`take_snapshot`] recursively enumerates every directory and regular file
//! beneath a canonical `content_root`, except the `.claude`/`.agents`
//! directories when they are **immediate children of `content_root` itself**
//! (spec §12: those two names are excluded only there, never at any deeper
//! nesting). Every accepted regular file is hashed in bounded chunks — never
//! loaded whole into memory — so the resulting [`WikiSnapshot`] can be
//! compared before/after a provider invocation without a second walk. Any
//! symlink, junction, reparse point, mount point, or other special entry
//! outside the excluded directories aborts with `UNSAFE_FILESYSTEM_ENTRY`
//! and is never descended into.
//!
//! [`compare_snapshots`] is a pure diff over two already-built snapshots: it
//! never touches the filesystem and cannot itself fail. Per spec §12,
//! `INTERNAL_ERROR` applies only when a snapshot could not be *taken* in the
//! first place (an unreadable entry produces [`SnapshotError::Unreadable`]),
//! never to the comparison of two complete snapshots.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{AppError, ErrorCode};

/// Whether a [`SnapshotEntry`] is a directory or a regular file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

/// One accepted filesystem entry beneath `content_root` (spec §12). Directory
/// entries carry no length or digest; file entries always carry both.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotEntry {
    pub relative_path: String,
    pub kind: EntryKind,
    pub byte_len: Option<u64>,
    pub sha256: Option<[u8; 32]>,
}

/// The complete content-root inventory at one point in time (spec §12).
/// Entries are sorted by `relative_path` for deterministic order.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiSnapshot {
    pub entries: Vec<SnapshotEntry>,
}

/// Failure while *taking* a snapshot. `Unsafe` is the spec's
/// `UNSAFE_FILESYSTEM_ENTRY` abort; `Unreadable` is any other I/O failure
/// (e.g. an unreadable file) that leaves the wrapper unable to complete the
/// comparison and therefore becomes `INTERNAL_ERROR` at the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotError {
    Unsafe(AppError),
    Unreadable(String),
}

const HASH_CHUNK_BYTES: usize = 64 * 1024;

#[cfg(windows)]
fn is_special_entry(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_type().is_symlink()
        || (md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || (!md.is_dir() && !md.is_file())
}

#[cfg(not(windows))]
fn is_special_entry(md: &fs::Metadata) -> bool {
    md.file_type().is_symlink() || (!md.is_dir() && !md.is_file())
}

/// Slash-separated relative path of `path` under `root`, regardless of host
/// path-separator convention.
fn normalize_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("walked path is always under its own root")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn hash_file(path: &Path) -> Result<([u8; 32], u64), SnapshotError> {
    let mut file = fs::File::open(path)
        .map_err(|e| SnapshotError::Unreadable(format!("cannot open file for hashing: {e}")))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; HASH_CHUNK_BYTES];
    let mut len: u64 = 0;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| SnapshotError::Unreadable(format!("cannot read file contents: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        len += n as u64;
    }
    Ok((hasher.finalize().into(), len))
}

/// Recursively walks `dir` (a descendant of `root`, or `root` itself on the
/// first call), skipping `.claude`/`.agents` only when `at_root` is true and
/// the entry is one of those two exact names.
fn walk_dir(
    dir: &Path,
    root: &Path,
    at_root: bool,
    entries: &mut Vec<SnapshotEntry>,
) -> Result<(), SnapshotError> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| SnapshotError::Unreadable(format!("cannot list directory contents: {e}")))?;
    for item in read_dir {
        let item = item
            .map_err(|e| SnapshotError::Unreadable(format!("cannot read directory entry: {e}")))?;
        let path = item.path();
        if at_root
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (name == ".claude" || name == ".agents")
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| {
            SnapshotError::Unreadable(format!("cannot inspect filesystem entry: {e}"))
        })?;
        if is_special_entry(&metadata) {
            return Err(SnapshotError::Unsafe(AppError::new(
                ErrorCode::UnsafeFilesystemEntry,
                "content root contains a symlink, junction, reparse point, mount point, or other special entry",
            )));
        }
        let relative_path = normalize_relative_path(root, &path);
        if metadata.is_dir() {
            entries.push(SnapshotEntry {
                relative_path,
                kind: EntryKind::Dir,
                byte_len: None,
                sha256: None,
            });
            walk_dir(&path, root, false, entries)?;
        } else {
            let (sha256, byte_len) = hash_file(&path)?;
            entries.push(SnapshotEntry {
                relative_path,
                kind: EntryKind::File,
                byte_len: Some(byte_len),
                sha256: Some(sha256),
            });
        }
    }
    Ok(())
}

/// Takes a full-content snapshot of every directory and regular file beneath
/// `content_root`, excluding only `.claude`/`.agents` immediately beneath it
/// (spec §12). `content_root` itself is not an entry.
pub fn take_snapshot(content_root: &Path) -> Result<WikiSnapshot, SnapshotError> {
    let mut entries = Vec::new();
    walk_dir(content_root, content_root, true, &mut entries)?;
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(WikiSnapshot { entries })
}

/// Compares two already-taken snapshots and returns the sorted, unique,
/// relative, slash-separated paths that were added, removed, changed kind, or
/// rewritten (byte length and/or digest differ) between them (spec §12). Pure
/// and infallible: given two complete snapshots, the diff itself cannot fail.
pub fn compare_snapshots(before: &WikiSnapshot, after: &WikiSnapshot) -> Vec<String> {
    use std::collections::BTreeMap;

    let before_map: BTreeMap<&str, &SnapshotEntry> = before
        .entries
        .iter()
        .map(|e| (e.relative_path.as_str(), e))
        .collect();
    let after_map: BTreeMap<&str, &SnapshotEntry> = after
        .entries
        .iter()
        .map(|e| (e.relative_path.as_str(), e))
        .collect();

    let mut changed: Vec<String> = Vec::new();
    for (path, before_entry) in &before_map {
        match after_map.get(path) {
            None => changed.push((*path).to_string()),
            Some(after_entry) => {
                if before_entry.kind != after_entry.kind
                    || before_entry.byte_len != after_entry.byte_len
                    || before_entry.sha256 != after_entry.sha256
                {
                    changed.push((*path).to_string());
                }
            }
        }
    }
    for path in after_map.keys() {
        if !before_map.contains_key(path) {
            changed.push((*path).to_string());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}
