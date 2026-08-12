//! `list` and `doctor` (spec §5.1, §5.3, §15; plan Task 11).
//!
//! `run_list` never resolves a provider executable or touches
//! `ProcessRunner` at all (spec §5.1: "without starting a provider"). Static
//! `run_doctor` checks reuse the same building blocks the query flow already
//! validated against (`config::resolve_wiki_roots`,
//! `config::resolve_and_check_artifact`, `wiki::preflight_content_root`,
//! each provider's `read_scope_broad_warning`) rather than re-deriving them.
//! The live check reuses `QueryService::query(.., QueryMode::Verification)`
//! wholesale (plan Task 11 Step 6) and publishes a probe only when that call
//! reports `ok: true` — exactly the same condition the query flow itself
//! requires for success (valid native output, a valid result, resolvable
//! citations, and a clean before/after snapshot).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{
    Config, ProviderWikiConfig, ViewerBackend, WikiConfig, resolve_and_check_artifact,
    resolve_wiki_roots, validate_entrypoint,
};
use crate::error::{AppError, ErrorCode, ErrorDetails};
use crate::output::{Agent, SCHEMA_VERSION};
use crate::probes::{
    FakeProbeReader, ProbeKey, ProbeRecord, ProbeWriter, QueryMode,
    check_no_plugin_lifecycle_components, current_timestamp,
};
use crate::process::{self, ResolvedExecutable};
use crate::providers::claude::ClaudeAdapter;
use crate::providers::codex::CodexAdapter;
use crate::providers::{ProcessRunner, ProviderAdapter};
use crate::query::{
    CompatibilityFingerprintInput, PROVIDER_CONTRACT_VERSION, QueryRequest, QueryService,
    compute_compatibility_fingerprint, compute_skill_fingerprint, load_mode_str,
};
use crate::wiki::{preflight_content_root, schema_absent_warning};

// ---------------------------------------------------------------------------
// list (spec §5.1, §5.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ListWikiEntry {
    pub id: String,
    pub title: String,
    pub default_agent: Option<Agent>,
    pub agents: Vec<Agent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str,
    pub wikis: Vec<ListWikiEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

/// `llm-wikis list` (spec §5.1/§5.3): enumerates the registry without
/// resolving a provider executable or invoking `ProcessRunner` at all — no
/// provider is ever started. `default_agent` is the wiki's own value only
/// when the global default is enabled for that wiki (spec §5.1); otherwise
/// `null`.
pub fn run_list(config: &Config) -> ListEnvelope {
    let wikis = config
        .wikis
        .iter()
        .map(|(id, wiki)| {
            let default_agent = config
                .default_agent
                .filter(|default| wiki.agents.contains(default));
            ListWikiEntry {
                id: id.clone(),
                title: wiki.title.clone(),
                default_agent,
                agents: wiki.agents.clone(),
            }
        })
        .collect();
    ListEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: true,
        operation: "list",
        wikis,
        error: None,
    }
}

/// The command-level `list` failure envelope (spec §5.3: a config/argument
/// failure "adds the same top-level `error` object" and "empty `wikis`").
pub fn list_error_envelope(err: AppError) -> ListEnvelope {
    ListEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        operation: "list",
        wikis: Vec::new(),
        error: Some(err),
    }
}

// ---------------------------------------------------------------------------
// doctor: check vocabulary and envelope shape (spec §15, §5.3)
// ---------------------------------------------------------------------------

/// The fixed, minimal live-doctor question (plan Task 11 Step 9). Never the
/// caller's own question — live doctor never accepts one.
pub const LIVE_QUESTION: &str = "Verify external-readonly mode by reporting one fact from this wiki with a valid citation. If the wiki has no pages, return no_relevant_material with a gap.";

const CHECK_CONFIG: &str = "config";
const CHECK_ROOTS: &str = "roots";
const CHECK_WIKI_STRUCTURE: &str = "wiki_structure";
const CHECK_ENTRYPOINT: &str = "entrypoint";
const CHECK_EXECUTABLE: &str = "executable";
const CHECK_AUTH: &str = "auth";
const CHECK_READ_SCOPE: &str = "read_scope";
const CHECK_LIVE_CONTRACT: &str = "live_contract";
const CHECK_MUTATION: &str = "mutation";
const CHECK_VIEWER: &str = "viewer";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

/// One `checks[]` entry (spec §15): `name` is restricted to exactly the nine
/// values in [`crate::output::DOCTOR_CHECK_NAMES`]; `code` is `null` for
/// pass, a stable warning code string for warn, or one Section 14
/// [`ErrorCode`] string for fail.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub code: Option<String>,
    pub message: String,
}

impl DoctorCheck {
    fn pass(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            code: None,
            message: message.into(),
        }
    }

    fn warn(name: &'static str, code: &str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            code: Some(code.to_string()),
            message: message.into(),
        }
    }

    fn fail(name: &'static str, code: ErrorCode, message: impl Into<String>) -> Self {
        Self::fail_str(name, code.as_str(), message)
    }

    fn fail_str(name: &'static str, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            code: Some(code.into()),
            message: message.into(),
        }
    }

    fn is_fail(&self) -> bool {
        self.status == CheckStatus::Fail
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorPairResult {
    pub wiki: String,
    pub agent: Agent,
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

/// `llm-wikis --json doctor` (spec §5.3). Check `status` is `pass`, `warn`,
/// or `fail`; a pair's own `ok` (and the top-level `ok`) is `false` exactly
/// when any of its checks failed — a warn-only matrix is `ok: true`.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str,
    pub live: bool,
    pub results: Vec<DoctorPairResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

/// The command-level `doctor` failure envelope (spec §5.3): same top-level
/// `error` object, empty `results`.
pub fn doctor_error_envelope(live: bool, err: AppError) -> DoctorEnvelope {
    DoctorEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        operation: "doctor",
        live,
        results: Vec::new(),
        error: Some(err),
    }
}

pub struct DoctorRequest {
    pub config: Config,
    pub config_dir: PathBuf,
    pub wiki: Option<String>,
    pub agent: Option<Agent>,
    pub live: bool,
    pub temp_base: PathBuf,
}

// ---------------------------------------------------------------------------
// Matrix resolution (spec §5.1: default = every wiki x every enabled agent;
// `--wiki`/`--agent` narrow it; `--live` requires exactly one pair)
// ---------------------------------------------------------------------------

fn resolve_matrix(request: &DoctorRequest) -> Result<Vec<(String, Agent)>, AppError> {
    if let Some(wiki_id) = &request.wiki
        && !request.config.wikis.contains_key(wiki_id)
    {
        return Err(AppError::new(
            ErrorCode::WikiNotAllowed,
            format!("wiki {wiki_id:?} is not registered"),
        ));
    }
    let mut pairs = Vec::new();
    for (id, wiki) in &request.config.wikis {
        if let Some(filter) = &request.wiki
            && filter != id
        {
            continue;
        }
        for agent in &wiki.agents {
            if let Some(filter) = request.agent
                && filter != *agent
            {
                continue;
            }
            pairs.push((id.clone(), *agent));
        }
    }
    if request.live && pairs.len() != 1 {
        return Err(AppError::new(
            ErrorCode::ArgumentInvalid,
            "doctor --live requires exactly one selected wiki/agent pair (both --wiki and --agent)",
        ));
    }
    Ok(pairs)
}

fn adapter_for(agent: Agent) -> Box<dyn ProviderAdapter> {
    match agent {
        Agent::Claude => Box::new(ClaudeAdapter),
        Agent::Codex => Box::new(CodexAdapter),
    }
}

fn provider_table(wiki: &WikiConfig, agent: Agent) -> &ProviderWikiConfig {
    match agent {
        Agent::Claude => wiki.claude.as_ref(),
        Agent::Codex => wiki.codex.as_ref(),
    }
    .expect("resolve_matrix only yields agents this wiki actually enables")
}

fn executable_value_for(config: &Config, agent: Agent) -> String {
    let configured = match agent {
        Agent::Claude => config
            .providers
            .claude
            .as_ref()
            .and_then(|p| p.executable.clone()),
        Agent::Codex => config
            .providers
            .codex
            .as_ref()
            .and_then(|p| p.executable.clone()),
    };
    configured.unwrap_or_else(|| match agent {
        Agent::Claude => "claude".to_string(),
        Agent::Codex => "codex".to_string(),
    })
}

fn join_maybe_absolute(anchor: &Path, configured: &str) -> PathBuf {
    let p = Path::new(configured);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        anchor.join(p)
    }
}

/// Mirrors `query.rs`'s private `skill_fingerprint_dir` (spec §15.1): that
/// function is not `pub`, and this task's file boundary does not include
/// `src/query.rs`, so the same small directory resolution is reproduced here
/// (never the fingerprint hash itself, which is reused via
/// [`compute_skill_fingerprint`]) rather than reached into.
fn skill_fingerprint_dir(
    config_dir: &Path,
    project_root: &Path,
    provider: &ProviderWikiConfig,
) -> Result<PathBuf, AppError> {
    let internal = |e: std::io::Error| {
        AppError::new(
            ErrorCode::InternalError,
            format!("cannot resolve skill/plugin directory for fingerprinting: {e}"),
        )
    };
    match provider.load {
        crate::config::LoadMode::ProjectSkill => {
            let skill_path = provider
                .skill_path
                .as_deref()
                .expect("resolve_and_check_artifact validated project_skill requires skill_path");
            let joined = join_maybe_absolute(project_root, skill_path);
            let canonical_file = fs::canonicalize(&joined).map_err(internal)?;
            Ok(canonical_file
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(canonical_file))
        }
        crate::config::LoadMode::LocalPlugin => {
            let plugin_dir = provider
                .plugin_dir
                .as_deref()
                .expect("resolve_and_check_artifact validated local_plugin requires plugin_dir");
            let joined = join_maybe_absolute(config_dir, plugin_dir);
            fs::canonicalize(&joined).map_err(internal)
        }
    }
}

// ---------------------------------------------------------------------------
// Static checks (spec §15)
// ---------------------------------------------------------------------------

/// Everything a successful static pass resolves that the live check and probe
/// publish both need, carried forward so neither has to re-resolve it.
struct StaticContext {
    project_root: PathBuf,
    content_root: PathBuf,
    executable: ResolvedExecutable,
    version: String,
}

fn wiki_structure_check(content_root: &Path) -> DoctorCheck {
    match preflight_content_root(content_root) {
        Ok(_) => match schema_absent_warning(content_root) {
            Some(w) => DoctorCheck::warn(CHECK_WIKI_STRUCTURE, &w.code, w.message),
            None => DoctorCheck::pass(
                CHECK_WIKI_STRUCTURE,
                "content_root contains at least one Markdown file and a SCHEMA.md.",
            ),
        },
        Err(e) => DoctorCheck::fail(CHECK_WIKI_STRUCTURE, e.code, e.message),
    }
}

fn entrypoint_check(
    config_dir: &Path,
    project_root: &Path,
    agent: Agent,
    provider: &ProviderWikiConfig,
) -> DoctorCheck {
    if let Err(e) = validate_entrypoint(agent, &provider.entrypoint) {
        return DoctorCheck::fail(CHECK_ENTRYPOINT, e.code, e.message);
    }
    // R-29/R-31: Claude's `-p` mode loads project_root/.claude/settings*.json
    // regardless of trust or load mode; deny any executable/reach-widening
    // key before ever invoking the provider (see the function doc for the
    // full threat and the allowlist rationale). `Ok(true)` means the
    // settings passed but declared `enabledPlugins` (R-32) -- advisory, not
    // a failure; surfaced as a warning on this same check below rather than
    // failing it.
    let mut declares_enabled_plugins = false;
    if agent == Agent::Claude {
        match crate::config::check_claude_wiki_settings_surface(project_root) {
            Ok(declares) => declares_enabled_plugins = declares,
            Err(e) => return DoctorCheck::fail(CHECK_ENTRYPOINT, e.code, e.message),
        }
    }
    if provider.load == crate::config::LoadMode::LocalPlugin
        && let Some(plugin_dir_str) = provider.plugin_dir.as_deref()
    {
        let joined = join_maybe_absolute(config_dir, plugin_dir_str);
        if let Ok(canonical) = fs::canonicalize(&joined)
            && let Err(e) = check_no_plugin_lifecycle_components(&canonical)
        {
            return DoctorCheck::fail(CHECK_ENTRYPOINT, e.code, e.message);
        }
    }
    match resolve_and_check_artifact(config_dir, project_root, provider) {
        Ok(()) if declares_enabled_plugins => DoctorCheck::warn(
            CHECK_ENTRYPOINT,
            crate::output::WrapperWarningCode::ClaudeEnabledPluginsDeclared.as_str(),
            crate::config::CLAUDE_ENABLED_PLUGINS_DECLARED_MESSAGE,
        ),
        Ok(()) => DoctorCheck::pass(
            CHECK_ENTRYPOINT,
            "Configured project skill or local plugin artifact is statically addressable.",
        ),
        Err(e) => DoctorCheck::fail(CHECK_ENTRYPOINT, e.code, e.message),
    }
}

fn read_scope_check(agent: Agent, project_root: &Path, content_root: &Path) -> DoctorCheck {
    match agent {
        Agent::Claude => {
            match crate::providers::claude::read_scope_broad_warning(project_root, content_root) {
                Some(w) => DoctorCheck::warn(CHECK_READ_SCOPE, &w.code, w.message),
                None => DoctorCheck::pass(
                    CHECK_READ_SCOPE,
                    "content_root equals project_root; Claude read reach is exactly the selected content root.",
                ),
            }
        }
        Agent::Codex => {
            let w = crate::providers::codex::read_scope_broad_warning();
            DoctorCheck::warn(CHECK_READ_SCOPE, &w.code, w.message)
        }
    }
}

/// Runs every static check (spec §15) for one (wiki, agent) pair. Returns the
/// checks plus, only when every one of them passed, the resolved context a
/// live check/probe-publish for this same pair can reuse without
/// re-resolving roots/executable/version.
fn run_static_checks(
    config: &Config,
    config_dir: &Path,
    wiki_id: &str,
    agent: Agent,
    runner: &dyn ProcessRunner,
) -> (Vec<DoctorCheck>, Option<StaticContext>) {
    let mut checks = Vec::new();
    let wiki = &config.wikis[wiki_id];
    let provider = provider_table(wiki, agent);

    match config.validate() {
        Ok(()) => checks.push(DoctorCheck::pass(
            CHECK_CONFIG,
            "Configuration is valid: supported config_version, no unknown keys.",
        )),
        Err(e) => checks.push(DoctorCheck::fail(CHECK_CONFIG, e.code, e.message)),
    }

    let roots = resolve_wiki_roots(config_dir, wiki);
    match &roots {
        Ok(r) => checks.push(DoctorCheck::pass(
            CHECK_ROOTS,
            format!(
                "Canonical roots resolved and contained: project_root={}, content_root={}.",
                r.project_root.display(),
                r.content_root.display()
            ),
        )),
        Err(e) => checks.push(DoctorCheck::fail(CHECK_ROOTS, e.code, e.message.clone())),
    }

    match &roots {
        Ok(r) => {
            checks.push(wiki_structure_check(&r.content_root));
            checks.push(entrypoint_check(
                config_dir,
                &r.project_root,
                agent,
                provider,
            ));
            checks.push(read_scope_check(agent, &r.project_root, &r.content_root));
        }
        Err(e) => {
            let cascade = |name| {
                DoctorCheck::fail(
                    name,
                    e.code,
                    format!("not evaluated: roots resolution failed ({})", e.message),
                )
            };
            checks.push(cascade(CHECK_WIKI_STRUCTURE));
            checks.push(cascade(CHECK_ENTRYPOINT));
            checks.push(cascade(CHECK_READ_SCOPE));
        }
    }

    let executable_value = executable_value_for(config, agent);
    let executable_result =
        process::resolve_executable(&executable_value, &crate::config::ProcessEnv);
    let mut version: Option<String> = None;
    match &executable_result {
        Ok(exe) => {
            let adapter = adapter_for(agent);
            match adapter.version(runner, exe) {
                Ok(v) => {
                    checks.push(DoctorCheck::pass(
                        CHECK_EXECUTABLE,
                        format!(
                            "Resolved provider executable at {} (version {v}).",
                            exe.path.display()
                        ),
                    ));
                    version = Some(v);
                }
                Err(e) => checks.push(DoctorCheck::fail(CHECK_EXECUTABLE, e.code, e.message)),
            }
            match adapter.auth_status(runner, exe) {
                Ok(_) => checks.push(DoctorCheck::pass(
                    CHECK_AUTH,
                    "Provider authentication is ready.",
                )),
                Err(e) => checks.push(DoctorCheck::fail(CHECK_AUTH, e.code, e.message)),
            }
        }
        Err(e) => {
            checks.push(DoctorCheck::fail(
                CHECK_EXECUTABLE,
                e.code,
                e.message.clone(),
            ));
            checks.push(DoctorCheck::fail(
                CHECK_AUTH,
                e.code,
                format!(
                    "not evaluated: provider executable was not resolved ({})",
                    e.message
                ),
            ));
        }
    }

    let any_fail = checks.iter().any(DoctorCheck::is_fail);
    let context = match (roots.ok(), executable_result.ok(), version) {
        (Some(r), Some(exe), Some(v)) if !any_fail => Some(StaticContext {
            project_root: r.project_root,
            content_root: r.content_root,
            executable: exe,
            version: v,
        }),
        _ => None,
    };

    (checks, context)
}

// ---------------------------------------------------------------------------
// Live check + probe publish (spec §15 live checks 1-6, §15.1; plan Task 11
// Step 6)
// ---------------------------------------------------------------------------

fn publish_probe(
    probe_writer: &dyn ProbeWriter,
    config: &Config,
    config_dir: &Path,
    wiki_id: &str,
    agent: Agent,
    provider: &ProviderWikiConfig,
    context: &StaticContext,
) -> Result<(), AppError> {
    let wiki = &config.wikis[wiki_id];
    let skill_dir = skill_fingerprint_dir(config_dir, &context.project_root, provider)?;
    let skill_fingerprint = compute_skill_fingerprint(&skill_dir)?;
    let executable_declaration = executable_value_for(config, agent);
    let provider_declaration = match agent {
        Agent::Claude => config.providers.claude.as_ref(),
        Agent::Codex => config.providers.codex.as_ref(),
    };
    let compat_input = CompatibilityFingerprintInput {
        wiki_id,
        title: &wiki.title,
        project_root: &wiki.project_root,
        content_root: &wiki.content_root,
        query_prompt: &wiki.query_prompt,
        agent,
        load: load_mode_str(provider.load),
        entrypoint: &provider.entrypoint,
        skill_path: provider.skill_path.as_deref(),
        plugin_dir: provider.plugin_dir.as_deref(),
        executable_declaration: &executable_declaration,
        model_declaration: provider_declaration.and_then(|p| p.model.as_deref()),
        effort_declaration: provider_declaration.and_then(|p| p.effort.as_deref()),
        provider_contract_version: PROVIDER_CONTRACT_VERSION,
    };
    let compatibility_fingerprint = compute_compatibility_fingerprint(&compat_input);
    let key = ProbeKey {
        wiki_id: wiki_id.to_string(),
        canonical_project_root: context.project_root.display().to_string(),
        canonical_content_root: context.content_root.display().to_string(),
        agent,
        load: provider.load,
        entrypoint: provider.entrypoint.clone(),
    };
    let record = ProbeRecord {
        agent_executable: context.executable.path.display().to_string(),
        agent_version: context.version.clone(),
        skill_fingerprint,
        compatibility_fingerprint,
        verified_at: current_timestamp(),
    };
    probe_writer.publish(&key, &record)
}

/// Live doctor (spec §15 items 1-6; plan Task 11 Step 6): calls
/// `QueryService::query(.., QueryMode::Verification)` with the fixed
/// [`LIVE_QUESTION`] and publishes a probe **only** when that call reports
/// `ok: true` — which already requires valid native output, a valid result,
/// resolvable citations, and a clean before/after snapshot (exactly what
/// `QueryService`'s own success path requires). Any failure publishes
/// nothing and leaves an existing record for this logical key untouched,
/// because [`ProbeWriter::publish`] is never called on that path.
#[allow(clippy::too_many_arguments)]
fn run_live_check<R: ProcessRunner>(
    runner: R,
    probe_writer: &dyn ProbeWriter,
    config: &Config,
    config_dir: &Path,
    wiki_id: &str,
    agent: Agent,
    provider: &ProviderWikiConfig,
    context: StaticContext,
    temp_base: &Path,
) -> (DoctorCheck, DoctorCheck) {
    let request = QueryRequest {
        config: config.clone(),
        config_dir: config_dir.to_path_buf(),
        wiki_id: wiki_id.to_string(),
        agent,
        question: LIVE_QUESTION.as_bytes().to_vec(),
        temp_base: temp_base.to_path_buf(),
    };
    // Verification mode never consults the probe reader (query.rs's gate
    // only runs under `QueryMode::Enforced`), so any reader works here.
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let envelope = match service.query(request, QueryMode::Verification) {
        Ok(e) => e,
        Err(e) => {
            return (
                DoctorCheck::fail(CHECK_LIVE_CONTRACT, e.code, e.message),
                DoctorCheck::pass(
                    CHECK_MUTATION,
                    "not evaluated: the live query failed before wiki/agent resolution",
                ),
            );
        }
    };

    if envelope.ok {
        return match publish_probe(
            probe_writer,
            config,
            config_dir,
            wiki_id,
            agent,
            provider,
            &context,
        ) {
            Ok(()) => (
                DoctorCheck::pass(
                    CHECK_LIVE_CONTRACT,
                    "Live probe query returned a valid wiki-query/v1 result with resolvable citations.",
                ),
                DoctorCheck::pass(
                    CHECK_MUTATION,
                    "No content-root mutation detected across the live probe.",
                ),
            ),
            Err(e) => (
                DoctorCheck::fail(
                    CHECK_LIVE_CONTRACT,
                    e.code,
                    format!(
                        "live query succeeded but the probe could not be published: {}",
                        e.message
                    ),
                ),
                DoctorCheck::pass(
                    CHECK_MUTATION,
                    "No content-root mutation detected across the live probe.",
                ),
            ),
        };
    }

    // ok:false. `READ_ONLY_VIOLATION` dominance (spec §12/§14) means a
    // detected mutation always surfaces as the top-level error even when an
    // earlier stage also failed; that earlier failure is preserved as
    // `secondary_error` and is what actually belongs to `live_contract`.
    //
    // ponytail: an `INTERNAL_ERROR` from an *incomplete* integrity comparison
    // (the AFTER snapshot could not even be taken) is not distinguished here
    // from any other `INTERNAL_ERROR` cause and surfaces as a `live_contract`
    // failure rather than a `mutation` one — neither implies a *confirmed*
    // mutation, so this task does not add message-text sniffing to tell them
    // apart. Revisit only if a real doctor run needs that distinction.
    let err = envelope
        .error
        .expect("ok:false envelope always carries error");
    if err.code == ErrorCode::ReadOnlyViolation {
        let secondary = err.details.as_ref().and_then(|d| match d {
            ErrorDetails::ReadOnlyViolation(rv) => rv.secondary_error.as_ref(),
            _ => None,
        });
        let live_contract = match secondary {
            Some(sec) => DoctorCheck::fail(CHECK_LIVE_CONTRACT, sec.code, sec.message.clone()),
            None => DoctorCheck::pass(
                CHECK_LIVE_CONTRACT,
                "Live probe query returned a valid wiki-query/v1 result with resolvable citations.",
            ),
        };
        (
            live_contract,
            DoctorCheck::fail(CHECK_MUTATION, err.code, err.message),
        )
    } else {
        (
            DoctorCheck::fail(CHECK_LIVE_CONTRACT, err.code, err.message),
            DoctorCheck::pass(
                CHECK_MUTATION,
                "No confirmed content-root mutation; the live check failed for a different reason.",
            ),
        )
    }
}

/// The `viewer` check (issue #8; task 08-12 design.md §3). Unlike every other
/// check here this one is a property of the installation, not of a
/// (wiki, agent) pair, so it is computed **once per doctor run** and the same
/// result is cloned into each pair's `checks` — probing once per pair would
/// spawn the viewer N times to learn the same fact.
///
/// A missing viewer is `warn`, never `fail`. Compare the `executable` check:
/// without a provider CLI there is no answer at all, but without a viewer the
/// answer is complete and correct and only its presentation degrades to raw
/// markdown. Failing here would push `doctor` to a non-zero exit
/// (`dominant_exit`) and break every script that gates on it, over a purely
/// cosmetic component.
fn viewer_check(config: &Config) -> DoctorCheck {
    if config.viewer.backend == ViewerBackend::Plain {
        return DoctorCheck::pass(
            CHECK_VIEWER,
            "Viewer backend is \"plain\": answers print as raw markdown and no external viewer is needed.",
        );
    }
    let unavailable = |detail: String| {
        DoctorCheck::warn(
            CHECK_VIEWER,
            crate::output::WrapperWarningCode::ViewerUnavailable.as_str(),
            format!("{detail} Answers will print as raw markdown until this is resolved."),
        )
    };
    let viewer = match crate::viewer::Viewer::resolve(&config.viewer, &crate::config::ProcessEnv) {
        Ok(v) => v,
        Err(e) => return unavailable(format!("Markdown viewer not found: {}.", e.message)),
    };
    match viewer.probe() {
        Ok(()) => DoctorCheck::pass(
            CHECK_VIEWER,
            format!(
                "Markdown viewer at {} renders inline output.",
                viewer.path().display()
            ),
        ),
        // The probe renders a document rather than asking for a version
        // string on purpose: `--inline` only exists in newer viewer releases,
        // and an older binary answers `--version` happily before failing at
        // the first real query.
        Err(e) => unavailable(format!(
            "Markdown viewer at {} cannot render inline output: {}.",
            viewer.path().display(),
            e.message
        )),
    }
}

// ---------------------------------------------------------------------------
// Top-level doctor entry point
// ---------------------------------------------------------------------------

/// `llm-wikis doctor` (spec §5.1, §5.3, §15). Runs every selected static
/// check for every selected (wiki, agent) pair before ever attempting the
/// live check (plan Task 11 Step 5/OFF-030) — the live check, when
/// requested, only runs for the single resolved pair `--live` requires, and
/// only after that pair's own static checks all passed.
pub fn run_doctor<R: ProcessRunner>(
    request: DoctorRequest,
    runner: R,
    probe_writer: &dyn ProbeWriter,
) -> DoctorEnvelope {
    let pairs = match resolve_matrix(&request) {
        Ok(p) => p,
        Err(e) => return doctor_error_envelope(request.live, e),
    };

    let mut results: Vec<DoctorPairResult> = Vec::new();
    let mut live_context: Option<StaticContext> = None;
    // Once per run, before the pair loop — see `viewer_check`'s doc comment.
    let viewer = viewer_check(&request.config);
    for (wiki_id, agent) in &pairs {
        let (mut checks, context) = run_static_checks(
            &request.config,
            &request.config_dir,
            wiki_id,
            *agent,
            &runner,
        );
        // Appended last so the nine pre-existing checks keep their positions.
        checks.push(viewer.clone());
        if request.live {
            live_context = context;
        }
        let ok = !checks.iter().any(DoctorCheck::is_fail);
        results.push(DoctorPairResult {
            wiki: wiki_id.clone(),
            agent: *agent,
            ok,
            checks,
        });
    }

    if request.live {
        // `resolve_matrix` guarantees exactly one pair whenever `live` is set.
        let (wiki_id, agent) = pairs[0].clone();
        let (live_contract, mutation) = match live_context {
            Some(context) => {
                let provider = provider_table(&request.config.wikis[&wiki_id], agent).clone();
                run_live_check(
                    runner,
                    probe_writer,
                    &request.config,
                    &request.config_dir,
                    &wiki_id,
                    agent,
                    &provider,
                    context,
                    &request.temp_base,
                )
            }
            None => {
                let pair = &results[0];
                let first_failure = pair.checks.iter().find(|c| c.is_fail());
                let (code, name) = match first_failure {
                    Some(c) => (
                        c.code
                            .clone()
                            .unwrap_or_else(|| ErrorCode::InternalError.as_str().to_string()),
                        c.name,
                    ),
                    None => (ErrorCode::InternalError.as_str().to_string(), "unknown"),
                };
                let message = format!(
                    "static checks failed; live check was not attempted (see the {name} check)"
                );
                (
                    DoctorCheck::fail_str(CHECK_LIVE_CONTRACT, code.clone(), message.clone()),
                    DoctorCheck::fail_str(CHECK_MUTATION, code, message),
                )
            }
        };
        let pair = &mut results[0];
        pair.checks.push(live_contract);
        pair.checks.push(mutation);
        pair.ok = !pair.checks.iter().any(DoctorCheck::is_fail);
    }

    let ok = !results.iter().any(|r| !r.ok);
    DoctorEnvelope {
        schema_version: SCHEMA_VERSION,
        ok,
        operation: "doctor",
        live: request.live,
        results,
        error: None,
    }
}
