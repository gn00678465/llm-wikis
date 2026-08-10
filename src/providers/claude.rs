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
    AuthStatus, InvokeOutcome, NON_INTERACTIVE_SYSTEM_DIRECTIVES, ProcessRunner, ProviderAdapter,
    ProviderRequest, cap_diagnostic, map_nonzero_exit, map_termination, no_child_outcome,
    result_json_schema,
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

/// The `--settings` value that neutralizes every hook declared by user,
/// project, local, or `--plugin-dir` settings, for this session (spec §10.2
/// R-27; PR #1 Codex review finding 1). CLI-supplied `--settings` outranks
/// user/project/local settings, so this holds even for the operator's own
/// trusted `~/.claude` settings, the wiki's own (loaded) project/local
/// settings, or a configured local plugin. **It does not reach an
/// admin-managed/enterprise-policy hook** (PR #1 Codex review iteration 6
/// finding A: an earlier version of this comment said "regardless of
/// source" without this exception, self-contradicting the honest residual
/// gap stated elsewhere) — no CLI flag or static check in this project
/// disables that class of hook; see spec §10.2/§12's own "Honest residual
/// gap" text.
pub const DISABLE_ALL_HOOKS_SETTINGS: &str = "{\"disableAllHooks\":true}";

/// R-34 (PRD 08-06-pre-0-1-0-cli-refinements, D6): a deliberate, narrower
/// reversal of the earlier "`--setting-sources` is not used here at all any
/// more" posture left over from R-27/R-28 (see [`build_argv`]'s own doc
/// comment for that full history). R-27's original `--setting-sources user`
/// *excluded* the `project` setting source and broke project-skill
/// discovery (R-28); `--setting-sources project` is the **opposite**
/// exclusion — it drops `user` and `local` (where a user-level
/// `enabledPlugins`/hook lives, the source of the `SessionEnd` "Hook
/// cancelled" pollution this task fixes) while keeping `project` (which
/// skill discovery itself depends on). Live-verified compatible with
/// [`DISABLE_ALL_HOOKS_SETTINGS`] and successful `/name` skill expansion,
/// zero `permission_denials` (research/provider-cli-flags.md §2, §3).
pub const SETTING_SOURCES_PROJECT: &str = "project";

/// Builds the exact Claude argv vector (spec §10.2). Never includes the
/// entrypoint or `query_prompt` — those exist only in the stdin prompt.
///
/// **Threat this argv's `--settings` flag defends against (spec §10.2
/// R-27)**: `invoke`'s child cwd is the wiki's own `project_root`, an
/// untrusted operator-controlled directory this wrapper does not own. Claude
/// Code's `-p` mode auto-loads that directory's `.claude/settings.json` /
/// `settings.local.json` and **runs any hooks they declare** (SessionStart,
/// PreToolUse, ...) as arbitrary shell — entirely outside the `--tools
/// Read,Grep,Glob` gate, which restricts only built-in tools, not hook
/// commands. [`DISABLE_ALL_HOOKS_SETTINGS`] disables every hook declared by
/// those settings sources — see its own doc comment for the admin-managed/
/// enterprise-policy exception this does **not** reach. `--setting-sources
/// user` was tried instead/in addition (R-27) and reverted (R-28) — it also
/// excludes the `project` setting source that Claude's project-skill
/// discovery itself depends on, breaking every `project_skill`-load-mode
/// wiki's entrypoint. `--setting-sources project` — the opposite exclusion,
/// dropping `user`/`local` while keeping `project` — **is** used here,
/// added later for an unrelated reason (R-34): see
/// [`SETTING_SOURCES_PROJECT`]'s own doc comment.
///
/// **This argv alone is not the trust boundary — do not read it as one.**
/// `--settings`/`--strict-mcp-config`/`--tools` bound hooks, MCP, and the
/// built-in tool surface respectively, but several documented settings keys
/// neither hook- nor tool-shaped still execute a command or widen reach on
/// their own (`apiKeyHelper`, confirmed live to execute even with hooks
/// disabled — `docs/verification/llm-wikis-execution.md` Task 15 "review
/// loop iteration 3"). The load-bearing gate for that is
/// [`crate::config::check_claude_wiki_settings_surface`] (spec §12/§15
/// R-29/R-30/R-31), a **separate function**, the single source of truth,
/// called both from static `doctor` and from `ClaudeAdapter::invoke`
/// immediately before the child is spawned — see that call site's own
/// comment for exactly where and why, and the function's own doc comment
/// for the full current allowlist and its honestly-stated TOCTOU residual.
/// This comment intentionally does not restate either, to avoid the two
/// going out of sync the way an earlier version of this comment did (PR #1
/// Codex review iteration 5 findings 4-6).
pub fn build_argv(
    content_root: &Path,
    mcp_config_path: &Path,
    json_schema: &str,
    plugin_dir: Option<&Path>,
    model: Option<&str>,
    effort: Option<&str>,
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
        OsString::from("--settings"),
        OsString::from(DISABLE_ALL_HOOKS_SETTINGS),
        // PRD 08-06-pre-0-1-0-cli-refinements item 3/D5: reinforces the
        // read-only/no-persistence contract already carried by
        // `--tools`/`--permission-mode` above with model-behavior
        // directives text alone cannot mechanically compel (answer and
        // stop, no save offer, no wiki/index/frontmatter/log update, no
        // unanswerable follow-up question) — a separate channel from the
        // closed `constraints` array in the stdin prompt (spec §7.1),
        // untouched by this change.
        OsString::from("--append-system-prompt"),
        OsString::from(NON_INTERACTIVE_SYSTEM_DIRECTIVES),
        // D6/R-34: see `SETTING_SOURCES_PROJECT`'s own doc comment.
        OsString::from("--setting-sources"),
        OsString::from(SETTING_SOURCES_PROJECT),
    ];
    if let Some(dir) = plugin_dir {
        args.push(OsString::from("--plugin-dir"));
        args.push(OsString::from(dir));
    }
    // Issue #7: appended last, and only when configured, so an operator who
    // sets neither gets the byte-identical argv this wrapper has always sent.
    // Each value is its own `OsString` — never spliced into another argument.
    if let Some(model) = model {
        args.push(OsString::from("--model"));
        args.push(OsString::from(model));
    }
    if let Some(effort) = effort {
        args.push(OsString::from("--effort"));
        args.push(OsString::from(effort));
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
                "claude auth status output was missing both the authenticated and loggedIn fields",
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
            request.model.as_deref(),
            request.effort.as_deref(),
        );
        // R-31 (PR #1 Codex review iteration 5 finding 3): the wiki-settings
        // surface check is the genuinely last thing before the child is
        // spawned -- after plugin-dir canonicalization, temp-dir/temp-file
        // creation, and argv/schema construction, none of which need to
        // precede it, and immediately before the one `runner.run` call that
        // actually starts the untrusted wiki's provider process. One
        // function, `crate::config::check_claude_wiki_settings_surface`, is
        // the single source of truth -- also called, unchanged, from static
        // `doctor` (`src/doctor.rs::entrypoint_check`) -- not duplicated.
        // This is a narrowing of the check-to-spawn window to the syscall
        // gap between this check returning and `runner.run` actually
        // executing `Command::spawn` -- **not a closure of that window**;
        // see the doc comment on `check_claude_wiki_settings_surface`.
        //
        // R-33 (PR #1 Codex review iteration 6 finding B): this call's
        // `Ok(bool)` used to be discarded (`if let Err(e) = ...`), which was
        // fine for enforcement (the `Err` path was already handled) but
        // silently dropped the one authoritative signal for whether
        // `CLAUDE_ENABLED_PLUGINS_DECLARED` should fire -- `QueryService`
        // separately re-read the same check *earlier*, before the version/
        // auth probes, so a wiki whose settings started declaring
        // `enabledPlugins` only between that earlier read and this
        // authoritative one would spawn with the warning silently missing.
        // Capturing the boolean here and carrying it through `InvokeOutcome`
        // makes this call the single source for both enforcement and the
        // warning -- there is no longer any earlier read to go stale.
        let claude_enabled_plugins_declared =
            match crate::config::check_claude_wiki_settings_surface(&request.project_root) {
                Ok(declares) => declares,
                Err(e) => return no_child_outcome(e),
            };
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
            // PR #1 Codex review iteration 8 finding 1: `no_child_outcome`
            // unconditionally sets `claude_enabled_plugins_declared: false`,
            // which is correct for the three call sites above (none of them
            // have run the authoritative check yet) but was wrong here --
            // the check above already ran and determined this wiki's
            // settings declare `enabledPlugins`; a spawn failure at this
            // point must not silently revert that back to `false` and drop
            // the warning the operator was promised. Construct the outcome
            // directly instead of delegating to `no_child_outcome`, so the
            // already-authoritative boolean survives this failure path too.
            Err(e) => {
                return InvokeOutcome {
                    model_result: Err(e),
                    child_exit_code: None,
                    raw_format: None,
                    diagnostics: Vec::new(),
                    claude_enabled_plugins_declared,
                };
            }
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
                claude_enabled_plugins_declared,
            };
        }
        if let Err(e) = map_nonzero_exit(&outcome) {
            return InvokeOutcome {
                model_result: Err(e),
                child_exit_code,
                raw_format: None,
                diagnostics: Vec::new(),
                claude_enabled_plugins_declared,
            };
        }
        match parse_claude_output(&outcome.stdout) {
            Ok((result, raw_format)) => InvokeOutcome {
                model_result: Ok(result),
                child_exit_code,
                raw_format: Some(raw_format),
                diagnostics: Vec::new(),
                claude_enabled_plugins_declared,
            },
            Err(e) => InvokeOutcome {
                model_result: Err(e),
                child_exit_code,
                raw_format: None,
                diagnostics: Vec::new(),
                claude_enabled_plugins_declared,
            },
        }
    }
}
