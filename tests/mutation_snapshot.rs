//! Full-content mutation snapshot tests (spec §12, plan Task 7).
//!
//! Covers: full-tree coverage record shape; deterministic entry order;
//! same-size rewrites with restored timestamps; additions/removals/type
//! changes; `changed_paths` sorted/unique/relative/slash-separated/no
//! content; `.claude`/`.agents` exclusion geometry (excluded only
//! immediately beneath `content_root`, not deeper); symlinks outside the
//! excluded directories aborting with `UNSAFE_FILESYSTEM_ENTRY`; unreadable
//! files; root-escape safety; and identical-tree no-violation. Also runs the
//! full walk against both Task 6 fixtures (`tests/fixtures/wiki-flat`,
//! `tests/fixtures/wiki-typed`) read-only, asserting every non-`.claude`/
//! `.agents` subtree (`SCHEMA.md`, `config/`, `bin/`, `raw/`, `assets/`,
//! `graph/`) is detected as part of the snapshot.

use std::fs;
use std::path::{Path, PathBuf};

use llm_wikis::snapshot::{EntryKind, SnapshotError, compare_snapshots, take_snapshot};

fn make_dir(p: &Path) {
    fs::create_dir_all(p).unwrap();
}

fn make_file(p: &Path, contents: &[u8]) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

#[cfg(windows)]
fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => true,
        Err(e) => {
            eprintln!(
                "SKIP: cannot create symlink fixture {} -> {} ({e}); Windows developer mode or an elevated privilege is required",
                link.display(),
                target.display()
            );
            false
        }
    }
}

#[cfg(not(windows))]
fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    match std::os::unix::fs::symlink(target, link) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("SKIP: cannot create symlink fixture ({e})");
            false
        }
    }
}

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

// ---------------------------------------------------------------------------
// Full-tree coverage record shape
// ---------------------------------------------------------------------------

#[test]
fn full_coverage_entry_shape_record() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("SCHEMA.md"), b"# schema");
    make_file(&root.join("config/settings.toml"), b"k = 1");
    make_file(&root.join("bin/reindex.sh"), b"#!/bin/sh\necho hi\n");
    make_file(&root.join("raw/source-notes.txt"), b"raw notes");
    make_file(&root.join("assets/diagram.svg"), b"<svg></svg>");
    make_file(&root.join("graph/graph.bin"), &[0u8, 1, 2, 255, 254, 0]);
    make_file(&root.join("wiki/pages/page.md"), b"# Page");

    let snap = take_snapshot(root).expect("snapshot succeeds");

    let by_path: std::collections::HashMap<&str, &llm_wikis::snapshot::SnapshotEntry> = snap
        .entries
        .iter()
        .map(|e| (e.relative_path.as_str(), e))
        .collect();

    for expected in [
        "SCHEMA.md",
        "config",
        "config/settings.toml",
        "bin",
        "bin/reindex.sh",
        "raw",
        "raw/source-notes.txt",
        "assets",
        "assets/diagram.svg",
        "graph",
        "graph/graph.bin",
        "wiki",
        "wiki/pages",
        "wiki/pages/page.md",
    ] {
        assert!(
            by_path.contains_key(expected),
            "missing expected entry {expected:?}; got {:?}",
            by_path.keys().collect::<Vec<_>>()
        );
    }

    let schema = by_path["SCHEMA.md"];
    assert_eq!(schema.kind, EntryKind::File);
    assert_eq!(schema.byte_len, Some(8));
    assert!(schema.sha256.is_some());

    let config_dir = by_path["config"];
    assert_eq!(config_dir.kind, EntryKind::Dir);
    assert_eq!(config_dir.byte_len, None);
    assert_eq!(config_dir.sha256, None);

    // Binary content under graph/ is hashed identically to text content.
    let graph_bin = by_path["graph/graph.bin"];
    assert_eq!(graph_bin.kind, EntryKind::File);
    assert_eq!(graph_bin.byte_len, Some(6));
    assert!(graph_bin.sha256.is_some());
}

#[test]
fn deterministic_order_is_stable_across_repeated_snapshots() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("wiki/b.md"), b"b");
    make_file(&root.join("wiki/a.md"), b"a");
    make_file(&root.join("SCHEMA.md"), b"schema");

    let first = take_snapshot(root).unwrap();
    let second = take_snapshot(root).unwrap();
    assert_eq!(first, second);

    let paths: Vec<&str> = first
        .entries
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(
        paths, sorted,
        "entries must be in sorted relative-path order"
    );
}

// ---------------------------------------------------------------------------
// Same-size rewrite with restored timestamps
// ---------------------------------------------------------------------------

#[test]
fn same_size_rewrite_with_restored_mtime_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let target = root.join("wiki/pages/architecture.md");
    make_file(&target, b"original content!!");

    let original_mtime = fs::metadata(&target).unwrap().modified().unwrap();
    let original_atime = filetime::FileTime::from_system_time(
        fs::metadata(&target)
            .unwrap()
            .accessed()
            .unwrap_or(original_mtime),
    );
    let original_mtime_ft = filetime::FileTime::from_system_time(original_mtime);

    let before = take_snapshot(root).unwrap();

    // Same byte length, different content.
    fs::write(&target, b"REWRITTEN CONTENT!!").unwrap();
    filetime::set_file_times(&target, original_atime, original_mtime_ft).unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().modified().unwrap(),
        original_mtime,
        "precondition: mtime must be restored to prove hashing (not mtime) drives detection"
    );

    let after = take_snapshot(root).unwrap();
    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["wiki/pages/architecture.md".to_string()]);
}

#[test]
fn identical_tree_produces_no_violation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("SCHEMA.md"), b"schema");
    make_file(&root.join("wiki/pages/a.md"), b"a");
    make_file(&root.join("config/settings.toml"), b"k=1");

    let before = take_snapshot(root).unwrap();
    let after = take_snapshot(root).unwrap();
    assert!(compare_snapshots(&before, &after).is_empty());
}

// ---------------------------------------------------------------------------
// Additions, removals, type changes — everywhere, not only under wiki/
// ---------------------------------------------------------------------------

#[test]
fn addition_anywhere_under_content_root_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("SCHEMA.md"), b"schema");
    make_dir(&root.join("config"));

    let before = take_snapshot(root).unwrap();
    make_file(&root.join("config/new-setting.toml"), b"added = true");
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["config/new-setting.toml".to_string()]);
}

#[test]
fn removal_anywhere_under_content_root_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("bin/reindex.sh"), b"#!/bin/sh\n");

    let before = take_snapshot(root).unwrap();
    fs::remove_file(root.join("bin/reindex.sh")).unwrap();
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["bin/reindex.sh".to_string()]);
}

#[test]
fn file_to_directory_type_change_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("raw/source-notes.txt"), b"notes");

    let before = take_snapshot(root).unwrap();
    fs::remove_file(root.join("raw/source-notes.txt")).unwrap();
    make_dir(&root.join("raw/source-notes.txt"));
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["raw/source-notes.txt".to_string()]);
    let after_entry = after
        .entries
        .iter()
        .find(|e| e.relative_path == "raw/source-notes.txt")
        .unwrap();
    assert_eq!(after_entry.kind, EntryKind::Dir);
}

#[test]
fn directory_to_file_type_change_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_dir(&root.join("assets"));

    let before = take_snapshot(root).unwrap();
    fs::remove_dir(root.join("assets")).unwrap();
    make_file(&root.join("assets"), b"now a file");
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["assets".to_string()]);
}

#[test]
fn schema_config_bin_raw_assets_graph_changes_are_all_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("SCHEMA.md"), b"schema v1");
    make_file(&root.join("config/settings.toml"), b"v = 1");
    make_file(&root.join("bin/reindex.sh"), b"echo v1");
    make_file(&root.join("raw/source-notes.txt"), b"v1 notes");
    make_file(&root.join("assets/diagram.svg"), b"<svg>v1</svg>");
    make_file(&root.join("graph/graph.bin"), &[1, 2, 3]);

    let before = take_snapshot(root).unwrap();

    make_file(&root.join("SCHEMA.md"), b"schema v2!");
    make_file(&root.join("config/settings.toml"), b"v = 2");
    make_file(&root.join("bin/reindex.sh"), b"echo v2!!");
    make_file(&root.join("raw/source-notes.txt"), b"v2 notes!");
    make_file(&root.join("assets/diagram.svg"), b"<svg>v2!!!</svg>");
    make_file(&root.join("graph/graph.bin"), &[9, 9, 9]);

    let after = take_snapshot(root).unwrap();
    let changed = compare_snapshots(&before, &after);
    assert_eq!(
        changed,
        vec![
            "SCHEMA.md".to_string(),
            "assets/diagram.svg".to_string(),
            "bin/reindex.sh".to_string(),
            "config/settings.toml".to_string(),
            "graph/graph.bin".to_string(),
            "raw/source-notes.txt".to_string(),
        ]
    );
}

#[test]
fn all_change_kinds_are_detected_together() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("kept-rewrite.md"), b"original content");
    make_file(&root.join("to-remove.md"), b"will be removed");
    make_file(&root.join("type-change"), b"file now, dir later");
    make_dir(&root.join("config"));

    let before = take_snapshot(root).unwrap();

    make_file(&root.join("added.md"), b"brand new"); // addition
    fs::remove_file(root.join("to-remove.md")).unwrap(); // removal
    fs::remove_file(root.join("type-change")).unwrap();
    make_dir(&root.join("type-change")); // file -> directory type change
    fs::write(root.join("kept-rewrite.md"), b"rewritten content!!").unwrap(); // content rewrite

    let after = take_snapshot(root).unwrap();
    let changed = compare_snapshots(&before, &after);
    assert_eq!(
        changed,
        vec![
            "added.md".to_string(),
            "kept-rewrite.md".to_string(),
            "to-remove.md".to_string(),
            "type-change".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// changed_paths shape: sorted, unique, relative, slash-separated, no content
// ---------------------------------------------------------------------------

#[test]
fn changed_paths_are_sorted_unique_relative_slash_separated_and_content_free() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("zed.md"), b"z");
    make_file(&root.join("alpha/one.md"), b"secret-content-alpha");
    make_file(&root.join("mid.md"), b"m");

    let before = take_snapshot(root).unwrap();
    fs::write(root.join("zed.md"), b"Z-CHANGED").unwrap();
    fs::write(root.join("alpha/one.md"), b"top-secret-rewrite!!").unwrap();
    fs::write(root.join("mid.md"), b"M-CHANGED").unwrap();
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);

    let mut expected_sorted = changed.clone();
    expected_sorted.sort();
    assert_eq!(changed, expected_sorted, "must already be sorted");

    let mut deduped = changed.clone();
    deduped.dedup();
    assert_eq!(changed, deduped, "must be unique");

    for path in &changed {
        assert!(!path.starts_with('/'), "must be relative: {path:?}");
        assert!(!path.contains(':'), "must have no drive prefix: {path:?}");
        assert!(!path.contains('\\'), "must be slash-separated: {path:?}");
        assert!(
            !path.split('/').any(|seg| seg == ".." || seg.is_empty()),
            "must contain no traversal: {path:?}"
        );
        assert!(
            !path.contains("secret") && !path.contains("SECRET"),
            "must never carry file content, only the path: {path:?}"
        );
    }
    assert_eq!(
        changed,
        vec![
            "alpha/one.md".to_string(),
            "mid.md".to_string(),
            "zed.md".to_string()
        ]
    );
}

// ---------------------------------------------------------------------------
// `.claude`/`.agents` exclusion geometry
// ---------------------------------------------------------------------------

#[test]
fn dot_claude_and_dot_agents_immediately_under_content_root_are_excluded() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(
        &root.join(".claude/skills/wiki-query/SKILL.md"),
        b"skill v1",
    );
    make_file(&root.join(".agents/notes.md"), b"agent notes v1");
    make_file(&root.join("SCHEMA.md"), b"schema");

    let before = take_snapshot(root).unwrap();
    assert!(
        before
            .entries
            .iter()
            .all(|e| !e.relative_path.starts_with(".claude")
                && !e.relative_path.starts_with(".agents")),
        "excluded subtrees must not appear as entries at all: {:?}",
        before
            .entries
            .iter()
            .map(|e| &e.relative_path)
            .collect::<Vec<_>>()
    );

    // Mutating content strictly inside the excluded top-level trees must
    // never surface as a change: it was never recorded in the first place.
    fs::write(
        root.join(".claude/skills/wiki-query/SKILL.md"),
        b"skill v2 CHANGED",
    )
    .unwrap();
    fs::write(root.join(".agents/notes.md"), b"agent notes v2 CHANGED").unwrap();
    let after = take_snapshot(root).unwrap();
    assert!(compare_snapshots(&before, &after).is_empty());
}

#[test]
fn nested_dot_claude_deeper_in_the_tree_is_not_excluded() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // A `.claude` directory that is NOT an immediate child of content_root
    // (it sits under `wiki/`) must be fully monitored like any other path.
    make_file(&root.join("wiki/.claude/nested.md"), b"nested v1");

    let before = take_snapshot(root).unwrap();
    assert!(
        before
            .entries
            .iter()
            .any(|e| e.relative_path == "wiki/.claude/nested.md"),
        "a .claude nested deeper than content_root's immediate children must be recorded"
    );

    fs::write(root.join("wiki/.claude/nested.md"), b"nested v2 CHANGED").unwrap();
    let after = take_snapshot(root).unwrap();
    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["wiki/.claude/nested.md".to_string()]);
}

// ---------------------------------------------------------------------------
// Unsafe filesystem entries
// ---------------------------------------------------------------------------

#[test]
fn special_entry_aborts_with_unsafe_filesystem_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let target_dir = tmp.path().join("outside-target");
    make_dir(&target_dir);
    make_file(&target_dir.join("real.md"), b"real content");

    let link = root.join("wiki/pages/linked-in");
    make_dir(link.parent().unwrap());
    if !try_symlink_dir(&target_dir, &link) {
        return; // SKIP: privilege not held on this machine.
    }

    let result = take_snapshot(root);
    match result {
        Err(SnapshotError::Unsafe(app_err)) => {
            assert_eq!(
                app_err.code,
                llm_wikis::error::ErrorCode::UnsafeFilesystemEntry
            );
        }
        other => panic!("expected SnapshotError::Unsafe, got {other:?}"),
    }
}

#[test]
fn symlink_nested_inside_excluded_top_level_dir_never_aborts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let target_dir = tmp.path().join("outside-target");
    make_dir(&target_dir);
    make_file(&target_dir.join("real.md"), b"real content");

    // A symlink placed *inside* the excluded `.claude` subtree must never be
    // inspected at all: the whole excluded subtree is skipped before any
    // metadata lookup happens on its contents.
    let link = root.join(".claude/skills/linked-skill");
    make_dir(link.parent().unwrap());
    if !try_symlink_dir(&target_dir, &link) {
        return; // SKIP: privilege not held on this machine.
    }

    let snap = take_snapshot(root).expect("excluded subtree symlink must not abort the snapshot");
    assert!(
        snap.entries
            .iter()
            .all(|e| !e.relative_path.starts_with(".claude"))
    );
}

#[cfg(windows)]
#[test]
fn unreadable_file_produces_unreadable_error_not_a_panic() {
    use std::os::windows::fs::OpenOptionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let locked = root.join("wiki/locked.md");
    make_file(&locked, b"locked content");

    // Open with share_mode(0): deny all sharing, so a concurrent open for
    // read (which take_snapshot must perform to hash the file) fails.
    let _handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .expect("can exclusively lock the fixture file");

    let result = take_snapshot(root);
    match result {
        Err(SnapshotError::Unreadable(_)) => {}
        other => panic!("expected SnapshotError::Unreadable, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Root-escape safety
// ---------------------------------------------------------------------------

#[test]
fn no_entry_relative_path_ever_escapes_content_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("wiki/pages/deep/nested/page.md"), b"deep");
    make_file(&root.join("SCHEMA.md"), b"schema");

    let snap = take_snapshot(root).unwrap();
    for entry in &snap.entries {
        assert!(!entry.relative_path.starts_with('/'));
        assert!(!entry.relative_path.contains(':'));
        assert!(!entry.relative_path.contains('\\'));
        assert!(
            entry
                .relative_path
                .split('/')
                .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        );
    }
}

// ---------------------------------------------------------------------------
// Executable helpers / raw sources / byte-level changes (plan Step 1 list)
// ---------------------------------------------------------------------------

#[test]
fn executable_helper_and_raw_source_single_byte_changes_are_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    make_file(&root.join("bin/reindex.sh"), b"#!/bin/sh\nexit 0\n");
    make_file(&root.join("raw/source.txt"), b"raw-source-data-A");

    let before = take_snapshot(root).unwrap();
    // Single-byte change, same length.
    make_file(&root.join("raw/source.txt"), b"raw-source-data-B");
    let after = take_snapshot(root).unwrap();

    let changed = compare_snapshots(&before, &after);
    assert_eq!(changed, vec!["raw/source.txt".to_string()]);
}

// ---------------------------------------------------------------------------
// Task 6 fixtures: full walk, read-only
// ---------------------------------------------------------------------------

#[test]
fn wiki_flat_fixture_full_walk_covers_every_declared_subtree() {
    let root = fixture_root("wiki-flat");
    let snap = take_snapshot(&root).expect("fixture snapshot succeeds");
    let paths: std::collections::HashSet<&str> = snap
        .entries
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect();

    for expected in [
        "SCHEMA.md",
        "config/settings.toml",
        "bin/reindex.sh",
        "raw/source-notes.txt",
        "assets/diagram.svg",
        "wiki/pages/architecture.md",
    ] {
        assert!(
            paths.contains(expected),
            "missing {expected:?} in {paths:?}"
        );
    }

    // Read-only: two consecutive snapshots of the committed fixture agree.
    let snap2 = take_snapshot(&root).unwrap();
    assert!(compare_snapshots(&snap, &snap2).is_empty());
}

#[test]
fn wiki_typed_fixture_full_walk_covers_graph_binary() {
    let root = fixture_root("wiki-typed");
    let snap = take_snapshot(&root).expect("fixture snapshot succeeds");
    let by_path: std::collections::HashMap<&str, &llm_wikis::snapshot::SnapshotEntry> = snap
        .entries
        .iter()
        .map(|e| (e.relative_path.as_str(), e))
        .collect();

    assert!(by_path.contains_key("graph/graph.bin"));
    let graph_entry = by_path["graph/graph.bin"];
    assert_eq!(graph_entry.kind, EntryKind::File);
    let on_disk_len = fs::metadata(root.join("graph/graph.bin")).unwrap().len();
    assert_eq!(graph_entry.byte_len, Some(on_disk_len));
    assert!(graph_entry.sha256.is_some());

    let snap2 = take_snapshot(&root).unwrap();
    assert!(compare_snapshots(&snap, &snap2).is_empty());
}
