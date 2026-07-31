//! Step 11: query each registered wiki's real entrypoint (spec §7.1, §10.2,
//! §10.3, §11.2, §12; plan Task 2 Step 11). **Consumes model quota and
//! touches real knowledge bases, read-only.** Runs only when explicitly
//! authorized; this file is invoked directly by the four commands in the
//! plan/checklist, one (wiki, agent) pair per process.

use crate::report::Report;
use process_wrap::std::CommandWrap;
#[cfg(windows)]
use process_wrap::std::JobObject;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const QUESTION: &str = "In one sentence, what topic area does this wiki cover?";
const TIMEOUT: Duration = Duration::from_secs(300);
const STDOUT_CAP: usize = 1_048_576; // spec §6 runtime default max_stdout_bytes
const STDERR_CAP: usize = 65_536; // spec §6 runtime default max_stderr_bytes

struct WikiConfig {
    id: &'static str,
    project_root: &'static str,
    content_root: &'static str,
    query_prompt: &'static str,
    claude_entrypoint: &'static str,
    codex_entrypoint: &'static str,
}

// Verbatim from spec §6's example registry — not guessed.
const AGENTS: WikiConfig = WikiConfig {
    id: "agents",
    project_root: r"D:\Wikis\agents",
    content_root: r"D:\Wikis\agents",
    query_prompt: "Use the wiki-query skill to answer from this wiki.",
    claude_entrypoint: "/wiki-query",
    codex_entrypoint: "$wiki-query",
};

const HARNESS: WikiConfig = WikiConfig {
    id: "harness-engineering",
    project_root: r"D:\Wikis\harness-engineering",
    content_root: r"D:\Wikis\harness-engineering\wiki",
    query_prompt: "Use the llm-wiki skill's query workflow to answer from this wiki.",
    claude_entrypoint: "/llm-wiki",
    codex_entrypoint: "$llm-wiki",
};

pub fn run(args: &[String]) -> i32 {
    let mut wiki_arg = None;
    let mut agent_arg = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wiki" => wiki_arg = it.next().cloned(),
            "--agent" => agent_arg = it.next().cloned(),
            _ => {}
        }
    }
    let (Some(wiki_arg), Some(agent_arg)) = (wiki_arg, agent_arg) else {
        eprintln!("usage: provider-contract --wiki <agents|harness> --agent <claude|codex>");
        return 2;
    };

    let wiki = match wiki_arg.as_str() {
        "agents" => &AGENTS,
        "harness" | "harness-engineering" => &HARNESS,
        other => {
            eprintln!("unknown --wiki {other}");
            return 2;
        }
    };

    let mut r = Report::new("provider-contract");
    r.check("args_parsed", true, format!("wiki={} agent={agent_arg}", wiki.id));

    let content_root = match canonical_no_verbatim_prefix(Path::new(wiki.content_root)) {
        Ok(p) => p,
        Err(e) => {
            r.check("content_root_canonicalizes", false, e);
            return r.finish();
        }
    };

    let before = match snapshot_content_root(&content_root) {
        Ok(s) => s,
        Err(e) => {
            r.check("before_snapshot", false, e);
            return r.finish();
        }
    };
    r.check("before_snapshot", true, format!("{} entries", before.len()));

    let scratch = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            r.check("scratch_tempdir", false, format!("{e}"));
            return r.finish();
        }
    };
    let schema_json = result_schema_json();

    let prompt = match agent_arg.as_str() {
        "claude" => build_prompt(wiki.claude_entrypoint, wiki, &content_root, "claude"),
        "codex" => build_prompt(wiki.codex_entrypoint, wiki, &content_root, "codex"),
        other => {
            eprintln!("unknown --agent {other}");
            return 2;
        }
    };

    let outcome = match agent_arg.as_str() {
        "claude" => {
            let mcp_config_path = scratch.path().join("mcp-config.json");
            if let Err(e) = std::fs::write(&mcp_config_path, "{\"mcpServers\":{}}") {
                r.check("write_mcp_config", false, format!("{e}"));
                return r.finish();
            }
            invoke_claude(wiki, &prompt, &schema_json, &mcp_config_path)
        }
        "codex" => {
            let schema_path = scratch.path().join("output-schema.json");
            if let Err(e) = std::fs::write(&schema_path, &schema_json) {
                r.check("write_output_schema", false, format!("{e}"));
                return r.finish();
            }
            invoke_codex(wiki, &prompt, &schema_path)
        }
        _ => unreachable!(),
    };

    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            r.check("provider_invocation", false, e);
            // Snapshot comparison still runs even on invocation failure (spec §12).
            let after = snapshot_content_root(&content_root);
            let identical = after.as_ref().map(|a| a == &before).unwrap_or(false);
            r.check("snapshot_identical_after_failure", identical, format!("after_snapshot_ok={}", after.is_ok()));
            return r.finish();
        }
    };

    r.check(
        "child_exit_status",
        !outcome.timed_out,
        format!(
            "exit_code={:?} duration_ms={} timed_out={}",
            outcome.exit_code,
            outcome.duration.as_millis(),
            outcome.timed_out
        ),
    );
    r.check(
        "stdout_stderr_within_caps",
        outcome.stdout.len() <= STDOUT_CAP && outcome.stderr.len() <= STDERR_CAP,
        format!("stdout_bytes={} stderr_bytes={}", outcome.stdout.len(), outcome.stderr.len()),
    );

    let after = match snapshot_content_root(&content_root) {
        Ok(s) => s,
        Err(e) => {
            r.check("after_snapshot", false, e);
            return r.finish();
        }
    };
    let identical = after == before;
    r.check(
        "content_root_snapshot_byte_identical",
        identical,
        format!("before_entries={} after_entries={} identical={identical}", before.len(), after.len()),
    );

    let mut codex_item_types: Vec<String> = Vec::new(); // precise set, drives the forbidden-action check
    if agent_arg == "codex" {
        let (recursive_count, recursive_types) = codex_event_type_summary(&outcome.stdout);
        let (precise_count, precise_types) = codex_precise_item_types(&outcome.stdout);
        r.check(
            "codex_jsonl_event_structure",
            true, // observational
            format!(
                "recursive_event_count={recursive_count} recursive_types={recursive_types:?} | precise_event_count={precise_count} precise_item_types={precise_types:?}"
            ),
        );
        codex_item_types = precise_types;

        let ordered = codex_ordered_event_types(&outcome.stdout);
        r.check(
            "codex_ordered_event_sequence",
            true, // observational
            format!("{ordered:?}"),
        );

        let item_details = codex_item_details(&outcome.stdout);
        r.check(
            "codex_item_detail",
            true, // observational
            format!("{item_details:?}"),
        );
    }

    let result_obj = match agent_arg.as_str() {
        "claude" => extract_claude_result(&outcome.stdout),
        "codex" => extract_codex_result(&outcome.stdout),
        _ => unreachable!(),
    };

    match result_obj {
        Some(obj) => {
            let (valid, problems) = validate_contract(&obj);
            r.check(
                "result_matches_wiki_query_v1",
                valid,
                format!("problems={problems:?}"),
            );

            let citations = citations_of(&obj);
            let resolved: Vec<(String, usize)> = citations
                .iter()
                .map(|slug| (slug.clone(), resolve_citation(&before, slug)))
                .collect();
            let resolved_count = resolved.iter().filter(|(_, n)| *n == 1).count();
            let status = obj.get("knowledge_status").and_then(|v| v.as_str()).unwrap_or("");
            // §7.3: "grounded" requires >=1 resolvable citation; "no_relevant_material"
            // requires an *empty* citation array, so zero citations there is correct,
            // not a failure of this check.
            let citations_ok = if status == "no_relevant_material" {
                citations.is_empty()
            } else {
                !citations.is_empty() && resolved_count >= 1
            };
            r.check(
                "citations_found_and_resolved",
                citations_ok,
                format!("status={status} citations={citations:?} resolution_counts={resolved:?}"),
            );

            let knowledge_status = obj.get("knowledge_status").and_then(|v| v.as_str()).unwrap_or("?");
            let gaps: Vec<String> = obj.get("gaps").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            let warnings: Vec<String> = obj.get("warnings").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            r.check(
                "result_shape_summary",
                true,
                format!(
                    "knowledge_status={knowledge_status} citations_count={} gaps_count={} warnings_count={}",
                    citations.len(), gaps.len(), warnings.len()
                ),
            );
            // Requested verbatim (not truncated): these are the model's own
            // tooling/process commentary, not wiki content — distinct from
            // `answer`, which is deliberately never recorded raw elsewhere
            // in this file.
            r.check(
                "gaps_and_warnings_verbatim",
                true, // observational
                format!("gaps={gaps:?} warnings={warnings:?}"),
            );
        }
        None => {
            r.check("result_matches_wiki_query_v1", false, "no object matching the six required keys found in provider output");
        }
    }

    let (text_detected, markers) = scan_forbidden_action_signals(&outcome.stdout, &outcome.stderr);
    // Codex's own item-type taxonomy is a second, structural signal: any
    // non-agent_message/text/thread/turn item type appearing (e.g.
    // mcp_tool_call, command_execution, file_change, apply_patch) means the
    // model attempted a capability beyond plain skill invocation.
    const BENIGN_CODEX_TYPES: [&str; 7] = [
        "agent_message", "text", "thread.started", "turn.started", "turn.completed",
        "item.started", "item.completed",
    ];
    let structural_types: Vec<&String> = codex_item_types
        .iter()
        .filter(|t| !BENIGN_CODEX_TYPES.contains(&t.as_str()))
        .collect();
    let forbidden_detected = text_detected || !structural_types.is_empty();
    r.check(
        "forbidden_action_attempt_signal",
        true, // observational, not pass/fail
        format!(
            "detected={forbidden_detected} text_markers={markers:?} codex_non_message_item_types={structural_types:?}"
        ),
    );

    r.finish()
}

// ---- envelope construction (spec §7.1) ----

fn json_str(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

/// spec §7.1 (R-26, v0.2.3): constraints are provider-specific — only the
/// second entry differs, because telling Codex "no shell tool is available"
/// is false (command execution, sandboxed read-only, is Codex's only read
/// mechanism) and left it unable to read any file (see Row 11 addendum 2).
/// Text is copied verbatim from the spec, not paraphrased.
fn build_prompt(entrypoint: &str, wiki: &WikiConfig, content_root: &Path, agent: &str) -> String {
    let required_result_block = r#"{
    "contract": "wiki-query/v1",
    "knowledge_status": "grounded | no_relevant_material",
    "answer": "string",
    "citations": ["bare page slug"],
    "gaps": ["string"],
    "warnings": ["string"]
  }"#;
    let second_constraint = if agent == "codex" {
        "Reading wiki files with read-only commands is permitted; the sandbox enforces read-only. Do not attempt writes, index regeneration, installs, or network access."
    } else {
        "Do not run scripts or shell commands; no such tool is available."
    };
    let constraints_block = format!(
        r#"[
    "Read only. Do not write, save, commit, log, cache, or regenerate anything.",
    {},
    "Do not offer to save the answer.",
    "Use the content_root above; do not infer a different wiki location.",
    "Answer only from this wiki; do not fill gaps from general knowledge.",
    "Return the required_result object as your final output."
  ]"#,
        json_str(second_constraint),
    );
    let external_query = format!(
        "EXTERNAL_QUERY:\n{{\n  \"contract\": \"wiki-query/v1\",\n  \"mode\": \"external-readonly\",\n  \"wiki_id\": {},\n  \"content_root\": {},\n  \"question\": {},\n  \"required_result\": {},\n  \"constraints\": {}\n}}",
        json_str(wiki.id),
        json_str(&content_root.display().to_string()),
        json_str(QUESTION),
        required_result_block,
        constraints_block,
    );
    format!("{entrypoint}\n\n{}\n\n{external_query}\n", wiki.query_prompt)
}

fn result_schema_json() -> String {
    serde_json::json!({
        "type": "object",
        "properties": {
            "contract": {"type": "string", "const": "wiki-query/v1"},
            "knowledge_status": {"type": "string", "enum": ["grounded", "no_relevant_material"]},
            "answer": {"type": "string"},
            "citations": {"type": "array", "items": {"type": "string"}},
            "gaps": {"type": "array", "items": {"type": "string"}},
            "warnings": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["contract", "knowledge_status", "answer", "citations", "gaps", "warnings"],
        "additionalProperties": false
    })
    .to_string()
}

// ---- process supervision (spec §10.1/§10.2/§10.3) ----

struct RunOutcome {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration: Duration,
    timed_out: bool,
}

fn spawn_capped_reader(
    mut reader: impl Read + Send + 'static,
    cap: usize,
    label: &'static str,
    tx: mpsc::Sender<(&'static str, Vec<u8>)>,
) {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() >= cap {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send((label, buf));
    });
}

fn run_capped(mut wrap: CommandWrap, stdin_payload: &str) -> Result<RunOutcome, String> {
    let start = Instant::now();
    let mut child = wrap.spawn().map_err(|e| format!("spawn error: {e}"))?;
    {
        let mut stdin = child.stdin().take().ok_or("child has no piped stdin")?;
        stdin
            .write_all(stdin_payload.as_bytes())
            .map_err(|e| format!("write stdin: {e}"))?;
        // stdin dropped here, closing it.
    }
    let stdout = child.stdout().take().ok_or("child has no piped stdout")?;
    let stderr = child.stderr().take().ok_or("child has no piped stderr")?;

    let (tx, rx) = mpsc::channel();
    spawn_capped_reader(stdout, STDOUT_CAP, "stdout", tx.clone());
    spawn_capped_reader(stderr, STDERR_CAP, "stderr", tx);

    let mut collected: std::collections::HashMap<&str, Vec<u8>> = std::collections::HashMap::new();
    let mut timed_out = false;
    for _ in 0..2 {
        let remaining = TIMEOUT.checked_sub(start.elapsed()).unwrap_or(Duration::ZERO);
        match rx.recv_timeout(remaining) {
            Ok((label, bytes)) => {
                collected.insert(label, bytes);
            }
            Err(_) => {
                timed_out = true;
                break;
            }
        }
    }
    if timed_out {
        let _ = child.start_kill();
    }
    let wait_res = child.wait();

    Ok(RunOutcome {
        exit_code: wait_res.ok().and_then(|s| s.code()),
        stdout: collected.remove("stdout").unwrap_or_default(),
        stderr: collected.remove("stderr").unwrap_or_default(),
        duration: start.elapsed(),
        timed_out,
    })
}

fn make_wrap(program: &str, build: impl FnOnce(&mut std::process::Command)) -> CommandWrap {
    let mut wrap = CommandWrap::with_new(program, |cmd| {
        build(cmd);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    });
    #[cfg(windows)]
    wrap.wrap(JobObject);
    wrap
}

fn invoke_claude(
    wiki: &WikiConfig,
    prompt: &str,
    schema_json: &str,
    mcp_config_path: &Path,
) -> Result<RunOutcome, String> {
    let wrap = make_wrap("claude", |cmd| {
        cmd.current_dir(wiki.project_root);
        cmd.arg("--add-dir").arg(wiki.content_root);
        cmd.arg("-p");
        cmd.args(["--input-format", "text"]);
        cmd.arg("--no-session-persistence");
        cmd.args(["--permission-mode", "dontAsk"]);
        cmd.args(["--tools", "Read,Grep,Glob"]);
        cmd.arg("--strict-mcp-config");
        cmd.arg("--mcp-config").arg(mcp_config_path);
        cmd.args(["--output-format", "json"]);
        cmd.arg("--json-schema").arg(schema_json);
    });
    run_capped(wrap, prompt)
}

fn invoke_codex(wiki: &WikiConfig, prompt: &str, schema_path: &Path) -> Result<RunOutcome, String> {
    let wrap = make_wrap("codex", |cmd| {
        cmd.args(["--ask-for-approval", "never"]);
        cmd.arg("exec");
        cmd.arg("-C").arg(wiki.project_root);
        cmd.args(["--sandbox", "read-only"]);
        cmd.arg("--ephemeral");
        cmd.arg("--skip-git-repo-check");
        cmd.arg("--ignore-user-config");
        // spec 0.2.2 / R-25 correction: --ignore-user-config alone did not
        // exclude MCP/browser/computer capabilities (see the mcp_tool_call
        // finding in the preflight report's prior Row 11). Explicitly zero
        // the MCP server table and disable both non-read-tool capabilities.
        cmd.args(["-c", "mcp_servers={}"]);
        cmd.args(["--disable", "browser_use"]);
        cmd.args(["--disable", "computer_use"]);
        cmd.arg("--output-schema").arg(schema_path);
        cmd.arg("--json");
        cmd.arg("-");
    });
    run_capped(wrap, prompt)
}

// ---- content-root snapshot (spec §12) ----

fn canonical_no_verbatim_prefix(p: &Path) -> Result<PathBuf, String> {
    let c = p.canonicalize().map_err(|e| format!("canonicalize {}: {e}", p.display()))?;
    let s = c.to_string_lossy();
    Ok(match s.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => c,
    })
}

fn snapshot_content_root(root: &Path) -> Result<BTreeMap<String, (u64, String)>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            let path = entry.path();
            let rel = path.strip_prefix(root).expect("child of root");
            if rel.components().count() == 1 {
                if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
                    if name == ".claude" || name == ".agents" {
                        continue; // spec §6.1/§12 exclusion
                    }
                }
            }
            let ft = entry.file_type().map_err(|e| format!("file_type: {e}"))?;
            if ft.is_symlink() {
                return Err(format!("UNSAFE_FILESYSTEM_ENTRY at {}", rel.display()));
            } else if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                let hash = format!("{:x}", hasher.finalize());
                let relstr = rel.to_string_lossy().replace('\\', "/");
                out.insert(relstr, (bytes.len() as u64, hash));
            }
        }
    }
    Ok(out)
}

fn resolve_citation(snapshot: &BTreeMap<String, (u64, String)>, slug: &str) -> usize {
    snapshot
        .keys()
        .filter(|k| {
            let p = Path::new(k);
            p.extension().and_then(|e| e.to_str()) == Some("md")
                && p.file_stem().and_then(|s| s.to_str()) == Some(slug)
        })
        .count()
}

// ---- result extraction (spec §10.2/§10.3) and validation (spec §7.3) ----

/// Recursively searches a JSON value tree (including JSON embedded as
/// string content) for an object containing all six `wiki-query/v1` keys.
fn find_result_object(v: &Value) -> Option<Value> {
    const KEYS: [&str; 6] = ["contract", "knowledge_status", "answer", "citations", "gaps", "warnings"];
    match v {
        Value::Object(o) => {
            if KEYS.iter().all(|k| o.contains_key(*k)) {
                return Some(v.clone());
            }
            o.values().find_map(find_result_object)
        }
        Value::Array(a) => a.iter().find_map(find_result_object),
        Value::String(s) => serde_json::from_str::<Value>(s).ok().as_ref().and_then(find_result_object),
        _ => None,
    }
}

fn extract_claude_result(stdout: &[u8]) -> Option<Value> {
    let v: Value = serde_json::from_slice(stdout).ok()?;
    // Prefer structured_output per spec §10.2 requirement "prefer structured_output".
    if let Some(so) = v.get("structured_output") {
        if let Some(found) = find_result_object(so) {
            return Some(found);
        }
    }
    find_result_object(&v)
}

fn extract_codex_result(stdout: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(stdout);
    let events: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect();
    events.iter().rev().find_map(find_result_object)
}

/// Structural-only diagnostic (event count + distinct "type" key values) —
/// never event content — to understand the actual codex-jsonl event shape
/// this exact codex version emits, without leaking prose.
fn codex_event_type_summary(stdout: &[u8]) -> (usize, Vec<String>) {
    fn collect_types(v: &Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if let Some(t) = o.get("type").and_then(|x| x.as_str()) {
                    out.insert(t.to_string());
                }
                for val in o.values() {
                    collect_types(val, out);
                }
            }
            Value::Array(a) => {
                for val in a {
                    collect_types(val, out);
                }
            }
            _ => {}
        }
    }
    let text = String::from_utf8_lossy(stdout);
    let mut types = std::collections::BTreeSet::new();
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            count += 1;
            collect_types(&v, &mut types);
        }
    }
    (count, types.into_iter().collect())
}

/// Precise item-type parse: the top-level "type" of each JSONL event, plus
/// — only for "item.started"/"item.completed" events — the "type" field of
/// that event's immediate "item" object. Does not recurse into unrelated
/// nested structures, unlike `codex_event_type_summary` above, so it
/// distinguishes an actually-occurring item from a type-shaped string
/// appearing elsewhere in the payload (e.g. inside embedded result JSON).
fn codex_precise_item_types(stdout: &[u8]) -> (usize, Vec<String>) {
    let text = String::from_utf8_lossy(stdout);
    let mut types = std::collections::BTreeSet::new();
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        count += 1;
        let Some(top_type) = v.get("type").and_then(|x| x.as_str()) else { continue };
        types.insert(top_type.to_string());
        if top_type == "item.started" || top_type == "item.completed" {
            if let Some(item_type) = v.get("item").and_then(|i| i.get("type")).and_then(|x| x.as_str()) {
                types.insert(item_type.to_string());
            }
        }
    }
    (count, types.into_iter().collect())
}

/// The top-level "type" of every JSONL event, in stream order (not
/// deduplicated), so a reviewer can see where in the turn a given item
/// occurs relative to `agent_message`/`turn.completed`.
fn codex_ordered_event_types(stdout: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v.get("type").and_then(|x| x.as_str()).map(str::to_owned))
        .collect()
}

/// For every `item.started`/`item.completed` event whose item type is not
/// the benign `agent_message`, records the item's key list plus a sanitized
/// identity subset: string values under keys whose name contains "name",
/// "server", "tool", "status", "state", or "error" (case-insensitive),
/// truncated to 80 chars. Never records arguments/inputs/outputs/content
/// fields, and never records non-string (nested object/array) values even
/// under a matching key name, to avoid accidentally capturing payload
/// content through a loosely-named field.
fn codex_item_details(stdout: &[u8]) -> Vec<Value> {
    const BENIGN_ITEM_TYPES: [&str; 1] = ["agent_message"];
    const IDENTITY_KEY_MARKERS: [&str; 6] = ["name", "server", "tool", "status", "state", "error"];

    let text = String::from_utf8_lossy(stdout);
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(top_type) = v.get("type").and_then(|x| x.as_str()) else { continue };
        if top_type != "item.started" && top_type != "item.completed" {
            continue;
        }
        let Some(item) = v.get("item").and_then(|i| i.as_object()) else { continue };
        let item_type = item.get("type").and_then(|x| x.as_str()).unwrap_or("?");
        if BENIGN_ITEM_TYPES.contains(&item_type) {
            continue;
        }

        let keys: Vec<String> = item.keys().cloned().collect();
        let mut identity = serde_json::Map::new();
        for (k, val) in item.iter() {
            let kl = k.to_lowercase();
            if !IDENTITY_KEY_MARKERS.iter().any(|m| kl.contains(m)) {
                continue;
            }
            match val {
                Value::String(s) => {
                    identity.insert(k.clone(), Value::String(s.chars().take(80).collect()));
                }
                Value::Bool(_) | Value::Number(_) | Value::Null => {
                    identity.insert(k.clone(), val.clone());
                }
                // Object/array under a matching key name is skipped: it may
                // be a content/argument container despite the key name.
                _ => {}
            }
        }

        out.push(serde_json::json!({
            "event_type": top_type,
            "item_type": item_type,
            "item_keys": keys,
            "identity": identity,
        }));
    }
    out
}

fn citations_of(v: &Value) -> Vec<String> {
    v.get("citations")
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn validate_contract(v: &Value) -> (bool, Vec<String>) {
    let mut problems = Vec::new();
    let Some(obj) = v.as_object() else {
        return (false, vec!["result is not a JSON object".into()]);
    };
    if obj.get("contract").and_then(|x| x.as_str()) != Some("wiki-query/v1") {
        problems.push("contract != wiki-query/v1".into());
    }
    let status = obj.get("knowledge_status").and_then(|x| x.as_str());
    if !matches!(status, Some("grounded") | Some("no_relevant_material")) {
        problems.push(format!("knowledge_status invalid: {status:?}"));
    }
    if !obj.get("answer").and_then(|x| x.as_str()).map(|s| !s.is_empty()).unwrap_or(false) {
        problems.push("answer missing or empty".into());
    }
    for key in ["citations", "gaps", "warnings"] {
        if !obj.get(key).map(|x| x.is_array()).unwrap_or(false) {
            problems.push(format!("{key} is not an array"));
        }
    }
    let citations = citations_of(v);
    let gaps_len = obj.get("gaps").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
    if status == Some("grounded") && citations.is_empty() {
        problems.push("grounded status but zero citations (CONTRACT_VIOLATION)".into());
    }
    if status == Some("no_relevant_material") && (!citations.is_empty() || gaps_len == 0) {
        problems.push("no_relevant_material invariant violated (CONTRACT_VIOLATION)".into());
    }
    (problems.is_empty(), problems)
}

/// Heuristic, evidence-safe scan for signs the model attempted a forbidden
/// action (write/edit/shell/etc.) and was blocked by the harness. Reports
/// only which generic marker keywords matched, never surrounding text, so
/// no wiki prose or model prose is recorded.
fn scan_forbidden_action_signals(stdout: &[u8], stderr: &[u8]) -> (bool, Vec<&'static str>) {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
    .to_lowercase();
    const MARKERS: [&str; 8] = [
        "permission denied",
        "not allowed",
        "cannot write",
        "unable to write",
        "read-only file system",
        "sandbox",
        "access is denied",
        "blocked",
    ];
    let hit: Vec<&'static str> = MARKERS.iter().copied().filter(|m| text.contains(m)).collect();
    (!hit.is_empty(), hit)
}
