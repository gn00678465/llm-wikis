//! Public result envelope and renderers (spec §13). Rendering receives already
//! normalized envelopes and performs no provider parsing or path access.

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::model::KnowledgeStatus;

/// `schema_version` is this literal on every operation envelope (spec §13).
pub const SCHEMA_VERSION: &str = "1.0";

/// Doctor `checks[].name` vocabulary (spec §15). Defined here for later tasks
/// (doctor implementation) and consumed today only by the spec-drift test.
pub const DOCTOR_CHECK_NAMES: [&str; 9] = [
    "config",
    "roots",
    "wiki_structure",
    "entrypoint",
    "executable",
    "auth",
    "read_scope",
    "live_contract",
    "mutation",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

/// `raw_format` is exactly `claude-json`, `codex-jsonl`, or `null` (spec §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RawFormat {
    ClaudeJson,
    CodexJsonl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub wiki: String,
    pub slug: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningSource {
    Wrapper,
    Provider,
}

/// The closed wrapper warning-code vocabulary (spec §13/§15). `INDEX_MAY_BE_STALE`
/// must never exist here (Design Decision 8 / R-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WrapperWarningCode {
    WikiSchemaAbsent,
    ClaudeReadScopeBroad,
    CodexReadScopeBroad,
}

impl WrapperWarningCode {
    pub const ALL: [WrapperWarningCode; 3] = [
        WrapperWarningCode::WikiSchemaAbsent,
        WrapperWarningCode::ClaudeReadScopeBroad,
        WrapperWarningCode::CodexReadScopeBroad,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            WrapperWarningCode::WikiSchemaAbsent => "WIKI_SCHEMA_ABSENT",
            WrapperWarningCode::ClaudeReadScopeBroad => "CLAUDE_READ_SCOPE_BROAD",
            WrapperWarningCode::CodexReadScopeBroad => "CODEX_READ_SCOPE_BROAD",
        }
    }
}

/// The fixed code every normalized model-supplied warning string receives (spec §13).
pub const PROVIDER_WARNING_CODE: &str = "PROVIDER_WARNING";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub source: WarningSource,
    pub code: String,
    pub message: String,
}

impl Warning {
    pub fn wrapper(code: WrapperWarningCode, message: impl Into<String>) -> Self {
        Self {
            source: WarningSource::Wrapper,
            code: code.as_str().to_string(),
            message: message.into(),
        }
    }

    /// Normalizes a raw model-supplied warning string (spec §13).
    pub fn provider(message: impl Into<String>) -> Self {
        Self {
            source: WarningSource::Provider,
            code: PROVIDER_WARNING_CODE.to_string(),
            message: message.into(),
        }
    }
}

/// Orders wrapper warnings before provider warnings, each group in its given order
/// (spec §13: wrapper warnings appear first in deterministic generation order,
/// followed by provider warnings in model order).
pub fn order_warnings(wrapper: Vec<Warning>, model_warnings: Vec<String>) -> Vec<Warning> {
    let mut out = wrapper;
    out.extend(model_warnings.into_iter().map(Warning::provider));
    out
}

/// The public `wiki-query/v1` result envelope (spec §13). `wiki`, `agent`, `contract`,
/// `knowledge_status`, `answer`, `child_exit_code`, and `raw_format` serialize as
/// `null` for any stage of the query flow that has not yet produced them — most
/// notably every field but `error` when an argument failure precedes resolution.
#[derive(Debug, Clone, Serialize)]
pub struct QueryEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str,
    pub wiki: Option<WikiRef>,
    pub agent: Option<Agent>,
    pub contract: Option<&'static str>,
    pub knowledge_status: Option<KnowledgeStatus>,
    pub answer: Option<String>,
    pub citations: Vec<Citation>,
    pub gaps: Vec<String>,
    pub warnings: Vec<Warning>,
    pub duration_ms: u64,
    pub child_exit_code: Option<i32>,
    pub raw_format: Option<RawFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

/// Renders the envelope as exactly one compact JSON document plus one trailing
/// newline, for stdout. Diagnostics never belong in this string — they go to
/// stderr, outside this renderer's concern entirely.
pub fn render_json(envelope: &QueryEnvelope) -> String {
    let mut rendered = serde_json::to_string(envelope).expect("QueryEnvelope always serializes");
    rendered.push('\n');
    rendered
}

/// Renders the human-readable form: the answer, then gaps, then warnings, in that
/// order (spec plan Task 4, Step 4).
pub fn render_human(envelope: &QueryEnvelope) -> String {
    let mut out = String::new();
    if let Some(answer) = &envelope.answer {
        out.push_str(answer);
        out.push('\n');
    } else if let Some(err) = &envelope.error {
        out.push_str(&format!("error: {} ({})\n", err.code.as_str(), err.message));
    }
    if !envelope.gaps.is_empty() {
        out.push_str("\nGaps:\n");
        for gap in &envelope.gaps {
            out.push_str(&format!("- {gap}\n"));
        }
    }
    if !envelope.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for warning in &envelope.warnings {
            out.push_str(&format!("- [{}] {}\n", warning.code, warning.message));
        }
    }
    out
}
