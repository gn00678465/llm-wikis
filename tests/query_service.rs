//! Single-wiki Query Service tests (spec §8.1, §12, §15.1; plan Task 10).
//!
//! Every test here runs against [`FakeProcessRunner`] and [`FakeProbeReader`]
//! plus tempdir fixtures — no real Claude/Codex process ever starts and no
//! model quota is spent.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use llm_wikis::config::{
    Config, LoadMode, ProviderConfig, ProviderWikiConfig, ProvidersConfig, RuntimeConfig,
    WikiConfig,
};
use llm_wikis::error::{AppError, ErrorCode};
use llm_wikis::output::Agent;
use llm_wikis::probes::{FakeProbeReader, ProbeKey, ProbeReader, ProbeRecord, QueryMode};
use llm_wikis::process::{
    ExecutableKind, ProcessOutcome, ProcessRequest, ResolvedExecutable, TerminationReason,
};
use llm_wikis::providers::{FakeProcessRunner, ProcessRunner, completed_outcome};
use llm_wikis::query::{
    CompatibilityFingerprintInput, PROVIDER_CONTRACT_VERSION, QueryRequest, QueryService,
    compute_compatibility_fingerprint, compute_skill_fingerprint, load_mode_str,
};

const WIKI_ID: &str = "harness-engineering";
const ENTRYPOINT_CLAUDE: &str = "/wiki-query";
const ENTRYPOINT_CODEX: &str = "$wiki-query";
const SKILL_RELATIVE: &str = ".claude/skills/wiki-query/SKILL.md";
const QUERY_PROMPT: &str = "Use the wiki-query skill to answer from this wiki.";

// ---------------------------------------------------------------------------
// Fixture construction
// ---------------------------------------------------------------------------

/// A disposable directory rooted under this crate's own `target/` -- not
/// `tempfile::tempdir()` (system temp). See tests/doctor.rs's own
/// `LocalTempDir` (identical implementation) for the two CI-observed
/// classes of system-temp breakage (macOS `/var` symlink;
/// `CONFIG_INVALID`/shell-metacharacters on a subset of Windows CI runs)
/// this sidesteps. Removed on drop.
struct LocalTempDir {
    path: PathBuf,
}

impl LocalTempDir {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("query-service-test-tmp")
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
    page_path: PathBuf,
    skill_dir: PathBuf,
    executable_path: PathBuf,
    temp_base: PathBuf,
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
    let page_path = pages_dir.join("harness-engineering.md");
    fs::write(&page_path, b"# Harness Engineering\n").unwrap();

    let temp_base = tmp.path().join("temp-base");
    fs::create_dir_all(&temp_base).unwrap();

    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let executable_path = bin_dir.join("fake-provider");
    fs::write(&executable_path, b"placeholder, never executed").unwrap();

    Fixture {
        _tmp: tmp,
        project_root: project_root.clone(),
        content_root: project_root,
        page_path,
        skill_dir,
        executable_path,
        temp_base,
    }
}

fn entrypoint_for(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => ENTRYPOINT_CLAUDE,
        Agent::Codex => ENTRYPOINT_CODEX,
    }
}

fn build_config(fixture: &Fixture, agent: Agent) -> Config {
    let provider_wiki = ProviderWikiConfig {
        load: LoadMode::ProjectSkill,
        entrypoint: entrypoint_for(agent).to_string(),
        skill_path: Some(SKILL_RELATIVE.to_string()),
        plugin_dir: None,
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

fn build_request(fixture: &Fixture, config: Config, agent: Agent, question: &[u8]) -> QueryRequest {
    QueryRequest {
        config,
        config_dir: fixture.project_root.clone(),
        wiki_id: WIKI_ID.to_string(),
        agent,
        question: question.to_vec(),
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

fn codex_success_stdout(answer: &str) -> Vec<u8> {
    let model_json = serde_json::json!({
        "contract": "wiki-query/v1",
        "knowledge_status": "grounded",
        "answer": answer,
        "citations": ["harness-engineering"],
        "gaps": [],
        "warnings": []
    })
    .to_string();
    let line = serde_json::json!({
        "type": "item.completed",
        "item": { "type": "agent_message", "text": model_json }
    });
    format!("{line}\n").into_bytes()
}

fn resolved_executable(fixture: &Fixture) -> ResolvedExecutable {
    ResolvedExecutable {
        path: fs::canonicalize(&fixture.executable_path).unwrap(),
        kind: ExecutableKind::Native,
    }
}

fn probe_version_for(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "2.1.220",
        Agent::Codex => "0.45.0",
    }
}

/// Builds a `ProbeRecord` that exactly matches what `QueryService::query` will
/// compute for `fixture`/`agent`/`config` in `Enforced` mode, so it seeds a
/// passing probe gate (spec §15.1 current-verification tuple).
fn matching_probe_record(fixture: &Fixture, config: &Config, agent: Agent) -> ProbeRecord {
    let wiki = &config.wikis[WIKI_ID];
    let provider_table = match agent {
        Agent::Claude => wiki.claude.as_ref().unwrap(),
        Agent::Codex => wiki.codex.as_ref().unwrap(),
    };
    let executable_declaration = fixture.executable_path.display().to_string();
    let skill_fingerprint = compute_skill_fingerprint(&fixture.skill_dir).unwrap();
    let compatibility_fingerprint =
        compute_compatibility_fingerprint(&CompatibilityFingerprintInput {
            wiki_id: WIKI_ID,
            title: &wiki.title,
            project_root: &wiki.project_root,
            content_root: &wiki.content_root,
            query_prompt: &wiki.query_prompt,
            agent,
            load: load_mode_str(provider_table.load),
            entrypoint: &provider_table.entrypoint,
            skill_path: provider_table.skill_path.as_deref(),
            plugin_dir: provider_table.plugin_dir.as_deref(),
            executable_declaration: &executable_declaration,
            provider_contract_version: PROVIDER_CONTRACT_VERSION,
        });
    ProbeRecord {
        agent_executable: resolved_executable(fixture).path.display().to_string(),
        agent_version: probe_version_for(agent).to_string(),
        skill_fingerprint,
        compatibility_fingerprint,
        verified_at: "2026-07-30T12:00:00Z".to_string(),
    }
}

fn probe_key(fixture: &Fixture, agent: Agent) -> ProbeKey {
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
        agent,
        load: LoadMode::ProjectSkill,
        entrypoint: entrypoint_for(agent).to_string(),
    }
}

fn seed_matching_probe(reader: &FakeProbeReader, fixture: &Fixture, config: &Config, agent: Agent) {
    reader.seed(
        probe_key(fixture, agent),
        matching_probe_record(fixture, config, agent),
    );
}

// ---------------------------------------------------------------------------
// Happy path (plan Task 10 Step 1)
// ---------------------------------------------------------------------------

#[test]
fn happy_path_claude_success_envelope() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Claude);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(envelope.ok, "expected success, got {envelope:?}");
    assert_eq!(envelope.wiki.unwrap().id, WIKI_ID);
    assert_eq!(envelope.agent, Some(Agent::Claude));
    assert_eq!(envelope.contract, Some("wiki-query/v1"));
    assert_eq!(envelope.citations.len(), 1);
    assert_eq!(envelope.citations[0].wiki, WIKI_ID);
    assert_eq!(envelope.citations[0].slug, "harness-engineering");
    assert_eq!(envelope.child_exit_code, Some(0));
    assert_eq!(
        envelope.raw_format,
        Some(llm_wikis::output::RawFormat::ClaudeJson)
    );
    assert!(envelope.error.is_none());
}

#[test]
fn happy_path_codex_success_envelope() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Codex);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Codex);
    runner.push_response(Ok(completed_outcome(
        &codex_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Codex);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Codex,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(envelope.ok, "expected success, got {envelope:?}");
    assert_eq!(
        envelope.raw_format,
        Some(llm_wikis::output::RawFormat::CodexJsonl)
    );
    // Codex always carries the unconditional read-scope warning (spec §10.3).
    assert!(
        envelope
            .warnings
            .iter()
            .any(|w| w.code == "CODEX_READ_SCOPE_BROAD")
    );
}

// ---------------------------------------------------------------------------
// OFF-107: step ordering
// ---------------------------------------------------------------------------

#[test]
fn step_ordering() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Claude);

    let steps: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&steps);
    let service = QueryService::new(runner, probes).with_step_observer(move |name| {
        recorder.lock().unwrap().push(name);
    });
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(envelope.ok, "expected success, got {envelope:?}");

    let expected = [
        "resolve_wiki_provider",
        "question_validate",
        "path_canonicalize",
        "preflight",
        "executable_resolve_probes",
        "fingerprint_probe_gate",
        "prompt_build",
        "before_snapshot",
        "invoke",
        "parse_native_contract",
        "citations_namespace",
        "after_snapshot",
        "emit_envelope",
    ];
    assert_eq!(steps.lock().unwrap().as_slice(), expected.as_slice());
}

// ---------------------------------------------------------------------------
// OFF-045 / question validation (plan Task 10 Step 2)
// ---------------------------------------------------------------------------

#[test]
fn question_too_large() {
    let fixture = build_fixture();

    // Exactly at the boundary (9 bytes: 3 x 3-byte Traditional Chinese
    // characters) must not be rejected as too large (it may still fail
    // later, e.g. content_root structure, but never with QUESTION_TOO_LARGE).
    let at_boundary = "繁體中".as_bytes();
    assert_eq!(at_boundary.len(), 9);
    let mut config = build_config(&fixture, Agent::Claude);
    config.runtime.max_question_bytes = 9;
    let runner = FakeProcessRunner::new();
    // The boundary-exact question passes length validation and proceeds
    // through the rest of the pipeline (executable/version/auth probes),
    // which needs queued responses even though this test only cares that it
    // never fails with QUESTION_TOO_LARGE; it will still fail later at the
    // probe gate since no record is seeded, which is fine.
    queue_success_probes(&runner, Agent::Claude);
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let request = build_request(&fixture, config, Agent::Claude, at_boundary);
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    if let Some(err) = &envelope.error {
        assert_ne!(err.code, ErrorCode::QuestionTooLarge);
    }

    // One byte over the boundary must fail with QUESTION_TOO_LARGE.
    let over_boundary = "繁體中文".as_bytes();
    assert_eq!(over_boundary.len(), 12);
    let mut config = build_config(&fixture, Agent::Claude);
    config.runtime.max_question_bytes = 9;
    let runner = FakeProcessRunner::new();
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let request = build_request(&fixture, config, Agent::Claude, over_boundary);
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(!envelope.ok);
    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::QuestionTooLarge);
    assert_eq!(err.code.exit_code(), 2);
}

#[test]
fn invalid_utf8_question_is_rejected() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let request = build_request(&fixture, config, Agent::Claude, &[0xff, 0xfe, 0x80]);
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(!envelope.ok);
    assert_eq!(envelope.error.unwrap().code, ErrorCode::QuestionInvalidUtf8);
    assert_eq!(envelope.child_exit_code, None);
}

// ---------------------------------------------------------------------------
// OFF-108: validated strictly before any spawn
// ---------------------------------------------------------------------------

#[test]
fn validate_before_spawn() {
    let fixture = build_fixture();

    // Invalid UTF-8: FakeProcessRunner has zero queued responses, so any
    // spawn attempt panics ("called with no queued response").
    let mut config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let request = build_request(&fixture, config.clone(), Agent::Claude, &[0xff, 0xfe]);
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(!envelope.ok);
    assert_eq!(envelope.child_exit_code, None);

    // Oversized question: same guarantee.
    config.runtime.max_question_bytes = 4;
    let runner = FakeProcessRunner::new();
    let probes = FakeProbeReader::new();
    let service = QueryService::new(runner, probes);
    let request = build_request(&fixture, config, Agent::Claude, b"a much too long question");
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(!envelope.ok);
    assert_eq!(envelope.error.unwrap().code, ErrorCode::QuestionTooLarge);
    assert_eq!(envelope.child_exit_code, None);
}

// ---------------------------------------------------------------------------
// R-29: query re-runs the wiki-settings surface check, closing the TOCTOU gap
// between a passing `doctor` run and a later `query` invocation (PR #1 Codex
// review iteration 3 finding 1). A forbidden key must fail closed here too,
// strictly before any provider process is spawned -- proven the same way
// validate_before_spawn above does: zero queued FakeProcessRunner responses,
// so reaching a real spawn attempt would panic instead of returning an
// envelope.
// ---------------------------------------------------------------------------

#[test]
fn query_rejects_a_forbidden_claude_wiki_settings_key_before_any_spawn() {
    // R-31 (PR #1 Codex review iteration 5 finding 8): the check now runs
    // inside `ClaudeAdapter::invoke`, the genuinely last thing before the
    // one `runner.run` call that spawns the real provider process -- after
    // the version/auth probes (Step 7) and the fingerprint gate (Step 8),
    // both of which also call the runner. A prior version of this test only
    // proved "before *some* spawn," which an accidental move of the check
    // back to an earlier step would not have caught (the version/auth
    // probes would simply go uncalled/unconsumed and nothing here would
    // notice). This version uses a `SharedRunner` so the captured-request
    // list is inspectable *after* `query()` returns, and asserts on it
    // directly: exactly the two probe requests (`--version`, then `auth
    // status --json`) were captured, in that order, proving the probes
    // genuinely ran -- and nothing else was, proving `invoke`'s own
    // `runner.run` (which would be a third, `--add-dir`-shaped request) was
    // never reached.
    let fixture = build_fixture();
    fs::write(
        fixture.project_root.join(".claude/settings.json"),
        br#"{"apiKeyHelper":"echo hooked"}"#,
    )
    .unwrap();
    let config = build_config(&fixture, Agent::Claude);
    let inner = Arc::new(FakeProcessRunner::new());
    queue_success_probes(&inner, Agent::Claude);
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Claude);
    let handle = Arc::clone(&inner);
    let service = QueryService::new(SharedRunner(inner), probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    assert_eq!(envelope.error.unwrap().code, ErrorCode::EntrypointInvalid);

    let captured = handle.captured_requests();
    assert_eq!(
        captured.len(),
        2,
        "expected exactly the version+auth probes to have run, nothing more: {captured:?}"
    );
    assert_eq!(captured[0].args, vec![OsString::from("--version")]);
    assert_eq!(
        captured[1].args,
        vec![
            OsString::from("auth"),
            OsString::from("status"),
            OsString::from("--json"),
        ]
    );
    assert_eq!(envelope.child_exit_code, None);
}

#[test]
fn query_still_succeeds_when_wiki_settings_declare_only_enabled_plugins() {
    let fixture = build_fixture();
    fs::write(
        fixture.project_root.join(".claude/settings.local.json"),
        br#"{"enabledPlugins":{"llm-wiki@llm-wiki":true}}"#,
    )
    .unwrap();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Claude);
    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(envelope.ok, "expected success, got {envelope:?}");
    // R-32: enabledPlugins is admitted but not silent -- the wrapper
    // surfaces CLAUDE_ENABLED_PLUGINS_DECLARED so an operator is told.
    assert!(
        envelope
            .warnings
            .iter()
            .any(|w| w.code == "CLAUDE_ENABLED_PLUGINS_DECLARED"),
        "expected the enabledPlugins advisory warning, got {:?}",
        envelope.warnings
    );
}

// ---------------------------------------------------------------------------
// Adversarial input (plan Task 10 Step 3)
// ---------------------------------------------------------------------------

/// A [`ProcessRunner`] that delegates to a shared, externally observable
/// [`FakeProcessRunner`] — needed whenever a test wants to inspect captured
/// requests *after* `QueryService::query` has consumed the runner by value.
struct SharedRunner(Arc<FakeProcessRunner>);

impl ProcessRunner for SharedRunner {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
        self.0.run(request)
    }
}

#[test]
fn adversarial_question_bytes_never_leak_into_argv() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let inner = Arc::new(FakeProcessRunner::new());
    queue_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new();
    seed_matching_probe(&probes, &fixture, &config, Agent::Claude);

    let adversarial = "繁體中文\n--leading-dash\n\"quotes\" `backticks` $() & | < > ^ % !";
    let handle = Arc::clone(&inner);
    let service = QueryService::new(SharedRunner(inner), probes);
    let request = build_request(&fixture, config, Agent::Claude, adversarial.as_bytes());
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(envelope.ok, "expected success, got {envelope:?}");

    let captured = handle.captured_requests();
    // The last captured request is the actual provider invocation (the first
    // two are the version/auth probes); its stdin carries the question, its
    // argv never does.
    let invoke_request = captured.last().expect("invoke was captured");
    let stdin_text = String::from_utf8_lossy(&invoke_request.stdin);
    // The question travels inside the JSON `EXTERNAL_QUERY` object, so
    // control characters (the literal newlines above) are JSON-escaped —
    // round-trip through a JSON parse instead of a raw substring match.
    let json_start = stdin_text
        .find("EXTERNAL_QUERY:\n")
        .expect("envelope marker present")
        + "EXTERNAL_QUERY:\n".len();
    let envelope_json: serde_json::Value = serde_json::from_str(&stdin_text[json_start..]).unwrap();
    assert_eq!(envelope_json["question"], adversarial);
    for arg in &invoke_request.args {
        let arg_text = arg.to_string_lossy();
        assert!(!arg_text.contains("繁體中文"));
        assert!(!arg_text.contains("leading-dash"));
    }
}

// ---------------------------------------------------------------------------
// OFF-111: probe gate in Enforced mode
// ---------------------------------------------------------------------------

#[test]
fn probe_gate_enforced_mode_absent_record() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    // No third response queued: invoke must never be reached.
    let probes = FakeProbeReader::new(); // nothing seeded

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::EntrypointUnverified);
    assert_eq!(err.code.exit_code(), 3);
}

#[test]
fn probe_gate_enforced_mode_mismatched_record() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    let probes = FakeProbeReader::new();
    let mut record = matching_probe_record(&fixture, &config, Agent::Claude);
    record.skill_fingerprint =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
    probes.seed(probe_key(&fixture, Agent::Claude), record);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    assert_eq!(
        envelope.error.unwrap().code,
        ErrorCode::EntrypointUnverified
    );
}

// ---------------------------------------------------------------------------
// Verification mode (plan Task 10 Step 5)
// ---------------------------------------------------------------------------

#[test]
fn verification_mode_skips_probe_gate() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let probes = FakeProbeReader::new(); // nothing seeded, deliberately

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Verification)
        .expect("Ok envelope");

    assert!(
        envelope.ok,
        "verification mode must skip the probe gate, got {envelope:?}"
    );
}

// ---------------------------------------------------------------------------
// Mutation detection (OFF-152/153/154/155)
// ---------------------------------------------------------------------------

/// Wraps a [`FakeProcessRunner`] and mutates a fixture file the instant
/// `run` is called — landing the mutation exactly between the before- and
/// after-snapshot, whatever outcome is queued.
struct MutatingRunner {
    inner: FakeProcessRunner,
    mutate: PathBuf,
    calls: Mutex<u32>,
}

impl MutatingRunner {
    fn new(inner: FakeProcessRunner, mutate: PathBuf) -> Self {
        Self {
            inner,
            mutate,
            calls: Mutex::new(0),
        }
    }
}

impl ProcessRunner for MutatingRunner {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
        // The version/auth probes (spec §8.1 step 7) go through this same
        // runner before the actual invocation (step 11) does. Only the
        // *third* call is the real provider invocation, which is exactly
        // where a mutation must land — between the before- and after-snapshot.
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 3 {
            fs::write(&self.mutate, b"mutated by provider").unwrap();
        }
        drop(calls);
        self.inner.run(request)
    }
}

fn mutation_test_setup(agent: Agent) -> (Fixture, Config, ProbeKey, ProbeRecord) {
    let fixture = build_fixture();
    let config = build_config(&fixture, agent);
    let key = probe_key(&fixture, agent);
    let record = matching_probe_record(&fixture, &config, agent);
    (fixture, config, key, record)
}

#[test]
fn mutation_rejects_result() {
    let (fixture, config, key, record) = mutation_test_setup(Agent::Claude);
    let inner = FakeProcessRunner::new();
    queue_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let runner = MutatingRunner::new(inner, fixture.page_path.clone());
    let probes = FakeProbeReader::new();
    probes.seed(key, record);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    // No file content anywhere in the envelope (check before any partial move).
    let envelope_json = serde_json::to_string(&envelope).unwrap();
    assert!(!envelope_json.contains("mutated by provider"));

    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::ReadOnlyViolation);
    assert_eq!(err.code.exit_code(), 7);
    let details = err.details.expect("READ_ONLY_VIOLATION carries details");
    let json = serde_json::to_value(&details).unwrap();
    let changed_paths = json["changed_paths"].as_array().unwrap();
    assert!(!changed_paths.is_empty());
    for p in changed_paths {
        let p = p.as_str().unwrap();
        assert!(!p.contains('\\'));
        assert!(!p.starts_with('/'));
    }
    let mut sorted = changed_paths.clone();
    sorted.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
    assert_eq!(&sorted, changed_paths);
}

#[test]
fn snapshot_runs_on_every_exit_path() {
    // Success path already covered by mutation_rejects_result. Cover
    // provider-failure (non-zero exit), timeout, and output-overflow paths.
    let cases: Vec<(&str, ProcessOutcome)> = vec![
        (
            "nonzero_exit",
            ProcessOutcome {
                stdout: Vec::new(),
                stderr: b"boom".to_vec(),
                exit_code: Some(1),
                elapsed: std::time::Duration::from_millis(1),
                termination: TerminationReason::Completed,
            },
        ),
        (
            "timeout",
            ProcessOutcome {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: None,
                elapsed: std::time::Duration::from_millis(1),
                termination: TerminationReason::TimedOut,
            },
        ),
        (
            "output_overflow",
            ProcessOutcome {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: None,
                elapsed: std::time::Duration::from_millis(1),
                termination: TerminationReason::OutputTooLarge {
                    stream: llm_wikis::error::Stream::Stdout,
                    observed_bytes: 100,
                },
            },
        ),
    ];

    for (name, outcome) in cases {
        let (fixture, config, key, record) = mutation_test_setup(Agent::Claude);
        let inner = FakeProcessRunner::new();
        queue_success_probes(&inner, Agent::Claude);
        inner.push_response(Ok(outcome));
        let runner = MutatingRunner::new(inner, fixture.page_path.clone());
        let probes = FakeProbeReader::new();
        probes.seed(key, record);

        let service = QueryService::new(runner, probes);
        let request = build_request(
            &fixture,
            config,
            Agent::Claude,
            b"What does this wiki cover?",
        );
        let envelope = service
            .query(request, QueryMode::Enforced)
            .expect("Ok envelope");

        assert!(!envelope.ok, "case {name}: expected failure envelope");
        assert_eq!(
            envelope.error.unwrap().code,
            ErrorCode::ReadOnlyViolation,
            "case {name}: mutation must still be detected"
        );
    }
}

#[test]
fn mutation_dominance_preserves_secondary_error() {
    let (fixture, config, key, record) = mutation_test_setup(Agent::Claude);
    let inner = FakeProcessRunner::new();
    queue_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(ProcessOutcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        elapsed: std::time::Duration::from_millis(1),
        termination: TerminationReason::TimedOut,
    }));
    let runner = MutatingRunner::new(inner, fixture.page_path.clone());
    let probes = FakeProbeReader::new();
    probes.seed(key, record);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::ReadOnlyViolation);
    let details = err.details.expect("details present");
    let json = serde_json::to_value(&details).unwrap();
    assert_eq!(json["secondary_error"]["code"], "TIMEOUT");
}

#[test]
fn internal_error_only_on_incomplete_comparison() {
    let (fixture, config, key, record) = mutation_test_setup(Agent::Claude);
    let inner = FakeProcessRunner::new();
    queue_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));

    // Instead of mutating a file, remove the whole content_root so the AFTER
    // snapshot cannot even be *taken* (fs::read_dir fails => Unreadable),
    // which is the one case spec §12 reserves INTERNAL_ERROR for.
    struct RemovingRunner {
        inner: FakeProcessRunner,
        remove: PathBuf,
        calls: Mutex<u32>,
    }
    impl ProcessRunner for RemovingRunner {
        fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
            let result = self.inner.run(request);
            // Only remove content_root once, right after the real invocation
            // (the third call; the first two are the version/auth probes) —
            // removing it again on a later call would itself error.
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 3 {
                fs::remove_dir_all(&self.remove).unwrap();
            }
            result
        }
    }
    let runner = RemovingRunner {
        inner,
        remove: fixture.content_root.clone(),
        calls: Mutex::new(0),
    };
    let probes = FakeProbeReader::new();
    probes.seed(key, record);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    assert!(!envelope.ok);
    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::InternalError);
    assert_eq!(err.code.exit_code(), 70);
}

// ---------------------------------------------------------------------------
// OFF-204 / OFF-222: probe reader is read-only and never a live doctor
// ---------------------------------------------------------------------------

/// A [`ProbeReader`] that delegates to a shared, externally observable
/// [`FakeProbeReader`], plus a call counter — needed whenever a test wants to
/// inspect probe-store state or call counts *after* `QueryService::query`
/// has consumed the reader by value.
struct SharedProbeReader {
    inner: Arc<FakeProbeReader>,
    calls: Arc<Mutex<u32>>,
}

impl ProbeReader for SharedProbeReader {
    fn current_record(&self, key: &ProbeKey) -> Result<Option<ProbeRecord>, AppError> {
        *self.calls.lock().unwrap() += 1;
        self.inner.current_record(key)
    }
}

#[test]
fn never_triggers_live_doctor() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let inner = Arc::new(FakeProbeReader::new());
    seed_matching_probe(&inner, &fixture, &config, Agent::Claude);
    let calls = Arc::new(Mutex::new(0));
    let probes = SharedProbeReader {
        inner,
        calls: Arc::clone(&calls),
    };

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(envelope.ok, "expected success, got {envelope:?}");

    // A plain `query` call reads the probe cache exactly once (the Enforced
    // gate) — there is no separate live-doctor verification loop to trigger,
    // because `QueryService<R, P>` is generic only over `P: ProbeReader` and
    // has no dependency capable of running one.
    assert_eq!(*calls.lock().unwrap(), 1);
}

/// `query` never writes to the probe cache: `QueryService`'s only
/// probe-shaped dependency is `P: ProbeReader`, which has no write method at
/// all (`ProbeWriter` does not exist until Task 11 and is never referenced
/// here). This is provable structurally (the code above compiles with only a
/// reader) and behaviorally: the seeded record is byte-identical before and
/// after a successful `Enforced` run.
#[test]
fn query_never_writes_probes() {
    let fixture = build_fixture();
    let config = build_config(&fixture, Agent::Claude);
    let runner = FakeProcessRunner::new();
    queue_success_probes(&runner, Agent::Claude);
    runner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));
    let inner = Arc::new(FakeProbeReader::new());
    seed_matching_probe(&inner, &fixture, &config, Agent::Claude);
    let key = probe_key(&fixture, Agent::Claude);
    let before = inner.current_record(&key).unwrap();
    let handle = Arc::clone(&inner);
    let probes = SharedProbeReader {
        inner,
        calls: Arc::new(Mutex::new(0)),
    };

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");
    assert!(envelope.ok, "expected success, got {envelope:?}");

    let after = handle.current_record(&key).unwrap();
    assert_eq!(before, after, "the seeded probe record must be untouched");
}

// ---------------------------------------------------------------------------
// SnapshotError::Unsafe surfacing during the AFTER snapshot
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn try_symlink_file(target: &std::path::Path, link: &std::path::Path) -> bool {
    match std::os::windows::fs::symlink_file(target, link) {
        Ok(()) => true,
        Err(e) => {
            eprintln!(
                "SKIP: cannot create symlink fixture {} -> {} ({e}); Windows developer mode or an elevated privilege is required",
                link.display(),
                target.display()
            );
            false
        }
    }
}

#[cfg(not(windows))]
fn try_symlink_file(target: &std::path::Path, link: &std::path::Path) -> bool {
    match std::os::unix::fs::symlink(target, link) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("SKIP: cannot create symlink fixture ({e})");
            false
        }
    }
}

/// A special filesystem entry appearing under `content_root` only *after*
/// the before-snapshot (which already proved the tree was clean at that
/// point) must abort the AFTER snapshot with `SnapshotError::Unsafe`, which
/// `query.rs` documents as a direct override: the `UNSAFE_FILESYSTEM_ENTRY`
/// error becomes the envelope's error as-is, bypassing
/// `apply_integrity_dominance` entirely (no `changed_paths` list is
/// constructible from a bare `SnapshotError::Unsafe`).
#[test]
fn after_snapshot_unsafe_entry_overrides_directly() {
    let (fixture, config, key, record) = mutation_test_setup(Agent::Claude);
    let inner = FakeProcessRunner::new();
    queue_success_probes(&inner, Agent::Claude);
    inner.push_response(Ok(completed_outcome(
        &claude_success_stdout("Grounded answer with [[harness-engineering]]."),
        b"",
        0,
    )));

    struct SymlinkingRunner {
        inner: FakeProcessRunner,
        target: PathBuf,
        link: PathBuf,
        calls: Mutex<u32>,
    }
    impl ProcessRunner for SymlinkingRunner {
        fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError> {
            let result = self.inner.run(request);
            // Only the third call is the real invocation (the first two are
            // the version/auth probes) — create the symlink exactly there,
            // between the before- and after-snapshot.
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 3 {
                try_symlink_file(&self.target, &self.link);
            }
            result
        }
    }
    let link = fixture.content_root.join("sneaky-link.md");
    let runner = SymlinkingRunner {
        inner,
        target: fixture.page_path.clone(),
        link: link.clone(),
        calls: Mutex::new(0),
    };
    let probes = FakeProbeReader::new();
    probes.seed(key, record);

    let service = QueryService::new(runner, probes);
    let request = build_request(
        &fixture,
        config,
        Agent::Claude,
        b"What does this wiki cover?",
    );
    let envelope = service
        .query(request, QueryMode::Enforced)
        .expect("Ok envelope");

    if fs::symlink_metadata(&link).is_err() {
        eprintln!(
            "SKIPPED: symlink fixture could not be created on this host (requires Windows developer mode / elevated privilege, or an unprivileged Unix account)"
        );
        return;
    }

    assert!(!envelope.ok);
    let err = envelope.error.unwrap();
    assert_eq!(err.code, ErrorCode::UnsafeFilesystemEntry);
    assert_eq!(
        err.details, None,
        "no changed_paths list is constructible from a bare SnapshotError::Unsafe"
    );
}
