//! Static and live doctor tests (spec §15, §5.3; plan Task 11 Steps 5-6).
//!
//! Every test runs against [`FakeProcessRunner`]/an in-memory or tempdir
//! [`llm_wikis::probes::FileProbeStore`] — no real Claude/Codex process ever
//! starts and no model quota is spent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use llm_wikis::config::{
    Config, LoadMode, ProviderConfig, ProviderWikiConfig, ProvidersConfig, RuntimeConfig,
    WikiConfig,
};
use llm_wikis::doctor::{CheckStatus, DoctorRequest, run_doctor};
use llm_wikis::error::AppError;
use llm_wikis::output::{Agent, DOCTOR_CHECK_NAMES};
use llm_wikis::probes::{FileProbeStore, ProbeKey, ProbeReader, ProbeRecord};
use llm_wikis::process::ProcessRequest;
use llm_wikis::providers::{FakeProcessRunner, ProcessRunner, completed_outcome};

const WIKI_ID: &str = "harness-engineering";
const ENTRYPOINT_CLAUDE: &str = "/wiki-query";
const ENTRYPOINT_CODEX: &str = "$wiki-query";
const SKILL_RELATIVE: &str = ".claude/skills/wiki-query/SKILL.md";
const QUERY_PROMPT: &str = "Use the wiki-query skill to answer from this wiki.";

// ---------------------------------------------------------------------------
// Fixture construction (mirrors tests/query_service.rs's conventions)
// ---------------------------------------------------------------------------

/// A disposable directory rooted under this crate's own `target/` -- not
/// `tempfile::tempdir()` (system temp). Two independent, CI-observed classes
/// of breakage come from system temp specifically: on macOS, `$TMPDIR`
/// resolves under `/var/folders/...`, and `/var` itself is a symlink to
/// `/private/var` -- `joined_checked`'s per-component special-entry scan
/// (spec §6.1: "any component ... even when its target would remain
/// contained") then correctly rejects the *configured* (uncanonicalized)
/// `project_root`/`content_root` string with `UNSAFE_FILESYSTEM_ENTRY`. On a
/// Windows CI runner, the observed failure is a config-check
/// `CONFIG_INVALID: provider executable must not contain shell
/// metacharacters` on a subset of runs -- consistent with the same class of
/// runner-temp aliasing already confirmed for `RUNNER~1`-style short names
/// elsewhere in this suite (tests/cli_contract.rs's own `LocalTempDir`),
/// even though this session could not pin the exact substring. Rooting
/// under `target/` (a stable path fixed at compile time, never resolved
/// through `%TEMP%`/`$TMPDIR`) sidesteps both classes without needing
/// per-platform special-casing. Removed on drop.
struct LocalTempDir {
    path: PathBuf,
}

impl LocalTempDir {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("doctor-test-tmp")
            .join(format!("{label}-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LocalTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Fixture {
    _tmp: LocalTempDir,
    project_root: PathBuf,
    content_root: PathBuf,
    skill_dir: PathBuf,
    executable_path: PathBuf,
    temp_base: PathBuf,
    probe_store_path: PathBuf,
}

fn build_fixture() -> Fixture {
    let tmp = LocalTempDir::new("fixture");
    let project_root = tmp.path().join("project");
    let skill_dir = project_root.join(".claude/skills/wiki-query");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), b"skill body").unwrap();
    fs::write(project_root.join("SCHEMA.md"), b"# schema\n").unwrap();
    let pages_dir = project_root.join("wiki/pages");
    fs::create_dir_all(&pages_dir).unwrap();
    fs::write(
        pages_dir.join("harness-engineering.md"),
        b"# Harness Engineering\n",
    )
    .unwrap();

    let temp_base = tmp.path().join("temp-base");
    fs::create_dir_all(&temp_base).unwrap();

    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let executable_path = bin_dir.join("fake-provider");
    fs::write(&executable_path, b"placeholder, never executed").unwrap();

    let probe_store_path = tmp.path().join("cache/llm-wikis/probes-v1.json");

    Fixture {
        _tmp: tmp,
        project_root: project_root.clone(),
        content_root: project_root,
        skill_dir,
        executable_path,
        temp_base,
        probe_store_path,
    }
}

fn entrypoint_for(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => ENTRYPOINT_CLAUDE,
        Agent::Codex => ENTRYPOINT_CODEX,
    }
}

fn build_config(fixture: &Fixture, agents: Vec<Agent>) -> Config {
    let mut wiki = WikiConfig {
        title: "Harness Engineering".to_string(),
        project_root: fixture.project_root.display().to_string(),
        content_root: fixture.content_root.display().to_string(),
        agents: agents.clone(),
        query_prompt: QUERY_PROMPT.to_string(),
        claude: None,
        codex: None,
    };
    let mut providers = ProvidersConfig {
        claude: None,
        codex: None,
    };
    for agent in &agents {
        let provider_wiki = ProviderWikiConfig {
            load: LoadMode::ProjectSkill,
            entrypoint: entrypoint_for(*agent).to_string(),
            skill_path: Some(SKILL_RELATIVE.to_string()),
            plugin_dir: None,
        };
        let provider_cfg = Some(ProviderConfig {
            executable: Some(fixture.executable_path.display().to_string()),
            model: None,
            effort: None,
        });
        match agent {
            Agent::Claude => {
                wiki.claude = Some(provider_wiki);
                providers.claude = provider_cfg;
            }
            Agent::Codex => {
                wiki.codex = Some(provider_wiki);
                providers.codex = provider_cfg;
            }
        }
    }
    let mut wikis = BTreeMap::new();
    wikis.insert(WIKI_ID.to_string(), wiki);

    Config {
        config_version: 1,
        default_agent: agents.first().copied(),
        providers,
        runtime: RuntimeConfig::default(),
        wikis,
    }
}

fn doctor_request(
    fixture: &Fixture,
    config: Config,
    wiki: Option<&str>,
    agent: Option<Agent>,
    live: bool,
) -> DoctorRequest {
    DoctorRequest {
        config,
        config_dir: fixture.project_root.clone(),
        wiki: wiki.map(str::to_string),
        agent,
        live,
        temp_base: fixture.temp_base.clone(),
    }
}

fn queue_success_probes(runner: &FakeProcessRunner, agent: Agent) {
    match agent {
        Agent::Claude => {
            runner.push_response(Ok(completed_outcome(b"2.1.220\n", b"", 0)));
            runner.push_response(Ok(completed_outcome(br#"{"authenticated":true}"#, b"", 0)));
        }
        Agent::Codex => {
            runner.push_response(Ok(completed_outcome(b"0.45.0\n", b"", 0)));
            runner.push_response(Ok(completed_outcome(
                b"Logged in as test@example.com\n",
                b"",
                0,
            )));
        }
    }
}

/// A `--live` run resolves the provider executable and probes version/auth
/// twice: once for doctor's own static `executable`/`auth` checks, and again
/// inside `QueryService::query`'s own step 7 (`run_live_check` reuses the
/// whole query flow wholesale rather than a leaner live-only path).
fn queue_live_success_probes(runner: &FakeProcessRunner, agent: Agent) {
    queue_success_probes(runner, agent);
    queue_success_probes(runner, agent);
}

fn claude_success_stdout(answer: &str) -> Vec<u8> {
    let body = serde_json::json!({
        "subtype": "success",
        "structured_output": {
            "contract": "wiki-query/v1",
            "knowledge_status": "grounded",
            "answer": answer,
            "citations": ["harness-engineering"],
            "gaps": [],
            "warnings": []
        }
    });
    serde_json::to_vec(&body).unwrap()
}

fn find_check<'a>(
    envelope: &'a llm_wikis::doctor::DoctorEnvelope,
    wiki: &str,
    agent: Agent,
    name: &str,
) -> Option<&'a llm_wikis::doctor::DoctorCheck> {
    envelope
        .results
        .iter()
        .find(|r| r.wiki == wiki && r.agent == agent)
        .and_then(|r| r.checks.iter().find(|c| c.name == name))
}

fn probe_store(fixture: &Fixture) -> FileProbeStore {
    FileProbeStore::new(fixture.probe_store_path.clone())
}

/// Builds a `local_plugin` wiki config (spec §6.2) rooted at
/// `fixture.project_root/plugin`, optionally declaring a rejected lifecycle
/// component (a `hooks/` directory) alongside a clean manifest.
fn build_local_plugin_config(fixture: &Fixture, agent: Agent, reject: bool) -> Config {
    let plugin_dir = fixture.project_root.join("plugin");
    let manifest_dir = plugin_dir.join(".claude-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        br#"{"name":"wiki-query-plugin","version":"1.0.0"}"#,
    )
    .unwrap();
    let skills_dir = plugin_dir.join("skills/wiki-query");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::write(skills_dir.join("SKILL.md"), b"# Wiki Query\nplugin body").unwrap();
    if reject {
        fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
    }

    let provider_wiki = ProviderWikiConfig {
        load: LoadMode::LocalPlugin,
        entrypoint: entrypoint_for(agent).to_string(),
        skill_path: Some("skills/wiki-query/SKILL.md".to_string()),
        plugin_dir: Some("plugin".to_string()),
    };
    let mut wiki = WikiConfig {
        title: "Harness Engineering".to_string(),
        project_root: fixture.project_root.display().to_string(),
        content_root: fixture.content_root.display().to_string(),
        agents: vec![agent],
        query_prompt: QUERY_PROMPT.to_string(),
        claude: None,
        codex: None,
    };
    match agent {
        Agent::Claude => wiki.claude = Some(provider_wiki),
        Agent::Codex => wiki.codex = Some(provider_wiki),
    }
    let mut wikis = BTreeMap::new();
    wikis.insert(WIKI_ID.to_string(), wiki);
    let provider_cfg = Some(ProviderConfig {
        executable: Some(fixture.executable_path.display().to_string()),
        model: None,
        effort: None,
    });
    let mut providers = ProvidersConfig {
        claude: None,
        codex: None,
    };
    match agent {
        Agent::Claude => providers.claude = provider_cfg,
        Agent::Codex => providers.codex = provider_cfg,
    }
    Config {
        config_version: 1,
        default_agent: Some(agent),
        providers,
        runtime: RuntimeConfig::default(),
        wikis,
    }
}

// ---------------------------------------------------------------------------
// OFF-156/OFF-191: local plugin lifecycle-component rejection (Phase 2-scope)
// ---------------------------------------------------------------------------

#[test]
fn plugin_lifecycle_rejected() {
    let fixture = build_fixture();
    let config = build_local_plugin_config(&fixture, Agent::Claude, true);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("ENTRYPOINT_INVALID"));
}

#[test]
fn plugin_clean_shape_passes_entrypoint_check() {
    let fixture = build_fixture();
    let config = build_local_plugin_config(&fixture, Agent::Claude, false);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Pass);
}

// ---------------------------------------------------------------------------
// OFF-195 / OFF-196: check-name vocabulary and code semantics
// ---------------------------------------------------------------------------

#[test]
fn check_name_vocabulary() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);

    let vocabulary: std::collections::BTreeSet<&str> = DOCTOR_CHECK_NAMES.iter().copied().collect();
    assert_eq!(vocabulary.len(), 9);
    assert!(!vocabulary.contains("profile"));
    assert!(!vocabulary.contains("index_freshness"));

    for result in &envelope.results {
        for check in &result.checks {
            assert!(
                vocabulary.contains(check.name),
                "check name {:?} is not in the 9-value vocabulary",
                check.name
            );
        }
    }
    // Static-only doctor never emits live_contract/mutation.
    assert!(
        !envelope.results[0]
            .checks
            .iter()
            .any(|c| c.name == "live_contract")
    );
    assert!(
        !envelope.results[0]
            .checks
            .iter()
            .any(|c| c.name == "mutation")
    );
}

#[test]
fn code_semantics() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);

    for result in &envelope.results {
        for check in &result.checks {
            match check.status {
                CheckStatus::Pass => assert_eq!(
                    check.code, None,
                    "pass check {:?} must carry code:null",
                    check.name
                ),
                CheckStatus::Warn | CheckStatus::Fail => {
                    assert!(
                        check.code.is_some(),
                        "{:?} check must carry a code",
                        check.name
                    )
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OFF-015/016: default matrix and narrowing
// ---------------------------------------------------------------------------

#[test]
fn default_matrix_covers_every_configured_pair() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude, Agent::Codex]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    queue_success_probes(&runner, Agent::Codex);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    assert_eq!(envelope.results.len(), 2);
    let agents: Vec<Agent> = envelope.results.iter().map(|r| r.agent).collect();
    assert!(agents.contains(&Agent::Claude));
    assert!(agents.contains(&Agent::Codex));
}

#[test]
fn wiki_and_agent_narrow_the_matrix() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude, Agent::Codex]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), false);
    let envelope = run_doctor(request, runner, &store);
    assert_eq!(envelope.results.len(), 1);
    assert_eq!(envelope.results[0].wiki, WIKI_ID);
    assert_eq!(envelope.results[0].agent, Agent::Claude);
}

// ---------------------------------------------------------------------------
// OFF-027/230: pass/warn/fail semantics for overall `ok`
// ---------------------------------------------------------------------------

#[test]
fn warn_only_matrix_is_ok_true() {
    // No SCHEMA.md at content_root -> WIKI_SCHEMA_ABSENT warn, nothing else fails.
    let fixture = build_fixture();
    fs::remove_file(fixture.content_root.join("SCHEMA.md")).unwrap();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);

    assert!(envelope.ok, "a warn-only matrix must still be ok:true");
    assert!(envelope.results[0].ok);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "wiki_structure").unwrap();
    assert_eq!(check.status, CheckStatus::Warn);
    assert_eq!(check.code.as_deref(), Some("WIKI_SCHEMA_ABSENT"));
}

#[test]
fn any_fail_makes_overall_ok_false() {
    let fixture = build_fixture();
    // Break the entrypoint syntax directly on the in-memory Config (bypasses
    // Config::load's own strict validation on purpose, per plan Task 11 Step
    // 5 — doctor performs its own explicit static checks).
    let mut config = build_config(&fixture, vec![Agent::Claude]);
    config
        .wikis
        .get_mut(WIKI_ID)
        .unwrap()
        .claude
        .as_mut()
        .unwrap()
        .entrypoint = "not-a-valid-entrypoint".to_string();
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);

    assert!(!envelope.ok);
    assert!(!envelope.results[0].ok);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("ENTRYPOINT_INVALID"));
}

// ---------------------------------------------------------------------------
// OFF-184..194: individual static-check fixtures
// ---------------------------------------------------------------------------

#[test]
fn config_check_fails_on_invalid_config_version() {
    let fixture = build_fixture();
    let mut config = build_config(&fixture, vec![Agent::Claude]);
    config.config_version = 2;
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "config").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("CONFIG_INVALID"));
}

#[test]
fn roots_check_passes_for_equal_and_strict_subdir_roots() {
    // Equal roots (this fixture's default).
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);
    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "roots").unwrap();
    assert_eq!(check.status, CheckStatus::Pass);

    // Strict subdirectory content_root.
    let fixture2 = build_fixture();
    let mut config2 = build_config(&fixture2, vec![Agent::Claude]);
    config2.wikis.get_mut(WIKI_ID).unwrap().content_root =
        fixture2.project_root.join("wiki").display().to_string();
    let runner2 = FakeProcessRunner::new();
    queue_success_probes(&runner2, Agent::Claude);
    let store2 = probe_store(&fixture2);
    let request2 = doctor_request(&fixture2, config2, None, None, false);
    let envelope2 = run_doctor(request2, runner2, &store2);
    let check2 = find_check(&envelope2, WIKI_ID, Agent::Claude, "roots").unwrap();
    assert_eq!(check2.status, CheckStatus::Pass);
    // Strict subdir also triggers CLAUDE_READ_SCOPE_BROAD.
    let read_scope2 = find_check(&envelope2, WIKI_ID, Agent::Claude, "read_scope").unwrap();
    assert_eq!(read_scope2.status, CheckStatus::Warn);
    assert_eq!(read_scope2.code.as_deref(), Some("CLAUDE_READ_SCOPE_BROAD"));
}

#[test]
fn wiki_structure_check_fails_on_empty_content_root() {
    let fixture = build_fixture();
    // Must stay contained by project_root (else "roots" fails first with
    // PATH_OUTSIDE_ALLOWED_ROOT rather than reaching "wiki_structure").
    let empty_root = fixture.project_root.join("empty-content");
    fs::create_dir_all(&empty_root).unwrap();
    let mut config = build_config(&fixture, vec![Agent::Claude]);
    config.wikis.get_mut(WIKI_ID).unwrap().content_root = empty_root.display().to_string();
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "wiki_structure").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("WIKI_INVALID"));
}

#[test]
fn config_check_fails_on_missing_query_prompt() {
    let fixture = build_fixture();
    let mut config = build_config(&fixture, vec![Agent::Claude]);
    config.wikis.get_mut(WIKI_ID).unwrap().query_prompt = String::new();
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "config").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("CONFIG_INVALID"));
}

#[test]
fn entrypoint_check_passes_for_real_registered_shape() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude, Agent::Codex]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    queue_success_probes(&runner, Agent::Codex);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    for agent in [Agent::Claude, Agent::Codex] {
        let check = find_check(&envelope, WIKI_ID, agent, "entrypoint").unwrap();
        assert_eq!(check.status, CheckStatus::Pass, "agent {agent:?}");
    }
}

// ---------------------------------------------------------------------------
// R-29: Claude wiki-side .claude/settings.json surface (PR #1 Codex review
// iteration 3 finding 1). Mirrors plugin_lifecycle_rejected above -- same
// check name ("entrypoint"), same closed ENTRYPOINT_INVALID code, applied to
// the wiki's own project settings this time instead of a local plugin's.
// ---------------------------------------------------------------------------

#[test]
fn claude_wiki_settings_declaring_a_forbidden_key_fails_entrypoint_check() {
    let fixture = build_fixture();
    fs::write(
        fixture.project_root.join(".claude/settings.json"),
        br#"{"apiKeyHelper":"echo hooked"}"#,
    )
    .unwrap();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("ENTRYPOINT_INVALID"));
}

#[test]
fn claude_wiki_settings_with_only_enabled_plugins_still_succeeds_with_the_r32_advisory_warning_matching_the_real_harness_engineering_wiki()
 {
    // R-32: enabledPlugins is still admitted (an explicit, evidence-backed
    // risk acceptance, not denied) -- so the real harness-engineering wiki's
    // settings.local.json keeps working -- but doctor now surfaces
    // CLAUDE_ENABLED_PLUGINS_DECLARED as a warning on the same "entrypoint"
    // check, not a silent pass, so an operator is told plainly.
    let fixture = build_fixture();
    fs::write(
        fixture.project_root.join(".claude/settings.local.json"),
        br#"{"enabledPlugins":{"llm-wiki@llm-wiki":true}}"#,
    )
    .unwrap();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Warn);
    assert_eq!(
        check.code.as_deref(),
        Some("CLAUDE_ENABLED_PLUGINS_DECLARED")
    );
}

#[test]
fn codex_pair_is_unaffected_by_a_forbidden_claude_wiki_settings_key() {
    // The settings-surface check is Claude-specific (Codex does not load
    // project_root/.claude/settings.json at all); a forbidden key there must
    // not fail the Codex pair's own entrypoint check.
    let fixture = build_fixture();
    fs::write(
        fixture.project_root.join(".claude/settings.json"),
        br#"{"apiKeyHelper":"echo hooked"}"#,
    )
    .unwrap();
    let config = build_config(&fixture, vec![Agent::Codex]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Codex);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Codex, "entrypoint").unwrap();
    assert_eq!(check.status, CheckStatus::Pass);
}

#[test]
fn executable_check_reports_canonical_path_and_version_without_leaking_env() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "executable").unwrap();
    assert_eq!(check.status, CheckStatus::Pass);
    let canonical = fs::canonicalize(&fixture.executable_path).unwrap();
    assert!(check.message.contains(&canonical.display().to_string()));
    assert!(check.message.contains("2.1.220"));
    assert!(!check.message.to_uppercase().contains("PATH="));
}

#[test]
fn auth_check_fails_on_logged_out_status() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    runner.push_response(Ok(completed_outcome(b"2.1.220\n", b"", 0)));
    runner.push_response(Ok(completed_outcome(br#"{"authenticated":false}"#, b"", 0)));
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "auth").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("AUTH_REQUIRED"));
}

#[test]
fn read_scope_check_absent_when_roots_equal_present_for_codex_always() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude, Agent::Codex]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    queue_success_probes(&runner, Agent::Codex);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);

    let claude_check = find_check(&envelope, WIKI_ID, Agent::Claude, "read_scope").unwrap();
    assert_eq!(
        claude_check.status,
        CheckStatus::Pass,
        "roots are equal, so no warning"
    );

    let codex_check = find_check(&envelope, WIKI_ID, Agent::Codex, "read_scope").unwrap();
    assert_eq!(
        codex_check.status,
        CheckStatus::Warn,
        "codex warning is unconditional"
    );
    assert_eq!(codex_check.code.as_deref(), Some("CODEX_READ_SCOPE_BROAD"));
    assert_eq!(
        codex_check.message,
        "Codex read-only sandbox prevents writes but does not limit reads to the selected wiki; use an OS sandbox or container for strict confidentiality."
    );
}

#[test]
fn claude_read_scope_broad_exact_message() {
    let fixture = build_fixture();
    let mut config = build_config(&fixture, vec![Agent::Claude]);
    config.wikis.get_mut(WIKI_ID).unwrap().content_root =
        fixture.project_root.join("wiki").display().to_string();
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "read_scope").unwrap();
    assert_eq!(check.status, CheckStatus::Warn);
    assert_eq!(
        check.message,
        "Claude read tools can inspect the configured project root, not only the selected content root; use an OS sandbox or container for stricter confidentiality."
    );
}

#[test]
fn wiki_schema_absent_exact_message() {
    let fixture = build_fixture();
    fs::remove_file(fixture.content_root.join("SCHEMA.md")).unwrap();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, false);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "wiki_structure").unwrap();
    assert_eq!(check.status, CheckStatus::Warn);
    assert_eq!(
        check.message,
        "No SCHEMA.md at the content root; most wiki toolchains place one there. Confirm content_root points at the wiki root rather than a parent or child directory."
    );
}

// ---------------------------------------------------------------------------
// OFF-030: static checks precede live checks
// ---------------------------------------------------------------------------

/// A [`ProcessRunner`] that records the order in which it is called relative
/// to a live-check marker set once the live query itself starts.
struct OrderSpyRunner {
    inner: FakeProcessRunner,
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl ProcessRunner for OrderSpyRunner {
    fn run(&self, request: ProcessRequest) -> Result<llm_wikis::process::ProcessOutcome, AppError> {
        self.order.lock().unwrap().push("process_call");
        self.inner.run(request)
    }
}

#[test]
fn static_before_live() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let inner = FakeProcessRunner::new();
    // 2 static probes (version, auth) + 2 more inside QueryService's own
    // step 7 + 1 live invocation.
    queue_live_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let order = Arc::new(Mutex::new(Vec::new()));
    let runner = OrderSpyRunner {
        inner,
        order: Arc::clone(&order),
    };
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);

    assert!(envelope.live);
    // All three process calls happened (version, auth, then the live
    // invocation) — the static checks' own calls (version/auth) necessarily
    // precede the live invocation because `run_static_checks` runs to
    // completion before `run_live_check` is ever called.
    assert_eq!(order.lock().unwrap().len(), 5);
    let live_contract = find_check(&envelope, WIKI_ID, Agent::Claude, "live_contract").unwrap();
    assert_eq!(live_contract.status, CheckStatus::Pass, "{envelope:?}");
}

// ---------------------------------------------------------------------------
// OFF-017/231: --live matrix + live_contract/mutation emission
// ---------------------------------------------------------------------------

#[test]
fn live_requires_exactly_one_pair() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude, Agent::Codex]);
    let runner = FakeProcessRunner::new();
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, None, None, true);
    let envelope = run_doctor(request, runner, &store);
    assert!(!envelope.ok);
    assert_eq!(envelope.results.len(), 0);
    let error = envelope.error.expect("command-level failure carries error");
    assert_eq!(error.code.as_str(), "ARGUMENT_INVALID");
}

#[test]
fn live_json_emits_live_contract_and_mutation_entries() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_live_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);

    let value = serde_json::to_value(&envelope).unwrap();
    let checks = value["results"][0]["checks"].as_array().unwrap();
    let names: Vec<&str> = checks.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"live_contract"));
    assert!(names.contains(&"mutation"));
}

// ---------------------------------------------------------------------------
// OFF-198..203: live check steps
// ---------------------------------------------------------------------------

#[test]
fn live_step1_invokes_entrypoint() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let inner = Arc::new(FakeProcessRunner::new());
    queue_live_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));

    struct SharedRunner(Arc<FakeProcessRunner>);
    impl ProcessRunner for SharedRunner {
        fn run(
            &self,
            request: ProcessRequest,
        ) -> Result<llm_wikis::process::ProcessOutcome, AppError> {
            self.0.run(request)
        }
    }

    let store = probe_store(&fixture);
    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let handle = Arc::clone(&inner);
    let envelope = run_doctor(request, SharedRunner(inner), &store);
    assert!(envelope.ok, "{envelope:?}");

    let captured = handle.captured_requests();
    let invoke = captured.last().expect("live invocation captured");
    let stdin_text = String::from_utf8_lossy(&invoke.stdin);
    assert!(stdin_text.starts_with(ENTRYPOINT_CLAUDE));
    assert!(stdin_text.contains(llm_wikis::doctor::LIVE_QUESTION));
}

#[test]
fn live_step2_requires_contract() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_live_success_probes(&runner, Agent::Claude);
    // Non-conforming: valid JSON, but not the wiki-query/v1 shape at all.
    runner.push_response(Ok(completed_outcome(
        br#"{"subtype":"success","structured_output":{"totally":"wrong-shape"}}"#,
        b"",
        0,
    )));
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);
    assert!(!envelope.ok);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "live_contract").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    // No probe was published.
    let store2 = probe_store(&fixture);
    let key = ProbeKey {
        wiki_id: WIKI_ID.to_string(),
        canonical_project_root: fs::canonicalize(&fixture.project_root)
            .unwrap()
            .display()
            .to_string(),
        canonical_content_root: fs::canonicalize(&fixture.content_root)
            .unwrap()
            .display()
            .to_string(),
        agent: Agent::Claude,
        load: LoadMode::ProjectSkill,
        entrypoint: ENTRYPOINT_CLAUDE.to_string(),
    };
    assert_eq!(store2.current_record(&key).unwrap(), None);
}

#[test]
fn live_step3_validates_both_native_and_normalized() {
    // Malformed native JSON: fails at the native-parse stage.
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_live_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(b"{ not json", b"", 0)));
    let store = probe_store(&fixture);
    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);
    let check = find_check(&envelope, WIKI_ID, Agent::Claude, "live_contract").unwrap();
    assert_eq!(check.status, CheckStatus::Fail);
    assert_eq!(check.code.as_deref(), Some("INVALID_NATIVE_OUTPUT"));

    // Well-formed native JSON, but violates the wiki-query/v1 contract rules
    // (grounded with no citations).
    let fixture2 = build_fixture();
    let config2 = build_config(&fixture2, vec![Agent::Claude]);
    let runner2 = FakeProcessRunner::new();
    queue_live_success_probes(&runner2, Agent::Claude);
    let bad_contract = serde_json::json!({
        "subtype": "success",
        "structured_output": {
            "contract": "wiki-query/v1",
            "knowledge_status": "grounded",
            "answer": "An answer with no citations.",
            "citations": [],
            "gaps": [],
            "warnings": []
        }
    });
    runner2.push_response(Ok(completed_outcome(
        &serde_json::to_vec(&bad_contract).unwrap(),
        b"",
        0,
    )));
    let store2 = probe_store(&fixture2);
    let request2 = doctor_request(&fixture2, config2, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope2 = run_doctor(request2, runner2, &store2);
    let check2 = find_check(&envelope2, WIKI_ID, Agent::Claude, "live_contract").unwrap();
    assert_eq!(check2.status, CheckStatus::Fail);
    assert_eq!(check2.code.as_deref(), Some("CONTRACT_VIOLATION"));
}

/// Mutates a fixture file the instant `run` is called for the third time
/// (the two probes, then the real live invocation) — landing the mutation
/// between the before- and after-snapshot, mirroring
/// `tests/query_service.rs`'s `MutatingRunner`.
struct MutatingRunner {
    inner: FakeProcessRunner,
    mutate: PathBuf,
    calls: Mutex<u32>,
}

impl ProcessRunner for MutatingRunner {
    fn run(&self, request: ProcessRequest) -> Result<llm_wikis::process::ProcessOutcome, AppError> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        // Calls 1-2 are doctor's own static executable/auth probes; calls
        // 3-4 are QueryService's own step-7 probes; call 5 is the real live
        // invocation — exactly where a mutation must land.
        if *calls == 5 {
            fs::write(&self.mutate, b"mutated by provider").unwrap();
        }
        drop(calls);
        self.inner.run(request)
    }
}

#[test]
fn live_step4_snapshot_identity_publishes_nothing_on_mutation() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let inner = FakeProcessRunner::new();
    queue_live_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let mutate_target = fixture
        .project_root
        .join("wiki/pages/harness-engineering.md");
    let runner = MutatingRunner {
        inner,
        mutate: mutate_target,
        calls: Mutex::new(0),
    };
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);

    assert!(!envelope.ok);
    let mutation_check = find_check(&envelope, WIKI_ID, Agent::Claude, "mutation").unwrap();
    assert_eq!(mutation_check.status, CheckStatus::Fail);
    assert_eq!(mutation_check.code.as_deref(), Some("READ_ONLY_VIOLATION"));

    let store2 = probe_store(&fixture);
    let key = ProbeKey {
        wiki_id: WIKI_ID.to_string(),
        canonical_project_root: fs::canonicalize(&fixture.project_root)
            .unwrap()
            .display()
            .to_string(),
        canonical_content_root: fs::canonicalize(&fixture.content_root)
            .unwrap()
            .display()
            .to_string(),
        agent: Agent::Claude,
        load: LoadMode::ProjectSkill,
        entrypoint: ENTRYPOINT_CLAUDE.to_string(),
    };
    assert_eq!(
        store2.current_record(&key).unwrap(),
        None,
        "a detected mutation must publish nothing"
    );
}

#[test]
fn live_step5_records_identity_outside_the_wiki() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_live_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let store = probe_store(&fixture);

    let request = doctor_request(&fixture, config, Some(WIKI_ID), Some(Agent::Claude), true);
    let envelope = run_doctor(request, runner, &store);
    assert!(envelope.ok, "{envelope:?}");

    // The probe cache path itself is outside both project_root/content_root.
    assert!(!fixture.probe_store_path.starts_with(&fixture.project_root));
    assert!(fixture.probe_store_path.is_file());

    let key = ProbeKey {
        wiki_id: WIKI_ID.to_string(),
        canonical_project_root: fs::canonicalize(&fixture.project_root)
            .unwrap()
            .display()
            .to_string(),
        canonical_content_root: fs::canonicalize(&fixture.content_root)
            .unwrap()
            .display()
            .to_string(),
        agent: Agent::Claude,
        load: LoadMode::ProjectSkill,
        entrypoint: ENTRYPOINT_CLAUDE.to_string(),
    };
    let record = store
        .current_record(&key)
        .unwrap()
        .expect("a probe record was published");
    assert_eq!(record.agent_version, "2.1.220");
}

#[test]
fn live_step6_invalidation_on_executable_version_fingerprint_or_query_prompt_change() {
    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_live_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let store = probe_store(&fixture);
    let request = doctor_request(
        &fixture,
        config.clone(),
        Some(WIKI_ID),
        Some(Agent::Claude),
        true,
    );
    let envelope = run_doctor(request, runner, &store);
    assert!(envelope.ok, "{envelope:?}");

    let key = ProbeKey {
        wiki_id: WIKI_ID.to_string(),
        canonical_project_root: fs::canonicalize(&fixture.project_root)
            .unwrap()
            .display()
            .to_string(),
        canonical_content_root: fs::canonicalize(&fixture.content_root)
            .unwrap()
            .display()
            .to_string(),
        agent: Agent::Claude,
        load: LoadMode::ProjectSkill,
        entrypoint: ENTRYPOINT_CLAUDE.to_string(),
    };
    let published = store.current_record(&key).unwrap().unwrap();

    // Changing only the recorded executable version invalidates the tuple.
    let mut version_changed = published.clone();
    version_changed.agent_version = "9.9.9".to_string();
    assert_ne!(version_changed, published);

    // Changing only skill_fingerprint invalidates it too.
    let mut fingerprint_changed = published.clone();
    fingerprint_changed.skill_fingerprint =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert_ne!(fingerprint_changed, published);

    // Changing query_prompt changes compatibility_fingerprint, which the
    // published record would no longer match against a freshly recomputed one.
    let mut config_with_new_prompt = config;
    config_with_new_prompt
        .wikis
        .get_mut(WIKI_ID)
        .unwrap()
        .query_prompt =
        "Use the wiki-query skill to answer from THIS wiki, differently.".to_string();
    let project_root_str = fixture.project_root.display().to_string();
    let content_root_str = fixture.content_root.display().to_string();
    let executable_str = fixture.executable_path.display().to_string();
    let build_compat_input =
        |query_prompt: &'static str| llm_wikis::query::CompatibilityFingerprintInput {
            wiki_id: WIKI_ID,
            title: "Harness Engineering",
            project_root: &project_root_str,
            content_root: &content_root_str,
            query_prompt,
            agent: Agent::Claude,
            load: "project_skill",
            entrypoint: ENTRYPOINT_CLAUDE,
            skill_path: Some(SKILL_RELATIVE),
            plugin_dir: None,
            executable_declaration: &executable_str,
            model_declaration: None,
            effort_declaration: None,
            provider_contract_version: llm_wikis::query::PROVIDER_CONTRACT_VERSION,
        };
    let compat_input_before = build_compat_input(QUERY_PROMPT);
    let compat_input_after =
        build_compat_input("Use the wiki-query skill to answer from THIS wiki, differently.");
    assert_ne!(
        llm_wikis::query::compute_compatibility_fingerprint(&compat_input_before),
        llm_wikis::query::compute_compatibility_fingerprint(&compat_input_after),
        "query_prompt must participate in compatibility_fingerprint"
    );
    assert_ne!(
        published.compatibility_fingerprint,
        llm_wikis::query::compute_compatibility_fingerprint(&compat_input_after)
    );
}

// ---------------------------------------------------------------------------
// OFF-204: doctor's live check is never triggered by a plain query
// (regression guard co-located here since it concerns the doctor/query
// boundary this module owns)
// ---------------------------------------------------------------------------

#[test]
fn plain_query_service_never_calls_doctor_publish() {
    use llm_wikis::probes::FakeProbeReader;
    use llm_wikis::query::{QueryRequest, QueryService};

    let fixture = build_fixture();
    let config = build_config(&fixture, vec![Agent::Claude]);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    let wiki = &config.wikis[WIKI_ID];
    let provider = wiki.claude.as_ref().unwrap();
    let skill_dir = fixture.skill_dir.clone();
    let skill_fingerprint = llm_wikis::query::compute_skill_fingerprint(&skill_dir).unwrap();
    let compat_input = llm_wikis::query::CompatibilityFingerprintInput {
        wiki_id: WIKI_ID,
        title: &wiki.title,
        project_root: &wiki.project_root,
        content_root: &wiki.content_root,
        query_prompt: &wiki.query_prompt,
        agent: Agent::Claude,
        load: llm_wikis::query::load_mode_str(provider.load),
        entrypoint: &provider.entrypoint,
        skill_path: provider.skill_path.as_deref(),
        plugin_dir: provider.plugin_dir.as_deref(),
        executable_declaration: &fixture.executable_path.display().to_string(),
        model_declaration: None,
        effort_declaration: None,
        provider_contract_version: llm_wikis::query::PROVIDER_CONTRACT_VERSION,
    };
    let compatibility_fingerprint =
        llm_wikis::query::compute_compatibility_fingerprint(&compat_input);
    probes.seed(
        ProbeKey {
            wiki_id: WIKI_ID.to_string(),
            canonical_project_root: fs::canonicalize(&fixture.project_root)
                .unwrap()
                .display()
                .to_string(),
            canonical_content_root: fs::canonicalize(&fixture.content_root)
                .unwrap()
                .display()
                .to_string(),
            agent: Agent::Claude,
            load: LoadMode::ProjectSkill,
            entrypoint: ENTRYPOINT_CLAUDE.to_string(),
        },
        ProbeRecord {
            agent_executable: fs::canonicalize(&fixture.executable_path)
                .unwrap()
                .display()
                .to_string(),
            agent_version: "2.1.220".to_string(),
            skill_fingerprint,
            compatibility_fingerprint,
            verified_at: "2026-07-30T12:00:00Z".to_string(),
        },
    );

    let service = QueryService::new(runner, probes);
    let request = QueryRequest {
        config,
        config_dir: fixture.project_root.clone(),
        wiki_id: WIKI_ID.to_string(),
        agent: Agent::Claude,
        question: b"What does this wiki cover?".to_vec(),
        temp_base: fixture.temp_base.clone(),
    };
    let envelope = service
        .query(request, llm_wikis::probes::QueryMode::Enforced)
        .unwrap();
    assert!(envelope.ok, "{envelope:?}");

    // No file was ever written at the probe-cache path: `QueryService` has
    // no `ProbeWriter` dependency at all, so no doctor-shaped call could have
    // happened even structurally.
    assert!(!fixture.probe_store_path.exists());
}
