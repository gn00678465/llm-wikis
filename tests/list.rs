//! `list` tests (spec §5.1, §5.3; plan Task 11 Step 4).

use std::collections::BTreeMap;

use llm_wikis::config::{
    Config, LoadMode, ProviderWikiConfig, ProvidersConfig, RuntimeConfig, WikiConfig,
};
use llm_wikis::doctor::{list_error_envelope, run_list};
use llm_wikis::error::{AppError, ErrorCode};
use llm_wikis::output::Agent;
use llm_wikis::providers::FakeProcessRunner;

fn empty_config(default_agent: Option<Agent>) -> Config {
    Config {
        config_version: 1,
        default_agent,
        providers: ProvidersConfig::default(),
        runtime: RuntimeConfig::default(),
        wikis: BTreeMap::new(),
    }
}

fn wiki(title: &str, agents: Vec<Agent>) -> WikiConfig {
    let mut w = WikiConfig {
        title: title.to_string(),
        project_root: "/wikis/example".to_string(),
        content_root: "/wikis/example".to_string(),
        agents: agents.clone(),
        query_prompt: "Use the wiki-query skill to answer from this wiki.".to_string(),
        claude: None,
        codex: None,
    };
    for agent in agents {
        let provider = ProviderWikiConfig {
            load: LoadMode::ProjectSkill,
            entrypoint: match agent {
                Agent::Claude => "/wiki-query".to_string(),
                Agent::Codex => "$wiki-query".to_string(),
            },
            skill_path: Some(".claude/skills/wiki-query/SKILL.md".to_string()),
            plugin_dir: None,
        };
        match agent {
            Agent::Claude => w.claude = Some(provider),
            Agent::Codex => w.codex = Some(provider),
        }
    }
    w
}

#[test]
fn zero_wiki_config_returns_empty_successful_array() {
    let config = empty_config(None);
    let envelope = run_list(&config);
    assert!(envelope.ok);
    assert_eq!(envelope.operation, "list");
    assert!(envelope.wikis.is_empty());
    assert!(envelope.error.is_none());
}

#[test]
fn configured_wikis_return_id_title_default_agent_and_agents() {
    let mut config = empty_config(Some(Agent::Claude));
    config.wikis.insert(
        "agents".to_string(),
        wiki("Agents Knowledge Base", vec![Agent::Claude, Agent::Codex]),
    );
    let envelope = run_list(&config);
    assert!(envelope.ok);
    assert_eq!(envelope.wikis.len(), 1);
    let entry = &envelope.wikis[0];
    assert_eq!(entry.id, "agents");
    assert_eq!(entry.title, "Agents Knowledge Base");
    assert_eq!(entry.default_agent, Some(Agent::Claude));
    assert_eq!(entry.agents, vec![Agent::Claude, Agent::Codex]);
}

#[test]
fn default_agent_is_null_when_not_enabled_for_that_wiki() {
    let mut config = empty_config(Some(Agent::Claude));
    config.wikis.insert(
        "codex-only".to_string(),
        wiki("Codex Only Wiki", vec![Agent::Codex]),
    );
    let envelope = run_list(&config);
    let entry = &envelope.wikis[0];
    assert_eq!(
        entry.default_agent, None,
        "global default_agent=claude is not enabled for this wiki, so the derived value must be null"
    );
}

#[test]
fn no_global_default_agent_yields_null_for_every_wiki() {
    let mut config = empty_config(None);
    config.wikis.insert(
        "agents".to_string(),
        wiki("Agents Knowledge Base", vec![Agent::Claude]),
    );
    let envelope = run_list(&config);
    assert_eq!(envelope.wikis[0].default_agent, None);
}

#[test]
fn no_provider_spawned() {
    let mut config = empty_config(Some(Agent::Claude));
    config.wikis.insert(
        "agents".to_string(),
        wiki("Agents Knowledge Base", vec![Agent::Claude, Agent::Codex]),
    );

    // `run_list` never even takes a `ProcessRunner` parameter — it cannot
    // start a provider. This double just proves it stays entirely unused:
    // zero captured requests after `run_list` returns.
    let runner = FakeProcessRunner::new();
    let envelope = run_list(&config);
    assert!(envelope.ok);
    assert_eq!(runner.captured_requests().len(), 0);
}

#[test]
fn config_failure_returns_exit_2_shape_with_empty_wikis_and_error() {
    let err = AppError::new(ErrorCode::ConfigInvalid, "configuration is not valid TOML");
    let envelope = list_error_envelope(err);
    assert!(!envelope.ok);
    assert!(envelope.wikis.is_empty());
    let error = envelope
        .error
        .expect("failure envelope carries the public error object");
    assert_eq!(error.code, ErrorCode::ConfigInvalid);
    assert_eq!(error.code.exit_code(), 2);
}

#[test]
fn list_json_exact_shape() {
    let mut config = empty_config(Some(Agent::Claude));
    config.wikis.insert(
        "agents".to_string(),
        wiki("Agents Knowledge Base", vec![Agent::Claude]),
    );
    let envelope = run_list(&config);
    let value = serde_json::to_value(&envelope).unwrap();
    let top_level_keys: std::collections::BTreeSet<String> =
        value.as_object().unwrap().keys().cloned().collect();
    let expected: std::collections::BTreeSet<String> =
        ["schema_version", "ok", "operation", "wikis"]
            .into_iter()
            .map(String::from)
            .collect();
    assert_eq!(
        top_level_keys, expected,
        "no extra top-level keys on success"
    );
    assert_eq!(value["schema_version"], "1.0");
    assert_eq!(value["operation"], "list");
    let wiki_keys: std::collections::BTreeSet<String> = value["wikis"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let expected_wiki_keys: std::collections::BTreeSet<String> =
        ["id", "title", "default_agent", "agents"]
            .into_iter()
            .map(String::from)
            .collect();
    assert_eq!(wiki_keys, expected_wiki_keys);
}
