//! Provider adapter trait, shared request/result types, and the fixed prompt
//! envelope builder (spec §7.1, §10.1; plan Core Interfaces).

pub mod claude;
pub mod codex;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::error::{AppError, ErrorCode, ErrorDetails, OutputTooLargeDetails, Stream};
use crate::model::ModelResult;
use crate::output::{Agent, RawFormat};
use crate::process::{ProcessOutcome, ProcessRequest, ResolvedExecutable, TerminationReason};

// ---------------------------------------------------------------------------
// Process runner seam (plan Core Interfaces)
// ---------------------------------------------------------------------------

/// Lets the adapters (and their tests) replace real process spawning with an
/// in-memory double. This crate's own test suite never spawns a real
/// provider process through this trait (plan Task 9 hard constraint).
pub trait ProcessRunner: Send + Sync {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError>;
}

/// Spawns the real child process via [`crate::process::run`] (production use).
pub struct RealProcessRunner;

impl ProcessRunner for RealProcessRunner {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
        crate::process::run(&request).map_err(|e| match e {
            crate::process::ProcessError::Spawn(err) => AppError::new(
                ErrorCode::CliNotFound,
                format!("failed to start provider process: {err}"),
            ),
            crate::process::ProcessError::TerminationFailed => AppError::new(
                ErrorCode::TerminationFailed,
                "process tree could not be confirmed terminated",
            ),
        })
    }
}

/// The parts of a [`ProcessRequest`] worth asserting on in tests, captured
/// because `ProcessRequest` itself holds a `Vec<u8>` stdin payload and is not
/// `Clone`.
#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub stdin: Vec<u8>,
}

/// An in-memory [`ProcessRunner`] test double. Queue canned outcomes with
/// [`push_response`](FakeProcessRunner::push_response); each `run` call
/// captures its request and returns the next queued response in FIFO order
/// (plan Task 9: "tested against fixture bytes and a fake ProcessRunner — NO
/// real provider processes, NO billable calls").
#[derive(Default)]
pub struct FakeProcessRunner {
    responses: Mutex<VecDeque<Result<ProcessOutcome, AppError>>>,
    requests: Mutex<Vec<CapturedRequest>>,
}

impl FakeProcessRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_response(&self, outcome: Result<ProcessOutcome, AppError>) {
        self.responses.lock().unwrap().push_back(outcome);
    }

    pub fn captured_requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl ProcessRunner for FakeProcessRunner {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
        self.requests.lock().unwrap().push(CapturedRequest {
            program: request.executable.path.clone(),
            args: request.args.clone(),
            cwd: request.cwd.clone(),
            stdin: request.stdin.clone(),
        });
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("FakeProcessRunner::run called with no queued response")
    }
}

/// Builds a trivial `Completed` outcome for tests that only care about
/// stdout/exit code.
pub fn completed_outcome(stdout: &[u8], stderr: &[u8], exit_code: i32) -> ProcessOutcome {
    ProcessOutcome {
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
        exit_code: Some(exit_code),
        elapsed: Duration::from_millis(1),
        termination: TerminationReason::Completed,
    }
}

// ---------------------------------------------------------------------------
// Auth readiness (spec §10.1)
// ---------------------------------------------------------------------------

/// The only interesting outcome of a successful auth-status probe: a
/// recognized logged-out state is returned as `Err(AppError{AUTH_REQUIRED})`
/// instead, so `Ok` always means ready to invoke (spec §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated,
}

// ---------------------------------------------------------------------------
// Provider request/adapter trait (plan Core Interfaces)
// ---------------------------------------------------------------------------

/// Everything one `invoke` call needs, already validated and resolved by the
/// caller (spec §8.1: the provider trait receives only validated, resolved
/// values — no CLI parsing, no TOML, no direct process creation leaks in
/// here). `temp_base`/`configured_roots` exist only so tests can point
/// temp-artifact creation at a disposable directory instead of the real
/// system temp dir, mirroring [`crate::process::resolve_safe_temp_root`]'s
/// own `base` parameter.
pub struct ProviderRequest {
    pub executable: ResolvedExecutable,
    pub wiki_id: String,
    pub project_root: PathBuf,
    pub content_root: PathBuf,
    pub entrypoint: String,
    pub query_prompt: String,
    pub question: String,
    pub plugin_dir: Option<PathBuf>,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub temp_base: PathBuf,
    pub configured_roots: Vec<PathBuf>,
}

/// The result of one provider invocation attempt (spec §8.1 steps 9, 11-14).
/// Deliberately *not* itself a `Result`: every field here is populated
/// regardless of whether `model_result` succeeded, because the caller
/// (`QueryService`) needs `child_exit_code`/`raw_format` on failure paths
/// too (plan Core Interfaces field-ownership table: `child_exit_code`
/// forwarded unchanged, `null` only when no child ran; `raw_format` only
/// after a native document or stream parses). A bare `Result<ModelResult,
/// AppError>` return (the plan's original `invoke` sketch) cannot carry
/// either on the `Err` path, which is why this type exists (orchestrator
/// correction, Task 10 follow-up: the plan's Core Interfaces escape clause
/// — "stable unless proven infeasible" — applies here).
pub struct InvokeOutcome {
    pub model_result: Result<ModelResult, AppError>,
    pub child_exit_code: Option<i32>,
    pub raw_format: Option<RawFormat>,
    /// Bounded, non-fatal provider-side diagnostics observed during this
    /// invocation attempt (spec §10.1/§10.3: "recorded in bounded
    /// diagnostics") — e.g. Codex's per-item `mcp_tool_call` signal
    /// classification. Empty for Claude, which has no equivalent signal.
    /// Not surfaced by the public envelope today; carried through for
    /// doctor/live-check reuse (Task 11).
    pub diagnostics: Vec<String>,
}

/// The outcome for a failure before any child process ever ran (temp-
/// artifact setup, the spawn attempt itself).
pub fn no_child_outcome(err: AppError) -> InvokeOutcome {
    InvokeOutcome {
        model_result: Err(err),
        child_exit_code: None,
        raw_format: None,
        diagnostics: Vec::new(),
    }
}

pub trait ProviderAdapter: Send + Sync {
    fn name(&self) -> Agent;

    fn version(
        &self,
        runner: &dyn ProcessRunner,
        executable: &ResolvedExecutable,
    ) -> Result<String, AppError>;

    fn auth_status(
        &self,
        runner: &dyn ProcessRunner,
        executable: &ResolvedExecutable,
    ) -> Result<AuthStatus, AppError>;

    /// `prompt` is already built (spec §8.1 step 9 happens in `QueryService`,
    /// strictly before the before-snapshot at step 10) and passed in rather
    /// than built from `request` here, so the normative step order is real,
    /// not cosmetic.
    fn invoke(
        &self,
        runner: &dyn ProcessRunner,
        request: ProviderRequest,
        prompt: String,
    ) -> InvokeOutcome;
}

// ---------------------------------------------------------------------------
// Shared process-outcome mapping (used by both adapters' `invoke`)
// ---------------------------------------------------------------------------

/// Maps a non-`Completed` termination to its public error (spec §14). Callers
/// still need to check `exit_code` themselves on `Completed`.
pub fn map_termination(
    outcome: &ProcessOutcome,
    max_stdout_bytes: u64,
    max_stderr_bytes: u64,
) -> Result<(), AppError> {
    match outcome.termination {
        TerminationReason::Completed => Ok(()),
        TerminationReason::TimedOut => Err(AppError::new(
            ErrorCode::Timeout,
            "provider invocation exceeded the configured timeout",
        )),
        TerminationReason::OutputTooLarge {
            stream,
            observed_bytes,
        } => {
            let limit = if stream == Stream::Stdout {
                max_stdout_bytes
            } else {
                max_stderr_bytes
            };
            let details =
                OutputTooLargeDetails::new(stream, limit, observed_bytes).unwrap_or_else(|_| {
                    OutputTooLargeDetails::new(stream, 0, observed_bytes.max(1))
                        .expect("observed_bytes.max(1) > 0")
                });
            Err(AppError::with_details(
                ErrorCode::OutputTooLarge,
                "provider output exceeded the configured byte limit",
                ErrorDetails::OutputTooLarge(details),
            )
            .expect("OutputTooLarge code/variant pairing is valid"))
        }
        TerminationReason::Cancelled => Err(AppError::new(
            ErrorCode::InternalError,
            "provider invocation was cancelled",
        )),
    }
}

/// Maps a non-zero completed exit code to `NONZERO_EXIT`, with capped stderr
/// folded into the message for diagnostics (spec §10.2/§10.3: "cap stderr in
/// diagnostics").
pub fn map_nonzero_exit(outcome: &ProcessOutcome) -> Result<(), AppError> {
    match outcome.exit_code {
        Some(0) => Ok(()),
        code => Err(AppError::new(
            ErrorCode::NonzeroExit,
            format!(
                "provider exited with code {code:?}; stderr: {}",
                cap_diagnostic(&outcome.stderr)
            ),
        )),
    }
}

/// The bounded length (in bytes of the lossy-decoded string) any single
/// diagnostic snippet (stderr, event-type list) is truncated to before it is
/// folded into a public error message (spec §10.2/§10.3: "cap stderr in
/// diagnostics" / "cap event diagnostics and stderr").
///
/// ponytail: one fixed cap rather than a configurable one — nothing in the
/// spec ties this to `max_stderr_bytes`, and 2000 bytes is generous for a
/// diagnostic snippet while keeping public error messages small. Revisit if
/// a real provider's diagnostic text needs more.
pub const DIAGNOSTIC_CAP_BYTES: usize = 2000;

/// Bounds an arbitrary diagnostic byte string to [`DIAGNOSTIC_CAP_BYTES`],
/// lossily decoding it as UTF-8 first (diagnostics are for humans, not for
/// round-tripping).
pub fn cap_diagnostic(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= DIAGNOSTIC_CAP_BYTES {
        text.into_owned()
    } else {
        let mut end = DIAGNOSTIC_CAP_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...(truncated)", &text[..end])
    }
}

// ---------------------------------------------------------------------------
// Prompt envelope (spec §7.1)
// ---------------------------------------------------------------------------

pub const CONTRACT: &str = "wiki-query/v1";
pub const MODE: &str = "external-readonly";

/// The five constraint entries byte-identical across providers (spec §7.1).
/// Transcribed verbatim, in order, with a gap left for the provider-specific
/// second entry.
const CONSTRAINT_0: &str =
    "Read only. Do not write, save, commit, log, cache, or regenerate anything.";
const CONSTRAINT_2: &str = "Do not offer to save the answer.";
const CONSTRAINT_3: &str = "Use the content_root above; do not infer a different wiki location.";
const CONSTRAINT_4: &str = "Answer only from this wiki; do not fill gaps from general knowledge.";
const CONSTRAINT_5: &str = "Return the required_result object as your final output.";

/// Claude's second constraint entry, transcribed verbatim (spec §7.1): no
/// shell tool exists at all under Claude.
pub const CLAUDE_SECOND_CONSTRAINT: &str =
    "Do not run scripts or shell commands; no such tool is available.";

/// Codex's second constraint entry, transcribed verbatim (spec §7.1, R-26):
/// read-only commands are Codex's only read mechanism, so the constraint
/// must say so truthfully rather than claim no such tool exists.
pub const CODEX_SECOND_CONSTRAINT: &str = "Reading wiki files with read-only commands is permitted; the sandbox enforces read-only. Do not attempt writes, index regeneration, installs, or network access.";

/// The complete, ordered, provider-specific constraints array (spec §7.1).
pub fn constraints_for(agent: Agent) -> [&'static str; 6] {
    let second = match agent {
        Agent::Claude => CLAUDE_SECOND_CONSTRAINT,
        Agent::Codex => CODEX_SECOND_CONSTRAINT,
    };
    [
        CONSTRAINT_0,
        second,
        CONSTRAINT_2,
        CONSTRAINT_3,
        CONSTRAINT_4,
        CONSTRAINT_5,
    ]
}

/// The `required_result` shape description embedded in the envelope (spec
/// §7.1) — every field's value documents the expected type/shape, not a real
/// answer. Fixed regardless of agent or query.
#[derive(Serialize)]
struct RequiredResultShape {
    contract: &'static str,
    knowledge_status: &'static str,
    answer: &'static str,
    citations: [&'static str; 1],
    gaps: [&'static str; 1],
    warnings: [&'static str; 1],
}

fn required_result_shape() -> RequiredResultShape {
    RequiredResultShape {
        contract: CONTRACT,
        knowledge_status: "grounded | no_relevant_material",
        answer: "string",
        citations: ["bare page slug"],
        gaps: ["string"],
        warnings: ["string"],
    }
}

/// The `EXTERNAL_QUERY` object (spec §7.1). Field declaration order is the
/// wire order `serde_json` serializes in: `contract, mode, wiki_id,
/// content_root, question, required_result, constraints` — exactly the
/// spec's seven keys, nothing else.
#[derive(Serialize)]
struct ExternalQuery<'a> {
    contract: &'static str,
    mode: &'static str,
    wiki_id: &'a str,
    content_root: String,
    question: &'a str,
    required_result: RequiredResultShape,
    constraints: [&'static str; 6],
}

/// Builds the complete stdin prompt (spec §7.1): entrypoint token, blank
/// line, `query_prompt`, blank line, `EXTERNAL_QUERY:` and the serialized
/// object — in that order and nowhere else.
///
/// `query_prompt` is placed as opaque text and is never parsed here, so
/// nothing in it can alter, override, or shadow any `EXTERNAL_QUERY` field:
/// the object below is built solely from `wiki_id`/`content_root`/`question`,
/// which come from resolved configuration and the caller's question, never
/// from `query_prompt` itself (plan Task 9 Step 2 / spec §7.2 "cannot
/// override"). The `question` field is serialized by `serde_json` as a
/// struct field — never formatted into a string template — so it is always
/// valid JSON regardless of its content.
pub fn build_prompt(
    agent: Agent,
    entrypoint: &str,
    query_prompt: &str,
    wiki_id: &str,
    content_root: &Path,
    question: &str,
) -> String {
    let query = ExternalQuery {
        contract: CONTRACT,
        mode: MODE,
        wiki_id,
        content_root: content_root.display().to_string(),
        question,
        required_result: required_result_shape(),
        constraints: constraints_for(agent),
    };
    let serialized = serde_json::to_string(&query).expect("ExternalQuery always serializes");
    format!("{entrypoint}\n\n{query_prompt}\n\nEXTERNAL_QUERY:\n{serialized}")
}

/// A hand-written JSON Schema for the `wiki-query/v1` result shape (spec
/// §7.2 item 9, §10.2, §10.3: "how item 9 is enforced mechanically rather
/// than by prose"). Shared by both adapters: Claude passes it inline
/// (`--json-schema`), Codex writes it to a temporary file (`--output-schema`).
///
/// ponytail: hand-written rather than derived from `ModelResult` via a schema
/// crate — no schema-generation dependency exists in `Cargo.toml` and this
/// task cannot add one (root `Cargo.toml` is off-limits). Five fields, add a
/// generator if the shape grows enough to make hand-sync error-prone.
pub fn result_json_schema() -> String {
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["contract", "knowledge_status", "answer", "citations", "gaps", "warnings"],
        "properties": {
            "contract": { "const": CONTRACT },
            "knowledge_status": { "enum": ["grounded", "no_relevant_material"] },
            "answer": { "type": "string", "minLength": 1 },
            "citations": { "type": "array", "items": { "type": "string" } },
            "gaps": { "type": "array", "items": { "type": "string" } },
            "warnings": { "type": "array", "items": { "type": "string" } }
        }
    });
    serde_json::to_string(&schema).expect("result_json_schema always serializes")
}
