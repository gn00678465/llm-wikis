//! Fingerprint and on-disk probe-store tests (spec §15.1; plan Task 11 Steps
//! 1-3).
//!
//! Fingerprint tests reuse `crate::query`'s already-implemented, pub
//! fingerprint functions (Task 10) rather than duplicating the algorithm.
//! Every skill-directory fixture, including its `__pycache__/` content, is
//! built at test run time in a `tempfile::tempdir()` — never under
//! `tests/fixtures/` (plan Task 11 Step 1).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use llm_wikis::config::LoadMode;
use llm_wikis::error::ErrorCode;
use llm_wikis::output::Agent;
use llm_wikis::probes::{
    FileProbeStore, ProbeKey, ProbeReader, ProbeRecord, ProbeWriter,
    check_no_plugin_lifecycle_components,
};
use llm_wikis::query::{
    CompatibilityFingerprintInput, PROVIDER_CONTRACT_VERSION, compute_compatibility_fingerprint,
    compute_skill_fingerprint,
};

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// Builds a small skill directory, including a regenerable `__pycache__/`
/// (spec §15.1's exclusion) at test run time.
fn build_skill_dir(root: &Path) -> PathBuf {
    let skill_dir = root.join("skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), b"# Wiki Query\nbody").unwrap();
    fs::write(skill_dir.join("script.py"), b"print('hello')\n").unwrap();
    let pycache = skill_dir.join("__pycache__");
    fs::create_dir_all(&pycache).unwrap();
    fs::write(pycache.join("script.cpython-311.pyc"), b"bytecode v1").unwrap();
    fs::write(skill_dir.join("script.pyo"), b"legacy optimized bytecode").unwrap();
    skill_dir
}

fn regenerate_pycache(skill_dir: &Path) {
    let pycache = skill_dir.join("__pycache__");
    fs::remove_dir_all(&pycache).unwrap();
    fs::create_dir_all(&pycache).unwrap();
    // Different bytes and even a different file name than before: a real
    // recompilation is not byte-stable across runs/interpreter versions.
    fs::write(
        pycache.join("script.cpython-312.pyc"),
        b"totally different bytecode, second generation",
    )
    .unwrap();
    fs::write(
        skill_dir.join("script.pyo"),
        b"different legacy optimized bytecode",
    )
    .unwrap();
}

fn sha256_hex_ok(fingerprint: &str) -> bool {
    let Some(hex) = fingerprint.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// Step 1: fingerprint tests
// ---------------------------------------------------------------------------

#[test]
fn fingerprint_algorithm() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = build_skill_dir(tmp.path());
    let fingerprint = compute_skill_fingerprint(&skill_dir).unwrap();
    assert!(
        sha256_hex_ok(&fingerprint),
        "expected lowercase sha256:<64 hex>, got {fingerprint:?}"
    );

    // Independently reproduce the exact byte stream spec §15.1 describes
    // (ordinal-sorted normalized relative path, `/`-separated, one NUL byte,
    // u64 big-endian length, raw bytes) and confirm it matches.
    use sha2::{Digest, Sha256};
    let mut relative_paths = Vec::new();
    for entry in walkdir_flat(&skill_dir) {
        let rel = entry
            .strip_prefix(&skill_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.split('/').any(|seg| seg == "__pycache__")
            || rel.ends_with(".pyc")
            || rel.ends_with(".pyo")
        {
            continue;
        }
        relative_paths.push((rel, entry));
    }
    relative_paths.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, path) in &relative_paths {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        let bytes = fs::read(path).unwrap();
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    let expected = format!("sha256:{:x}", hasher.finalize());
    assert_eq!(fingerprint, expected);
}

/// Minimal recursive file lister for the independent-reimplementation check
/// above; deliberately not the crate's own walker.
fn walkdir_flat(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            out.extend(walkdir_flat(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn pycache_excluded_regeneration_does_not_change_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = build_skill_dir(tmp.path());
    let before = compute_skill_fingerprint(&skill_dir).unwrap();

    regenerate_pycache(&skill_dir);
    let after_pycache_regen = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_eq!(
        before, after_pycache_regen,
        "regenerating __pycache__/*.pyc and *.pyo must not change the fingerprint"
    );

    // Every other file must still count: a real content change must move it.
    fs::write(skill_dir.join("SKILL.md"), b"# Wiki Query\nCHANGED body").unwrap();
    let after_real_change = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_ne!(
        before, after_real_change,
        "a real skill-file change must change the fingerprint"
    );
}

#[test]
fn fingerprint_full_coverage() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = build_skill_dir(tmp.path());
    let baseline = compute_skill_fingerprint(&skill_dir).unwrap();

    // Adding a file changes it.
    fs::write(skill_dir.join("extra.md"), b"new content").unwrap();
    let with_extra = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_ne!(baseline, with_extra);

    // Modifying a nested file changes it.
    let nested = skill_dir.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("deep.md"), b"deep content v1").unwrap();
    let with_nested = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_ne!(with_extra, with_nested);
    fs::write(nested.join("deep.md"), b"deep content v2").unwrap();
    let with_nested_changed = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_ne!(with_nested, with_nested_changed);

    // Removing a file changes it back.
    fs::remove_file(skill_dir.join("extra.md")).unwrap();
    let after_remove = compute_skill_fingerprint(&skill_dir).unwrap();
    assert_ne!(with_nested_changed, after_remove);
}

#[cfg(unix)]
fn try_symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}
#[cfg(windows)]
fn try_symlink(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

#[test]
fn fingerprint_special_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = build_skill_dir(tmp.path());
    let target = skill_dir.join("SKILL.md");
    let link = skill_dir.join("sneaky-link.md");
    if !try_symlink(&target, &link) {
        eprintln!(
            "SKIPPED: cannot create symlink fixture on this host (requires Windows developer mode / elevated privilege, or an unprivileged Unix account)"
        );
        return;
    }
    let err = compute_skill_fingerprint(&skill_dir).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsafeFilesystemEntry);
}

// ---------------------------------------------------------------------------
// Claude local-plugin fingerprint / lifecycle-component rejection
// ---------------------------------------------------------------------------

fn build_plugin_dir(root: &Path) -> PathBuf {
    let plugin_dir = root.join("plugin");
    let manifest_dir = plugin_dir.join(".claude-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        br#"{"name":"wiki-query-plugin","version":"1.0.0"}"#,
    )
    .unwrap();
    let skills_dir = plugin_dir.join("skills/wiki-query");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::write(skills_dir.join("SKILL.md"), b"# Wiki Query\nplugin body").unwrap();
    plugin_dir
}

#[test]
fn plugin_fingerprint_includes_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let plugin_dir = build_plugin_dir(tmp.path());
    check_no_plugin_lifecycle_components(&plugin_dir).expect("clean plugin passes");
    let before = compute_skill_fingerprint(&plugin_dir).unwrap();

    // The manifest is a regular file under plugin_dir, so the generic
    // fingerprint walk already includes it (spec §15.1) — prove it by
    // changing only the manifest and observing the fingerprint move.
    fs::write(
        plugin_dir.join(".claude-plugin/plugin.json"),
        br#"{"name":"wiki-query-plugin","version":"1.0.1"}"#,
    )
    .unwrap();
    let after = compute_skill_fingerprint(&plugin_dir).unwrap();
    assert_ne!(
        before, after,
        "manifest content must participate in the fingerprint"
    );
}

#[test]
fn plugin_lifecycle_components_rejected() {
    let tmp = tempfile::tempdir().unwrap();

    let plugin_dir = build_plugin_dir(&tmp.path().join("hooks-dir-case"));
    fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
    let err = check_no_plugin_lifecycle_components(&plugin_dir).unwrap_err();
    assert_eq!(err.code, ErrorCode::EntrypointInvalid);

    let plugin_dir2 = build_plugin_dir(&tmp.path().join("mcp-json-case"));
    fs::write(plugin_dir2.join("mcp.json"), b"{}").unwrap();
    let err2 = check_no_plugin_lifecycle_components(&plugin_dir2).unwrap_err();
    assert_eq!(err2.code, ErrorCode::EntrypointInvalid);

    let plugin_dir3 = build_plugin_dir(&tmp.path().join("manifest-key-case"));
    fs::write(
        plugin_dir3.join(".claude-plugin/plugin.json"),
        br#"{"name":"x","version":"1.0.0","hooks":{"pre":"do-something"}}"#,
    )
    .unwrap();
    let err3 = check_no_plugin_lifecycle_components(&plugin_dir3).unwrap_err();
    assert_eq!(err3.code, ErrorCode::EntrypointInvalid);
}

// ---------------------------------------------------------------------------
// compatibility_fingerprint sensitivity (OFF-216, OFF-217, OFF-218)
// ---------------------------------------------------------------------------

fn baseline_input(query_prompt: &str) -> CompatibilityFingerprintInput<'static> {
    CompatibilityFingerprintInput {
        wiki_id: "agents",
        title: "Agents Knowledge Base",
        project_root: "/wikis/agents",
        content_root: "/wikis/agents",
        query_prompt: Box::leak(query_prompt.to_string().into_boxed_str()),
        agent: Agent::Claude,
        load: "project_skill",
        entrypoint: "/wiki-query",
        skill_path: Some(".claude/skills/wiki-query/SKILL.md"),
        plugin_dir: None,
        executable_declaration: "claude",
        provider_contract_version: PROVIDER_CONTRACT_VERSION,
    }
}

#[test]
fn compatibility_fingerprint_inputs_sensitivity() {
    let base = baseline_input("Use the wiki-query skill to answer from this wiki.");
    let base_fp = compute_compatibility_fingerprint(&base);

    let mut changed_wiki_id = baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_wiki_id.wiki_id = "harness-engineering";
    assert_ne!(base_fp, compute_compatibility_fingerprint(&changed_wiki_id));

    let mut changed_title = baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_title.title = "Different Title";
    assert_ne!(base_fp, compute_compatibility_fingerprint(&changed_title));

    let mut changed_project_root =
        baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_project_root.project_root = "/wikis/other";
    assert_ne!(
        base_fp,
        compute_compatibility_fingerprint(&changed_project_root)
    );

    let mut changed_content_root =
        baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_content_root.content_root = "/wikis/agents/sub";
    assert_ne!(
        base_fp,
        compute_compatibility_fingerprint(&changed_content_root)
    );

    let mut changed_agent = baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_agent.agent = Agent::Codex;
    assert_ne!(base_fp, compute_compatibility_fingerprint(&changed_agent));

    let mut changed_load = baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_load.load = "local_plugin";
    assert_ne!(base_fp, compute_compatibility_fingerprint(&changed_load));

    let mut changed_entrypoint =
        baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_entrypoint.entrypoint = "/other-entrypoint";
    assert_ne!(
        base_fp,
        compute_compatibility_fingerprint(&changed_entrypoint)
    );

    let mut changed_skill_path =
        baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_skill_path.skill_path = Some(".claude/skills/other/SKILL.md");
    assert_ne!(
        base_fp,
        compute_compatibility_fingerprint(&changed_skill_path)
    );

    let mut changed_exe = baseline_input("Use the wiki-query skill to answer from this wiki.");
    changed_exe.executable_declaration = "/abs/path/to/claude";
    assert_ne!(base_fp, compute_compatibility_fingerprint(&changed_exe));
}

#[test]
fn query_prompt_invalidates_compatibility_fingerprint() {
    let a = baseline_input("Use the wiki-query skill to answer from this wiki.");
    let b = baseline_input("Use the wiki-query skill to answer from THIS wiki.");
    assert_ne!(
        compute_compatibility_fingerprint(&a),
        compute_compatibility_fingerprint(&b),
        "any query_prompt byte change must invalidate compatibility_fingerprint"
    );
}

#[test]
fn presentation_fields_no_invalidation() {
    // `CompatibilityFingerprintInput` structurally has no timeout/byte-limit/
    // comment/TOML-key-order field at all (spec §15.1: these must never
    // invalidate a probe) — two runs built from configs that differ only in
    // `[runtime]` settings or TOML formatting still produce the exact same
    // input struct and therefore the identical fingerprint.
    let run_1 = baseline_input("Use the wiki-query skill to answer from this wiki.");
    let run_2 = baseline_input("Use the wiki-query skill to answer from this wiki.");
    assert_eq!(
        compute_compatibility_fingerprint(&run_1),
        compute_compatibility_fingerprint(&run_2)
    );
}

// ---------------------------------------------------------------------------
// Step 2: on-disk probe store
// ---------------------------------------------------------------------------

fn probe_store_path(tmp: &Path) -> PathBuf {
    tmp.join("cache").join("llm-wikis").join("probes-v1.json")
}

fn sample_key(entrypoint: &str, agent: Agent) -> ProbeKey {
    ProbeKey {
        wiki_id: "agents".into(),
        canonical_project_root: "/wikis/agents".into(),
        canonical_content_root: "/wikis/agents".into(),
        agent,
        load: LoadMode::ProjectSkill,
        entrypoint: entrypoint.into(),
    }
}

fn sample_record(version: &str) -> ProbeRecord {
    ProbeRecord {
        agent_executable: "/usr/local/bin/claude".into(),
        agent_version: version.into(),
        skill_fingerprint:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        compatibility_fingerprint:
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        verified_at: "2026-07-30T12:00:00Z".into(),
    }
}

#[test]
fn document_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let store = FileProbeStore::new(path.clone());
    let key = sample_key("/wiki-query", Agent::Claude);
    let record = sample_record("2.1.220");
    store.publish(&key, &record).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schema_version"], 1);
    let records = value["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r["wiki_id"], "agents");
    assert_eq!(r["canonical_project_root"], "/wikis/agents");
    assert_eq!(r["canonical_content_root"], "/wikis/agents");
    assert_eq!(r["agent"], "claude");
    assert_eq!(r["agent_executable"], "/usr/local/bin/claude");
    assert_eq!(r["agent_version"], "2.1.220");
    assert_eq!(r["load"], "project_skill");
    assert_eq!(r["entrypoint"], "/wiki-query");
    assert_eq!(r["skill_fingerprint"], record.skill_fingerprint);
    assert_eq!(
        r["compatibility_fingerprint"],
        record.compatibility_fingerprint
    );
    assert_eq!(r["verified_at"], "2026-07-30T12:00:00Z");

    let object_keys: std::collections::BTreeSet<String> =
        r.as_object().unwrap().keys().cloned().collect();
    let expected: std::collections::BTreeSet<String> = [
        "wiki_id",
        "canonical_project_root",
        "canonical_content_root",
        "agent",
        "agent_executable",
        "agent_version",
        "load",
        "entrypoint",
        "skill_fingerprint",
        "compatibility_fingerprint",
        "verified_at",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        object_keys, expected,
        "no prompt/answer/wiki content field ever appears"
    );
}

#[test]
fn no_content_stored() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let store = FileProbeStore::new(path.clone());
    store
        .publish(
            &sample_key("/wiki-query", Agent::Claude),
            &sample_record("2.1.220"),
        )
        .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    for forbidden in [
        "prompt",
        "answer",
        "citations",
        "gaps",
        "warnings",
        "question",
    ] {
        assert!(
            !text.contains(forbidden),
            "probe document must never carry a {forbidden:?} field"
        );
    }
}

#[test]
fn logical_key() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FileProbeStore::new(probe_store_path(tmp.path()));

    let claude_key = sample_key("/wiki-query", Agent::Claude);
    let codex_key = sample_key("$wiki-query", Agent::Codex);
    store
        .publish(&claude_key, &sample_record("2.1.220"))
        .unwrap();
    store.publish(&codex_key, &sample_record("0.45.0")).unwrap();

    assert_eq!(
        store
            .current_record(&claude_key)
            .unwrap()
            .unwrap()
            .agent_version,
        "2.1.220"
    );
    assert_eq!(
        store
            .current_record(&codex_key)
            .unwrap()
            .unwrap()
            .agent_version,
        "0.45.0"
    );

    // A different entrypoint is a different logical key entirely.
    let other_entrypoint_key = sample_key("/other", Agent::Claude);
    assert_eq!(store.current_record(&other_entrypoint_key).unwrap(), None);

    // A different content_root is a different logical key entirely.
    let mut other_root_key = sample_key("/wiki-query", Agent::Claude);
    other_root_key.canonical_content_root = "/wikis/agents/sub".into();
    assert_eq!(store.current_record(&other_root_key).unwrap(), None);
}

#[test]
fn verification_tuple() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FileProbeStore::new(probe_store_path(tmp.path()));
    let key = sample_key("/wiki-query", Agent::Claude);
    let record = sample_record("2.1.220");
    store.publish(&key, &record).unwrap();

    let read_back = store.current_record(&key).unwrap().unwrap();
    assert_eq!(read_back, record);
}

#[test]
fn replace_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FileProbeStore::new(probe_store_path(tmp.path()));
    let key = sample_key("/wiki-query", Agent::Claude);

    store.publish(&key, &sample_record("2.1.220")).unwrap();
    store.publish(&key, &sample_record("2.1.221")).unwrap();

    let current = store.current_record(&key).unwrap().unwrap();
    assert_eq!(current.agent_version, "2.1.221");

    let text = fs::read_to_string(store.path()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let records = value["records"].as_array().unwrap();
    let matching: Vec<_> = records
        .iter()
        .filter(|r| r["wiki_id"] == "agents" && r["entrypoint"] == "/wiki-query")
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "no historical fingerprint may accumulate for the same logical key"
    );
}

#[test]
fn unverified_states() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let key = sample_key("/wiki-query", Agent::Claude);

    // Missing document.
    let store = FileProbeStore::new(path.clone());
    assert_eq!(store.current_record(&key).unwrap(), None);

    // Malformed JSON.
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{ not json").unwrap();
    assert_eq!(store.current_record(&key).unwrap(), None);

    // Duplicate records for the same logical key (hand-written, never
    // produced by `publish` itself) must not be arbitrarily resolved.
    let duplicate_doc = serde_json::json!({
        "schema_version": 1,
        "records": [
            {
                "wiki_id": "agents",
                "canonical_project_root": "/wikis/agents",
                "canonical_content_root": "/wikis/agents",
                "agent": "claude",
                "agent_executable": "/usr/local/bin/claude",
                "agent_version": "2.1.220",
                "load": "project_skill",
                "entrypoint": "/wiki-query",
                "skill_fingerprint": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "compatibility_fingerprint": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "verified_at": "2026-07-30T12:00:00Z"
            },
            {
                "wiki_id": "agents",
                "canonical_project_root": "/wikis/agents",
                "canonical_content_root": "/wikis/agents",
                "agent": "claude",
                "agent_executable": "/usr/local/bin/claude",
                "agent_version": "2.1.221",
                "load": "project_skill",
                "entrypoint": "/wiki-query",
                "skill_fingerprint": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "compatibility_fingerprint": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "verified_at": "2026-07-30T13:00:00Z"
            }
        ]
    });
    fs::write(&path, serde_json::to_vec(&duplicate_doc).unwrap()).unwrap();
    assert_eq!(
        store.current_record(&key).unwrap(),
        None,
        "a duplicate record for the same logical key must be unverified, never an arbitrary pick"
    );

    // Mismatched schema_version is also treated as absent/malformed.
    let wrong_schema = serde_json::json!({ "schema_version": 2, "records": [] });
    fs::write(&path, serde_json::to_vec(&wrong_schema).unwrap()).unwrap();
    assert_eq!(store.current_record(&key).unwrap(), None);
}

#[test]
fn atomic_write() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let store = FileProbeStore::new(path.clone());
    store
        .publish(
            &sample_key("/wiki-query", Agent::Claude),
            &sample_record("2.1.220"),
        )
        .unwrap();

    // No leftover temp file in the same directory after a successful publish.
    let parent = path.parent().unwrap();
    let leftover_tmp = fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with(".probes-"));
    assert!(
        !leftover_tmp,
        "no dangling temp file after a successful publish"
    );

    // The final file itself parses cleanly (never truncated/interleaved).
    let text = fs::read_to_string(&path).unwrap();
    let _: serde_json::Value = serde_json::from_str(&text).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "probe file must be user-only readable/writable"
        );
    }
}

#[test]
fn concurrent_publish() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let key = sample_key("/wiki-query", Agent::Claude);

    let thread_count = 6;
    let barrier = Arc::new(Barrier::new(thread_count));
    let mut handles = Vec::new();
    for i in 0..thread_count {
        let path = path.clone();
        let key = key.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let store = FileProbeStore::new(path);
            barrier.wait();
            store
                .publish(&key, &sample_record(&format!("2.1.22{i}")))
                .unwrap();
        }));
    }
    for h in handles {
        h.join().expect("publisher thread must not panic");
    }

    let text = fs::read_to_string(&path).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&text).expect("final document must be valid, complete JSON");
    let records = value["records"].as_array().unwrap();
    let matching: Vec<_> = records
        .iter()
        .filter(|r| r["wiki_id"] == "agents" && r["entrypoint"] == "/wiki-query")
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "two concurrent publishes for the same logical key must leave exactly one record"
    );
}

#[test]
fn reader_during_publish_sees_old_or_new_complete_document() {
    let tmp = tempfile::tempdir().unwrap();
    let path = probe_store_path(tmp.path());
    let key = sample_key("/wiki-query", Agent::Claude);
    let store = FileProbeStore::new(path.clone());
    store.publish(&key, &sample_record("2.1.220")).unwrap();

    let writer_path = path.clone();
    let writer_key = key.clone();
    let writer = thread::spawn(move || {
        let store = FileProbeStore::new(writer_path);
        for i in 0..50 {
            store
                .publish(&writer_key, &sample_record(&format!("2.1.{i}")))
                .unwrap();
        }
    });

    let reader_path = path.clone();
    let reader = thread::spawn(move || {
        for _ in 0..200 {
            match fs::read_to_string(&reader_path) {
                Ok(text) => {
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);
                    assert!(
                        parsed.is_ok(),
                        "a reader racing a publish must see a complete document, never a partial one"
                    );
                }
                Err(_) => {
                    // A transient "not found"/"in use" during the atomic
                    // rename swap is acceptable; a garbled read is not.
                }
            }
        }
    });

    writer.join().expect("writer thread must not panic");
    reader.join().expect("reader thread must not panic");
}
