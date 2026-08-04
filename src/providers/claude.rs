//! Claude adapter: argv, native-JSON parser, version/auth probes, and the
//! conditional read-scope warning (spec §10.2).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{AppError, ErrorCode};
use crate::output::{Agent, RawFormat, Warning, WrapperWarningCode};
use crate::process::{ProcessRequest, ResolvedExecutable};

use super::{
    AuthStatus, InvokeOutcome, ProcessRunner, ProviderAdapter, ProviderRequest, cap_diagnostic,
    map_nonzero_exit, map_termination, no_child_outcome, result_json_schema,
};

/// The fixed, minimum tool set (spec §10.2/§7.2): no Write, Edit, Bash, web,
/// MCP, or subagent tool exists in this argv at all.
pub const TOOLS: &str = "Read,Grep,Glob";

/// The exact `CLAUDE_READ_SCOPE_BROAD` message text (spec §10.2), emitted
/// only when `content_root` is a strict subdirectory of `project_root`.
pub const CLAUDE_READ_SCOPE_BROAD_MESSAGE: &str = "Claude read tools can inspect the configured project root, not only the selected content root; use an OS sandbox or container for stricter confidentiality.";

/// Emits `CLAUDE_READ_SCOPE_BROAD` exactly when `content_root` is a *strict*
/// subdirectory of `project_root` (spec §10.2: "When the two roots are equal
/// the warning is not emitted, because read reach is exactly `content_root`
/// and the message would be false"). `Path::starts_with` treats a path as its
/// own prefix, so the equality check is required in addition to it.
pub fn read_scope_broad_warning(project_root: &Path, content_root: &Path) -> Option<Warning> {
    if content_root != project_root && content_root.starts_with(project_root) {
        Some(Warning::wrapper(
            WrapperWarningCode::ClaudeReadScopeBroad,
            CLAUDE_READ_SCOPE_BROAD_MESSAGE,
        ))
    } else {
        None
    }
}

/// The `--settings` value that neutralizes every hook, from every source
/// (user/project/local settings and any `--plugin-dir`), for this session
/// (spec §10.2 R-27; PR #1 Codex review finding 1). CLI-supplied `--settings`
/// outranks user/project/local settings, so this holds even for the
/// operator's own trusted `~/.claude` settings or a configured local plugin.
pub const DISABLE_ALL_HOOKS_SETTINGS: &str = "{\"disableAllHooks\":true}";

/// Builds the exact Claude argv vector (spec §10.2). Never includes the
/// entrypoint or `query_prompt` — those exist only in the stdin prompt.
///
/// **Threat this argv defends against (spec §10.2 R-27, PR #1 Codex review
/// finding 1)**: `invoke`'s child cwd is the wiki's own `project_root`, an
/// untrusted operator-controlled directory this wrapper does not own. Claude
/// Code 2.1.220's `-p` mode auto-loads that directory's `.claude/settings.json`
/// / `settings.local.json` and **runs any hooks they declare**
/// (SessionStart, PreToolUse, ...) as arbitrary shell — entirely outside the
/// `--tools Read,Grep,Glob` gate, which restricts only built-in tools, not
/// hook commands. `.claude/`/`.agents/` immediately under `content_root` are
/// also excluded from the mutation snapshot (spec §12), so a hook's writes
/// there would be undetectable. `--setting-sources user` means only the
/// operator's own `~/.claude` settings are read at all — the untrusted
/// wiki-side project/local settings files are never loaded. `--settings`
/// with [`DISABLE_ALL_HOOKS_SETTINGS`] is defense in depth on top of that.
/// Confirmed live: the argv without these two flags let a wiki-side
/// SessionStart hook execute; the argv with them did not, and the query
/// still succeeded normally (`docs/verification/llm-wikis-execution.md`
/// Task 15, "review loop iteration 1"). **Honest residual gap**: no CLI flag
/// disables an admin-managed/enterprise-policy hook — out of scope, and this
/// project verified no managed settings exist on its own implementation
/// machine.
pub fn build_argv(
    content_root: &Path,
    mcp_config_path: &Path,
    json_schema: &str,
    plugin_dir: Option<&Path>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--add-dir"),
        OsString::from(content_root),
        OsString::from("-p"),
        OsString::from("--input-format"),
        OsString::from("text"),
        OsString::from("--no-session-persistence"),
        OsString::from("--permission-mode"),
        OsString::from("dontAsk"),
        OsString::from("--tools"),
        OsString::from(TOOLS),
        OsString::from("--strict-mcp-config"),
        OsString::from("--mcp-config"),
        OsString::from(mcp_config_path),
        OsString::from("--output-format"),
        OsString::from("json"),
        OsString::from("--json-schema"),
        OsString::from(json_schema),
        OsString::from("--setting-sources"),
        OsString::from("user"),
        OsString::from("--settings"),
        OsString::from(DISABLE_ALL_HOOKS_SETTINGS),
    ];
    if let Some(dir) = plugin_dir {
        args.push(OsString::from("--plugin-dir"));
        args.push(OsString::from(dir));
    }
    args
}

/// The empty MCP configuration file content Claude's `--strict-mcp-config
/// --mcp-config <this>` reads (spec §10.1: "Generated empty MCP configuration
/// ... live in a fresh system temporary directory").
pub const EMPTY_MCP_CONFIG: &[u8] = b"{\"mcpServers\":{}}";

fn invalid_native(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidNativeOutput, message)
}

/// Claude's `--output-format json` document shape (subset used by this
/// wrapper): `is_error`/`subtype` gate success, `structured_output` is
/// preferred over the free-text `result` field (spec §10.2, §7.3).
#[derive(Debug, Deserialize)]
struct ClaudeNativeDocument {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
}

/// Parses Claude's stdout as exactly one native JSON document (spec §10.2,
/// plan Task 9 Step 4/6). Returns the validated `wiki-query/v1` result
/// together with [`RawFormat::ClaudeJson`] — the pairing is the mechanism by
/// which `raw_format` "becomes `claude-json` only after a native document
/// parses" (checklist OFF-115): any `Err` return means it never became
/// non-null.
pub fn parse_claude_output(
    stdout: &[u8],
) -> Result<(crate::model::ModelResult, RawFormat), AppError> {
    let text = std::str::from_utf8(stdout)
        .map_err(|e| invalid_native(format!("claude stdout was not valid UTF-8: {e}")))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(invalid_native("claude produced no stdout"));
    }
    let mut de = serde_json::Deserializer::from_str(trimmed);
    let doc: ClaudeNativeDocument = serde::Deserialize::deserialize(&mut de)
        .map_err(|e| invalid_native(format!("malformed claude native JSON: {e}")))?;
    de.end()
        .map_err(|_| invalid_native("trailing bytes after the claude native JSON document"))?;

    if doc.is_error || doc.subtype.as_deref() != Some("success") {
        return Err(invalid_native(
            "claude native output reported a non-success result (is_error or non-success subtype)",
        ));
    }

    let model_json = if let Some(structured) = doc.structured_output {
        structured
    } else if let Some(result_text) = &doc.result {
        serde_json::from_str(result_text).map_err(|e| {
            invalid_native(format!(
                "claude result text was not valid wiki-query/v1 JSON: {e}"
            ))
        })?
    } else {
        return Err(invalid_native(
            "claude native output carried neither structured_output nor a result field",
        ));
    };

    let result: crate::model::ModelResult = serde_json::from_value(model_json)
        .map_err(|e| invalid_native(format!("claude structured output was malformed: {e}")))?;
    result
        .validate()
        .map_err(|e| AppError::new(e.error_code(), e.to_string()))?;
    Ok((result, RawFormat::ClaudeJson))
}

/// The real Claude CLI's `auth status --json` field name has drifted at
/// least once in the field (2.1.220 emits `loggedIn`; older/other
/// documentation names it `authenticated`) — both are accepted so this check
/// does not fail closed purely on field-name drift across CLI versions.
#[derive(Debug, Deserialize)]
struct ClaudeAuthStatusDocument {
    #[serde(default)]
    authenticated: Option<bool>,
    #[serde(default, rename = "loggedIn")]
    logged_in: Option<bool>,
}

impl ClaudeAuthStatusDocument {
    fn is_authenticated(&self) -> Option<bool> {
        self.authenticated.or(self.logged_in)
    }
}

/// The Claude provider adapter (spec §10.2).
pub struct ClaudeAdapter;

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

impl ProviderAdapter for ClaudeAdapter {
    fn name(&self) -> Agent {
        Agent::Claude
    }

    /// Runs `claude --version` (spec §8.1 step 7: "bounded version...probe";
    /// Task 2 Row 3 confirmed this exact flag).
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

    /// Runs `claude auth status --json` (spec §10.1, Task 2 Row 3: "the exact
    /// command named in the plan ... exists and works"). A recognized
    /// logged-out state (`authenticated: false`) is `AUTH_REQUIRED`;
    /// anything that fails to parse as the expected shape is
    /// `INVALID_NATIVE_OUTPUT`; a non-zero exit unrelated to that shape is an
    /// ordinary `NONZERO_EXIT` (spec §10.1: "unless the verified provider
    /// uses nonzero specifically for logged-out state" — Claude does not).
    fn auth_status(
        &self,
        runner: &dyn ProcessRunner,
        executable: &ResolvedExecutable,
    ) -> Result<AuthStatus, AppError> {
        let outcome = runner.run(probe_request(
            executable,
            vec![
                OsString::from("auth"),
                OsString::from("status"),
                OsString::from("--json"),
            ],
        ))?;
        map_termination(&outcome, 65_536, 65_536)?;
        if outcome.exit_code != Some(0) {
            return Err(AppError::new(
                ErrorCode::NonzeroExit,
                format!(
                    "claude auth status exited {:?}; stderr: {}",
                    outcome.exit_code,
                    cap_diagnostic(&outcome.stderr)
                ),
            ));
        }
        let text = std::str::from_utf8(&outcome.stdout).map_err(|_| {
            AppError::new(
                ErrorCode::InvalidNativeOutput,
                "claude auth status output was not valid UTF-8",
            )
        })?;
        let parsed: ClaudeAuthStatusDocument = serde_json::from_str(text.trim()).map_err(|_| {
            AppError::new(
                ErrorCode::InvalidNativeOutput,
                "claude auth status output was not valid JSON",
            )
        })?;
        match parsed.is_authenticated() {
            Some(true) => Ok(AuthStatus::Authenticated),
            Some(false) => Err(AppError::new(
                ErrorCode::AuthRequired,
                "claude auth status reports a logged-out state",
            )),
            None => Err(AppError::new(
                ErrorCode::InvalidNativeOutput,
                "claude auth status output was missing the authenticated field",
            )),
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
        let mcp_config = match crate::process::TempArtifact::create(
            &temp_dir,
            "mcp-config.json",
            EMPTY_MCP_CONFIG,
        ) {
            Ok(a) => a,
            Err(e) => return no_child_outcome(e),
        };
        let schema = result_json_schema();
        let args = build_argv(
            &request.content_root,
            mcp_config.path(),
            &schema,
            request.plugin_dir.as_deref(),
        );
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
        match parse_claude_output(&outcome.stdout) {
            Ok((result, raw_format)) => InvokeOutcome {
                model_result: Ok(result),
                child_exit_code,
                raw_format: Some(raw_format),
                diagnostics: Vec::new(),
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
