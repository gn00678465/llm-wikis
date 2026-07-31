//! Single-wiki Query Service (spec §8, §8.1; plan Task 10 / Core Interfaces).
//!
//! `QueryService::query` performs the whole §8.1 flow *from* step 3 (resolve
//! wiki/provider) *through* step 18 (emit envelope). Steps 1-2 (parse CLI
//! input, load config) happen upstream of this module — the CLI (a later
//! task) parses arguments and calls [`crate::config::Config::load`], then
//! hands the already-loaded [`Config`] to [`QueryRequest`]. This is why the
//! type is generic only over [`ProcessRunner`] and [`ProbeReader`] (plan Core
//! Interfaces): everything CLI/TOML/probe-cache-file-shaped is kept out.
//!
//! Ownership boundary: `QueryService` depends on [`ProbeReader`] only, never
//! on a [`ProbeWriter`](crate::probes) (which does not exist until Task 11).
//! It never spawns a process directly either — every child invocation goes
//! through the injected `R: ProcessRunner`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::citations::{markdown_stems, resolve_and_check};
use crate::config::{
    Config, LoadMode, ProviderWikiConfig, resolve_and_check_artifact, resolve_wiki_roots,
};
use crate::error::{AppError, ErrorCode, IntegrityCheck, apply_integrity_dominance};
use crate::model::{ModelResult, WIKI_QUERY_CONTRACT};
use crate::output::{
    Agent, Citation, QueryEnvelope, RawFormat, SCHEMA_VERSION, Warning, WikiRef, order_warnings,
};
use crate::probes::{ProbeKey, ProbeReader, QueryMode};
use crate::process::ResolvedExecutable;
use crate::providers::claude::ClaudeAdapter;
use crate::providers::codex::CodexAdapter;
use crate::providers::{
    InvokeOutcome, ProcessRunner, ProviderAdapter, ProviderRequest, build_prompt,
};
use crate::snapshot::{SnapshotError, compare_snapshots, take_snapshot};
use crate::wiki::{preflight_content_root, schema_absent_warning};

// ---------------------------------------------------------------------------
// Monotonic clock seam (plan Core Interfaces)
// ---------------------------------------------------------------------------

/// Injected wall-clock seam so `duration_ms` (spec §13; plan Core Interfaces
/// field ownership: "`QueryService` monotonic clock, preflight through
/// after-snapshot") never depends directly on `std::time::Instant::now()`.
pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

/// The real wall clock, for production use.
pub struct RealClock;

impl MonotonicClock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

// ---------------------------------------------------------------------------
// Provider registry (plan Core Interfaces)
// ---------------------------------------------------------------------------

/// Dispatches to the two provider adapters (spec §8's "Provider Adapter"
/// bounded component) for the parts of the flow that fit the frozen
/// [`ProviderAdapter`] trait shape (`version`/`auth_status`). The actual
/// query invocation (steps 9-14) is *not* routed through
/// [`ProviderAdapter::invoke`] — that trait method returns only
/// `Result<ModelResult, AppError>`, with no way to recover the child's exit
/// code or which native format parsed, both of which the public envelope
/// needs (`child_exit_code`, `raw_format`; plan Core Interfaces field
/// ownership table). [`QueryService`] instead calls the same public
/// building blocks `invoke` itself uses (`build_prompt`, each adapter's
/// `build_argv`/`parse_*_output`) directly, in [`QueryService::invoke_claude`]
/// / [`QueryService::invoke_codex`], so it can capture both.
pub struct ProviderRegistry {
    claude: ClaudeAdapter,
    codex: CodexAdapter,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            claude: ClaudeAdapter,
            codex: CodexAdapter,
        }
    }

    fn adapter(&self, agent: Agent) -> &dyn ProviderAdapter {
        match agent {
            Agent::Claude => &self.claude,
            Agent::Codex => &self.codex,
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn agent_command_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude",
        Agent::Codex => "codex",
    }
}

// ---------------------------------------------------------------------------
// Request (plan Core Interfaces)
// ---------------------------------------------------------------------------

/// Everything one `query` call needs, already parsed and loaded by the
/// caller (spec §8.1 steps 1-2 happen before this type is constructed).
pub struct QueryRequest {
    pub config: Config,
    pub config_dir: PathBuf,
    pub wiki_id: String,
    pub agent: Agent,
    /// Raw stdin/argument bytes. UTF-8 and `max_question_bytes` validation
    /// (spec §8.1 step 4) happens inside [`QueryService::query`], never
    /// before it.
    pub question: Vec<u8>,
    /// A fresh, writable base directory for this call's generated temp
    /// artifacts (spec §10.1). Production callers pass `std::env::temp_dir()`;
    /// tests point it at a disposable directory disjoint from their fixture
    /// roots (mirrors [`ProviderRequest::temp_base`]'s own reason for being
    /// an injected parameter rather than a hardcoded call).
    pub temp_base: PathBuf,
}

// ---------------------------------------------------------------------------
// Fingerprints (spec §15.1) — computed here, not in `src/probes.rs`
// ---------------------------------------------------------------------------
//
// Task 9 defined only probe *types* and `ProbeReader` in `src/probes.rs`;
// `src/probes.rs` gains fingerprint computation and `ProbeWriter` in Task 11,
// which this task's hard file-boundary (only `src/query.rs` may be created)
// cannot reach into. Spec §8.1 step 8 ("Fingerprint the selected
// skill/local-plugin artifact") is nonetheless this task's job, so the
// algorithm is implemented here, self-contained, against spec §15.1's exact
// byte-stream description. Task 11 may re-export or duplicate this for
// doctor's own publish path; that is Task 11's call, not this one's.

const FINGERPRINT_EXCLUDED_DIR: &str = "__pycache__";

#[cfg(windows)]
fn is_special_entry(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_type().is_symlink() || (md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

#[cfg(not(windows))]
fn is_special_entry(md: &fs::Metadata) -> bool {
    md.file_type().is_symlink()
}

fn normalize_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("walked path is always under its own root")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_fingerprint_files(
    dir: &Path,
    root: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) -> Result<(), AppError> {
    let entries = fs::read_dir(dir).map_err(|e| {
        AppError::new(
            ErrorCode::InternalError,
            format!("cannot scan skill directory: {e}"),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            AppError::new(
                ErrorCode::InternalError,
                format!("cannot read skill directory entry: {e}"),
            )
        })?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if file_name == FINGERPRINT_EXCLUDED_DIR {
            continue;
        }
        let md = fs::symlink_metadata(&path).map_err(|e| {
            AppError::new(
                ErrorCode::InternalError,
                format!("cannot inspect skill directory entry: {e}"),
            )
        })?;
        if is_special_entry(&md) {
            return Err(AppError::new(
                ErrorCode::UnsafeFilesystemEntry,
                "skill directory contains a symlink, junction, reparse point, or mount point",
            ));
        }
        if md.is_dir() {
            collect_fingerprint_files(&path, root, out)?;
        } else if md.is_file() {
            if file_name.ends_with(".pyc") || file_name.ends_with(".pyo") {
                continue;
            }
            out.insert(normalize_relative(root, &path), path);
        }
    }
    Ok(())
}

/// SHA-256 over the deterministic byte stream from spec §15.1: for every
/// regular file sorted by ordinal normalized relative path (`BTreeMap`'s
/// iteration order over `String` keys is exactly ordinal), the UTF-8
/// `/`-separated relative path, one NUL byte, the file's length as an
/// unsigned 64-bit big-endian integer, then its raw bytes. Excludes
/// `__pycache__/` and `*.pyc`/`*.pyo`. Rejects any special filesystem entry
/// with `UNSAFE_FILESYSTEM_ENTRY`.
///
/// ponytail: reads each file fully into memory rather than streaming in
/// chunks (unlike `snapshot::take_snapshot`) — skill directories are a
/// handful of small text files, not wiki content; revisit only if a real
/// skill directory turns out to contain something large.
pub fn compute_skill_fingerprint(dir: &Path) -> Result<String, AppError> {
    let mut files = BTreeMap::new();
    collect_fingerprint_files(dir, dir, &mut files)?;
    let mut hasher = Sha256::new();
    for (relative_path, path) in files {
        hasher.update(relative_path.as_bytes());
        hasher.update([0u8]);
        let bytes = fs::read(&path).map_err(|e| {
            AppError::new(
                ErrorCode::InternalError,
                format!("cannot read skill file for fingerprinting: {e}"),
            )
        })?;
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

/// Locates the directory `compute_skill_fingerprint` hashes (spec §15.1: "the
/// wiki's own skill directory" for `project_skill`; the whole `plugin_dir`
/// for `local_plugin`). Callable only after
/// [`resolve_and_check_artifact`] has already proven the configured artifact
/// exists in the right shape — this just re-derives the same canonical path
/// rather than threading it back out of that function's `Result<(), _>`.
fn skill_fingerprint_dir(
    config_dir: &Path,
    project_root: &Path,
    provider: &ProviderWikiConfig,
) -> Result<PathBuf, AppError> {
    let internal_error = |e: std::io::Error| {
        AppError::new(
            ErrorCode::InternalError,
            format!("cannot resolve skill/plugin directory for fingerprinting: {e}"),
        )
    };
    match provider.load {
        LoadMode::ProjectSkill => {
            let skill_path = provider
                .skill_path
                .as_deref()
                .expect("validated by config.rs: project_skill requires skill_path");
            let joined = join_maybe_absolute(project_root, skill_path);
            let canonical_file = fs::canonicalize(&joined).map_err(internal_error)?;
            Ok(canonical_file
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(canonical_file))
        }
        LoadMode::LocalPlugin => {
            let plugin_dir = provider
                .plugin_dir
                .as_deref()
                .expect("validated by config.rs: local_plugin requires plugin_dir");
            let joined = join_maybe_absolute(config_dir, plugin_dir);
            fs::canonicalize(&joined).map_err(internal_error)
        }
    }
}

fn join_maybe_absolute(anchor: &Path, configured: &str) -> PathBuf {
    let p = Path::new(configured);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        anchor.join(p)
    }
}

/// The implementation-owned provider safety/contract shape version (spec
/// §15.1: "the implementation-owned provider safety/contract version").
/// Bump this whenever `build_prompt`/`build_argv`'s wire shape changes in a
/// way that should invalidate every existing probe record.
pub const PROVIDER_CONTRACT_VERSION: &str = "1";

/// The exact inputs spec §15.1 names for `compatibility_fingerprint`. Public
/// so a test (or Task 11's doctor, when it publishes a record) can compute
/// the same value [`QueryService::query`] checks against, without a second,
/// possibly-drifting implementation of this shape.
#[derive(Serialize)]
pub struct CompatibilityFingerprintInput<'a> {
    pub wiki_id: &'a str,
    pub title: &'a str,
    pub project_root: &'a str,
    pub content_root: &'a str,
    pub query_prompt: &'a str,
    pub agent: Agent,
    pub load: &'static str,
    pub entrypoint: &'a str,
    pub skill_path: Option<&'a str>,
    pub plugin_dir: Option<&'a str>,
    pub executable_declaration: &'a str,
    pub provider_contract_version: &'static str,
}

/// SHA-256 over a stable (not cross-implementation-canonical, only
/// self-consistent) JSON serialization of exactly the inputs spec §15.1
/// names: "the selected wiki, its provider table, its `query_prompt`, the
/// provider executable declaration, and the implementation-owned provider
/// safety/contract version" — deliberately excluding `[runtime]` timeouts and
/// byte limits, which the same sentence says must never invalidate a probe.
pub fn compute_compatibility_fingerprint(input: &CompatibilityFingerprintInput) -> String {
    let json =
        serde_json::to_string(input).expect("CompatibilityFingerprintInput always serializes");
    format!("sha256:{:x}", Sha256::digest(json.as_bytes()))
}

pub fn load_mode_str(load: LoadMode) -> &'static str {
    match load {
        LoadMode::ProjectSkill => "project_skill",
        LoadMode::LocalPlugin => "local_plugin",
    }
}

fn entrypoint_unverified() -> AppError {
    AppError::new(
        ErrorCode::EntrypointUnverified,
        "the selected entrypoint fingerprint has not passed a current live doctor probe",
    )
}

// ---------------------------------------------------------------------------
// QueryService (plan Core Interfaces)
// ---------------------------------------------------------------------------

pub struct QueryService<R: ProcessRunner, P: ProbeReader> {
    runner: R,
    probes: P,
    providers: ProviderRegistry,
    clock: Box<dyn MonotonicClock>,
    /// Test-only step-order spy (checklist OFF-107). Never set in production;
    /// `new` leaves it `None`, so [`QueryService::step`] is a no-op there.
    observer: Option<Box<dyn Fn(&'static str) + Send + Sync>>,
}

impl<R: ProcessRunner, P: ProbeReader> QueryService<R, P> {
    pub fn new(runner: R, probes: P) -> Self {
        Self {
            runner,
            probes,
            providers: ProviderRegistry::new(),
            clock: Box::new(RealClock),
            observer: None,
        }
    }

    pub fn with_clock(mut self, clock: Box<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    /// Registers a step-order spy (checklist OFF-107's "spy over a fake
    /// provider"). Not part of the frozen Core Interfaces surface — a test
    /// hook only, since neither `ProcessRunner` nor `ProbeReader` alone can
    /// observe the purely-internal steps (path canonicalization, preflight,
    /// prompt building, citation resolution, snapshot comparison) that never
    /// touch either injected dependency.
    #[doc(hidden)]
    pub fn with_step_observer(
        mut self,
        observer: impl Fn(&'static str) + Send + Sync + 'static,
    ) -> Self {
        self.observer = Some(Box::new(observer));
        self
    }

    fn step(&self, name: &'static str) {
        if let Some(observer) = &self.observer {
            observer(name);
        }
    }

    fn elapsed_ms(&self, start: Instant) -> u64 {
        self.clock.now().duration_since(start).as_millis() as u64
    }

    /// Delegates the actual invocation (spec §8.1 steps 9, 11-14) to the
    /// selected `ProviderAdapter::invoke` (orchestrator correction, Task 10
    /// follow-up: `QueryService` no longer bypasses the trait — the trait's
    /// `invoke` now returns the richer [`InvokeOutcome`] the public envelope
    /// needs, so there is exactly one seam here, reused by Task 11's live
    /// doctor).
    fn invoke_provider(
        &self,
        agent: Agent,
        request: ProviderRequest,
        prompt: String,
    ) -> InvokeOutcome {
        self.providers
            .adapter(agent)
            .invoke(&self.runner, request, prompt)
    }

    /// Runs the complete spec §8.1 flow (steps 3-18; steps 1-2 are the
    /// caller's concern, see [`QueryRequest`]'s doc comment) and returns
    /// exactly one public envelope.
    ///
    /// Returns `Err` only for the failures spec §13 calls out as preceding
    /// wiki/agent resolution (`WIKI_NOT_ALLOWED`, `AGENT_UNSUPPORTED`) — the
    /// caller renders those with `wiki: null, agent: null`. Every failure
    /// from question validation onward is returned as `Ok(QueryEnvelope)`
    /// with `ok: false`, `wiki`/`agent` populated, and `error` set, because
    /// by that point both are already known from `request` itself.
    pub fn query(&self, request: QueryRequest, mode: QueryMode) -> Result<QueryEnvelope, AppError> {
        let start = self.clock.now();

        // Step 3 (spec §8.1): resolve exactly one allowlisted wiki ID and enabled provider.
        self.step("resolve_wiki_provider");
        let wiki = request.config.wikis.get(&request.wiki_id).ok_or_else(|| {
            AppError::new(
                ErrorCode::WikiNotAllowed,
                format!("wiki {:?} is not registered", request.wiki_id),
            )
        })?;
        let agent = request.agent;
        if !wiki.agents.contains(&agent) {
            return Err(AppError::new(
                ErrorCode::AgentUnsupported,
                format!("agent is not enabled for wiki {:?}", request.wiki_id),
            ));
        }
        let wiki_ref = WikiRef {
            id: request.wiki_id.clone(),
            title: wiki.title.clone(),
        };
        let provider_table: &ProviderWikiConfig = match agent {
            Agent::Claude => wiki.claude.as_ref(),
            Agent::Codex => wiki.codex.as_ref(),
        }
        .expect("Config::validate guarantees a provider table for every enabled agent");
        let executable_value = match agent {
            Agent::Claude => request
                .config
                .providers
                .claude
                .as_ref()
                .and_then(|p| p.executable.clone()),
            Agent::Codex => request
                .config
                .providers
                .codex
                .as_ref()
                .and_then(|p| p.executable.clone()),
        }
        .unwrap_or_else(|| agent_command_name(agent).to_string());

        let fail = |warnings: Vec<Warning>,
                    child_exit_code: Option<i32>,
                    raw_format: Option<RawFormat>,
                    error: AppError,
                    elapsed: u64| QueryEnvelope {
            schema_version: SCHEMA_VERSION,
            ok: false,
            operation: "query",
            wiki: Some(wiki_ref.clone()),
            agent: Some(agent),
            contract: None,
            knowledge_status: None,
            answer: None,
            citations: Vec::new(),
            gaps: Vec::new(),
            warnings,
            duration_ms: elapsed,
            child_exit_code,
            raw_format,
            error: Some(error),
        };

        // Step 4 (spec §8.1): validate the complete UTF-8 question and
        // `max_question_bytes`, strictly before any provider process starts.
        self.step("question_validate");
        let question = match String::from_utf8(request.question) {
            Ok(q) => q,
            Err(_) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(
                    Vec::new(),
                    None,
                    None,
                    AppError::new(
                        ErrorCode::QuestionInvalidUtf8,
                        "question is not valid UTF-8",
                    ),
                    elapsed,
                ));
            }
        };
        if question.len() as u64 > request.config.runtime.max_question_bytes {
            let elapsed = self.elapsed_ms(start);
            return Ok(fail(
                Vec::new(),
                None,
                None,
                AppError::new(
                    ErrorCode::QuestionTooLarge,
                    "question exceeds the configured max_question_bytes",
                ),
                elapsed,
            ));
        }

        // Step 5 (spec §8.1): canonicalize all configured paths and verify containment.
        self.step("path_canonicalize");
        let roots = match resolve_wiki_roots(&request.config_dir, wiki) {
            Ok(r) => r,
            Err(e) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(Vec::new(), None, None, e, elapsed));
            }
        };

        // Step 6 (spec §8.1): content-root minimum structure, `WIKI_SCHEMA_ABSENT`,
        // and the statically addressable project-skill/local-plugin artifacts.
        self.step("preflight");
        let markdown_files = match preflight_content_root(&roots.content_root) {
            Ok(files) => files,
            Err(e) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(Vec::new(), None, None, e, elapsed));
            }
        };
        let mut warnings = Vec::new();
        if let Some(w) = schema_absent_warning(&roots.content_root) {
            warnings.push(w);
        }
        if let Err(e) =
            resolve_and_check_artifact(&request.config_dir, &roots.project_root, provider_table)
        {
            let elapsed = self.elapsed_ms(start);
            return Ok(fail(warnings, None, None, e, elapsed));
        }
        match agent {
            Agent::Claude => {
                if let Some(w) = crate::providers::claude::read_scope_broad_warning(
                    &roots.project_root,
                    &roots.content_root,
                ) {
                    warnings.push(w);
                }
            }
            Agent::Codex => {
                warnings.push(crate::providers::codex::read_scope_broad_warning());
            }
        }

        // Step 7 (spec §8.1): resolve the provider executable; run its bounded
        // version and non-billable auth probes.
        self.step("executable_resolve_probes");
        let executable: ResolvedExecutable =
            match crate::process::resolve_executable(&executable_value, &crate::config::ProcessEnv)
            {
                Ok(exe) => exe,
                Err(e) => {
                    let elapsed = self.elapsed_ms(start);
                    return Ok(fail(warnings, None, None, e, elapsed));
                }
            };
        let adapter = self.providers.adapter(agent);
        let version = match adapter.version(&self.runner, &executable) {
            Ok(v) => v,
            Err(e) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(warnings, None, None, e, elapsed));
            }
        };
        if let Err(e) = adapter.auth_status(&self.runner, &executable) {
            let elapsed = self.elapsed_ms(start);
            return Ok(fail(warnings, None, None, e, elapsed));
        }

        // Step 8 (spec §8.1): fingerprint the selected artifact and, in
        // `Enforced` mode, require exactly one matching current probe record.
        self.step("fingerprint_probe_gate");
        if mode == QueryMode::Enforced {
            let key = ProbeKey {
                wiki_id: request.wiki_id.clone(),
                canonical_project_root: roots.project_root.display().to_string(),
                canonical_content_root: roots.content_root.display().to_string(),
                agent,
                load: provider_table.load,
                entrypoint: provider_table.entrypoint.clone(),
            };
            let gate_result = (|| -> Result<(), AppError> {
                let record = self
                    .probes
                    .current_record(&key)?
                    .ok_or_else(entrypoint_unverified)?;
                if record.agent_executable != executable.path.display().to_string() {
                    return Err(entrypoint_unverified());
                }
                if record.agent_version != version {
                    return Err(entrypoint_unverified());
                }
                let skill_dir = skill_fingerprint_dir(
                    &request.config_dir,
                    &roots.project_root,
                    provider_table,
                )?;
                let current_skill_fingerprint = compute_skill_fingerprint(&skill_dir)?;
                if record.skill_fingerprint != current_skill_fingerprint {
                    return Err(entrypoint_unverified());
                }
                let compat_input = CompatibilityFingerprintInput {
                    wiki_id: &request.wiki_id,
                    title: &wiki.title,
                    project_root: &wiki.project_root,
                    content_root: &wiki.content_root,
                    query_prompt: &wiki.query_prompt,
                    agent,
                    load: load_mode_str(provider_table.load),
                    entrypoint: &provider_table.entrypoint,
                    skill_path: provider_table.skill_path.as_deref(),
                    plugin_dir: provider_table.plugin_dir.as_deref(),
                    executable_declaration: &executable_value,
                    provider_contract_version: PROVIDER_CONTRACT_VERSION,
                };
                let current_compatibility_fingerprint =
                    compute_compatibility_fingerprint(&compat_input);
                if record.compatibility_fingerprint != current_compatibility_fingerprint {
                    return Err(entrypoint_unverified());
                }
                Ok(())
            })();
            if let Err(e) = gate_result {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(warnings, None, None, e, elapsed));
            }
        }

        // Step 9 (spec §8.1): build the fixed prompt envelope.
        self.step("prompt_build");
        let prompt = build_prompt(
            agent,
            &provider_table.entrypoint,
            &wiki.query_prompt,
            &request.wiki_id,
            &roots.content_root,
            &question,
        );

        // Step 10 (spec §8.1): the before snapshot, with content hashes.
        self.step("before_snapshot");
        let before_snapshot = match take_snapshot(&roots.content_root) {
            Ok(s) => s,
            Err(SnapshotError::Unsafe(e)) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(warnings, None, None, e, elapsed));
            }
            Err(SnapshotError::Unreadable(msg)) => {
                let elapsed = self.elapsed_ms(start);
                return Ok(fail(
                    warnings,
                    None,
                    None,
                    AppError::new(ErrorCode::InternalError, msg),
                    elapsed,
                ));
            }
        };

        // Steps 11-12 (spec §8.1): invoke the provider; timeout/output-cap/
        // process-tree enforcement happens inside the process supervisor.
        self.step("invoke");
        let plugin_dir = match provider_table.load {
            LoadMode::LocalPlugin => provider_table
                .plugin_dir
                .as_deref()
                .map(|p| join_maybe_absolute(&request.config_dir, p))
                .map(|p| fs::canonicalize(&p).unwrap_or(p)),
            LoadMode::ProjectSkill => None,
        };
        let provider_request = ProviderRequest {
            executable: executable.clone(),
            wiki_id: request.wiki_id.clone(),
            project_root: roots.project_root.clone(),
            content_root: roots.content_root.clone(),
            entrypoint: provider_table.entrypoint.clone(),
            query_prompt: wiki.query_prompt.clone(),
            question: question.clone(),
            plugin_dir,
            timeout: Duration::from_secs(request.config.runtime.timeout_seconds),
            max_stdout_bytes: request.config.runtime.max_stdout_bytes as usize,
            max_stderr_bytes: request.config.runtime.max_stderr_bytes as usize,
            temp_base: request.temp_base.clone(),
            configured_roots: vec![roots.project_root.clone(), roots.content_root.clone()],
        };
        let invoke_outcome = self.invoke_provider(agent, provider_request, prompt);
        let child_exit_code = invoke_outcome.child_exit_code;
        let raw_format = invoke_outcome.raw_format;

        // Steps 13-14 (spec §8.1): parse native output, validate `wiki-query/v1`
        // (both already performed by `invoke_provider`, via `parse_*_output`).
        self.step("parse_native_contract");
        let mut primary_error: Option<AppError> = None;
        let mut model_result: Option<ModelResult> = None;
        match invoke_outcome.model_result {
            Ok(result) => model_result = Some(result),
            Err(e) => primary_error = Some(e),
        }

        // Steps 15-16 (spec §8.1): extract/resolve citations against actual
        // pages; add the wiki namespace.
        self.step("citations_namespace");
        let mut citations: Vec<Citation> = Vec::new();
        if primary_error.is_none() {
            let result = model_result
                .as_ref()
                .expect("no primary_error implies model_result is Some");
            let stems = markdown_stems(&markdown_files);
            match resolve_and_check(
                &request.wiki_id,
                result.knowledge_status,
                &result.answer,
                &result.citations,
                &result.gaps,
                &stems,
            ) {
                Ok(resolved) => citations = resolved,
                Err(e) => primary_error = Some(e),
            }
        }

        // Step 17 (spec §8.1): recompute and compare the complete snapshot, in
        // a guaranteed cleanup path — this runs regardless of whether the
        // steps above succeeded or failed (checklist OFF-153).
        self.step("after_snapshot");
        enum AfterOutcome {
            Compared(IntegrityCheck),
            UnsafeAbort(AppError),
        }
        let after_outcome = match take_snapshot(&roots.content_root) {
            Ok(after) => {
                let changed = compare_snapshots(&before_snapshot, &after);
                if changed.is_empty() {
                    AfterOutcome::Compared(IntegrityCheck::Clean)
                } else {
                    AfterOutcome::Compared(IntegrityCheck::Violated {
                        changed_paths: changed,
                    })
                }
            }
            Err(SnapshotError::Unreadable(_)) => AfterOutcome::Compared(IntegrityCheck::Incomplete),
            Err(SnapshotError::Unsafe(e)) => AfterOutcome::UnsafeAbort(e),
        };

        // Step 18 (spec §8.1): emit exactly one public result envelope.
        self.step("emit_envelope");
        let elapsed = self.elapsed_ms(start);
        match after_outcome {
            AfterOutcome::UnsafeAbort(e) => {
                Ok(fail(warnings, child_exit_code, raw_format, e, elapsed))
            }
            AfterOutcome::Compared(integrity) => {
                if primary_error.is_none() && integrity == IntegrityCheck::Clean {
                    let result = model_result
                        .expect("clean integrity with no primary_error implies a parsed result");
                    Ok(QueryEnvelope {
                        schema_version: SCHEMA_VERSION,
                        ok: true,
                        operation: "query",
                        wiki: Some(wiki_ref),
                        agent: Some(agent),
                        contract: Some(WIKI_QUERY_CONTRACT),
                        knowledge_status: Some(result.knowledge_status),
                        answer: Some(result.answer),
                        citations,
                        gaps: result.gaps,
                        warnings: order_warnings(warnings, result.warnings),
                        duration_ms: elapsed,
                        child_exit_code,
                        raw_format,
                        error: None,
                    })
                } else {
                    let final_error = apply_integrity_dominance(primary_error, integrity)
                        .unwrap_or_else(|_| {
                            AppError::new(
                                ErrorCode::InternalError,
                                "failed to construct integrity violation details",
                            )
                        });
                    Ok(fail(
                        warnings,
                        child_exit_code,
                        raw_format,
                        final_error,
                        elapsed,
                    ))
                }
            }
        }
    }
}
