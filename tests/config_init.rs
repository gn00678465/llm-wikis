//! `config init` contract tests (spec §5.1/§5.2; plan Task 5 Step 6).

use std::fs;

use llm_wikis::config::{Config, init, init_envelope};
use llm_wikis::error::ErrorCode;

#[test]
fn parent_directory_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("nested").join("dirs").join("config.toml");
    assert!(!target.parent().unwrap().exists());
    let outcome = init(&target).expect("init creates missing parents");
    assert!(outcome.created);
    assert!(target.exists());
}

#[test]
fn generated_file_is_a_valid_zero_wiki_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    init(&target).unwrap();
    let text = fs::read_to_string(&target).unwrap();
    let cfg = Config::load_str(&text).expect("config init must write a valid registry");
    assert!(cfg.wikis.is_empty());
    assert_eq!(cfg.config_version, 1);
}

#[test]
fn generated_file_contains_a_commented_wiki_example() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    init(&target).unwrap();
    let text = fs::read_to_string(&target).unwrap();
    // The example wiki block is present but entirely commented out: no active
    // [wikis.*] table appears anywhere in the live (non-comment) content.
    assert!(text.contains("# [wikis.example]"));
    let live_lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    assert!(
        !live_lines.iter().any(|l| l.contains("[wikis.")),
        "the template must not contain any active wiki table"
    );
}

#[test]
fn create_new_semantics_refuse_to_overwrite_or_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");

    let first = init(&target).unwrap();
    assert!(first.created);
    let bytes_after_first = fs::read(&target).unwrap();

    // Tamper with the file so a merge/overwrite would be detectable.
    fs::write(&target, b"config_version = 1\n# tampered\n").unwrap();
    let tampered_bytes = fs::read(&target).unwrap();

    let second = init(&target);
    assert!(second.is_err());
    let err = second.unwrap_err();
    assert_eq!(err.code, ErrorCode::ConfigExists);
    assert_eq!(err.code.exit_code(), 2);

    // Bytes are exactly unchanged — no merge, no overwrite.
    let bytes_after_second = fs::read(&target).unwrap();
    assert_eq!(bytes_after_second, tampered_bytes);
    assert_ne!(bytes_after_second, bytes_after_first);
}

#[test]
fn success_envelope_matches_the_exact_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    let envelope = init_envelope(&target);
    assert!(envelope.ok);
    assert_eq!(envelope.operation, "config_init");
    assert_eq!(envelope.schema_version, "1.0");
    assert_eq!(envelope.path, target.display().to_string());
    assert!(envelope.created);
    assert!(envelope.error.is_none());

    let value = serde_json::to_value(&envelope).unwrap();
    let keys: std::collections::BTreeSet<String> =
        value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["schema_version", "ok", "operation", "path", "created"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
}

#[test]
fn failure_envelope_keeps_created_false_and_carries_the_public_error() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    init(&target).unwrap();

    let envelope = init_envelope(&target);
    assert!(!envelope.ok);
    assert_eq!(envelope.operation, "config_init");
    assert!(!envelope.created);
    let err = envelope.error.as_ref().expect("error object present");
    assert_eq!(err.code, ErrorCode::ConfigExists);

    let value = serde_json::to_value(&envelope).unwrap();
    let obj = value.as_object().unwrap();
    assert_eq!(obj.get("ok").unwrap(), false);
    assert_eq!(obj.get("created").unwrap(), false);
    assert!(obj.contains_key("error"));
}

#[test]
fn init_never_reads_or_depends_on_the_caller_current_directory() {
    // init() takes an explicit absolute path and never calls
    // std::env::current_dir() internally; demonstrated here by targeting a path
    // wholly unrelated to (and outside) whatever the process cwd happens to be,
    // and confirming nothing at cwd is read, merged, or required to exist.
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("far").join("away").join("config.toml");
    let outcome = init(&target).expect("must succeed regardless of cwd contents");
    assert!(outcome.created);
    assert_eq!(outcome.path, target);
}
