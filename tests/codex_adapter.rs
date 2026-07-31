//! Codex adapter tests (spec §10.3; plan Task 9 Steps 5, 6, 7).
//!
//! Argv/prompt assertions never spawn a process. Parser assertions run
//! against sanitized fixture bytes under `tests/fixtures/codex/`. `invoke`
//! assertions run against [`FakeProcessRunner`] — no real Codex process ever
//! starts in this file.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use llm_wikis::error::ErrorCode;
use llm_wikis::output::RawFormat;
use llm_wikis::process::{ExecutableKind, ResolvedExecutable};
use llm_wikis::providers::codex::{
    CODEX_READ_SCOPE_BROAD_MESSAGE, CodexAdapter, McpToolCallSignal, build_argv,
    parse_codex_output, read_scope_broad_warning,
};
use llm_wikis::providers::{
    FakeProcessRunner, ProviderAdapter, ProviderRequest, completed_outcome,
};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/codex")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"))
}

fn fake_executable() -> ResolvedExecutable {
    ResolvedExecutable {
        path: PathBuf::from("codex"),
        kind: ExecutableKind::Native,
    }
}

// ---------------------------------------------------------------------------
// Parser tests (plan Task 9 Step 6)
// ---------------------------------------------------------------------------

#[test]
fn jsonl_line_by_line() {
    let good = fixture("success.jsonl");
    assert!(parse_codex_output(&good).is_ok());

    let bad = fixture("malformed_line.jsonl");
    let err = parse_codex_output(&bad).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);
}

#[test]
fn failed_turn_rejected() {
    let bytes = fixture("failed_turn.jsonl");
    assert!(
        parse_codex_output(&bytes).is_err(),
        "a failed-turn event must never surface as success"
    );
}

#[test]
fn last_message_selected() {
    let bytes = fixture("multiple_messages.jsonl");
    let outcome = parse_codex_output(&bytes).expect("valid fixture parses");
    assert_eq!(outcome.result.answer, "final answer citing [[overview]]");
}

#[test]
fn no_final_message() {
    let bytes = fixture("no_final_message.jsonl");
    let err = parse_codex_output(&bytes).unwrap_err();
    assert_eq!(err.code, ErrorCode::NoFinalMessage);
    assert_eq!(err.code.exit_code(), 6);
}

#[test]
fn final_message_contract() {
    let bytes = fixture("final_message_schema_violation.jsonl");
    let err = parse_codex_output(&bytes).unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);
}

#[test]
fn raw_format_gate() {
    let good = fixture("success.jsonl");
    let outcome = parse_codex_output(&good).expect("valid fixture parses");
    assert_eq!(outcome.raw_format, RawFormat::CodexJsonl);

    let bad = fixture("malformed_line.jsonl");
    assert!(
        parse_codex_output(&bad).is_err(),
        "a parse failure must never produce a raw_format value"
    );
}

#[test]
fn diagnostics_cap() {
    let runner = FakeProcessRunner::new();
    let huge_stderr = vec![b'e'; 50_000];
    runner.push_response(Ok(completed_outcome(b"not used", &huge_stderr, 1)));
    let adapter = CodexAdapter;
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
fn read_scope_broad_always() {
    let warning = read_scope_broad_warning();
    assert_eq!(warning.message, CODEX_READ_SCOPE_BROAD_MESSAGE);
}

#[test]
fn item_type_discipline() {
    let benign = fixture("success_benign_introspection.jsonl");
    let outcome = parse_codex_output(&benign).expect("valid fixture parses");
    assert_eq!(
        outcome.mcp_signals,
        vec![McpToolCallSignal::BenignIntrospection]
    );

    let forbidden = fixture("forbidden_mcp_server.jsonl");
    let outcome = parse_codex_output(&forbidden)
        .expect("valid fixture parses (signal is diagnostic, not fatal)");
    assert_eq!(
        outcome.mcp_signals,
        vec![McpToolCallSignal::ForbiddenCapability]
    );
}

#[test]
fn invalid_native_output_exit() {
    let bytes = fixture("malformed_line.jsonl");
    let err = parse_codex_output(&bytes).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidNativeOutput);
    assert_eq!(err.code.exit_code(), 6);
}

// ---------------------------------------------------------------------------
// Argv tests (plan Task 9 Step 5)
// ---------------------------------------------------------------------------

#[test]
fn exact_argv() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    let expected: Vec<OsString> = vec![
        OsString::from("--ask-for-approval"),
        OsString::from("never"),
        OsString::from("exec"),
        OsString::from("-C"),
        OsString::from(project_root),
        OsString::from("--sandbox"),
        OsString::from("read-only"),
        OsString::from("--ephemeral"),
        OsString::from("--skip-git-repo-check"),
        OsString::from("--ignore-user-config"),
        OsString::from("-c"),
        OsString::from("mcp_servers={}"),
        OsString::from("--disable"),
        OsString::from("browser_use"),
        OsString::from("--disable"),
        OsString::from("computer_use"),
        OsString::from("--output-schema"),
        OsString::from(schema_path),
        OsString::from("--json"),
        OsString::from("-"),
    ];
    assert_eq!(args, expected);
}

#[test]
fn flag_order() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    let approval_pos = args.iter().position(|a| a == "--ask-for-approval").unwrap();
    let exec_pos = args.iter().position(|a| a == "exec").unwrap();
    assert!(
        approval_pos < exec_pos,
        "--ask-for-approval must precede exec as a top-level option"
    );
}

#[test]
fn no_add_dir() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    assert!(!args.iter().any(|a| a == "--add-dir"));
}

#[test]
fn sandbox_flag() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    let pos = args.iter().position(|a| a == "--sandbox").unwrap();
    assert_eq!(args[pos + 1], OsString::from("read-only"));
}

#[test]
fn no_session_persistence() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    assert!(args.iter().any(|a| a == "--ephemeral"));
}

#[test]
fn capability_exclusion() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    // MCP exclusion is explicit (spec §10.1 R-25): both the zeroed table and
    // the two built-in-server disable flags must be present.
    assert!(
        args.windows(2)
            .any(|w| w[0] == "-c" && w[1] == "mcp_servers={}")
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--disable" && w[1] == "browser_use")
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--disable" && w[1] == "computer_use")
    );
    // No blanket write/full-access sandbox substitution.
    assert!(
        !args
            .iter()
            .any(|a| a == "workspace-write" || a == "danger-full-access")
    );
    assert!(
        !args
            .iter()
            .any(|a| a == "--dangerously-bypass-approvals-and-sandbox")
    );
    let sandbox_pos = args.iter().position(|a| a == "--sandbox").unwrap();
    assert_eq!(args[sandbox_pos + 1], OsString::from("read-only"));
}

#[test]
fn no_prompt_in_argv() {
    let project_root = Path::new("D:/Wikis/agents");
    let schema_path = Path::new("D:/Temp/llm-wikis-xyz/output-schema.json");
    let args = build_argv(project_root, schema_path);
    let joined: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert!(!joined.iter().any(|a| a.contains("$wiki-query")));
    assert!(
        !joined
            .iter()
            .any(|a| a.contains("Use the wiki-query skill"))
    );
}

#[test]
fn installed_plugin_fails_closed() {
    // Codex `$plugin-name:skill-name` entrypoints are rejected at
    // configuration time (spec §6.3; `llm_wikis::config::validate_entrypoint`),
    // and this adapter's argv/prompt builders take only an already-validated
    // entrypoint string — there is no code path here that could invoke an
    // installed Codex plugin, since `--ignore-user-config` means user-level
    // plugin configuration is never consulted at all.
    let outcome = llm_wikis::config::validate_entrypoint(
        llm_wikis::output::Agent::Codex,
        "$knowledge-tools:ask-wiki",
    );
    assert!(
        outcome.is_err(),
        "a configured Codex plugin entrypoint must fail closed before any provider invocation"
    );
}

// ---------------------------------------------------------------------------
// `invoke`/probe tests against a fake runner (plan Task 9 Step 7)
// ---------------------------------------------------------------------------

fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join("llm-wikis-codex-adapter-tests");
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
        entrypoint: "$wiki-query".to_string(),
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
fn auth_status_probe() {
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(b"Logged in using an API key", b"", 0)));
    let adapter = CodexAdapter;
    adapter
        .auth_status(&runner, &fake_executable())
        .expect("authenticated fixture succeeds");
    let captured = runner.captured_requests();
    let args: Vec<String> = captured[0]
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert_eq!(args, vec!["login", "status"]);

    // Verified logged-out: Codex uses a non-zero exit specifically for this.
    runner.push_response(Ok(completed_outcome(b"Not logged in", b"", 1)));
    let err = adapter
        .auth_status(&runner, &fake_executable())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthRequired);

    // Malformed status output (exit 0, unrecognized text).
    runner.push_response(Ok(completed_outcome(b"???", b"", 0)));
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

#[test]
fn invoke_end_to_end_against_fake_runner() {
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(&fixture("success.jsonl"), b"", 0)));
    let adapter = CodexAdapter;
    let result = adapter
        .invoke(
            &runner,
            provider_request(fake_executable()),
            "test prompt".to_string(),
        )
        .model_result
        .expect("invoke succeeds");
    assert_eq!(
        result.answer,
        "Synthetic grounded answer citing [[overview]]."
    );
    let captured = runner.captured_requests();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].cwd,
        provider_request(fake_executable()).project_root
    );
}
