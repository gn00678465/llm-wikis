//! `wiki-query/v1` model result contract (spec §7.3).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::ErrorCode;

/// The fixed contract identifier every provider result must declare.
pub const WIKI_QUERY_CONTRACT: &str = "wiki-query/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Grounded,
    NoRelevantMaterial,
}

/// Both providers must produce this shape (spec §7.3). Closed to unknown fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelResult {
    pub contract: String,
    pub knowledge_status: KnowledgeStatus,
    pub answer: String,
    pub citations: Vec<String>,
    pub gaps: Vec<String>,
    pub warnings: Vec<String>,
}

/// Why raw provider-native output failed to become a valid `ModelResult`.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ModelResultError {
    #[error("provider native output was not valid wiki-query/v1 JSON: {0}")]
    Malformed(String),
    #[error("contract must equal \"wiki-query/v1\", got {0:?}")]
    WrongContract(String),
    #[error("answer must be a non-empty string")]
    EmptyAnswer,
    #[error("knowledge_status = grounded requires at least one citation")]
    GroundedWithoutCitations,
    #[error(
        "knowledge_status = no_relevant_material requires an empty citations array and at least one non-empty gap"
    )]
    NoRelevantMaterialInvalid,
}

impl ModelResultError {
    /// The public error code this validation failure maps to (spec §14): a
    /// structurally malformed document is `INVALID_NATIVE_OUTPUT`; a well-formed
    /// document that violates the `wiki-query/v1` rules is `CONTRACT_VIOLATION`.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            ModelResultError::Malformed(_) => ErrorCode::InvalidNativeOutput,
            ModelResultError::WrongContract(_)
            | ModelResultError::EmptyAnswer
            | ModelResultError::GroundedWithoutCitations
            | ModelResultError::NoRelevantMaterialInvalid => ErrorCode::ContractViolation,
        }
    }
}

impl ModelResult {
    /// Parses and validates raw provider-native JSON against the `wiki-query/v1`
    /// contract in one step.
    pub fn from_json(raw: &str) -> Result<Self, ModelResultError> {
        let value: ModelResult =
            serde_json::from_str(raw).map_err(|e| ModelResultError::Malformed(e.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validates the contract invariants (spec §7.3) on an already-deserialized result.
    pub fn validate(&self) -> Result<(), ModelResultError> {
        if self.contract != WIKI_QUERY_CONTRACT {
            return Err(ModelResultError::WrongContract(self.contract.clone()));
        }
        if self.answer.is_empty() {
            return Err(ModelResultError::EmptyAnswer);
        }
        match self.knowledge_status {
            KnowledgeStatus::Grounded => {
                if self.citations.is_empty() {
                    return Err(ModelResultError::GroundedWithoutCitations);
                }
            }
            KnowledgeStatus::NoRelevantMaterial => {
                let has_gap = self.gaps.iter().any(|g| !g.is_empty());
                if !self.citations.is_empty() || !has_gap {
                    return Err(ModelResultError::NoRelevantMaterialInvalid);
                }
            }
        }
        Ok(())
    }
}
