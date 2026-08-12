//! Claude adapter tests (spec §10.2; plan Task 9 Steps 3, 4, 7).
//!
//! Argv/prompt assertions never spawn a process. Parser assertions run
//! against sanitized fixture bytes under `tests/fixtures/claude/`. `invoke`
//! assertions run against [`FakeProcessRunner`] — no real Claude process ever
//! starts in this file.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use llm_wikis::error::{AppError, ErrorCode};
use llm_wikis::output::RawFormat;
use llm_wikis::process::{ExecutableKind, ResolvedExecutable};
use llm_wikis::providers::claude::{
    CLAUDE_READ_SCOPE_BROAD_MESSAGE, ClaudeAdapter, DISABLE_ALL_HOOKS_SETTINGS,
    SETTING_SOURCES_PROJECT, build_argv, parse_claude_output, read_scope_broad_warning,
};
use llm_wikis::providers::{
    FakeProcessRunner, NON_INTERACTIVE_SYSTEM_DIRECTIVES, ProviderAdapter, ProviderRequest,
    completed_outcome,
};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"))
}

fn fake_executable() -> ResolvedExecutable {
    ResolvedExecutable {
        path: PathBuf::from("claude"),
        kind: ExecutableKind::Native,
    }
}

// ---------------------------------------------------------------------------
// Parser tests (plan Task 9 Step 4)
// ---------------------------------------------------------------------------

#[test]
fn one_json_document() {
    let good = fixture("success_structured.json");
    assert!(parse_claude_output(&good).is_ok());

    let trailing = fixture("malformed_trailing_bytes.json");
    let err = parse_claude_output(&trailing).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);
}

#[test]
fn error_subtype_rejected() {
    let fixture_bytes = fixture("error_subtype.json");
    let result = parse_claude_output(&fixture_bytes);
    assert!(
        result.is_err(),
        "is_error:true must never surface as success"
    );
}

#[test]
fn raw_format_gate() {
    let good = fixture("success_structured.json");
    let (_, format) = parse_claude_output(&good).expect("valid fixture parses");
    assert_eq!(format, RawFormat::ClaudeJson);

    let malformed = fixture("malformed_trailing_bytes.json");
    assert!(
        parse_claude_output(&malformed).is_err(),
        "a parse failure must never produce a raw_format value"
    );
}

#[test]
fn stderr_cap() {
    let runner = FakeProcessRunner::new();
    let huge_stderr = vec![b'e'; 50_000];
    runner.push_response(Ok(completed_outcome(b"not used", &huge_stderr, 1)));
    let adapter = ClaudeAdapter;
    let err = adapter
        .invoke(
            &runner,
            provider_request(fake_executable()),
            "test prompt".to_string(),
        )
        .model_result
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NonzeroExit);
    assert!(
        err.message.len() < 50_000,
        "stderr must be capped in the diagnostic message, not embedded whole"
    );
}

#[test]
fn read_scope_broad_conditional() {
    let project_root = Path::new("D:/Wikis/harness-engineering");
    let strict_subdir = Path::new("D:/Wikis/harness-engineering/wiki");
    let warning = read_scope_broad_warning(project_root, strict_subdir)
        .expect("strict subdirectory must emit the warning");
    assert_eq!(warning.message, CLAUDE_READ_SCOPE_BROAD_MESSAGE);

    let equal_roots = Path::new("D:/Wikis/agents");
    assert!(
        read_scope_broad_warning(equal_roots, equal_roots).is_none(),
        "equal roots must not emit the warning"
    );
}

#[test]
fn prefers_structured_output() {
    let bytes = fixture("prefers_structured_output.json");
    let (result, _) = parse_claude_output(&bytes).expect("valid fixture parses");
    assert_eq!(result.answer, "structured answer citing [[overview]]");
    assert_eq!(result.citations, vec!["overview".to_string()]);
}

#[test]
fn missing_structured_result_and_contract_violation() {
    let missing = fixture("missing_structured_result.json");
    let err = parse_claude_output(&missing).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);

    let empty_answer = fixture("contract_violation_empty_answer.json");
    let err = parse_claude_output(&empty_answer).unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);
}

// ---------------------------------------------------------------------------
// Argv tests (plan Task 9 Step 3)
// ---------------------------------------------------------------------------

fn schema() -> String {
    llm_wikis::providers::result_json_schema()
}

#[test]
fn exact_argv() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let expected: Vec<OsString> = vec![
        OsString::from("--add-dir"),
        OsString::from(content_root),
        OsString::from("-p"),
        OsString::from("--input-format"),
        OsString::from("text"),
        OsString::from("--no-session-persistence"),
        OsString::from("--permission-mode"),
        OsString::from("dontAsk"),
        OsString::from("--tools"),
        OsString::from("Read,Grep,Glob"),
        OsString::from("--strict-mcp-config"),
        OsString::from("--mcp-config"),
        OsString::from(mcp_config),
        OsString::from("--output-format"),
        OsString::from("json"),
        OsString::from("--json-schema"),
        OsString::from(&schema_text),
        OsString::from("--settings"),
        // Literal expected string, not `DISABLE_ALL_HOOKS_SETTINGS` (PR #1
        // Codex review iteration 3 finding 2): pinning against the
        // production constant only proves internal self-consistency —
        // flipping the constant to e.g. `{"disableAllHooks":false}` would
        // keep this assertion green. The literal byte value is what
        // actually reaches the real `claude` process.
        OsString::from(r#"{"disableAllHooks":true}"#),
        // PRD 08-06-pre-0-1-0-cli-refinements item 3/D5 and item 4/D6.
        OsString::from("--append-system-prompt"),
        OsString::from(NON_INTERACTIVE_SYSTEM_DIRECTIVES),
        OsString::from("--setting-sources"),
        OsString::from("project"),
    ];
    assert_eq!(args, expected);
    // The constant itself must still equal the literal this test pins —
    // catches the constant and the real argv drifting from each other.
    assert_eq!(DISABLE_ALL_HOOKS_SETTINGS, r#"{"disableAllHooks":true}"#);
    assert_eq!(SETTING_SOURCES_PROJECT, "project");
}

// Issue #7: `model`/`effort` are appended, in that order, after everything the
// unconfigured argv already carried. `exact_argv` above is the other half of
// this pair — it is what proves an operator who configures neither still gets
// the byte-identical argv this wrapper has always sent.
#[test]
fn exact_argv_with_model_and_effort() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let baseline = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let args = build_argv(
        content_root,
        mcp_config,
        &schema_text,
        None,
        Some("opus"),
        Some("high"),
    );

    let mut expected = baseline.clone();
    expected.push(OsString::from("--model"));
    expected.push(OsString::from("opus"));
    expected.push(OsString::from("--effort"));
    expected.push(OsString::from("high"));
    assert_eq!(args, expected);
    // Each value is its own argv element — never glued onto its flag, never
    // spliced into a single shell-ish string.
    assert_eq!(args[args.len() - 3], OsString::from("opus"));
    assert_eq!(args[args.len() - 1], OsString::from("high"));
}

#[test]
fn model_and_effort_are_independently_optional() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();

    let model_only = build_argv(
        content_root,
        mcp_config,
        &schema_text,
        None,
        Some("opus"),
        None,
    );
    assert!(model_only.iter().any(|a| a == "--model"));
    assert!(!model_only.iter().any(|a| a == "--effort"));

    let effort_only = build_argv(
        content_root,
        mcp_config,
        &schema_text,
        None,
        None,
        Some("high"),
    );
    assert!(!effort_only.iter().any(|a| a == "--model"));
    assert!(effort_only.iter().any(|a| a == "--effort"));
}

// The version and auth probes are a separate, fixed argv built by their own
// call sites (`probe_request`), never by `build_argv`. This pins that
// separation: a future refactor that routes the probes through `build_argv`
// would start sending model/effort on `--version`, which spec §10.1's bounded
// probe contract does not allow.
#[test]
fn probe_argv_never_carries_model_or_effort() {
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(b"1.2.3", b"", 0)));
    runner.push_response(Ok(completed_outcome(br#"{"authenticated":true}"#, b"", 0)));
    let exe = ResolvedExecutable {
        path: PathBuf::from("D:/tools/claude.exe"),
        kind: ExecutableKind::Native,
    };
    let adapter = ClaudeAdapter;
    adapter.version(&runner, &exe).expect("version probe");
    adapter.auth_status(&runner, &exe).expect("auth probe");

    for captured in runner.captured_requests() {
        assert!(
            !captured
                .args
                .iter()
                .any(|a| a == "--model" || a == "--effort"),
            "probe argv must stay free of model/effort: {:?}",
            captured.args
        );
    }
}

/// Regression test for a review-found vulnerability (PR #1, Codex review
/// finding 1, `docs/verification/llm-wikis-execution.md` Task 15 "review
/// loop iteration 1"), **corrected in review loop iteration 2** after a live
/// four-arm experiment showed the original fix's `--setting-sources user`
/// broke project-skill discovery itself (every `project_skill`-load-mode
/// wiki's slash entrypoint resolved to `"Unknown command: /<name>"` instead
/// of invoking the skill) — `--setting-sources` was **not** used at all,
/// only `--settings {"disableAllHooks":true}`, from iteration 2 until this
/// task.
///
/// **Updated for PRD 08-06-pre-0-1-0-cli-refinements, D6 (R-34)**:
/// `--setting-sources project` is now added back — the *opposite*
/// exclusion from the reverted `user` value (drops `user`/`local`, keeps
/// `project`), added for an unrelated reason (fixing a `SessionEnd` "Hook
/// cancelled" pollution source traced to a user-level plugin/hook) and
/// live-verified not to reproduce the iteration-2 regression: a project
/// skill still resolved and answered correctly with both
/// `--setting-sources project` and `--settings {"disableAllHooks":true}`
/// present together (research/provider-cli-flags.md §2, §3).
///
/// The threat `--settings {"disableAllHooks":true}` itself still defends
/// against: the child's cwd is the wiki's own `project_root`
/// (`src/providers/claude.rs::invoke`), so Claude Code's `-p` mode
/// auto-loads that untrusted directory's `.claude/settings.json` /
/// `settings.local.json` and **runs any hooks they declare** (SessionStart,
/// PreToolUse, ...) — arbitrary shell, entirely outside the `--tools
/// Read,Grep,Glob` gate, which restricts only built-in tools, not hook
/// commands. `.claude/`/`.agents/` immediately under `content_root` are also
/// excluded from the mutation snapshot (spec §12), so a hook's writes there
/// are undetectable. `--settings {"disableAllHooks":true}` disables every
/// hook declared by user, project, or local settings (CLI-supplied
/// `--settings` outranks all of those) — not an admin-managed/
/// enterprise-policy hook, which no flag here reaches — while leaving
/// project settings otherwise loaded, so skill discovery still works.
/// Empirically confirmed live (checkpoint,
/// iteration 2): with the corrected argv, a project skill resolved and
/// answered correctly *and* a project-declared `SessionStart` hook on the
/// same fixture did not fire; a control run with neither flag confirmed the
/// hook is genuinely live in that fixture.
#[test]
fn hook_neutralization_settings_flag_present_without_excluding_setting_sources() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);

    let schema_pos = args.iter().position(|a| a == "--json-schema").unwrap();
    assert_eq!(
        args[schema_pos + 2],
        OsString::from("--settings"),
        "--settings must directly follow the --json-schema pair, before any optional --plugin-dir"
    );
    assert_eq!(
        args[schema_pos + 3],
        OsString::from(r#"{"disableAllHooks":true}"#),
        "literal expected value, not the production constant (iteration 3 finding 2)"
    );
    // D6/R-34: `--setting-sources project` is now present (the opposite
    // exclusion from the reverted `user` value) — it must appear after the
    // `--settings` pair and before any optional `--plugin-dir`, same as
    // `--append-system-prompt`.
    let setting_sources_pos = args
        .iter()
        .position(|a| a == "--setting-sources")
        .expect("--setting-sources project must be present (D6/R-34)");
    assert_eq!(
        args[setting_sources_pos + 1],
        OsString::from(SETTING_SOURCES_PROJECT)
    );

    // Even with a local_plugin --plugin-dir, the hook-neutralization flag
    // still precedes it — a plugin-declared hook is neutralized too.
    let plugin_dir = Path::new("D:/Wikis/agents/plugins/knowledge-tools");
    let with_plugin = build_argv(
        content_root,
        mcp_config,
        &schema_text,
        Some(plugin_dir),
        None,
        None,
    );
    let settings_pos = with_plugin.iter().position(|a| a == "--settings").unwrap();
    let plugin_pos = with_plugin
        .iter()
        .position(|a| a == "--plugin-dir")
        .unwrap();
    let setting_sources_pos_with_plugin = with_plugin
        .iter()
        .position(|a| a == "--setting-sources")
        .unwrap();
    assert!(settings_pos < plugin_pos);
    assert!(setting_sources_pos_with_plugin < plugin_pos);
}

#[test]
fn plugin_dir_flag() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let without_plugin = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    assert!(!without_plugin.contains(&OsString::from("--plugin-dir")));

    let plugin_dir = Path::new("D:/Wikis/agents/plugins/knowledge-tools");
    let with_plugin = build_argv(
        content_root,
        mcp_config,
        &schema_text,
        Some(plugin_dir),
        None,
        None,
    );
    let pos = with_plugin
        .iter()
        .position(|a| a == "--plugin-dir")
        .expect("--plugin-dir must be present for local_plugin");
    assert_eq!(with_plugin[pos + 1], OsString::from(plugin_dir));
}

#[test]
fn tool_restriction() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let pos = args.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(args[pos + 1], OsString::from("Read,Grep,Glob"));
    assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
}

#[test]
fn no_session_persistence() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    assert!(args.iter().any(|a| a == "--no-session-persistence"));
}

#[test]
fn capability_exclusion() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let forbidden = [
        "--dangerously-skip-permissions",
        "--allow-dangerously-skip-permissions",
        "Write",
        "Edit",
        "Bash",
        "WebFetch",
        "WebSearch",
        "Task",
        "--resume",
        "--continue",
        "--permission-mode",
        "acceptEdits",
    ];
    for token in forbidden {
        if token == "--permission-mode" {
            // present, but only ever paired with "dontAsk" — checked below.
            continue;
        }
        assert!(
            !args.iter().any(|a| a == token),
            "argv must never contain forbidden capability token {token:?}"
        );
    }
    let pos = args.iter().position(|a| a == "--permission-mode").unwrap();
    assert_eq!(args[pos + 1], OsString::from("dontAsk"));
    // No command-execution tool exists in the tool set at all.
    let tools_pos = args.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(args[tools_pos + 1], OsString::from("Read,Grep,Glob"));
}

#[test]
fn json_schema_inline() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let pos = args.iter().position(|a| a == "--json-schema").unwrap();
    let value = &args[pos + 1];
    assert_eq!(value, &OsString::from(&schema_text));
    assert!(
        value.to_string_lossy().trim_start().starts_with('{'),
        "the --json-schema value must be inline JSON text, not a path"
    );
    assert!(
        !Path::new(value).exists(),
        "the --json-schema value must not resolve to a file on disk"
    );
}

/// Regression test: OpenAI's structured-output validator (codex-cli's
/// `--output-schema`) rejects any `properties` entry that lacks a `"type"`
/// key, even when a `const`/`enum` constrains it — confirmed live via a
/// captured-argv replay (`docs/verification/llm-wikis-execution.md` Task 15,
/// the LIVE-02 root cause: a 400 `invalid_json_schema` API error, "schema
/// must have a 'type' key", on `properties.contract`). Claude's `--json-schema`
/// tolerated the missing `type` (LIVE-01/03/09 all passed against it), which
/// is why this shared schema — used by both adapters — was never caught
/// until a real Codex live call hit OpenAI's stricter validator. Every
/// property must carry `"type"`, `contract`/`knowledge_status` included
/// alongside their existing `const`/`enum`.
#[test]
fn every_result_schema_property_has_a_type_key() {
    let schema_text = schema();
    let value: serde_json::Value =
        serde_json::from_str(&schema_text).expect("result_json_schema is valid JSON");
    let properties = value["properties"]
        .as_object()
        .expect("schema has a properties object");
    assert!(!properties.is_empty(), "schema must declare properties");
    for (name, prop) in properties {
        assert!(
            prop.get("type").is_some(),
            "property {name:?} is missing a \"type\" key: {prop}"
        );
    }
    assert_eq!(value["properties"]["contract"]["type"], "string");
    assert_eq!(value["properties"]["knowledge_status"]["type"], "string");
}

#[test]
fn no_prompt_in_argv() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None, None, None);
    let joined: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(!joined.iter().any(|a| a.contains("/wiki-query")));
    assert!(
        !joined
            .iter()
            .any(|a| a.contains("Use the wiki-query skill"))
    );
}

#[test]
fn invalid_native_output_exit() {
    let bytes = fixture("malformed_trailing_bytes.json");
    let err = parse_claude_output(&bytes).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);
    assert_eq!(err.code.exit_code(), 6);
}

// ---------------------------------------------------------------------------
// `invoke`/probe tests against a fake runner (plan Task 9 Step 7)
// ---------------------------------------------------------------------------

/// A dedicated scratch tree under the system temp directory, distinct from
/// `provider_request`'s own `temp_base` sibling — both are children of this
/// directory but never nested in each other, which is exactly what
/// `resolve_safe_temp_root`'s disjointness check requires.
fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join("llm-wikis-claude-adapter-tests");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn provider_request(executable: ResolvedExecutable) -> ProviderRequest {
    let temp_base = scratch_root().join("temp-base");
    fs::create_dir_all(&temp_base).unwrap();
    let project_root = scratch_root().join("project-root");
    fs::create_dir_all(&project_root).unwrap();
    ProviderRequest {
        executable,
        wiki_id: "agents".to_string(),
        project_root: project_root.clone(),
        content_root: project_root.clone(),
        entrypoint: "/wiki-query".to_string(),
        query_prompt: "Use the wiki-query skill to answer from this wiki.".to_string(),
        question: "What does this wiki cover?".to_string(),
        plugin_dir: None,
        model: None,
        effort: None,
        timeout: Duration::from_secs(30),
        max_stdout_bytes: 1_048_576,
        max_stderr_bytes: 65_536,
        temp_base,
        configured_roots: vec![project_root],
    }
}

#[test]
fn cwd_is_project_root() {
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(
        &fixture("success_structured.json"),
        b"",
        0,
    )));
    let adapter = ClaudeAdapter;
    let mut request = provider_request(fake_executable());
    let distinct_project_root = scratch_root().join("distinct-project-root");
    fs::create_dir_all(&distinct_project_root).unwrap();
    request.project_root = distinct_project_root.clone();
    request.content_root = distinct_project_root.clone();
    request.configured_roots = vec![distinct_project_root.clone()];
    adapter
        .invoke(&runner, request, "test prompt".to_string())
        .model_result
        .expect("invoke succeeds");
    let captured = runner.captured_requests();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].cwd, distinct_project_root);
}

#[test]
fn spawn_failure_after_authoritative_check_still_reports_enabled_plugins_declared() {
    // PR #1 Codex review iteration 8 finding 1: `no_child_outcome` unconditionally
    // sets `claude_enabled_plugins_declared: false` (correct for the three call
    // sites that run before the authoritative settings check), but the fourth
    // call site -- the `runner.run` spawn attempt itself -- runs *after* that
    // check has already determined the wiki's settings declare `enabledPlugins`.
    // Routing a spawn failure through `no_child_outcome` there silently reverted
    // an already-`true` value back to `false`, dropping the
    // `CLAUDE_ENABLED_PLUGINS_DECLARED` warning the operator was promised on
    // exactly the query that needed it (a failed one, worth investigating).
    let runner = FakeProcessRunner::new();
    runner.push_response(Err(AppError::new(
        ErrorCode::InternalError,
        "simulated spawn failure",
    )));
    let adapter = ClaudeAdapter;
    let mut request = provider_request(fake_executable());
    let project_root = scratch_root().join("spawn-failure-enabled-plugins-project-root");
    fs::create_dir_all(project_root.join(".claude")).unwrap();
    fs::write(
        project_root.join(".claude/settings.local.json"),
        br#"{"enabledPlugins":{"x@y":true}}"#,
    )
    .unwrap();
    request.project_root = project_root.clone();
    request.content_root = project_root.clone();
    request.configured_roots = vec![project_root];

    let outcome = adapter.invoke(&runner, request, "test prompt".to_string());

    assert!(
        outcome.model_result.is_err(),
        "expected the simulated spawn failure to surface as an error"
    );
    assert!(
        outcome.claude_enabled_plugins_declared,
        "the authoritative settings check already found enabledPlugins declared before \
         the spawn was attempted -- a subsequent spawn failure must not silently revert \
         that back to false"
    );
}

#[test]
fn no_session_persistence_via_invoke() {
    // Re-asserts the argv-level `no_session_persistence` test through a full
    // `invoke` call, proving the flag survives end to end.
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(
        &fixture("success_structured.json"),
        b"",
        0,
    )));
    let adapter = ClaudeAdapter;
    adapter
        .invoke(
            &runner,
            provider_request(fake_executable()),
            "test prompt".to_string(),
        )
        .model_result
        .expect("invoke succeeds");
    let captured = runner.captured_requests();
    let args: Vec<String> = captured[0]
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(args.iter().any(|a| a == "--no-session-persistence"));
}

#[test]
fn auth_status_probe() {
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(br#"{"authenticated":true}"#, b"", 0)));
    let adapter = ClaudeAdapter;
    adapter
        .auth_status(&runner, &fake_executable())
        .expect("authenticated fixture succeeds");
    let captured = runner.captured_requests();
    let args: Vec<String> = captured[0]
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert_eq!(args, vec!["auth", "status", "--json"]);

    // Verified logged-out.
    runner.push_response(Ok(completed_outcome(br#"{"authenticated":false}"#, b"", 0)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthRequired);

    // Malformed status output.
    runner.push_response(Ok(completed_outcome(b"not json at all", b"", 0)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);

    // Ordinary process failure (nonzero exit, unrelated to auth state).
    runner.push_response(Ok(completed_outcome(b"", b"boom", 1)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NonzeroExit);
}

/// Regression test: real Claude CLI 2.1.220's `claude auth status --json`
/// emits `loggedIn` (verified live, `docs/verification/llm-wikis-execution.md`
/// Task 15), not the `authenticated` field the parser originally required
/// exclusively. Both field shapes must be accepted so the check does not fail
/// closed against a real, current CLI purely on field-name drift, without
/// dropping support for the `authenticated` shape older fixtures/tests use.
#[test]
fn auth_status_probe_accepts_the_real_logged_in_field_shape() {
    let runner = FakeProcessRunner::new();
    let adapter = ClaudeAdapter;

    runner.push_response(Ok(completed_outcome(br#"{"loggedIn":true}"#, b"", 0)));
    adapter
        .auth_status(&runner, &fake_executable())
        .expect("loggedIn:true fixture succeeds");

    runner.push_response(Ok(completed_outcome(br#"{"loggedIn":false}"#, b"", 0)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthRequired);

    // Neither field present: still INVALID_NATIVE_OUTPUT, not a silent pass.
    runner.push_response(Ok(completed_outcome(br#"{"other":true}"#, b"", 0)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);
}
