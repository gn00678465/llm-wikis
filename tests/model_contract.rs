use llm_wikis::error::ErrorCode;
use llm_wikis::model::{KnowledgeStatus, ModelResult, ModelResultError, WIKI_QUERY_CONTRACT};

#[test]
fn accepts_valid_grounded_result() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "grounded",
        "answer": "Grounded answer with [[wiki-slug]] citations.",
        "citations": ["wiki-slug"],
        "gaps": [],
        "warnings": []
    }"#;
    let result = ModelResult::from_json(json).expect("valid grounded result");
    assert_eq!(result.contract, WIKI_QUERY_CONTRACT);
    assert_eq!(result.knowledge_status, KnowledgeStatus::Grounded);
    assert_eq!(result.citations, vec!["wiki-slug".to_string()]);
}

#[test]
fn accepts_valid_no_relevant_material_result() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "no_relevant_material",
        "answer": "I could not find this in the wiki.",
        "citations": [],
        "gaps": ["No page covers this topic."],
        "warnings": []
    }"#;
    let result = ModelResult::from_json(json).expect("valid no_relevant_material result");
    assert_eq!(result.knowledge_status, KnowledgeStatus::NoRelevantMaterial);
    assert!(result.citations.is_empty());
}

#[test]
fn rejects_unknown_field() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "grounded",
        "answer": "Answer.",
        "citations": ["slug"],
        "gaps": [],
        "warnings": [],
        "extra_field": "not allowed"
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert!(matches!(err, ModelResultError::Malformed(_)));
    assert_eq!(err.error_code(), ErrorCode::InvalidNativeOutput);
}

#[test]
fn rejects_wrong_contract() {
    let json = r#"{
        "contract": "wiki-query/v2",
        "knowledge_status": "grounded",
        "answer": "Answer.",
        "citations": ["slug"],
        "gaps": [],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(
        err,
        ModelResultError::WrongContract("wiki-query/v2".to_string())
    );
    assert_eq!(err.error_code(), ErrorCode::ContractViolation);
}

#[test]
fn rejects_empty_answer() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "grounded",
        "answer": "",
        "citations": ["slug"],
        "gaps": [],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(err, ModelResultError::EmptyAnswer);
    assert_eq!(err.error_code(), ErrorCode::ContractViolation);
}

#[test]
fn rejects_grounded_without_citations() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "grounded",
        "answer": "Answer.",
        "citations": [],
        "gaps": [],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(err, ModelResultError::GroundedWithoutCitations);
}

#[test]
fn rejects_no_relevant_material_with_citations() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "no_relevant_material",
        "answer": "Answer.",
        "citations": ["slug"],
        "gaps": ["a gap"],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(err, ModelResultError::NoRelevantMaterialInvalid);
}

#[test]
fn rejects_no_relevant_material_without_gap() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "no_relevant_material",
        "answer": "Answer.",
        "citations": [],
        "gaps": [],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(err, ModelResultError::NoRelevantMaterialInvalid);
}

#[test]
fn rejects_no_relevant_material_with_only_empty_gap_strings() {
    let json = r#"{
        "contract": "wiki-query/v1",
        "knowledge_status": "no_relevant_material",
        "answer": "Answer.",
        "citations": [],
        "gaps": [""],
        "warnings": []
    }"#;
    let err = ModelResult::from_json(json).unwrap_err();
    assert_eq!(err, ModelResultError::NoRelevantMaterialInvalid);
}
