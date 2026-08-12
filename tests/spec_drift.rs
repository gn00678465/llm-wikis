//! Parses `docs/2026-07-28-llm-wikis-external-query-design.md` and asserts, in both
//! directions, that the implementation agrees with three normative tables/sentences:
//! the Section 14 error table, the Section 15 `checks[].name` vocabulary, and the
//! Section 13 wrapper warning-code vocabulary. The specification stays the
//! authority — this test reports disagreement without deciding which side is
//! wrong. A parse failure (heading or anchor sentence not found) is itself a
//! signal that a normative table was restructured.

use std::collections::BTreeSet;
use std::fs;

use llm_wikis::error::ErrorCode;
use llm_wikis::output::{DOCTOR_CHECK_NAMES, WrapperWarningCode};

fn spec_text() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/2026-07-28-llm-wikis-external-query-design.md"
    );
    fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read spec at {path}: {e}"))
}

/// Extracts every `` `backtick` ``-delimited token from `s`, in order.
fn backtick_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                tokens.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    tokens
}

/// Returns the text strictly between `start_heading` (inclusive) and the next
/// occurrence of `end_heading`, or to end-of-file if `end_heading` is absent.
fn section<'a>(text: &'a str, start_heading: &str, end_heading: &str) -> &'a str {
    let start = text.find(start_heading).unwrap_or_else(|| {
        panic!("spec drift: heading {start_heading:?} not found - normative table restructured")
    });
    let rest = &text[start..];
    let end = rest.find(end_heading).unwrap_or(rest.len());
    &rest[..end]
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SpecErrorRow {
    code: String,
    exit: u16,
}

/// Parses `| \`CODE\` | N | meaning |` rows out of the Section 14 table, skipping
/// the header and separator rows (neither contains a backtick).
fn parse_error_table(section_text: &str) -> Vec<SpecErrorRow> {
    let mut rows = Vec::new();
    for line in section_text.lines() {
        let line = line.trim();
        if !line.starts_with('|') || !line.contains('`') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() < 3 {
            continue;
        }
        let Some(code) = backtick_tokens(cells[1]).into_iter().next() else {
            continue;
        };
        let Ok(exit) = cells[2].trim().parse::<u16>() else {
            continue;
        };
        rows.push(SpecErrorRow { code, exit });
    }
    rows
}

#[test]
fn section_14_error_table_matches_implemented_enum_bidirectionally() {
    let text = spec_text();
    let sec14 = section(&text, "## 14. Error and Exit Contract", "## 15. Doctor");
    let spec_rows = parse_error_table(sec14);
    assert_eq!(
        spec_rows.len(),
        27,
        "expected 27 error rows in the spec's Section 14 table, found {}: {spec_rows:?}",
        spec_rows.len()
    );

    let implemented: BTreeSet<(String, u16)> = ErrorCode::ALL
        .iter()
        .map(|c| (c.as_str().to_string(), c.exit_code() as u16))
        .collect();
    let spec_set: BTreeSet<(String, u16)> =
        spec_rows.iter().map(|r| (r.code.clone(), r.exit)).collect();

    let only_in_spec: Vec<_> = spec_set.difference(&implemented).collect();
    let only_in_impl: Vec<_> = implemented.difference(&spec_set).collect();
    assert!(
        only_in_spec.is_empty() && only_in_impl.is_empty(),
        "spec/implementation disagreement on Section 14 - only in spec: {only_in_spec:?}, only in implementation: {only_in_impl:?}"
    );
}

#[test]
fn section_15_check_names_match_implemented_set() {
    let text = spec_text();
    let anchor = "Doctor `checks[].name` is one of ";
    let idx = text.find(anchor).expect(
        "spec drift: Section 15 checks[].name sentence not found - normative text restructured",
    );
    let rest = &text[idx + anchor.len()..];
    let end = rest.find(". ").unwrap_or(rest.len());
    let sentence = &rest[..end];
    let spec_names: BTreeSet<String> = backtick_tokens(sentence).into_iter().collect();

    let implemented: BTreeSet<String> = DOCTOR_CHECK_NAMES.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        spec_names, implemented,
        "spec/implementation disagreement on Section 15 checks[].name vocabulary"
    );
    assert_eq!(spec_names.len(), 10);
}

#[test]
fn section_13_wrapper_warning_codes_match_implemented_set() {
    let text = spec_text();
    let anchor = "Wrapper warning codes are ";
    let idx = text.find(anchor).expect(
        "spec drift: Section 13 wrapper warning-code sentence not found - normative text restructured",
    );
    let rest = &text[idx + anchor.len()..];
    let end = rest.find(". ").unwrap_or(rest.len());
    let sentence = &rest[..end];
    let spec_codes: BTreeSet<String> = backtick_tokens(sentence).into_iter().collect();

    let implemented: BTreeSet<String> = WrapperWarningCode::ALL
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();

    assert_eq!(
        spec_codes, implemented,
        "spec/implementation disagreement on Section 13 wrapper warning-code vocabulary"
    );
    assert!(!spec_codes.contains("INDEX_MAY_BE_STALE"));
    assert!(!implemented.contains("INDEX_MAY_BE_STALE"));
}
