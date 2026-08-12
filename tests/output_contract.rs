use llm_wikis::error::{AppError, ErrorCode};
use llm_wikis::model::KnowledgeStatus;
use llm_wikis::output::{
    Agent, Citation, PROVIDER_WARNING_CODE, QueryEnvelope, RawFormat, SCHEMA_VERSION, Warning,
    WarningSource, WrapperWarningCode, order_warnings, render_human, render_json,
};

fn success_envelope() -> QueryEnvelope {
    QueryEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: true,
        operation: "query",
        wiki: Some(llm_wikis::output::WikiRef {
            id: "harness-engineering".into(),
            title: "Harness Engineering".into(),
        }),
        agent: Some(Agent::Claude),
        contract: Some("wiki-query/v1"),
        knowledge_status: Some(KnowledgeStatus::Grounded),
        answer: Some("Grounded answer with [[harness-engineering]].".into()),
        citations: vec![Citation {
            wiki: "harness-engineering".into(),
            slug: "harness-engineering".into(),
        }],
        gaps: vec![],
        warnings: vec![Warning::wrapper(
            WrapperWarningCode::ClaudeReadScopeBroad,
            "Claude read tools can inspect the configured project root, not only the selected content root; use an OS sandbox or container for stricter confidentiality.",
        )],
        duration_ms: 63352,
        child_exit_code: Some(0),
        raw_format: Some(RawFormat::ClaudeJson),
        error: None,
    }
}

fn early_failure_envelope() -> QueryEnvelope {
    QueryEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        operation: "query",
        wiki: None,
        agent: None,
        contract: None,
        knowledge_status: None,
        answer: None,
        citations: vec![],
        gaps: vec![],
        warnings: vec![],
        duration_ms: 4,
        child_exit_code: None,
        raw_format: None,
        error: Some(AppError::new(
            ErrorCode::ArgumentInvalid,
            "invalid or ambiguous CLI input",
        )),
    }
}

#[test]
fn json_render_is_exactly_one_document_and_one_trailing_newline() {
    let rendered = render_json(&success_envelope());
    assert_eq!(
        rendered.matches('\n').count(),
        1,
        "expected exactly one newline, got: {rendered:?}"
    );
    assert!(rendered.ends_with('\n'));
    let body = rendered.trim_end_matches('\n');
    assert!(!body.contains('\n'), "body must be a single line: {body:?}");
    let _value: serde_json::Value = serde_json::from_str(body).expect("single JSON document");
}

#[test]
fn schema_version_is_the_literal_1_0_on_every_envelope() {
    assert_eq!(success_envelope().schema_version, "1.0");
    assert_eq!(early_failure_envelope().schema_version, "1.0");
    let v: serde_json::Value = serde_json::from_str(&render_json(&success_envelope())).unwrap();
    assert_eq!(v["schema_version"], "1.0");
    let v: serde_json::Value =
        serde_json::from_str(&render_json(&early_failure_envelope())).unwrap();
    assert_eq!(v["schema_version"], "1.0");
}

#[test]
fn wrapper_warning_codes_are_exactly_the_closed_five() {
    // R-32 added CLAUDE_ENABLED_PLUGINS_DECLARED, growing this from three to
    // four codes; issue #8 added VIEWER_UNAVAILABLE, growing it to five.
    let codes: Vec<&str> = WrapperWarningCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        codes,
        vec![
            "WIKI_SCHEMA_ABSENT",
            "CLAUDE_READ_SCOPE_BROAD",
            "CODEX_READ_SCOPE_BROAD",
            "CLAUDE_ENABLED_PLUGINS_DECLARED",
            "VIEWER_UNAVAILABLE"
        ]
    );
    assert!(serde_json::from_str::<WrapperWarningCode>("\"INDEX_MAY_BE_STALE\"").is_err());
}

#[test]
fn model_warnings_normalize_to_provider_warning_after_wrapper_warnings() {
    let wrapper = vec![Warning::wrapper(
        WrapperWarningCode::CodexReadScopeBroad,
        "wrapper message",
    )];
    let model_warnings = vec![
        "a model warning".to_string(),
        "another model warning".to_string(),
    ];
    let ordered = order_warnings(wrapper, model_warnings);
    assert_eq!(ordered.len(), 3);
    assert_eq!(ordered[0].source, WarningSource::Wrapper);
    assert_eq!(ordered[0].code, "CODEX_READ_SCOPE_BROAD");
    assert_eq!(ordered[1].source, WarningSource::Provider);
    assert_eq!(ordered[1].code, PROVIDER_WARNING_CODE);
    assert_eq!(ordered[1].message, "a model warning");
    assert_eq!(ordered[2].source, WarningSource::Provider);
    assert_eq!(ordered[2].code, PROVIDER_WARNING_CODE);
    assert_eq!(ordered[2].message, "another model warning");
}

#[test]
fn raw_format_is_closed_to_claude_json_codex_jsonl_or_null() {
    assert_eq!(
        serde_json::to_string(&RawFormat::ClaudeJson).unwrap(),
        "\"claude-json\""
    );
    assert_eq!(
        serde_json::to_string(&RawFormat::CodexJsonl).unwrap(),
        "\"codex-jsonl\""
    );
    let none: Option<RawFormat> = None;
    assert_eq!(serde_json::to_string(&none).unwrap(), "null");
    assert!(serde_json::from_str::<RawFormat>("\"claude-jsonl\"").is_err());
}

#[test]
fn wiki_and_agent_are_null_when_argument_failure_precedes_resolution() {
    let v: serde_json::Value =
        serde_json::from_str(&render_json(&early_failure_envelope())).unwrap();
    assert!(v["wiki"].is_null());
    assert!(v["agent"].is_null());
    assert!(v["contract"].is_null());
    assert!(v["knowledge_status"].is_null());
    assert!(v["answer"].is_null());
    assert!(v["child_exit_code"].is_null());
    assert!(v["raw_format"].is_null());
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn human_mode_prints_answer_then_gaps_then_warnings_in_order() {
    let mut envelope = success_envelope();
    envelope.gaps = vec!["a documented gap".to_string()];
    let rendered = render_human(&envelope);
    let answer_pos = rendered.find("Grounded answer").expect("answer present");
    let gap_pos = rendered.find("a documented gap").expect("gap present");
    let warning_pos = rendered.find("Claude read tools").expect("warning present");
    assert!(answer_pos < gap_pos, "answer must precede gaps");
    assert!(gap_pos < warning_pos, "gaps must precede warnings");
}
