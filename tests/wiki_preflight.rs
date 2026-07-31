//! Content-root preflight tests (spec §8.1 step 6, §15 `WIKI_SCHEMA_ABSENT`;
//! plan Task 6 Steps 2-3).
//!
//! No index discovery, index parsing, freshness comparison, `SCHEMA.md`
//! parsing beyond existence, or `link_style` selection/resolution is exercised
//! or implemented anywhere in this file — none of that is in scope.

use std::fs;
use std::path::{Path, PathBuf};

use llm_wikis::error::ErrorCode;
use llm_wikis::output::WarningSource;
use llm_wikis::wiki::{preflight_content_root, schema_absent_warning};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn wiki_flat() -> PathBuf {
    fixtures_dir().join("wiki-flat")
}

fn wiki_typed() -> PathBuf {
    fixtures_dir().join("wiki-typed")
}

fn make_dir(p: &Path) {
    fs::create_dir_all(p).unwrap();
}

fn make_file(p: &Path, contents: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, contents).unwrap();
}

// ---------------------------------------------------------------------------
// Step 2: minimum-structure tests
// ---------------------------------------------------------------------------

#[test]
fn missing_content_root_is_wiki_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    let err = preflight_content_root(&missing).unwrap_err();
    assert_eq!(err.code, ErrorCode::WikiInvalid);
}

#[test]
fn content_root_that_is_a_file_is_wiki_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("not-a-directory");
    make_file(&file, "just a file, not a directory");
    let err = preflight_content_root(&file).unwrap_err();
    assert_eq!(err.code, ErrorCode::WikiInvalid);
}

#[test]
fn empty_directory_is_wiki_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("empty");
    make_dir(&root);
    let err = preflight_content_root(&root).unwrap_err();
    assert_eq!(err.code, ErrorCode::WikiInvalid);
}

#[test]
fn directory_with_only_non_markdown_files_is_wiki_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("no-markdown");
    make_file(&root.join("notes.txt"), "plain text, not markdown");
    make_file(&root.join("data.json"), "{}");
    make_file(&root.join("sub").join("more.csv"), "a,b,c");
    let err = preflight_content_root(&root).unwrap_err();
    assert_eq!(err.code, ErrorCode::WikiInvalid);
}

#[test]
fn markdown_several_levels_deep_is_sufficient() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("deep");
    make_file(
        &root.join("a").join("b").join("c").join("d").join("page.md"),
        "# deep page",
    );
    let markdown_files = preflight_content_root(&root).expect("one deep .md file suffices");
    assert_eq!(markdown_files.len(), 1);
}

#[test]
fn both_fixtures_pass_preflight_unchanged() {
    // The layout-agnostic property (spec §8.1 step 6): "the wrapper asserts no
    // internal wiki layout." Neither the agents-shaped nor the
    // harness-engineering-shaped fixture is special-cased.
    let flat_files = preflight_content_root(&wiki_flat()).expect("wiki-flat must pass unchanged");
    assert!(!flat_files.is_empty());
    let typed_files =
        preflight_content_root(&wiki_typed()).expect("wiki-typed must pass unchanged");
    assert!(!typed_files.is_empty());
}

// ---------------------------------------------------------------------------
// Step 3: WIKI_SCHEMA_ABSENT warning semantics
// ---------------------------------------------------------------------------

#[test]
fn schema_absent_warning_not_emitted_for_either_fixture() {
    assert!(schema_absent_warning(&wiki_flat()).is_none());
    assert!(schema_absent_warning(&wiki_typed()).is_none());
}

#[test]
fn schema_absent_warning_emitted_when_content_root_is_one_level_above_a_fixture() {
    // fixtures_dir() is the parent of wiki-flat/ and wiki-typed/; it has no
    // SCHEMA.md of its own directly beneath it, even though both children do.
    let warning = schema_absent_warning(&fixtures_dir()).expect("must warn one level up");
    assert_eq!(warning.source, WarningSource::Wrapper);
    assert_eq!(warning.code, "WIKI_SCHEMA_ABSENT");
    assert_eq!(
        warning.message,
        "No SCHEMA.md at the content root; most wiki toolchains place one there. Confirm content_root points at the wiki root rather than a parent or child directory."
    );
}

#[test]
fn schema_absent_warning_emitted_for_a_directory_with_no_schema_md_at_all() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("no-schema");
    make_file(&root.join("page.md"), "# page");
    assert!(schema_absent_warning(&root).is_some());
}

#[test]
fn schema_present_one_level_deeper_does_not_count_at_the_root() {
    // SCHEMA.md nested inside a subdirectory is not "at the content root" —
    // existence is checked only at the immediate content_root, never recursively.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("nested-schema");
    make_file(&root.join("sub").join("SCHEMA.md"), "# nested schema");
    make_file(&root.join("page.md"), "# page");
    assert!(schema_absent_warning(&root).is_some());
}

#[test]
fn schema_md_that_is_a_directory_not_a_file_still_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("schema-is-a-dir");
    make_dir(&root.join("SCHEMA.md"));
    make_file(&root.join("page.md"), "# page");
    assert!(schema_absent_warning(&root).is_some());
}

// The warning function's return type, `Option<Warning>`, guarantees by
// construction (not merely by convention) that this check can never become a
// pass/fail outcome: there is no `AppError`/`Result::Err` path through
// `schema_absent_warning` at all, so `ok` staying `true` and exit staying `0`
// for this check follow from the type signature rather than from a runtime
// branch a future change could accidentally invert.
#[test]
fn schema_absent_warning_type_cannot_express_a_failure() {
    fn assert_is_option_warning(_: fn(&Path) -> Option<llm_wikis::output::Warning>) {}
    assert_is_option_warning(schema_absent_warning);
}
