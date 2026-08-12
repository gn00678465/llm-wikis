//! External terminal markdown viewer (`leaf --inline`), the single
//! implementation shared by `query`'s rendering path and `doctor`'s viewer
//! check (issue #8; task 08-12 design.md §1).
//!
//! Two properties drive every decision in this module:
//!
//! 1. **Capture, never inherit.** The rendered bytes are collected in full and
//!    returned to the caller, which writes them to stdout only after success.
//!    Nothing else can guarantee issue #8's requirement that a viewer failure
//!    never leaves a half-rendered answer on stdout followed by a second,
//!    complete copy.
//! 2. **Explicit `SPEC`, never leaf's auto-detection.** Because the child's
//!    stdout is a pipe, leaf's own `is_stdout_terminal()` check would report
//!    "not a terminal" and silently downgrade to plain text at a fixed 80
//!    columns. The caller already knows the real terminal state, so the format
//!    and width are passed explicitly and leaf's detection never runs.
//!
//! Together these dissolve what looked like a structural tension between "safe
//! fallback" and "correct rendering" (research/leaf-inline.md, risk 1).

use std::ffi::OsString;
use std::time::Duration;

use crate::config::{EnvLookup, ViewerConfig};
use crate::error::{AppError, ErrorCode};
use crate::process::{ProcessRequest, ResolvedExecutable, TerminationReason, resolve_executable};

/// The bare command name looked up on `PATH` when `[viewer].executable` is
/// unset. On Windows the extensionless name is correct as written: PATH
/// resolution applies `PATHEXT` and selects `leaf.exe` (see `resolve_executable`).
pub const DEFAULT_VIEWER_COMMAND: &str = "leaf";

/// Wall-clock ceiling for one viewer invocation. Rendering is local, CPU-bound
/// and measured in tens of milliseconds; anything approaching this is a hung
/// child, not slow work.
const VIEWER_TIMEOUT: Duration = Duration::from_secs(10);

/// Matches leaf's own documented stdin ceiling, so this wrapper never becomes
/// the tighter of the two limits.
const MAX_VIEWER_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_VIEWER_STDERR_BYTES: usize = 64 * 1024;

/// Narrowest width leaf accepts (its own `LEAF_WIDTH` floor).
const MIN_WIDTH: u16 = 20;

/// The probe document. Deliberately trivial and fixed: this exists to prove the
/// binary understands `--inline`, not to exercise markdown features.
const PROBE_MARKDOWN: &str = "# probe\n";

/// The rendering mode handed to `--inline`, as a closed set. The `SPEC`
/// argument is never built from a free-form string: leaf treats an
/// unrecognized `SPEC` as a *filename* and fails with a misleading "cannot
/// read" error, so a typo would surface as a missing file rather than a bad
/// flag (research/leaf-inline.md F2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineSpec {
    Plain,
    Ansi(u16),
}

impl InlineSpec {
    fn as_arg(self) -> String {
        match self {
            InlineSpec::Plain => "plain".to_string(),
            InlineSpec::Ansi(width) => format!("ansi:{}", width.max(MIN_WIDTH)),
        }
    }
}

/// A resolved, ready-to-run viewer binary.
#[derive(Debug, Clone)]
pub struct Viewer {
    executable: ResolvedExecutable,
}

fn viewer_unavailable(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::CliNotFound, message)
}

impl Viewer {
    /// Resolves `[viewer].executable`, or the platform default command when it
    /// is unset. Reuses the provider executable resolver so the viewer inherits
    /// its `PATH`/`PATHEXT` semantics and its rejection of argument-bearing or
    /// relative values.
    pub fn resolve(config: &ViewerConfig, env: &dyn EnvLookup) -> Result<Viewer, AppError> {
        let value = config
            .executable
            .clone()
            .unwrap_or_else(|| DEFAULT_VIEWER_COMMAND.to_string());
        let executable = resolve_executable(&value, env)?;
        Ok(Viewer { executable })
    }

    /// The canonical path this viewer resolved to, for doctor's report.
    pub fn path(&self) -> &std::path::Path {
        &self.executable.path
    }

    /// Runs one `--inline` invocation and returns its stdout.
    fn run_inline(&self, markdown: &str, spec: InlineSpec) -> Result<String, AppError> {
        let request = ProcessRequest {
            executable: self.executable.clone(),
            args: vec![OsString::from("--inline"), OsString::from(spec.as_arg())],
            cwd: std::env::temp_dir(),
            stdin: markdown.as_bytes().to_vec(),
            timeout: VIEWER_TIMEOUT,
            max_stdout_bytes: MAX_VIEWER_STDOUT_BYTES,
            max_stderr_bytes: MAX_VIEWER_STDERR_BYTES,
            cancel: None,
        };
        let outcome = crate::process::run(&request)
            .map_err(|e| viewer_unavailable(format!("viewer could not be started: {e}")))?;
        if outcome.termination != TerminationReason::Completed {
            return Err(viewer_unavailable(
                "viewer did not finish within its time and output limits",
            ));
        }
        if outcome.exit_code != Some(0) {
            // leaf reports every failure as exit 1 with an explanatory stderr
            // line and an empty stdout, so one condition covers the whole
            // family (research/leaf-inline.md F4). The child's stderr is
            // summarized rather than forwarded whole.
            let detail = String::from_utf8_lossy(&outcome.stderr);
            let first_line = detail.lines().next().unwrap_or("no diagnostic").trim();
            return Err(viewer_unavailable(format!(
                "viewer exited with a failure: {first_line}"
            )));
        }
        let rendered = String::from_utf8(outcome.stdout)
            .map_err(|_| viewer_unavailable("viewer produced output that is not valid UTF-8"))?;
        if rendered.is_empty() {
            return Err(viewer_unavailable("viewer produced no output"));
        }
        Ok(rendered)
    }

    /// Renders `markdown` for a real terminal of `width` columns.
    pub fn render(&self, markdown: &str, width: u16) -> Result<String, AppError> {
        self.run_inline(markdown, InlineSpec::Ansi(width))
    }

    /// Doctor's readiness probe. Actually renders a fixed document rather than
    /// asking for `--version`: `--inline` only exists from leaf 1.21.0 onward,
    /// so an older binary answers `--version` perfectly well and then fails at
    /// the first real query (research/leaf-inline.md F11).
    pub fn probe(&self) -> Result<(), AppError> {
        self.run_inline(PROBE_MARKDOWN, InlineSpec::Plain)
            .map(|_| ())
    }
}

/// The terminal width to render for. `console` is already in the dependency
/// tree behind `indicatif`, so this needs no additional terminal backend.
pub fn terminal_width() -> u16 {
    console::Term::stdout()
        .size_checked()
        .map(|(_, cols)| cols)
        .unwrap_or(MIN_WIDTH)
        .max(MIN_WIDTH)
}
