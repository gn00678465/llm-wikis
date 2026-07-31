use std::collections::BTreeSet;

use llm_wikis::error::{
    AppError, CitationAmbiguousDetails, DetailsError, ErrorCode, ErrorDetails, IntegrityCheck,
    OutputTooLargeDetails, ReadOnlyViolationDetails, SecondaryError, Stream,
    apply_integrity_dominance, dominant_exit,
};

const REMOVED_CODES: [&str; 5] = [
    "LINK_STYLE_UNSUPPORTED",
    "INDEX_STALE",
    "QUERY_PROFILE_NOT_FOUND",
    "PROVIDER_PROFILE_MISSING",
    "CONTRACT_UNSUPPORTED",
];

#[test]
fn exactly_twenty_seven_codes_exist() {
    assert_eq!(ErrorCode::ALL.len(), 27);
}

#[test]
fn removed_codes_do_not_exist() {
    for code in REMOVED_CODES {
        let quoted = format!("\"{code}\"");
        assert!(
            serde_json::from_str::<ErrorCode>(&quoted).is_err(),
            "removed code {code} must not deserialize"
        );
        assert!(
            !ErrorCode::ALL.iter().any(|c| c.as_str() == code),
            "removed code {code} must not be in ErrorCode::ALL"
        );
    }
}

#[test]
fn every_code_maps_to_its_exact_exit_class() {
    let table: &[(ErrorCode, u8)] = &[
        (ErrorCode::ArgumentInvalid, 2),
        (ErrorCode::QuestionInvalidUtf8, 2),
        (ErrorCode::QuestionTooLarge, 2),
        (ErrorCode::ConfigInvalid, 2),
        (ErrorCode::ConfigExists, 2),
        (ErrorCode::WikiNotAllowed, 2),
        (ErrorCode::PathOutsideAllowedRoot, 2),
        (ErrorCode::UnsafeFilesystemEntry, 2),
        (ErrorCode::WikiInvalid, 2),
        (ErrorCode::ProviderConfigMissing, 2),
        (ErrorCode::AgentUnsupported, 2),
        (ErrorCode::EntrypointInvalid, 2),
        (ErrorCode::CitationInvalid, 2),
        (ErrorCode::CitationNotFound, 2),
        (ErrorCode::CitationAmbiguous, 2),
        (ErrorCode::CliNotFound, 3),
        (ErrorCode::AuthRequired, 3),
        (ErrorCode::EntrypointUnverified, 3),
        (ErrorCode::NonzeroExit, 4),
        (ErrorCode::Timeout, 5),
        (ErrorCode::OutputTooLarge, 5),
        (ErrorCode::TerminationFailed, 5),
        (ErrorCode::InvalidNativeOutput, 6),
        (ErrorCode::NoFinalMessage, 6),
        (ErrorCode::ContractViolation, 6),
        (ErrorCode::ReadOnlyViolation, 7),
        (ErrorCode::InternalError, 70),
    ];
    assert_eq!(table.len(), 27);
    for (code, expected_exit) in table {
        assert_eq!(code.exit_code(), *expected_exit, "{code:?}");
    }
    for code in ErrorCode::ALL {
        assert!(
            table.iter().any(|(c, _)| *c == code),
            "{code:?} missing from table"
        );
    }
}

#[test]
fn doctor_precedence_is_pure_max_over_failure_set() {
    // Ordering per spec §14: 70, 7, 6, 5, 4, 3, 2, 0.
    assert_eq!(dominant_exit(&[]), 0);
    assert_eq!(dominant_exit(&[ErrorCode::ArgumentInvalid]), 2);
    assert_eq!(
        dominant_exit(&[ErrorCode::ArgumentInvalid, ErrorCode::CliNotFound]),
        3
    );
    assert_eq!(
        dominant_exit(&[
            ErrorCode::ArgumentInvalid,
            ErrorCode::CliNotFound,
            ErrorCode::NonzeroExit
        ]),
        4
    );
    assert_eq!(
        dominant_exit(&[
            ErrorCode::ArgumentInvalid,
            ErrorCode::CliNotFound,
            ErrorCode::NonzeroExit,
            ErrorCode::Timeout
        ]),
        5
    );
    assert_eq!(
        dominant_exit(&[
            ErrorCode::ArgumentInvalid,
            ErrorCode::CliNotFound,
            ErrorCode::NonzeroExit,
            ErrorCode::Timeout,
            ErrorCode::ContractViolation
        ]),
        6
    );
    assert_eq!(
        dominant_exit(&[
            ErrorCode::ArgumentInvalid,
            ErrorCode::CliNotFound,
            ErrorCode::NonzeroExit,
            ErrorCode::Timeout,
            ErrorCode::ContractViolation,
            ErrorCode::ReadOnlyViolation
        ]),
        7
    );
    assert_eq!(
        dominant_exit(&[
            ErrorCode::ArgumentInvalid,
            ErrorCode::CliNotFound,
            ErrorCode::NonzeroExit,
            ErrorCode::Timeout,
            ErrorCode::ContractViolation,
            ErrorCode::ReadOnlyViolation,
            ErrorCode::InternalError
        ]),
        70
    );
    // Order-independence over the input slice.
    assert_eq!(
        dominant_exit(&[
            ErrorCode::InternalError,
            ErrorCode::ArgumentInvalid,
            ErrorCode::ReadOnlyViolation
        ]),
        70
    );
}

#[test]
fn read_only_violation_dominates_2_through_6_with_sanitized_secondary() {
    for primary_code in [
        ErrorCode::ArgumentInvalid,   // 2
        ErrorCode::CliNotFound,       // 3
        ErrorCode::NonzeroExit,       // 4
        ErrorCode::Timeout,           // 5
        ErrorCode::ContractViolation, // 6
    ] {
        let primary = AppError::new(primary_code, "a sanitized diagnostic message");
        let combined = apply_integrity_dominance(
            Some(primary),
            IntegrityCheck::Violated {
                changed_paths: vec!["wiki/pages/a.md".to_string()],
            },
        )
        .expect("valid details");
        assert_eq!(combined.code, ErrorCode::ReadOnlyViolation);
        assert_eq!(combined.code.exit_code(), 7);
        match combined.details.expect("details present") {
            ErrorDetails::ReadOnlyViolation(d) => {
                assert_eq!(d.changed_paths, vec!["wiki/pages/a.md".to_string()]);
                let secondary = d.secondary_error.expect("secondary error preserved");
                assert_eq!(secondary.code, primary_code);
            }
            other => panic!("unexpected details variant: {other:?}"),
        }
    }
}

#[test]
fn incomplete_integrity_comparison_becomes_internal_error() {
    let primary = AppError::new(
        ErrorCode::Timeout,
        "the provider exceeded the configured deadline",
    );
    let combined = apply_integrity_dominance(Some(primary), IntegrityCheck::Incomplete)
        .expect("internal error always constructible");
    assert_eq!(combined.code, ErrorCode::InternalError);
    assert_eq!(combined.code.exit_code(), 70);
    assert!(combined.details.is_none());
}

#[test]
fn incomplete_integrity_dominates_even_without_a_primary_failure() {
    let combined =
        apply_integrity_dominance(None, IntegrityCheck::Incomplete).expect("constructible");
    assert_eq!(combined.code, ErrorCode::InternalError);
}

#[test]
fn clean_integrity_passes_primary_through_unchanged() {
    let primary = AppError::new(
        ErrorCode::WikiNotAllowed,
        "wiki id is absent from the registry",
    );
    let combined = apply_integrity_dominance(Some(primary.clone()), IntegrityCheck::Clean).unwrap();
    assert_eq!(combined, primary);
}

#[test]
fn output_too_large_details_closed_and_validated() {
    let details = OutputTooLargeDetails::new(Stream::Stdout, 1_048_576, 1_048_577).unwrap();
    let value = serde_json::to_value(&details).unwrap();
    let keys: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["stream", "limit_bytes", "observed_bytes"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );

    assert!(matches!(
        OutputTooLargeDetails::new(Stream::Stdout, 100, 100),
        Err(DetailsError::OutputNotOverLimit)
    ));
    assert!(matches!(
        OutputTooLargeDetails::new(Stream::Stdout, 100, 50),
        Err(DetailsError::OutputNotOverLimit)
    ));

    assert!(serde_json::from_str::<Stream>("\"stdin\"").is_err());

    let mut bad = value.clone();
    bad.as_object_mut()
        .unwrap()
        .insert("path".into(), serde_json::json!("/etc/passwd"));
    assert!(serde_json::from_value::<OutputTooLargeDetails>(bad).is_err());
}

#[test]
fn citation_ambiguous_details_closed_no_paths_match_count_at_least_two() {
    assert!(matches!(
        CitationAmbiguousDetails::new("loop-engineering", 1),
        Err(DetailsError::AmbiguousMatchCountTooLow)
    ));
    let details = CitationAmbiguousDetails::new("loop-engineering", 2).unwrap();
    let value = serde_json::to_value(&details).unwrap();
    let keys: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["slug", "match_count"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert!(!value.to_string().contains("path"));

    let mut bad = value.clone();
    bad.as_object_mut()
        .unwrap()
        .insert("matched_paths".into(), serde_json::json!(["a.md", "b.md"]));
    assert!(serde_json::from_value::<CitationAmbiguousDetails>(bad).is_err());
}

#[test]
fn read_only_violation_details_closed_sorted_unique_relative() {
    assert!(matches!(
        ReadOnlyViolationDetails::new(vec![], None),
        Err(DetailsError::ChangedPathsEmpty)
    ));

    for bad_path in [
        "/abs/path.md",
        "wiki\\pages\\a.md",
        "../escape.md",
        "C:/abs.md",
        "",
    ] {
        let result = ReadOnlyViolationDetails::new(vec![bad_path.to_string()], None);
        assert!(result.is_err(), "expected rejection for {bad_path:?}");
    }

    assert!(matches!(
        ReadOnlyViolationDetails::new(vec!["b.md".to_string(), "a.md".to_string()], None),
        Err(DetailsError::ChangedPathsUnsorted)
    ));
    assert!(matches!(
        ReadOnlyViolationDetails::new(vec!["a.md".to_string(), "a.md".to_string()], None),
        Err(DetailsError::ChangedPathsUnsorted)
    ));

    let details = ReadOnlyViolationDetails::new(
        vec!["a.md".to_string(), "b.md".to_string()],
        Some(SecondaryError {
            code: ErrorCode::Timeout,
            message: "the provider exceeded the configured deadline".into(),
        }),
    )
    .unwrap();
    let value = serde_json::to_value(&details).unwrap();
    let keys: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        ["changed_paths", "secondary_error"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );

    let secondary_value = value.get("secondary_error").unwrap();
    let secondary_keys: BTreeSet<String> = secondary_value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        secondary_keys,
        ["code", "message"].iter().map(|s| s.to_string()).collect()
    );

    let mut bad = value.clone();
    bad.as_object_mut()
        .unwrap()
        .insert("content".into(), serde_json::json!("leaked file content"));
    assert!(serde_json::from_value::<ReadOnlyViolationDetails>(bad).is_err());
}

#[test]
fn app_error_rejects_details_variant_mismatched_with_code() {
    let mismatched = AppError::with_details(
        ErrorCode::CitationAmbiguous,
        "message",
        ErrorDetails::OutputTooLarge(OutputTooLargeDetails::new(Stream::Stdout, 1, 2).unwrap()),
    );
    assert!(matches!(mismatched, Err(DetailsError::CodeMismatch(_))));
}

#[test]
fn constructed_failure_json_contains_no_sensitive_leakage() {
    let fixtures = [
        AppError::new(
            ErrorCode::AuthRequired,
            "provider authentication is unavailable",
        ),
        AppError::new(
            ErrorCode::EntrypointUnverified,
            "the selected entrypoint fingerprint has not passed a current live doctor probe",
        ),
        apply_integrity_dominance(
            Some(AppError::new(
                ErrorCode::Timeout,
                "the provider exceeded the configured deadline",
            )),
            IntegrityCheck::Violated {
                changed_paths: vec!["wiki/pages/harness-engineering.md".to_string()],
            },
        )
        .unwrap(),
    ];
    let forbidden = [
        "Traceback",
        "sha256:",
        "C:\\",
        "D:\\Wikis",
        "/home/",
        "panicked at",
    ];
    for fixture in fixtures {
        let json = serde_json::to_string(&fixture).unwrap();
        for pattern in forbidden {
            assert!(
                !json.contains(pattern),
                "leaked sensitive pattern {pattern:?} in {json}"
            );
        }
    }
}
