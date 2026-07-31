//! Citation grammar, resolution, and strictness tests (spec §11; plan Task 6
//! Steps 4-7).

use std::fs;
use std::path::{Path, PathBuf};

use llm_wikis::citations::{
    extract_inline_slugs, is_valid_slug, markdown_stems, resolve_and_check, resolve_citations,
};
use llm_wikis::error::{ErrorCode, ErrorDetails};
use llm_wikis::model::KnowledgeStatus;
use llm_wikis::wiki::list_markdown_files;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn wiki_flat() -> PathBuf {
    fixtures_dir().join("wiki-flat")
}

fn wiki_typed() -> PathBuf {
    fixtures_dir().join("wiki-typed")
}

// ---------------------------------------------------------------------------
// Step 4: citation grammar
// ---------------------------------------------------------------------------

#[test]
fn bare_form_extracts_the_slug() {
    assert_eq!(
        extract_inline_slugs("see [[getting-started]] for details"),
        vec!["getting-started".to_string()]
    );
}

#[test]
fn labelled_form_extracts_the_text_before_the_first_pipe() {
    assert_eq!(
        extract_inline_slugs("see [[getting-started|Getting Started Guide]] for details"),
        vec!["getting-started".to_string()]
    );
}

#[test]
fn markdown_form_extracts_the_slug() {
    assert_eq!(
        extract_inline_slugs("see [[getting-started](wiki/pages/getting-started.md)] for details"),
        vec!["getting-started".to_string()]
    );
}

#[test]
fn all_three_forms_in_one_answer_in_order() {
    let answer = "[[first]] then [[second|Second]] then [[third](x/y.md)]";
    assert_eq!(
        extract_inline_slugs(answer),
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ]
    );
}

#[test]
fn raw_and_assets_targets_are_ignored_not_rejected() {
    assert!(extract_inline_slugs("footnote: raw/source-notes.txt").is_empty());
    assert!(extract_inline_slugs("footnote: assets/diagram.svg").is_empty());
    assert!(extract_inline_slugs("[[raw/source-notes.txt]]").is_empty());
    assert!(extract_inline_slugs("[[assets/diagram.svg]]").is_empty());
}

#[test]
fn http_urls_and_ordinary_markdown_links_are_ignored() {
    assert!(extract_inline_slugs("see https://example.invalid/page for more").is_empty());
    assert!(extract_inline_slugs("see [an ordinary link](https://example.invalid/)").is_empty());
}

#[test]
fn malformed_and_dangling_wikilink_prose_is_ignored() {
    for prose in [
        "an unterminated [[dangling",
        "empty brackets [[]]",
        "just one bracket [not-a-wikilink]",
        "[[trailing-pipe-no-close|label",
        "[[markdown-no-close](path/no-close",
    ] {
        assert!(
            extract_inline_slugs(prose).is_empty(),
            "expected no slugs from {prose:?}"
        );
    }
}

#[test]
fn uppercase_or_underscored_slugs_are_ignored() {
    assert!(extract_inline_slugs("[[Getting-Started]]").is_empty());
    assert!(extract_inline_slugs("[[getting_started]]").is_empty());
}

#[test]
fn a_form_spanning_a_newline_is_ignored() {
    assert!(extract_inline_slugs("[[getting\n-started]]").is_empty());
    assert!(extract_inline_slugs("[[getting-started|label\nmore]]").is_empty());
    assert!(extract_inline_slugs("[[getting-started](path\n/x.md)]").is_empty());
}

#[test]
fn is_valid_slug_matches_exactly_a_dash_0_9_lowercase() {
    assert!(is_valid_slug("getting-started"));
    assert!(is_valid_slug("a1-b2-c3"));
    for bad in [
        "",
        "Getting-Started",
        "getting_started",
        "getting/started",
        "../getting-started",
        "getting-started#anchor",
        "https://example.invalid/getting-started",
        "getting started",
    ] {
        assert!(!is_valid_slug(bad), "{bad:?} must not be a valid slug");
    }
}

// ---------------------------------------------------------------------------
// Step 5: resolution
// ---------------------------------------------------------------------------

#[test]
fn resolution_consumes_a_supplied_file_list_without_touching_disk() {
    // No tempdir, no real files anywhere: the stem list is entirely fabricated,
    // proving resolution never walks a tree itself (spec §11.2).
    let stems = vec!["fabricated-page".to_string()];
    let citations = resolve_citations("agents", "see [[fabricated-page]]", &[], &stems).unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].wiki, "agents");
    assert_eq!(citations[0].slug, "fabricated-page");
}

#[test]
fn zero_matches_is_citation_not_found() {
    let stems = vec!["some-other-page".to_string()];
    let err = resolve_citations("agents", "", &["missing-page".to_string()], &stems).unwrap_err();
    assert_eq!(err.code, ErrorCode::CitationNotFound);
}

#[test]
fn two_or_more_matches_is_citation_ambiguous_with_slug_and_match_count_and_no_paths() {
    let stems = vec![
        "dup-page".to_string(),
        "dup-page".to_string(),
        "dup-page".to_string(),
    ];
    let err = resolve_citations("agents", "", &["dup-page".to_string()], &stems).unwrap_err();
    assert_eq!(err.code, ErrorCode::CitationAmbiguous);
    match err.details {
        Some(ErrorDetails::CitationAmbiguous(details)) => {
            assert_eq!(details.slug, "dup-page");
            assert_eq!(details.match_count, 3);
        }
        other => panic!("expected CitationAmbiguous details, got {other:?}"),
    }
}

#[test]
fn ambiguity_via_a_deliberately_duplicated_slug_fixture_case() {
    // A real fixture, not a fabricated list: two genuine `dup-page.md` files in
    // different subdirectories of the same synthetic tree.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wiki-ambiguous");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a").join("dup-page.md"), "# a copy").unwrap();
    fs::write(root.join("b").join("dup-page.md"), "# b copy").unwrap();

    let markdown_files = list_markdown_files(&root);
    let stems = markdown_stems(&markdown_files);
    let err = resolve_citations("agents", "", &["dup-page".to_string()], &stems).unwrap_err();
    assert_eq!(err.code, ErrorCode::CitationAmbiguous);
    match err.details {
        Some(ErrorDetails::CitationAmbiguous(details)) => {
            assert_eq!(details.slug, "dup-page");
            assert_eq!(details.match_count, 2);
        }
        other => panic!("expected CitationAmbiguous details, got {other:?}"),
    }
}

#[test]
fn windows_case_folding_does_not_authorize_a_case_mismatched_citation() {
    // A real file named with a capitalized stem; the filesystem walk records
    // its real on-disk casing. A lowercase citation for the "same" name under
    // Windows' case-insensitive path lookups must still fail to resolve,
    // because resolution is an exact string comparison over the walked stem
    // list, never a filesystem existence probe.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wiki-case");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Architecture.md"), "# capitalized on disk").unwrap();

    let markdown_files = list_markdown_files(&root);
    let stems = markdown_stems(&markdown_files);
    assert_eq!(stems, vec!["Architecture".to_string()]);

    let err = resolve_citations("agents", "", &["architecture".to_string()], &stems).unwrap_err();
    assert_eq!(err.code, ErrorCode::CitationNotFound);
}

// ---------------------------------------------------------------------------
// Step 6: strictness (run against both dual fixtures, spec §11.1 dual-fixture
// property; explicit array is strict, inline extraction is best-effort)
// ---------------------------------------------------------------------------

/// Locates the fixture's decoy-wikilinks page and a known-good real slug,
/// layout-agnostically (by filename, never by a hardcoded subdirectory path).
fn decoy_answer_and_valid_slug(fixture_root: &Path) -> (String, String) {
    let markdown_files = list_markdown_files(fixture_root);
    let decoy_path = markdown_files
        .iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some("decoy-wikilinks.md"))
        .expect("fixture must contain a decoy-wikilinks.md page");
    let decoy_answer = fs::read_to_string(decoy_path).unwrap();

    let stems = markdown_stems(&markdown_files);
    let valid_slug = stems
        .iter()
        .find(|s| is_valid_slug(s) && s.as_str() != "decoy-wikilinks")
        .expect("fixture must contain at least one other valid, resolvable slug")
        .clone();

    (decoy_answer, valid_slug)
}

fn assert_strictness_suite(fixture_root: &Path) {
    let (decoy_answer, valid_slug) = decoy_answer_and_valid_slug(fixture_root);
    let markdown_files = list_markdown_files(fixture_root);
    let stems = markdown_stems(&markdown_files);

    // The decoy page's inline wikilinks all resolve to nothing in this
    // fixture; every candidate is dropped silently rather than rejected.
    assert!(
        !extract_inline_slugs(&decoy_answer).is_empty(),
        "decoy fixture must contain at least one syntactically valid wikilink form"
    );

    // An answer whose inline slugs are all unresolvable but whose explicit
    // array resolves is accepted.
    let citations = resolve_and_check(
        "agents",
        KnowledgeStatus::Grounded,
        &decoy_answer,
        std::slice::from_ref(&valid_slug),
        &[],
        &stems,
    )
    .expect("array-resolved citation must be accepted despite dangling inline prose");
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].slug, valid_slug);

    // No resolvable citation from either source, with an empty array: a
    // contract violation, not a citation error.
    let err = resolve_and_check(
        "agents",
        KnowledgeStatus::Grounded,
        &decoy_answer,
        &[],
        &[],
        &stems,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);

    // no_relevant_material with a resolved citation (from inline extraction,
    // best-effort though it is) is a contract violation.
    let answer_with_resolvable_inline = format!("{decoy_answer}\n\nAlso see [[{valid_slug}]].");
    let err = resolve_and_check(
        "agents",
        KnowledgeStatus::NoRelevantMaterial,
        &answer_with_resolvable_inline,
        &[],
        &["a real gap".to_string()],
        &stems,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);

    // no_relevant_material without a non-empty gap is a contract violation,
    // even with zero resolved citations.
    let err = resolve_and_check(
        "agents",
        KnowledgeStatus::NoRelevantMaterial,
        &decoy_answer,
        &[],
        &[],
        &stems,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);

    let err = resolve_and_check(
        "agents",
        KnowledgeStatus::NoRelevantMaterial,
        &decoy_answer,
        &[],
        &[String::new()],
        &stems,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ContractViolation);

    // The correct no_relevant_material shape: zero resolved citations and a
    // genuine non-empty gap succeeds with an empty citation list.
    let citations = resolve_and_check(
        "agents",
        KnowledgeStatus::NoRelevantMaterial,
        &decoy_answer,
        &[],
        &["a real gap".to_string()],
        &stems,
    )
    .expect("zero resolved citations plus a non-empty gap must be accepted");
    assert!(citations.is_empty());
}

#[test]
fn strictness_suite_against_wiki_flat() {
    assert_strictness_suite(&wiki_flat());
}

#[test]
fn strictness_suite_against_wiki_typed() {
    assert_strictness_suite(&wiki_typed());
}

#[test]
fn explicit_array_unsafe_target_is_citation_invalid_fail_fast() {
    let stems = vec!["safe-page".to_string()];
    for bad in [
        "../traversal",
        "path/separator",
        "anchor#here",
        "https://example.invalid/x",
        "UPPERCASE",
        "under_score",
        "",
    ] {
        let err = resolve_citations("agents", "", &[bad.to_string()], &stems).unwrap_err();
        assert_eq!(err.code, ErrorCode::CitationInvalid, "{bad:?}");
    }
}

#[test]
fn explicit_array_fails_fast_before_later_elements_are_considered() {
    // The first bad element (invalid) is reported even though a later element
    // would have resolved fine; strict fail-fast in array order.
    let stems = vec!["good-page".to_string()];
    let err = resolve_citations(
        "agents",
        "",
        &["not valid".to_string(), "good-page".to_string()],
        &stems,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::CitationInvalid);
}

#[test]
fn inline_unresolvable_slugs_are_dropped_silently_not_rejected() {
    let stems = vec!["real-page".to_string()];
    let citations = resolve_citations(
        "agents",
        "[[does-not-exist]] but also [[real-page]]",
        &[],
        &stems,
    )
    .unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].slug, "real-page");
}

// ---------------------------------------------------------------------------
// Step 7: ordering and namespacing
// ---------------------------------------------------------------------------

#[test]
fn inline_slugs_in_answer_order_then_array_slugs_in_array_order_deduplicated() {
    let stems = vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
        "delta".to_string(),
    ];
    // Inline order: gamma, alpha (alpha repeats an array slug -> keep first
    // occurrence, which is the inline one). Array order: alpha, beta, delta.
    let answer = "first [[gamma]] then [[alpha]]";
    let array = vec!["alpha".to_string(), "beta".to_string(), "delta".to_string()];
    let citations = resolve_citations("agents", answer, &array, &stems).unwrap();
    let slugs: Vec<&str> = citations.iter().map(|c| c.slug.as_str()).collect();
    assert_eq!(slugs, vec!["gamma", "alpha", "beta", "delta"]);
}

#[test]
fn every_public_citation_carries_the_wrapper_supplied_wiki_id_never_a_model_provided_one() {
    let stems = vec!["alpha".to_string()];
    // Nothing in the grammar can express a namespace; even an attempt reads as
    // ordinary ignored prose or malformed wikilink syntax.
    let answer = "see other-wiki:alpha and [[alpha]] and [[other-wiki/alpha]]";
    let citations = resolve_citations("agents", answer, &[], &stems).unwrap();
    assert_eq!(citations.len(), 1);
    for c in &citations {
        assert_eq!(c.wiki, "agents");
    }

    let citations2 = resolve_citations("harness-engineering", answer, &[], &stems).unwrap();
    for c in &citations2 {
        assert_eq!(c.wiki, "harness-engineering");
    }
}
