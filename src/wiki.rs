//! Content-root preflight (spec §8.1 step 6, §15 `WIKI_SCHEMA_ABSENT`).
//!
//! The wrapper asserts no wiki layout. This module does **no** index discovery,
//! index parsing, freshness comparison, `SCHEMA.md` parsing beyond existence, or
//! `link_style` selection/resolution — none of that exists here, on purpose.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, ErrorCode};
use crate::output::{Warning, WrapperWarningCode};

/// The exact `WIKI_SCHEMA_ABSENT` warning message (spec §15). Verbatim.
pub const WIKI_SCHEMA_ABSENT_MESSAGE: &str = "No SCHEMA.md at the content root; most wiki toolchains place one there. Confirm content_root points at the wiki root rather than a parent or child directory.";

/// Every regular file beneath `root` whose name ends with the exact,
/// case-sensitive ASCII suffix `.md`, at any depth, in deterministic
/// (sorted-path) order. This single walk is also the file list citation
/// resolution consumes (spec §11.2: "reuses the file list the before-snapshot
/// already built") — nothing beyond this function walks the content root here.
///
/// ponytail: follows the OS's default symlink handling for the recursive
/// descent below `root` itself; it does not re-run `config.rs`'s
/// `UNSAFE_FILESYSTEM_ENTRY` special-entry scan (that scan already covers
/// `content_root` earlier in the real query flow, at config-resolution time).
pub fn list_markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(md) = fs::symlink_metadata(&path) else {
            continue;
        };
        if md.is_dir() {
            walk(&path, out);
        } else if md.is_file() {
            let is_markdown = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".md"));
            if is_markdown {
                out.push(path);
            }
        }
    }
}

/// Minimum-structure preflight (spec §8.1 step 6): `content_root` must exist,
/// be a directory, and contain at least one regular `.md` file anywhere
/// beneath it. Returns the discovered markdown file list on success, so a
/// caller can hand it straight to citation resolution without a second walk.
pub fn preflight_content_root(content_root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let metadata = fs::metadata(content_root)
        .map_err(|_| AppError::new(ErrorCode::WikiInvalid, "content_root does not exist"))?;
    if !metadata.is_dir() {
        return Err(AppError::new(
            ErrorCode::WikiInvalid,
            "content_root is not a directory",
        ));
    }
    let markdown_files = list_markdown_files(content_root);
    if markdown_files.is_empty() {
        return Err(AppError::new(
            ErrorCode::WikiInvalid,
            "content_root contains no regular .md file anywhere beneath it",
        ));
    }
    Ok(markdown_files)
}

/// `WIKI_SCHEMA_ABSENT` (spec §15): a warning, never a failure, emitted under
/// check name `wiki_structure` when no `SCHEMA.md` exists at the content root.
/// Existence only — the file's contents are never read or parsed.
pub fn schema_absent_warning(content_root: &Path) -> Option<Warning> {
    let schema_path = content_root.join("SCHEMA.md");
    match fs::metadata(&schema_path) {
        Ok(md) if md.is_file() => None,
        _ => Some(Warning::wrapper(
            WrapperWarningCode::WikiSchemaAbsent,
            WIKI_SCHEMA_ABSENT_MESSAGE,
        )),
    }
}
