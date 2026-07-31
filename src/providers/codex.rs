//! Codex adapter: argv, incremental JSONL parser, version/auth probes, and
//! the unconditional read-scope warning (spec §10.3).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{AppError, ErrorCode};
use crate::model::ModelResult;
use crate::output::{Agent, RawFormat, Warning, WrapperWarningCode};
use crate::process::{ProcessRequest, ResolvedExecutable};

use super::{
    AuthStatus, InvokeOutcome, ProcessRunner, ProviderAdapter, ProviderRequest, cap_diagnostic,
    map_nonzero_exit, map_termination, no_child_outcome, result_json_schema,
};

/// The exact, unconditional `CODEX_READ_SCOPE_BROAD` message text (spec
/// §10.3). Unlike Claude's warning this is never conditional — the read-only
/// sandbox never limits reads to the selected wiki, regardless of roots.
pub const CODEX_READ_SCOPE_BROAD_MESSAGE: &str = "Codex read-only sandbox prevents writes but does not limit reads to the selected wiki; use an OS sandbox or container for strict confidentiality.";

/// Emitted unconditionally on every Codex query and static doctor check
/// (spec §10.3).
pub fn read_scope_broad_warning() -> Warning {
    Warning::wrapper(
        WrapperWarningCode::CodexReadScopeBroad,
        CODEX_READ_SCOPE_BROAD_MESSAGE,
    )
}

/// Builds the exact Codex argv vector (spec §10.3). Never includes
/// `--add-dir` (that flag grants an additional *writable* root) and never
/// includes the entrypoint or `query_prompt` — those exist only in the
/// stdin prompt.
pub fn build_argv(project_root: &Path, output_schema_path: &Path) -> Vec<OsString> {
    vec![
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
        OsString::from(output_schema_path),
        OsString::from("--json"),
        OsString::from("-"),
    ]
}

fn invalid_native(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidNativeOutput, message)
}

/// Whether an observed `mcp_tool_call` item is Codex's own benign,
/// undisableable internal introspection surface, or a forbidden-capability
/// signal (spec §10.1/§10.3, R-26; checklist OFF-233).
///
/// ponytail: classification keys only on `server` (the field that actually
/// carries "external reach or not" per the spec's own reasoning — Row 11
/// addendum 2 found `server: "codex"` is the identifying fact, not the tool
/// name). Add a `tool` allowlist check only if a `server: "codex"` call with
/// a non-introspection tool is ever observed in practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolCallSignal {
    BenignIntrospection,
    ForbiddenCapability,
}

fn classify_mcp_tool_call(server: Option<&str>) -> McpToolCallSignal {
    match server {
        Some("codex") => McpToolCallSignal::BenignIntrospection,
        _ => McpToolCallSignal::ForbiddenCapability,
    }
}

/// One JSONL event's top-level `type` (spec §10.3). Reads only the event's
/// own `type` field, plus — for `item.started`/`item.completed` — that
/// event's immediate `item.type` field. Never a recursive string scan
/// (plan Task 9 Step 6 / checklist OFF-233's "item-type discipline").
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RawEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted {},
    #[serde(rename = "turn.started")]
    TurnStarted {},
    #[serde(rename = "turn.completed")]
    TurnCompleted {},
    #[serde(rename = "turn.failed")]
    TurnFailed {},
    #[serde(rename = "error")]
    Error {},
    #[serde(rename = "item.started")]
    ItemStarted {},
    #[serde(rename = "item.completed")]
    ItemCompleted { item: RawItem },
    /// Any other top-level event type this wrapper does not act on
    /// (forward-compatible, never a parse failure by itself).
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RawItem {
    #[serde(rename = "agent_message")]
    AgentMessage {
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename = "mcp_tool_call")]
    McpToolCall {
        #[serde(default)]
        server: Option<String>,
    },
    /// `command_execution` and any other item type: not acted on beyond
    /// existing (spec §10.3's read-only sandbox governs these, not this
    /// parser).
    #[serde(other)]
    Other,
}

/// The result of successfully parsing a Codex JSONL stream: the validated
/// `wiki-query/v1` result, [`RawFormat::CodexJsonl`] (paired the same way as
/// the Claude adapter's parse function, so `raw_format` only ever becomes
/// non-null after this parse succeeds), and every `mcp_tool_call` signal
/// observed along the way (bounded — one entry per such item in the stream).
#[derive(Debug)]
pub struct CodexParseOutcome {
    pub result: ModelResult,
    pub raw_format: RawFormat,
    pub mcp_signals: Vec<McpToolCallSignal>,
}

/// Parses every non-empty stdout line as JSON (spec §10.3, plan Task 9 Step
/// 6): rejects error/failed-turn events, selects the last completed agent
/// message, and validates it against `wiki-query/v1`.
pub fn parse_codex_output(stdout: &[u8]) -> Result<CodexParseOutcome, AppError> {
    let text = std::str::from_utf8(stdout)
        .map_err(|e| invalid_native(format!("codex stdout was not valid UTF-8: {e}")))?;

    let mut last_agent_message: Option<String> = None;
    let mut mcp_signals = Vec::new();
    let mut saw_failure = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: RawEvent = serde_json::from_str(line)
            .map_err(|e| invalid_native(format!("malformed codex JSONL event: {e}")))?;
        match event {
            RawEvent::TurnFailed {} | RawEvent::Error {} => saw_failure = true,
            RawEvent::ItemCompleted { item } => match item {
                RawItem::AgentMessage { text } => {
                    if let Some(t) = text {
                        last_agent_message = Some(t);
                    }
                }
                RawItem::McpToolCall { server } => {
                    mcp_signals.push(classify_mcp_tool_call(server.as_deref()));
                }
                RawItem::Other => {}
            },
            RawEvent::ItemStarted {}
            | RawEvent::ThreadStarted {}
            | RawEvent::TurnStarted {}
            | RawEvent::TurnCompleted {}
            | RawEvent::Other => {}
        }
    }

    if saw_failure {
        return Err(invalid_native(
            "codex JSONL stream contained an error or failed-turn event",
        ));
    }

    let final_message = last_agent_message.ok_or_else(|| {
        AppError::new(
            ErrorCode::NoFinalMessage,
            "codex produced no completed agent message",
        )
    })?;

    let result = ModelResult::from_json(&final_message)
        .map_err(|e| AppError::new(e.error_code(), e.to_string()))?;

    Ok(CodexParseOutcome {
        result,
        raw_format: RawFormat::CodexJsonl,
        mcp_signals,
    })
}

/// Recognizes Codex's `login status` plain-text output (spec §10.1). Unlike
/// Claude, Codex is the provider that "uses nonzero specifically for
/// logged-out state" (spec §10.1's carve-out), so a recognized marker wins
/// over the exit code.
fn classify_login_status(stdout: &str) -> Option<bool> {
    let lower = stdout.to_lowercase();
    if lower.contains("not logged in") || lower.contains("not authenticated") {
        Some(false)
    } else if lower.contains("logged in") || lower.contains("authenticated") {
        Some(true)
    } else {
        None
    }
}

/// The Codex provider adapter (spec §10.3).
pub struct CodexAdapter;

fn probe_request(executable: &ResolvedExecutable, args: Vec<OsString>) -> ProcessRequest {
    ProcessRequest {
        executable: executable.clone(),
        args,
        cwd: std::env::temp_dir(),
        stdin: Vec::new(),
        timeout: Duration::from_secs(15),
        max_stdout_bytes: 65_536,
        max_stderr_bytes: 65_536,
        cancel: None,
    }
}

impl ProviderAdapter for CodexAdapter {
    fn name(&self) -> Agent {
        Agent::Codex
    }

    /// Runs `codex --version` (Task 2 Row 3 confirmed this exact flag).
    fn version(
        &self,
        runner: &dyn ProcessRunner,
        executable: &ResolvedExecutable,
    ) -> Result<String, AppError> {
        let outcome = runner.run(probe_request(executable, vec![OsString::from("--version")]))?;
        map_termination(&outcome, 65_536, 65_536)?;
        map_nonzero_exit(&outcome)?;
        Ok(String::from_utf8_lossy(&outcome.stdout).trim().to_string())
    }

    /// Runs `codex login status` (spec §10.1, Task 2 Row 3: "the exact
    /// command named in the plan ... exists and works").
    fn auth_status(
        &self,
        runner: &dyn ProcessRunner,
        executable: &ResolvedExecutable,
    ) -> Result<AuthStatus, AppError> {
        let outcome = runner.run(probe_request(
            executable,
            vec![OsString::from("login"), OsString::from("status")],
        ))?;
        map_termination(&outcome, 65_536, 65_536)?;
        let stdout = String::from_utf8_lossy(&outcome.stdout);
        match classify_login_status(&stdout) {
            Some(false) => Err(AppError::new(
                ErrorCode::AuthRequired,
                "codex login status reports a logged-out state",
            )),
            Some(true) => {
                if outcome.exit_code == Some(0) {
                    Ok(AuthStatus::Authenticated)
                } else {
                    Err(AppError::new(
                        ErrorCode::NonzeroExit,
                        format!(
                            "codex login status exited {:?} despite reporting logged in; stderr: {}",
                            outcome.exit_code,
                            cap_diagnostic(&outcome.stderr)
                        ),
                    ))
                }
            }
            None => {
                if outcome.exit_code != Some(0) {
                    Err(AppError::new(
                        ErrorCode::NonzeroExit,
                        format!(
                            "codex login status exited {:?}; stderr: {}",
                            outcome.exit_code,
                            cap_diagnostic(&outcome.stderr)
                        ),
                    ))
                } else {
                    Err(AppError::new(
                        ErrorCode::InvalidNativeOutput,
                        "codex login status output was not recognized",
                    ))
                }
            }
        }
    }

    fn invoke(
        &self,
        runner: &dyn ProcessRunner,
        request: ProviderRequest,
        prompt: String,
    ) -> InvokeOutcome {
        let temp_dir = match crate::process::resolve_safe_temp_root(
            &request.temp_base,
            &request.configured_roots,
        ) {
            Ok(d) => d,
            Err(e) => return no_child_outcome(e),
        };
        let schema_json = result_json_schema();
        let schema_artifact = match crate::process::TempArtifact::create(
            &temp_dir,
            "output-schema.json",
            schema_json.as_bytes(),
        ) {
            Ok(a) => a,
            Err(e) => return no_child_outcome(e),
        };
        let args = build_argv(&request.project_root, schema_artifact.path());
        let outcome = match runner.run(ProcessRequest {
            executable: request.executable.clone(),
            args,
            cwd: request.project_root.clone(),
            stdin: prompt.into_bytes(),
            timeout: request.timeout,
            max_stdout_bytes: request.max_stdout_bytes,
            max_stderr_bytes: request.max_stderr_bytes,
            cancel: None,
        }) {
            Ok(o) => o,
            Err(e) => return no_child_outcome(e),
        };
        let child_exit_code = outcome.exit_code;
        if let Err(e) = map_termination(
            &outcome,
            request.max_stdout_bytes as u64,
            request.max_stderr_bytes as u64,
        ) {
            return InvokeOutcome {
                model_result: Err(e),
                child_exit_code,
                raw_format: None,
                diagnostics: Vec::new(),
            };
        }
        if let Err(e) = map_nonzero_exit(&outcome) {
            return InvokeOutcome {
                model_result: Err(e),
                child_exit_code,
                raw_format: None,
                diagnostics: Vec::new(),
            };
        }
        match parse_codex_output(&outcome.stdout) {
            Ok(parsed) => InvokeOutcome {
                model_result: Ok(parsed.result),
                child_exit_code,
                raw_format: Some(parsed.raw_format),
                diagnostics: parsed
                    .mcp_signals
                    .iter()
                    .map(|signal| format!("{signal:?}"))
                    .collect(),
            },
            Err(e) => InvokeOutcome {
                model_result: Err(e),
                child_exit_code,
                raw_format: None,
                diagnostics: Vec::new(),
            },
        }
    }
}
