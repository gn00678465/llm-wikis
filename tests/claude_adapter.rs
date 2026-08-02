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

use llm_wikis::error::ErrorCode;
use llm_wikis::output::RawFormat;
use llm_wikis::process::{ExecutableKind, ResolvedExecutable};
use llm_wikis::providers::claude::{
    CLAUDE_READ_SCOPE_BROAD_MESSAGE, ClaudeAdapter, build_argv, parse_claude_output,
    read_scope_broad_warning,
};
use llm_wikis::providers::{
    FakeProcessRunner, ProviderAdapter, ProviderRequest, completed_outcome,
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
    let args = build_argv(content_root, mcp_config, &schema_text, None);
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
    ];
    assert_eq!(args, expected);
}

#[test]
fn plugin_dir_flag() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let without_plugin = build_argv(content_root, mcp_config, &schema_text, None);
    assert!(!without_plugin.contains(&OsString::from("--plugin-dir")));

    let plugin_dir = Path::new("D:/Wikis/agents/plugins/knowledge-tools");
    let with_plugin = build_argv(content_root, mcp_config, &schema_text, Some(plugin_dir));
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
    let args = build_argv(content_root, mcp_config, &schema_text, None);
    let pos = args.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(args[pos + 1], OsString::from("Read,Grep,Glob"));
    assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
}

#[test]
fn no_session_persistence() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None);
    assert!(args.iter().any(|a| a == "--no-session-persistence"));
}

#[test]
fn capability_exclusion() {
    let content_root = Path::new("D:/Wikis/agents");
    let mcp_config = Path::new("D:/Temp/llm-wikis-xyz/mcp-config.json");
    let schema_text = schema();
    let args = build_argv(content_root, mcp_config, &schema_text, None);
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
    let args = build_argv(content_root, mcp_config, &schema_text, None);
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
    let args = build_argv(content_root, mcp_config, &schema_text, None);
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
