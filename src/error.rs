//! Public error/exit contract (spec §14) and closed error-detail schemas (spec §12/§13).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The complete public error code vocabulary (spec §14). Exactly 27 codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ArgumentInvalid,
    QuestionInvalidUtf8,
    QuestionTooLarge,
    ConfigInvalid,
    ConfigExists,
    WikiNotAllowed,
    PathOutsideAllowedRoot,
    UnsafeFilesystemEntry,
    WikiInvalid,
    ProviderConfigMissing,
    AgentUnsupported,
    EntrypointInvalid,
    CitationInvalid,
    CitationNotFound,
    CitationAmbiguous,
    CliNotFound,
    AuthRequired,
    EntrypointUnverified,
    NonzeroExit,
    Timeout,
    OutputTooLarge,
    TerminationFailed,
    InvalidNativeOutput,
    NoFinalMessage,
    ContractViolation,
    ReadOnlyViolation,
    InternalError,
}

impl ErrorCode {
    /// Every implemented code, in specification Section 14 table order.
    pub const ALL: [ErrorCode; 27] = [
        ErrorCode::ArgumentInvalid,
        ErrorCode::QuestionInvalidUtf8,
        ErrorCode::QuestionTooLarge,
        ErrorCode::ConfigInvalid,
        ErrorCode::ConfigExists,
        ErrorCode::WikiNotAllowed,
        ErrorCode::PathOutsideAllowedRoot,
        ErrorCode::UnsafeFilesystemEntry,
        ErrorCode::WikiInvalid,
        ErrorCode::ProviderConfigMissing,
        ErrorCode::AgentUnsupported,
        ErrorCode::EntrypointInvalid,
        ErrorCode::CitationInvalid,
        ErrorCode::CitationNotFound,
        ErrorCode::CitationAmbiguous,
        ErrorCode::CliNotFound,
        ErrorCode::AuthRequired,
        ErrorCode::EntrypointUnverified,
        ErrorCode::NonzeroExit,
        ErrorCode::Timeout,
        ErrorCode::OutputTooLarge,
        ErrorCode::TerminationFailed,
        ErrorCode::InvalidNativeOutput,
        ErrorCode::NoFinalMessage,
        ErrorCode::ContractViolation,
        ErrorCode::ReadOnlyViolation,
        ErrorCode::InternalError,
    ];

    /// The wire string for this code, e.g. `"ARGUMENT_INVALID"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ArgumentInvalid => "ARGUMENT_INVALID",
            ErrorCode::QuestionInvalidUtf8 => "QUESTION_INVALID_UTF8",
            ErrorCode::QuestionTooLarge => "QUESTION_TOO_LARGE",
            ErrorCode::ConfigInvalid => "CONFIG_INVALID",
            ErrorCode::ConfigExists => "CONFIG_EXISTS",
            ErrorCode::WikiNotAllowed => "WIKI_NOT_ALLOWED",
            ErrorCode::PathOutsideAllowedRoot => "PATH_OUTSIDE_ALLOWED_ROOT",
            ErrorCode::UnsafeFilesystemEntry => "UNSAFE_FILESYSTEM_ENTRY",
            ErrorCode::WikiInvalid => "WIKI_INVALID",
            ErrorCode::ProviderConfigMissing => "PROVIDER_CONFIG_MISSING",
            ErrorCode::AgentUnsupported => "AGENT_UNSUPPORTED",
            ErrorCode::EntrypointInvalid => "ENTRYPOINT_INVALID",
            ErrorCode::CitationInvalid => "CITATION_INVALID",
            ErrorCode::CitationNotFound => "CITATION_NOT_FOUND",
            ErrorCode::CitationAmbiguous => "CITATION_AMBIGUOUS",
            ErrorCode::CliNotFound => "CLI_NOT_FOUND",
            ErrorCode::AuthRequired => "AUTH_REQUIRED",
            ErrorCode::EntrypointUnverified => "ENTRYPOINT_UNVERIFIED",
            ErrorCode::NonzeroExit => "NONZERO_EXIT",
            ErrorCode::Timeout => "TIMEOUT",
            ErrorCode::OutputTooLarge => "OUTPUT_TOO_LARGE",
            ErrorCode::TerminationFailed => "TERMINATION_FAILED",
            ErrorCode::InvalidNativeOutput => "INVALID_NATIVE_OUTPUT",
            ErrorCode::NoFinalMessage => "NO_FINAL_MESSAGE",
            ErrorCode::ContractViolation => "CONTRACT_VIOLATION",
            ErrorCode::ReadOnlyViolation => "READ_ONLY_VIOLATION",
            ErrorCode::InternalError => "INTERNAL_ERROR",
        }
    }

    /// The documented process exit code for this error class (spec §14).
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorCode::ArgumentInvalid
            | ErrorCode::QuestionInvalidUtf8
            | ErrorCode::QuestionTooLarge
            | ErrorCode::ConfigInvalid
            | ErrorCode::ConfigExists
            | ErrorCode::WikiNotAllowed
            | ErrorCode::PathOutsideAllowedRoot
            | ErrorCode::UnsafeFilesystemEntry
            | ErrorCode::WikiInvalid
            | ErrorCode::ProviderConfigMissing
            | ErrorCode::AgentUnsupported
            | ErrorCode::EntrypointInvalid
            | ErrorCode::CitationInvalid
            | ErrorCode::CitationNotFound
            | ErrorCode::CitationAmbiguous => 2,
            ErrorCode::CliNotFound | ErrorCode::AuthRequired | ErrorCode::EntrypointUnverified => 3,
            ErrorCode::NonzeroExit => 4,
            ErrorCode::Timeout | ErrorCode::OutputTooLarge | ErrorCode::TerminationFailed => 5,
            ErrorCode::InvalidNativeOutput
            | ErrorCode::NoFinalMessage
            | ErrorCode::ContractViolation => 6,
            ErrorCode::ReadOnlyViolation => 7,
            ErrorCode::InternalError => 70,
        }
    }
}

/// Doctor/query exit-class dominance (spec §14): the highest exit class present in a
/// mixed failure set wins; an empty set means success (`0`). Pure function — the
/// ordering `70, 7, 6, 5, 4, 3, 2, 0` falls straight out of taking the maximum.
pub fn dominant_exit(failures: &[ErrorCode]) -> u8 {
    failures.iter().map(|c| c.exit_code()).max().unwrap_or(0)
}

/// `stream` in `OUTPUT_TOO_LARGE` details (spec §13): exactly `stdout` or `stderr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Closed `OUTPUT_TOO_LARGE` details (spec §13): exactly `{stream, limit_bytes, observed_bytes}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputTooLargeDetails {
    pub stream: Stream,
    pub limit_bytes: u64,
    pub observed_bytes: u64,
}

/// Closed `CITATION_AMBIGUOUS` details (spec §13): exactly `{slug, match_count}`, no paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitationAmbiguousDetails {
    pub slug: String,
    pub match_count: u32,
}

/// The displaced primary failure preserved under a dominating `READ_ONLY_VIOLATION`
/// (spec §12/§13). Structurally carries no `details` field, so it can never nest one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecondaryError {
    pub code: ErrorCode,
    pub message: String,
}

/// Closed `READ_ONLY_VIOLATION` details (spec §12/§13): exactly
/// `{changed_paths, secondary_error?}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyViolationDetails {
    pub changed_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub secondary_error: Option<SecondaryError>,
}

/// `error.details`, closed to unknown keys per code (spec §13). Untagged: the wire
/// shape is exactly the inner struct's fields, with no extra discriminant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorDetails {
    OutputTooLarge(OutputTooLargeDetails),
    CitationAmbiguous(CitationAmbiguousDetails),
    ReadOnlyViolation(ReadOnlyViolationDetails),
}

/// Validation failures for the closed detail schemas and their code correspondence.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DetailsError {
    #[error("OUTPUT_TOO_LARGE requires observed_bytes > limit_bytes")]
    OutputNotOverLimit,
    #[error("CITATION_AMBIGUOUS requires match_count >= 2")]
    AmbiguousMatchCountTooLow,
    #[error("READ_ONLY_VIOLATION changed_paths must be non-empty")]
    ChangedPathsEmpty,
    #[error("READ_ONLY_VIOLATION changed_paths must be sorted and unique")]
    ChangedPathsUnsorted,
    #[error("READ_ONLY_VIOLATION changed_paths entry {0:?} is not a relative slash-separated path")]
    ChangedPathNotRelative(String),
    #[error("details variant does not match error code {0}")]
    CodeMismatch(&'static str),
}

impl OutputTooLargeDetails {
    pub fn new(
        stream: Stream,
        limit_bytes: u64,
        observed_bytes: u64,
    ) -> Result<Self, DetailsError> {
        if observed_bytes <= limit_bytes {
            return Err(DetailsError::OutputNotOverLimit);
        }
        Ok(Self {
            stream,
            limit_bytes,
            observed_bytes,
        })
    }
}

impl CitationAmbiguousDetails {
    pub fn new(slug: impl Into<String>, match_count: u32) -> Result<Self, DetailsError> {
        if match_count < 2 {
            return Err(DetailsError::AmbiguousMatchCountTooLow);
        }
        Ok(Self {
            slug: slug.into(),
            match_count,
        })
    }
}

/// A relative, slash-separated path with no traversal and no drive/absolute prefix.
fn is_relative_slash_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

impl ReadOnlyViolationDetails {
    pub fn new(
        changed_paths: Vec<String>,
        secondary_error: Option<SecondaryError>,
    ) -> Result<Self, DetailsError> {
        if changed_paths.is_empty() {
            return Err(DetailsError::ChangedPathsEmpty);
        }
        for p in &changed_paths {
            if !is_relative_slash_path(p) {
                return Err(DetailsError::ChangedPathNotRelative(p.clone()));
            }
        }
        let mut sorted = changed_paths.clone();
        sorted.sort();
        sorted.dedup();
        if sorted != changed_paths {
            return Err(DetailsError::ChangedPathsUnsorted);
        }
        Ok(Self {
            changed_paths,
            secondary_error,
        })
    }
}

/// A public failure: required `code`/`message` plus an optional closed `details` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub details: Option<ErrorDetails>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Attaches `details`, rejecting any code/variant combination the spec does not
    /// define (§13 defines closed detail schemas only for these three codes).
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
    ) -> Result<Self, DetailsError> {
        let matches_code = matches!(
            (code, &details),
            (ErrorCode::OutputTooLarge, ErrorDetails::OutputTooLarge(_))
                | (
                    ErrorCode::CitationAmbiguous,
                    ErrorDetails::CitationAmbiguous(_)
                )
                | (
                    ErrorCode::ReadOnlyViolation,
                    ErrorDetails::ReadOnlyViolation(_)
                )
        );
        if !matches_code {
            return Err(DetailsError::CodeMismatch(code.as_str()));
        }
        Ok(Self {
            code,
            message: message.into(),
            details: Some(details),
        })
    }

    /// The sanitized `{code, message}` pair preserved as `secondary_error` when this
    /// failure is displaced by a dominating `READ_ONLY_VIOLATION` (spec §12/§13).
    pub fn to_secondary(&self) -> SecondaryError {
        SecondaryError {
            code: self.code,
            message: self.message.clone(),
        }
    }
}

/// Outcome of the mandatory before/after content-root integrity comparison (spec §12).
#[derive(Debug, Clone, PartialEq)]
pub enum IntegrityCheck {
    Clean,
    Violated { changed_paths: Vec<String> },
    Incomplete,
}

/// Applies the `READ_ONLY_VIOLATION` / `INTERNAL_ERROR` dominance rule (spec §12, §14):
/// an incomplete integrity comparison always becomes `INTERNAL_ERROR`; a violated
/// comparison always becomes `READ_ONLY_VIOLATION` and preserves any primary failure
/// as a sanitized `secondary_error`; a clean comparison passes the primary through.
pub fn apply_integrity_dominance(
    primary: Option<AppError>,
    integrity: IntegrityCheck,
) -> Result<AppError, DetailsError> {
    match integrity {
        IntegrityCheck::Incomplete => Ok(AppError::new(
            ErrorCode::InternalError,
            "the content-root integrity comparison could not be completed",
        )),
        IntegrityCheck::Violated { changed_paths } => {
            let secondary_error = primary.map(|e| e.to_secondary());
            let details = ReadOnlyViolationDetails::new(changed_paths, secondary_error)?;
            AppError::with_details(
                ErrorCode::ReadOnlyViolation,
                "a protected wiki path, type, or content digest changed",
                ErrorDetails::ReadOnlyViolation(details),
            )
        }
        IntegrityCheck::Clean => Ok(primary.unwrap_or_else(|| {
            AppError::new(
                ErrorCode::InternalError,
                "no failure and no integrity violation",
            )
        })),
    }
}
