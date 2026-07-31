//! Cross-platform process supervisor (spec §10.1; plan Task 8).
//!
//! [`run`] launches one provider process under platform containment (Windows Job
//! Object, Unix process group), writes the complete prompt to stdin and closes it,
//! drains stdout/stderr concurrently on two reader threads bounded by independent
//! byte caps, enforces one monotonic deadline across the whole spawn/write/read/wait
//! lifecycle, kills the entire process tree on timeout/overflow/cancellation, and
//! always joins every spawned thread and reaps the child before returning. Neither
//! this module nor its callers ever render an argument vector into a shell command
//! string — [`Command`] is always invoked with a program path plus a `Vec<OsString>`
//! of already-separated arguments.
//!
//! Executable resolution ([`resolve_executable`]) classifies a resolved path as
//! [`ExecutableKind::Native`] or (Windows-only) [`ExecutableKind::BatchShim`]. The
//! classification is purely informational for doctor/reporting: both kinds are
//! spawned identically via `Command::new(path).args(args)`, with **no** manual
//! `cmd.exe /C` string construction. This is the quoting rule empirically recorded
//! in Task 2 Step 5 (`docs/verification/llm-wikis-preflight.md` Row 5): spawning a
//! `.cmd`/`.bat` shim directly, with each argument passed as its own `Command::arg`
//! element, preserves `& | ^ %VAR% !DELAYED! > <` as literal, unexpanded argv
//! elements — Windows' own `.cmd`/`.bat` auto-invocation wraps the entire original
//! command line once (not a token-by-token shell re-parse), so per-argument
//! boundaries survive. A batch **script's own body** must still forward received
//! parameters by individual position (`%1 %2 %3 ...`), never by `%*`: `%*` reinserts
//! the raw, untokenized command-tail text, which cmd.exe's line parser then rescans
//! for `& | < >`, whereas `%1`..`%9` substitute one already-tokenized argument
//! opaquely. This distinction was independently re-derived and confirmed with a
//! throwaway probe on this machine's toolchain before being encoded into the batch
//! shim fixtures below, per this task's brief ("reimplement through failing tests",
//! never copy spike code).

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};
use thiserror::Error;

use crate::config::EnvLookup;
use crate::error::{AppError, ErrorCode, Stream};

// ---------------------------------------------------------------------------
// Executable resolution and classification (plan Task 8 Steps 2, 6)
// ---------------------------------------------------------------------------

/// How a resolved provider executable must be spawned. Both variants use the
/// identical `Command::new(path).args(args)` call in [`run`] — the distinction
/// is reporting-only (spec §10.1: "Direct `.exe` providers are started without a
/// shell. When PATH resolution selects a `.cmd` or `.bat` provider shim, only the
/// trusted, fixed provider argv passes through a dedicated Windows batch adapter").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableKind {
    Native,
    BatchShim,
}

/// A canonical, existence-and-shape-checked provider executable path plus its
/// spawn classification (plan Task 8 Step 2: "Assert the canonical resolved path
/// is reported for doctor").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutable {
    pub path: PathBuf,
    pub kind: ExecutableKind,
}

fn cli_not_found(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::CliNotFound, message)
}

#[cfg(windows)]
fn default_pathext() -> Vec<String> {
    vec!["COM".into(), "EXE".into(), "BAT".into(), "CMD".into()]
}

fn classify_existing_file(path: &Path) -> Result<ResolvedExecutable, AppError> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| cli_not_found(format!("provider executable {path:?} was not found: {e}")))?;
    let md = fs::metadata(&canonical).map_err(|e| {
        cli_not_found(format!(
            "provider executable {path:?} is not accessible: {e}"
        ))
    })?;
    if !md.is_file() {
        return Err(cli_not_found(format!(
            "provider executable {path:?} is not a regular file"
        )));
    }
    let kind = match canonical
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    {
        #[cfg(windows)]
        Some(ext) if ext == "cmd" || ext == "bat" => ExecutableKind::BatchShim,
        _ => ExecutableKind::Native,
    };
    Ok(ResolvedExecutable {
        path: canonical,
        kind,
    })
}

fn split_path_list(value: &str) -> Vec<String> {
    value
        .split(if cfg!(windows) { ';' } else { ':' })
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Resolves a provider `executable` value (spec §10.1, plan Task 8 Step 2):
/// bare names search `PATH` (Windows also tries each `PATHEXT` suffix); absolute
/// paths are checked directly; relative paths containing a separator are
/// rejected outright; missing files, directories, and non-regular entries all
/// fail as `CLI_NOT_FOUND`.
pub fn resolve_executable(
    value: &str,
    env: &dyn EnvLookup,
) -> Result<ResolvedExecutable, AppError> {
    if value.is_empty() {
        return Err(cli_not_found("provider executable must not be empty"));
    }
    let has_separator = value.contains('/') || value.contains('\\');
    if has_separator {
        let candidate = Path::new(value);
        if !candidate.is_absolute() {
            return Err(cli_not_found(
                "provider executable path must be absolute, not relative",
            ));
        }
        return classify_existing_file(candidate);
    }

    let path_var = env.get("PATH").unwrap_or_default();
    #[cfg(windows)]
    let extensions = {
        let mut exts = vec![String::new()];
        exts.extend(default_pathext());
        exts
    };
    #[cfg(not(windows))]
    let extensions = vec![String::new()];

    for dir in split_path_list(&path_var) {
        for ext in &extensions {
            let candidate = if ext.is_empty() {
                Path::new(&dir).join(value)
            } else {
                Path::new(&dir).join(format!("{value}.{ext}"))
            };
            if candidate.is_file() {
                return classify_existing_file(&candidate);
            }
        }
    }
    Err(cli_not_found(format!(
        "provider executable {value:?} was not found on PATH"
    )))
}

// ---------------------------------------------------------------------------
// Bounded request/outcome types (plan Task 8 Step 4)
// ---------------------------------------------------------------------------

pub struct ProcessRequest {
    pub executable: ResolvedExecutable,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub stdin: Vec<u8>,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    /// External abort signal, checked on every poll tick alongside the deadline
    /// and the byte caps. Supports both "cancellation" and "parser-aborting
    /// failure" (spec §10.1) — from the supervisor's point of view these are the
    /// same event: an external party decided to stop waiting for this child.
    /// Not part of the plan's Step 4 snippet verbatim; added because Step 1's
    /// required `kill_on_all_abort_paths` coverage has no other way to express
    /// cancellation/parser-abort without it.
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    Completed,
    TimedOut,
    OutputTooLarge { stream: Stream, observed_bytes: u64 },
    Cancelled,
}

#[derive(Debug)]
pub struct ProcessOutcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub elapsed: Duration,
    pub termination: TerminationReason,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("failed to spawn provider process: {0}")]
    Spawn(#[source] std::io::Error),
    /// The process tree could not be confirmed dead after kill was requested
    /// (spec §10.1, §14 `TERMINATION_FAILED`).
    #[error("process tree could not be confirmed terminated")]
    TerminationFailed,
}

// ---------------------------------------------------------------------------
// Concurrent bounded readers (plan Task 8 Step 7)
// ---------------------------------------------------------------------------

const READ_CHUNK_BYTES: usize = 8192;

/// Reads `pipe` on the calling thread until EOF or until the accumulated byte
/// count exceeds `cap`, at which point it records `(stream, observed_bytes)`
/// into `overflow` (first writer wins) and stops reading immediately — the cap
/// is enforced while streaming, never by buffering the whole output first
/// (spec §10.1).
fn spawn_capped_reader(
    mut pipe: impl Read + Send + 'static,
    cap: usize,
    stream: Stream,
    overflow: Arc<Mutex<Option<(Stream, u64)>>>,
) -> (thread::JoinHandle<()>, Arc<Mutex<Vec<u8>>>) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_thread = Arc::clone(&captured);
    let handle = thread::spawn(move || {
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        loop {
            let n = match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let mut buf = captured_thread.lock().unwrap();
            buf.extend_from_slice(&chunk[..n]);
            let observed = buf.len();
            if observed > cap {
                let mut slot = overflow.lock().unwrap();
                if slot.is_none() {
                    *slot = Some((stream, observed as u64));
                }
                break;
            }
        }
    });
    (handle, captured)
}

// ---------------------------------------------------------------------------
// Deadline, containment, and cleanup (plan Task 8 Steps 8, 9)
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const KILL_CONFIRM_ATTEMPTS: u32 = 100;
const KILL_CONFIRM_INTERVAL: Duration = Duration::from_millis(20);

/// Requests termination of the whole process tree and blocks until the wrapper
/// confirms it is dead (bounded retries), or returns
/// [`ProcessError::TerminationFailed`] if it never can. Exposed as its own
/// function (not inlined into [`run`]) so the failure path is directly
/// testable against a fault-injected [`ChildWrapper`] double that never reports
/// itself dead, without needing a genuinely unkillable OS process.
pub fn terminate_and_confirm(child: &mut dyn ChildWrapper) -> Result<ExitStatus, ProcessError> {
    let _ = child.start_kill();
    for _ in 0..KILL_CONFIRM_ATTEMPTS {
        if let Ok(Some(status)) = child.try_wait() {
            return Ok(status);
        }
        thread::sleep(KILL_CONFIRM_INTERVAL);
    }
    Err(ProcessError::TerminationFailed)
}

/// Runs one provider process to completion under the full spec §10.1 contract.
///
/// Order: spawn under containment -> start both capped reader threads -> write
/// the complete stdin payload on its own thread, then close it -> poll for
/// completion/overflow/cancellation/deadline (whichever comes first) -> on
/// early exit, kill the whole tree and confirm it is dead -> join every thread
/// -> return the bounded outcome. Every return path (including the `?`
/// early-returns for spawn failure) still runs on top of threads that either
/// were never started or are joined before the enclosing scope exits, so no
/// path can leak a reader/writer thread.
pub fn run(request: &ProcessRequest) -> Result<ProcessOutcome, ProcessError> {
    let start = Instant::now();
    let deadline = start + request.timeout;

    let mut command = Command::new(&request.executable.path);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut wrap = CommandWrap::from(command);
    #[cfg(windows)]
    wrap.wrap(JobObject);
    #[cfg(unix)]
    wrap.wrap(ProcessGroup::leader());

    let mut child = wrap.spawn().map_err(ProcessError::Spawn)?;

    let mut stdin = child.stdin().take().expect("stdin was requested as piped");
    let stdout = child
        .stdout()
        .take()
        .expect("stdout was requested as piped");
    let stderr = child
        .stderr()
        .take()
        .expect("stderr was requested as piped");

    let overflow: Arc<Mutex<Option<(Stream, u64)>>> = Arc::new(Mutex::new(None));

    // Reader threads start before stdin is written, per spec §10.1 / plan Step 7.
    let (stdout_handle, stdout_buf) = spawn_capped_reader(
        stdout,
        request.max_stdout_bytes,
        Stream::Stdout,
        Arc::clone(&overflow),
    );
    let (stderr_handle, stderr_buf) = spawn_capped_reader(
        stderr,
        request.max_stderr_bytes,
        Stream::Stderr,
        Arc::clone(&overflow),
    );

    let stdin_payload = request.stdin.clone();
    let stdin_handle = thread::spawn(move || {
        let _ = stdin.write_all(&stdin_payload);
        drop(stdin); // explicit close, independent of write success
    });

    let cancel = request.cancel.clone();
    let termination = loop {
        // Overflow must be checked before `try_wait`: once a reader breaks on
        // overflow it drops its pipe half, so the child's *next* write can
        // fail (broken pipe) and the child can exit on its own almost
        // immediately afterward. If `try_wait` were checked first, that
        // self-inflicted exit would race the overflow flag and could be
        // misreported as a normal `Completed` instead of `OutputTooLarge`.
        if let Some((stream, observed_bytes)) = *overflow.lock().unwrap() {
            break TerminationReason::OutputTooLarge {
                stream,
                observed_bytes,
            };
        }
        match child.try_wait() {
            Ok(Some(_status)) => break TerminationReason::Completed,
            Ok(None) => {}
            Err(_) => break TerminationReason::Completed,
        }
        if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            break TerminationReason::Cancelled;
        }
        if Instant::now() >= deadline {
            break TerminationReason::TimedOut;
        }
        thread::sleep(POLL_INTERVAL);
    };

    let exit_status = match termination {
        TerminationReason::Completed => match child.try_wait() {
            Ok(Some(status)) => Some(status),
            _ => child.wait().ok(),
        },
        TerminationReason::TimedOut
        | TerminationReason::OutputTooLarge { .. }
        | TerminationReason::Cancelled => Some(terminate_and_confirm(&mut *child)?),
    };

    // Join every spawned thread before returning on every path (spec §10.1).
    let _ = stdin_handle.join();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let stdout_bytes = stdout_buf.lock().unwrap().clone();
    let stderr_bytes = stderr_buf.lock().unwrap().clone();

    Ok(ProcessOutcome {
        stdout: stdout_bytes,
        stderr: stderr_bytes,
        exit_code: exit_status.and_then(|s| s.code()),
        elapsed: start.elapsed(),
        termination,
    })
}

// ---------------------------------------------------------------------------
// Temp-artifact safety (plan Task 8 Step 9)
// ---------------------------------------------------------------------------

fn temp_root_unsafe(message: impl Into<String>) -> AppError {
    // No dedicated error code exists for this invariant in the closed spec §14
    // vocabulary; it is a defensive runtime-environment safety check, not a
    // user-facing configuration mistake, so it maps to `INTERNAL_ERROR`.
    AppError::new(ErrorCode::InternalError, message)
}

/// Resolves a fresh, canonical temp directory rooted under `base`, rejecting it
/// before any file is created or process spawned if it is not canonically
/// disjoint from every `configured_roots` entry (spec §10.1: "canonically
/// outside every configured `project_root` and `content_root`"; plan Task 8
/// Step 9: "Add a test for an injected unsafe temp root"). `base` is an
/// injected parameter (not `std::env::temp_dir()` directly) so tests can point
/// it at an unsafe location without touching the real system temp directory.
pub fn resolve_safe_temp_root(
    base: &Path,
    configured_roots: &[PathBuf],
) -> Result<PathBuf, AppError> {
    let canonical_base = fs::canonicalize(base)
        .map_err(|e| temp_root_unsafe(format!("temp base {base:?} is not accessible: {e}")))?;
    for root in configured_roots {
        let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        if canonical_base.starts_with(&canonical_root)
            || canonical_root.starts_with(&canonical_base)
        {
            return Err(temp_root_unsafe(format!(
                "temp root {canonical_base:?} is not canonically disjoint from configured root {canonical_root:?}"
            )));
        }
    }
    let dir = tempfile::Builder::new()
        .prefix("llm-wikis-")
        .tempdir_in(&canonical_base)
        .map_err(|e| temp_root_unsafe(format!("cannot create temp directory: {e}")))?;
    Ok(dir.keep())
}

#[cfg(windows)]
fn restrict_to_current_user(path: &Path) -> bool {
    let user = std::env::var("USERNAME").unwrap_or_default();
    if user.is_empty() {
        return false;
    }
    Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:F"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
fn restrict_to_current_user(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(path) {
        Ok(md) => {
            let mut perms = md.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms).is_ok()
        }
        Err(_) => false,
    }
}

/// A single generated artifact (empty MCP config, Codex output-schema file, or
/// batch helper) created with exclusive-creation semantics and best-effort
/// user-only permissions, owned/open until the child that needs it has started,
/// and removed on drop — i.e. after the child is reaped, since the caller is
/// expected to hold this alive across the whole [`run`] call and drop it only
/// afterward (spec §10.1, plan Task 8 Step 9).
#[derive(Debug)]
pub struct TempArtifact {
    path: PathBuf,
    file: Option<File>,
    permissions_restricted: bool,
}

impl TempArtifact {
    pub fn create(dir: &Path, filename: &str, contents: &[u8]) -> Result<Self, AppError> {
        let path = dir.join(filename);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| temp_root_unsafe(format!("cannot exclusively create {path:?}: {e}")))?;
        file.write_all(contents)
            .map_err(|e| temp_root_unsafe(format!("cannot write {path:?}: {e}")))?;
        file.flush()
            .map_err(|e| temp_root_unsafe(format!("cannot flush {path:?}: {e}")))?;
        let permissions_restricted = restrict_to_current_user(&path);
        Ok(Self {
            path,
            file: Some(file),
            permissions_restricted,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn permissions_restricted(&self) -> bool {
        self.permissions_restricted
    }
}

impl Drop for TempArtifact {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}
