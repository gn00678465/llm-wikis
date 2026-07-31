//! Disposable measurement spike for the layout-agnostic wrapper direction.
//!
//! Read-only. Never writes anywhere except the JSON output path.
//! Evidence only — must not be copied into production.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

// ---------------------------------------------------------------- targets

struct Target {
    id: &'static str,
    root: &'static str,
    index_rel: Option<&'static str>,
    note: &'static str,
}

const TARGETS: &[Target] = &[
    Target {
        id: "A",
        root: r"D:\Wikis\agents",
        index_rel: Some("wiki/index.md"),
        note: "content_root, equal to project_root",
    },
    Target {
        id: "B",
        root: r"D:\Wikis\harness-engineering\wiki",
        index_rel: Some("index.md"),
        note: "content_root, strict containment",
    },
    Target {
        id: "C",
        root: r"D:\Wikis\harness-engineering",
        index_rel: None,
        note: "deliberately wrong root",
    },
];

/// Directories excluded by standing decision D2 — provider skill trees are
/// covered by skill_fingerprint, not by the content snapshot.
const D2_EXCLUDED: &[&str] = &[".claude", ".agents"];

// ---------------------------------------------------------------- walking

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Dir,
    File,
}

struct Entry {
    rel: String,
    kind: Kind,
    len: Option<u64>,
    sha: Option<String>,
    mtime: Option<SystemTime>,
}

#[cfg(windows)]
fn is_special(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_special(md: &fs::Metadata) -> bool {
    md.file_type().is_symlink()
}

struct WalkOpts {
    exclude_d2: bool,
    hash: bool,
}

struct Walked {
    entries: Vec<Entry>,
    specials: Vec<String>,
    errors: Vec<String>,
}

fn walk(root: &Path, opts: &WalkOpts) -> Walked {
    let mut out = Walked { entries: Vec::new(), specials: Vec::new(), errors: Vec::new() };
    let mut stack = vec![(root.to_path_buf(), String::new())];

    while let Some((dir, rel_prefix)) = stack.pop() {
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                out.errors.push(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        for item in rd {
            let item = match item {
                Ok(i) => i,
                Err(e) => {
                    out.errors.push(format!("{}: {e}", dir.display()));
                    continue;
                }
            };
            let name = item.file_name().to_string_lossy().into_owned();
            let rel = if rel_prefix.is_empty() {
                name.clone()
            } else {
                format!("{rel_prefix}/{name}")
            };
            let path = item.path();

            // symlink_metadata: do not follow, so a reparse point is visible.
            let md = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    out.errors.push(format!("{rel}: {e}"));
                    continue;
                }
            };

            if is_special(&md) {
                out.specials.push(rel.clone());
                continue; // never descend into a reparse point
            }

            if md.is_dir() {
                if opts.exclude_d2 && rel_prefix.is_empty() && D2_EXCLUDED.contains(&name.as_str())
                {
                    continue;
                }
                out.entries.push(Entry {
                    rel: rel.clone(),
                    kind: Kind::Dir,
                    len: None,
                    sha: None,
                    mtime: md.modified().ok(),
                });
                stack.push((path, rel));
            } else if md.is_file() {
                let sha = if opts.hash {
                    match hash_file(&path) {
                        Ok(h) => Some(h),
                        Err(e) => {
                            out.errors.push(format!("{rel}: {e}"));
                            None
                        }
                    }
                } else {
                    None
                };
                out.entries.push(Entry {
                    rel,
                    kind: Kind::File,
                    len: Some(md.len()),
                    sha,
                    mtime: md.modified().ok(),
                });
            }
        }
    }

    out.entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    out.specials.sort();
    out.errors.sort();
    out
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

// ---------------------------------------------------------------- slugs

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Superset grammar: `[[slug]]`, `[[slug|label]]`, `[[slug](any/path)]`.
/// Returns (slug, form) where form is "bare", "labelled", or "markdown".
fn extract_citations(text: &str) -> Vec<(String, &'static str)> {
    let b = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0usize;

    while i + 1 < b.len() {
        if b[i] != b'[' || b[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let start = i + 2;
        let mut j = start;
        let mut cut: Option<usize> = None; // position of `|`
        let mut form: Option<&'static str> = None;
        let mut end_slug = None;
        let mut resume = None;

        while j < b.len() {
            if b[j] == b'\n' {
                break; // a wiki link does not span lines
            }
            if b[j] == b'|' && cut.is_none() {
                cut = Some(j);
                j += 1;
                continue;
            }
            if b[j] == b']' && j + 1 < b.len() && b[j + 1] == b']' {
                end_slug = Some(cut.unwrap_or(j));
                form = Some(if cut.is_some() { "labelled" } else { "bare" });
                resume = Some(j + 2);
                break;
            }
            if b[j] == b']' && j + 1 < b.len() && b[j + 1] == b'(' {
                // `[[slug](path)]`
                let mut k = j + 2;
                while k < b.len() && b[k] != b')' && b[k] != b'\n' {
                    k += 1;
                }
                if k + 1 < b.len() && b[k] == b')' && b[k + 1] == b']' {
                    end_slug = Some(cut.unwrap_or(j));
                    form = Some("markdown");
                    resume = Some(k + 2);
                }
                break;
            }
            j += 1;
        }

        match (end_slug, form, resume) {
            (Some(e), Some(f), Some(r)) => {
                let slug = &text[start..e];
                if is_slug(slug) {
                    found.push((slug.to_string(), f));
                }
                i = r;
            }
            _ => i += 2,
        }
    }
    found
}

// ---------------------------------------------------------------- checks

fn fmt_time(t: Option<SystemTime>) -> String {
    match t.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()) {
        Some(d) => format!("{}", d.as_secs()),
        None => "?".into(),
    }
}

fn run_target(t: &Target) -> Value {
    let root = PathBuf::from(t.root);
    println!("\n================ TARGET {} — {} ================", t.id, t.root);
    println!("({})", t.note);

    // ---- check 1: minimum preflight
    let exists = root.exists();
    let is_dir = root.is_dir();
    let full = walk(&root, &WalkOpts { exclude_d2: true, hash: false });
    let first_md = full
        .entries
        .iter()
        .find(|e| e.kind == Kind::File && e.rel.to_ascii_lowercase().ends_with(".md"))
        .map(|e| e.rel.clone());
    let md_count = full
        .entries
        .iter()
        .filter(|e| e.kind == Kind::File && e.rel.to_ascii_lowercase().ends_with(".md"))
        .count();
    let preflight_pass = exists && is_dir && first_md.is_some();
    println!("\n[1] minimum preflight");
    println!("    exists={exists} is_dir={is_dir} md_files={md_count}");
    println!("    first .md: {}", first_md.clone().unwrap_or("<none>".into()));
    println!("    VERDICT: {}", if preflight_pass { "PASS" } else { "FAIL" });

    // SCHEMA.md at content root?
    let schema_at_root = root.join("SCHEMA.md").is_file();
    println!("    SCHEMA.md at content root: {schema_at_root}  (absent => WIKI_SCHEMA_ABSENT warning)");

    // ---- check 2: special entries
    let no_excl = walk(&root, &WalkOpts { exclude_d2: false, hash: false });
    println!("\n[2] special filesystem entries");
    println!("    without D2 exclusion: {}", no_excl.specials.len());
    for s in &no_excl.specials {
        println!("        {s}");
    }
    println!("    with    D2 exclusion: {}", full.specials.len());
    for s in &full.specials {
        println!("        {s}");
    }

    // ---- check 3: snapshot stability and cost
    let t0 = Instant::now();
    let s1 = walk(&root, &WalkOpts { exclude_d2: true, hash: true });
    let cold = t0.elapsed();
    let t1 = Instant::now();
    let s2 = walk(&root, &WalkOpts { exclude_d2: true, hash: true });
    let warm = t1.elapsed();

    let m1: BTreeMap<&str, (&Option<u64>, &Option<String>)> = s1
        .entries
        .iter()
        .map(|e| (e.rel.as_str(), (&e.len, &e.sha)))
        .collect();
    let m2: BTreeMap<&str, (&Option<u64>, &Option<String>)> = s2
        .entries
        .iter()
        .map(|e| (e.rel.as_str(), (&e.len, &e.sha)))
        .collect();
    let mut diffs: Vec<String> = Vec::new();
    for (k, v) in &m1 {
        match m2.get(k) {
            None => diffs.push(format!("removed: {k}")),
            Some(v2) if v.0 != v2.0 || v.1 != v2.1 => diffs.push(format!("changed: {k}")),
            _ => {}
        }
    }
    for k in m2.keys() {
        if !m1.contains_key(k) {
            diffs.push(format!("added: {k}"));
        }
    }
    let total_bytes: u64 = s1.entries.iter().filter_map(|e| e.len).sum();
    println!("\n[3] snapshot stability and cost");
    println!(
        "    entries={} (files={}) bytes={} cold={:?} warm={:?}",
        s1.entries.len(),
        s1.entries.iter().filter(|e| e.kind == Kind::File).count(),
        total_bytes,
        cold,
        warm
    );
    println!("    run-to-run differences: {}", diffs.len());
    for d in diffs.iter().take(20) {
        println!("        {d}");
    }
    if !s1.errors.is_empty() {
        println!("    read errors: {}", s1.errors.len());
        for e in s1.errors.iter().take(10) {
            println!("        {e}");
        }
    }

    // ---- check 4: slug index and ambiguity
    let mut stems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in s1.entries.iter().filter(|e| e.kind == Kind::File) {
        let name = e.rel.rsplit('/').next().unwrap_or(&e.rel);
        if let Some(stem) = name.strip_suffix(".md") {
            if is_slug(stem) {
                stems.entry(stem.to_string()).or_default().push(e.rel.clone());
            }
        }
    }
    let ambiguous: Vec<(&String, &Vec<String>)> =
        stems.iter().filter(|(_, v)| v.len() > 1).collect();
    println!("\n[4] slug index and ambiguity");
    println!("    slug-shaped stems: {}", stems.len());
    println!("    ambiguous stems (>=2 files): {}", ambiguous.len());
    for (k, v) in ambiguous.iter().take(30) {
        println!("        {k}");
        for p in v.iter() {
            println!("            {p}");
        }
    }

    // ---- check 5: citation grammar extraction
    let mut total = 0usize;
    let mut by_form: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut distinct: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut unresolved: Vec<(String, &'static str, String)> = Vec::new();
    for e in s1.entries.iter().filter(|e| e.kind == Kind::File) {
        if !e.rel.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        let text = match fs::read_to_string(root.join(e.rel.replace('/', "\\"))) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (slug, form) in extract_citations(&text) {
            total += 1;
            *by_form.entry(form).or_default() += 1;
            distinct.entry(slug.clone()).or_insert(form);
            if !stems.contains_key(&slug) {
                unresolved.push((slug, form, e.rel.clone()));
            }
        }
    }
    let mut unresolved_distinct: BTreeMap<&String, (&str, &String)> = BTreeMap::new();
    for (s, f, src) in &unresolved {
        unresolved_distinct.entry(s).or_insert((f, src));
    }
    println!("\n[5] citation grammar extraction (superset)");
    println!("    total occurrences: {total}");
    for (f, n) in &by_form {
        println!("        {f:<10} {n}");
    }
    println!("    distinct slugs: {}", distinct.len());
    println!(
        "    unresolved occurrences: {}  (distinct: {})",
        unresolved.len(),
        unresolved_distinct.len()
    );
    for (s, (f, src)) in unresolved_distinct.iter().take(20) {
        println!("        {s}  [{f}]  in {src}");
    }

    // ---- check 6: index freshness by mtime
    let mut freshness = json!(null);
    println!("\n[6] index freshness by mtime");
    match t.index_rel {
        None => println!("    no index_path declared for this target — skipped"),
        Some(rel) => {
            let idx = s1.entries.iter().find(|e| e.rel == rel);
            match idx {
                None => println!("    declared index {rel} NOT FOUND"),
                Some(idx) => {
                    let idx_t = idx.mtime;
                    let mut newer: Vec<(&str, Option<SystemTime>)> = s1
                        .entries
                        .iter()
                        .filter(|e| {
                            e.kind == Kind::File
                                && e.rel != rel
                                && e.rel.to_ascii_lowercase().ends_with(".md")
                                && match (e.mtime, idx_t) {
                                    (Some(a), Some(b)) => a > b,
                                    _ => false,
                                }
                        })
                        .map(|e| (e.rel.as_str(), e.mtime))
                        .collect();
                    newer.sort_by(|a, b| b.1.cmp(&a.1));
                    let audit_only = newer
                        .iter()
                        .all(|(p, _)| p.rsplit('/').next().unwrap_or(p).starts_with("audit-"));
                    println!("    index: {rel}  mtime={}", fmt_time(idx_t));
                    println!("    newer .md files: {}", newer.len());
                    for (p, tm) in newer.iter().take(10) {
                        println!("        {p}   mtime={}", fmt_time(*tm));
                    }
                    if !newer.is_empty() {
                        println!(
                            "    all newer files are audit-*.md: {audit_only}  \
                             (if true, excluding audit-* clears the stale verdict)"
                        );
                    }
                    freshness = json!({
                        "index": rel,
                        "newer_count": newer.len(),
                        "stale": !newer.is_empty(),
                        "all_newer_are_audit": audit_only,
                        "newest": newer.iter().take(10)
                            .map(|(p, t)| json!({"path": p, "mtime": fmt_time(*t)}))
                            .collect::<Vec<_>>(),
                    });
                }
            }
        }
    }

    json!({
        "id": t.id,
        "root": t.root,
        "note": t.note,
        "preflight": {
            "exists": exists,
            "is_dir": is_dir,
            "md_files": md_count,
            "first_md": first_md,
            "pass": preflight_pass,
            "schema_md_at_root": schema_at_root,
        },
        "special_entries": {
            "without_d2_exclusion": no_excl.specials,
            "with_d2_exclusion": full.specials,
        },
        "snapshot": {
            "entries": s1.entries.len(),
            "files": s1.entries.iter().filter(|e| e.kind == Kind::File).count(),
            "total_bytes": total_bytes,
            "cold_ms": cold.as_millis(),
            "warm_ms": warm.as_millis(),
            "run_to_run_differences": diffs,
            "read_errors": s1.errors,
        },
        "slugs": {
            "stem_count": stems.len(),
            "ambiguous": ambiguous.iter()
                .map(|(k, v)| json!({"slug": k, "paths": v}))
                .collect::<Vec<_>>(),
        },
        "citations": {
            "total_occurrences": total,
            "by_form": by_form.iter()
                .map(|(k, v)| (k.to_string(), json!(v)))
                .collect::<serde_json::Map<String, Value>>(),
            "distinct_slugs": distinct.len(),
            "unresolved_occurrences": unresolved.len(),
            "unresolved_distinct": unresolved_distinct.len(),
            "unresolved_examples": unresolved_distinct.iter().take(20)
                .map(|(s, (f, src))| json!({"slug": s, "form": f, "in": src}))
                .collect::<Vec<_>>(),
        },
        "freshness": freshness,
    })
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| {
        r".scratch\spec-plan-correction\spike-results.json".to_string()
    });

    let results: Vec<Value> = TARGETS.iter().map(run_target).collect();
    let doc = json!({
        "spike": "layout-agnostic",
        "rustc": option_env!("CARGO_PKG_VERSION"),
        "targets": results,
    });

    match fs::write(&out_path, serde_json::to_string_pretty(&doc).unwrap()) {
        Ok(()) => println!("\n\nJSON written to {out_path}"),
        Err(e) => eprintln!("\n\nfailed to write {out_path}: {e}"),
    }
}
