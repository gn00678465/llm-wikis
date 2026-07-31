//! Citation grammar, resolution, and strictness (spec §11).
//!
//! The wrapper reads no schema file and selects no parser: one fixed grammar
//! (§11.1) is always applied. Extraction is a bounded scanner over the answer
//! string — never a general Markdown parser and never a regex (none is added
//! to `Cargo.toml`).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::error::{AppError, CitationAmbiguousDetails, ErrorCode, ErrorDetails};
use crate::model::KnowledgeStatus;
use crate::output::Citation;

// ---------------------------------------------------------------------------
// §11.1 grammar
// ---------------------------------------------------------------------------

/// The slug charset: `[a-z0-9-]+` (spec §11.1), and nothing else.
fn is_slug_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

/// Whether `s` is, in its entirety, a valid page-provenance slug. Used for the
/// model's explicit `citations` array, which is validated as whole strings
/// (spec §11.3: "does not match `[a-z0-9-]+`" is itself one `CITATION_INVALID`
/// ground, alongside path separators/traversal/anchors/URLs/unsafe characters —
/// all of which this single closed-charset check already excludes).
pub fn is_valid_slug(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_slug_char)
}

/// Extracts every inline page-provenance slug from `answer`, in the order each
/// occurrence appears (spec §11.1). Recognizes exactly the three closed forms
/// (`[[slug]]`, `[[slug|label]]`, `[[slug](any/path)]`); anything else —
/// `raw/<file>`, `assets/<file>`, HTTP(S) URLs, `[[raw/...]]`, `[[assets/...]]`,
/// ordinary Markdown links, malformed or dangling wiki-link prose, uppercase or
/// underscored slugs, and any form spanning a newline — is silently skipped,
/// never rejected. Duplicates are preserved here; deduplication happens later,
/// after resolution (spec §11.4), by first occurrence across both sources.
pub fn extract_inline_slugs(answer: &str) -> Vec<String> {
    let chars: Vec<char> = answer.chars().collect();
    let len = chars.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < len {
        if chars[i] == '['
            && chars[i + 1] == '['
            && let Some((slug, next)) = scan_wikilink(&chars, i)
        {
            out.push(slug);
            i = next;
            continue;
        }
        i += 1;
    }
    out
}

/// Attempts to parse one wikilink starting at `start` (where `chars[start..start+2]`
/// is known to be `"[["`). Returns the extracted slug and the index just past the
/// form's closing bracket on success, or `None` if what follows is not one of the
/// three closed forms (malformed/dangling prose, or a form spanning a newline).
fn scan_wikilink(chars: &[char], start: usize) -> Option<(String, usize)> {
    let len = chars.len();
    let mut k = start + 2;
    let mut slug = String::new();
    while k < len && is_slug_char(chars[k]) {
        slug.push(chars[k]);
        k += 1;
    }
    if slug.is_empty() || k >= len || chars[k] == '\n' {
        return None;
    }
    match chars[k] {
        ']' => {
            // Bare form: "]]" immediately follows the slug.
            if k + 1 < len && chars[k + 1] == ']' {
                return Some((slug, k + 2));
            }
            // Markdown form: "](any/path)]" follows the slug.
            if k + 1 < len && chars[k + 1] == '(' {
                let mut m = k + 2;
                while m < len && chars[m] != ')' && chars[m] != '\n' {
                    m += 1;
                }
                if m >= len || chars[m] != ')' {
                    return None; // dangling, or a newline intervened
                }
                let after = m + 1;
                if after < len && chars[after] == ']' {
                    return Some((slug, after + 1));
                }
            }
            None
        }
        '|' => {
            // Labelled form: label text up to "]]", never crossing a newline.
            let mut m = k + 1;
            while m < len && chars[m] != '\n' {
                if chars[m] == ']' && m + 1 < len && chars[m + 1] == ']' {
                    return Some((slug, m + 2));
                }
                m += 1;
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// §11.2 resolution
// ---------------------------------------------------------------------------

/// The exact, case-sensitive ASCII filename stem of every markdown file in a
/// supplied file list (spec §11.2). Takes the list as input rather than
/// walking a tree itself — the caller (preflight, and later the mutation
/// snapshot) owns the single directory walk.
pub fn markdown_stems(markdown_files: &[PathBuf]) -> Vec<String> {
    markdown_files
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .filter_map(|name| name.strip_suffix(".md"))
        .map(str::to_string)
        .collect()
}

/// Resolves one slug against a supplied list of exact filename stems (spec
/// §11.2). Matching is an exact ASCII string comparison — independent of any
/// filesystem case-folding the host applies when the stems were listed, so a
/// case-mismatched citation is never authorized.
fn resolve_slug(slug: &str, stems: &[String]) -> Result<(), AppError> {
    let match_count = stems.iter().filter(|s| s.as_str() == slug).count();
    match match_count {
        0 => Err(AppError::new(
            ErrorCode::CitationNotFound,
            format!("citation slug {slug:?} does not match any page beneath content_root"),
        )),
        1 => Ok(()),
        n => {
            let details = CitationAmbiguousDetails::new(slug, n as u32)
                .expect("match_count > 1 checked above");
            Err(AppError::with_details(
                ErrorCode::CitationAmbiguous,
                format!("citation slug {slug:?} matches multiple pages beneath content_root"),
                ErrorDetails::CitationAmbiguous(details),
            )
            .expect("code/variant pairing is valid"))
        }
    }
}

// ---------------------------------------------------------------------------
// §11.3 strictness, §11.4 ordering and namespacing
// ---------------------------------------------------------------------------

/// Resolves both citation sources and orders/dedupes the result (spec §11.3,
/// §11.4). The explicit `citations` array is strict and fails fast on the
/// first unsafe or unresolvable element, in array order; slugs extracted
/// inline from `answer` are best-effort and unresolvable ones are dropped
/// silently. `wiki_id` is wrapper-supplied and stamped onto every public
/// citation — nothing in the grammar can smuggle a different namespace in.
pub fn resolve_citations(
    wiki_id: &str,
    answer: &str,
    array: &[String],
    stems: &[String],
) -> Result<Vec<Citation>, AppError> {
    let mut resolved_array = Vec::new();
    for raw in array {
        if !is_valid_slug(raw) {
            return Err(AppError::new(
                ErrorCode::CitationInvalid,
                format!("citation {raw:?} is not a valid page-provenance slug"),
            ));
        }
        resolve_slug(raw, stems)?;
        resolved_array.push(raw.clone());
    }

    let resolved_inline: Vec<String> = extract_inline_slugs(answer)
        .into_iter()
        .filter(|slug| resolve_slug(slug, stems).is_ok())
        .collect();

    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for slug in resolved_inline.into_iter().chain(resolved_array) {
        if seen.insert(slug.clone()) {
            ordered.push(slug);
        }
    }

    Ok(ordered
        .into_iter()
        .map(|slug| Citation {
            wiki: wiki_id.to_string(),
            slug,
        })
        .collect())
}

/// `resolve_citations` plus the `knowledge_status` contract rule (spec §11.3):
/// `grounded` requires at least one citation that resolves from either
/// source, after deduplication; `no_relevant_material` requires zero resolved
/// citations and at least one non-empty gap. Either violation is
/// `CONTRACT_VIOLATION`, never a citation error — the wrapper never infers
/// status from prose.
pub fn resolve_and_check(
    wiki_id: &str,
    knowledge_status: KnowledgeStatus,
    answer: &str,
    array: &[String],
    gaps: &[String],
    stems: &[String],
) -> Result<Vec<Citation>, AppError> {
    let citations = resolve_citations(wiki_id, answer, array, stems)?;
    match knowledge_status {
        KnowledgeStatus::Grounded => {
            if citations.is_empty() {
                return Err(AppError::new(
                    ErrorCode::ContractViolation,
                    "knowledge_status = grounded requires at least one citation that resolves",
                ));
            }
        }
        KnowledgeStatus::NoRelevantMaterial => {
            let has_gap = gaps.iter().any(|g| !g.is_empty());
            if !citations.is_empty() || !has_gap {
                return Err(AppError::new(
                    ErrorCode::ContractViolation,
                    "knowledge_status = no_relevant_material requires zero resolved citations and a non-empty gap",
                ));
            }
        }
    }
    Ok(citations)
}
