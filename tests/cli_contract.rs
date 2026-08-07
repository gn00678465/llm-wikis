//! Public CLI contract tests (spec §5.1-§5.3, §13, §14; plan Task 12).
//!
//! Every test drives the *compiled binary* through `assert_cmd`, never the
//! library directly — this is what proves `src/cli.rs`'s `clap` wiring and
//! dispatch actually work end to end, on top of what `tests/query_service.rs`,
//! `tests/doctor.rs`, `tests/list.rs`, and `tests/config_init.rs` already
//! prove at the library layer.
//!
//! No test here ever invokes a real `claude`/`codex`. Tests that must reach
//! past argument validation into `QueryService`/`doctor`'s own resolution
//! flow either:
//! - fail earlier than any provider probe (a wiki with fabricated,
//!   never-existing paths, so the failure — `CONFIG_INVALID` from a failed
//!   `canonicalize`, or `QUESTION_TOO_LARGE` before that — proves the CLI
//!   argument layer *did* hand off to the real flow, without ever resolving
//!   an executable), or
//! - use a bogus provider command name guaranteed absent from `PATH`
//!   (`CLI_NOT_FOUND`), or
//! - (Windows only) point the configured provider executable at a generated
//!   `.cmd` fixture script that only ever echoes canned, sanitized output —
//!   never a real provider binary.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("llm-wikis").unwrap()
}

fn to_toml_string(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn stdout_json(assert: &Assert) -> serde_json::Value {
    let bytes = &assert.get_output().stdout;
    serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "stdout must be exactly one JSON document: {e}\nstdout was: {:?}",
            String::from_utf8_lossy(bytes)
        )
    })
}

/// An absolute path that does not exist, without hardcoding a platform drive
/// letter or POSIX root (`Path::is_absolute()` treats a bare `/foo` as *not*
/// absolute on Windows, so a literal like that would be rejected by
/// `--config`'s own absolute-only check instead of reaching `Config::load`).
fn nonexistent_config_path() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nonexistent-llm-wikis.toml");
    (tmp, path)
}

// ---------------------------------------------------------------------------
// Fixture: a syntactically valid registry whose one wiki's paths never exist.
// Every test that only needs to prove an *argument-stage* decision (not the
// filesystem/provider stages after it) uses this — cheap, and it doubles as
// proof the CLI really did hand off past argument validation whenever the
// resulting error is something other than `ARGUMENT_INVALID` (query.rs's own
// step order runs question validation, then path resolution, strictly after
// wiki/agent selection; a fabricated path fails only once path resolution is
// actually reached).
// ---------------------------------------------------------------------------

struct MinimalRegistry {
    _tmp: TempDir,
    config_path: PathBuf,
}

fn minimal_registry(default_agent: Option<&str>, wiki_agents: &str) -> MinimalRegistry {
    let tmp = tempfile::tempdir().unwrap();
    let fake_root = tmp.path().join("does-not-exist");
    let config_path = tmp.path().join("config.toml");
    let default_agent_line = match default_agent {
        Some(a) => format!("default_agent = \"{a}\"\n"),
        None => String::new(),
    };
    let text = format!(
        r#"config_version = 1
{default_agent_line}
[providers.claude]
executable = "claude"

[providers.codex]
executable = "codex"

[runtime]
max_question_bytes = 65536

[wikis.demo]
title = "Demo"
project_root = "{root}"
content_root = "{root}"
agents = [{agents}]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"

[wikis.demo.codex]
load = "project_skill"
entrypoint = "$wiki-query"
skill_path = ".agents/skills/wiki-query/SKILL.md"
"#,
        root = to_toml_string(&fake_root),
        agents = wiki_agents,
    );
    fs::write(&config_path, text).unwrap();
    MinimalRegistry {
        _tmp: tmp,
        config_path,
    }
}

/// Same shape, but `max_question_bytes` is tiny — used by the
/// stdin/byte-boundary tests so `QUESTION_TOO_LARGE` (not path resolution)
/// is the first thing reached, proving stdin content was actually read and
/// measured rather than merely accepted.
fn tiny_question_limit_registry() -> MinimalRegistry {
    let tmp = tempfile::tempdir().unwrap();
    let fake_root = tmp.path().join("does-not-exist");
    let config_path = tmp.path().join("config.toml");
    let text = format!(
        r#"config_version = 1
default_agent = "claude"

[providers.claude]
executable = "claude"

[runtime]
max_question_bytes = 4

[wikis.demo]
title = "Demo"
project_root = "{root}"
content_root = "{root}"
agents = ["claude"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"
"#,
        root = to_toml_string(&fake_root),
    );
    fs::write(&config_path, text).unwrap();
    MinimalRegistry {
        _tmp: tmp,
        config_path,
    }
}

/// A registry whose provider executables are bogus command names guaranteed
/// absent from `PATH` — `doctor`'s `executable` check fails deterministically
/// with `CLI_NOT_FOUND` and nothing is ever spawned.
fn unresolvable_provider_registry() -> MinimalRegistry {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("home.md"), b"# Home\n").unwrap();
    let config_path = tmp.path().join("config.toml");
    let text = format!(
        r#"config_version = 1
default_agent = "claude"

[providers.claude]
executable = "llm-wikis-cli-contract-nonexistent-provider-claude"

[providers.codex]
executable = "llm-wikis-cli-contract-nonexistent-provider-codex"

[wikis.demo]
title = "Demo"
project_root = "{root}"
content_root = "{root}"
agents = ["claude", "codex"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"

[wikis.demo.codex]
load = "project_skill"
entrypoint = "$wiki-query"
skill_path = ".agents/skills/wiki-query/SKILL.md"
"#,
        root = to_toml_string(&root),
    );
    // The skill path itself does not need to exist for the `executable`
    // check to run and fail; this fixture only asserts on that check and on
    // overall matrix shape/`ok`.
    fs::write(&config_path, text).unwrap();
    MinimalRegistry {
        _tmp: tmp,
        config_path,
    }
}

// ---------------------------------------------------------------------------
// Every public command exists; no `skill`/`agent-context` subcommand; help
// and product name are stable (plan Task 12 Step 1).
// ---------------------------------------------------------------------------

#[test]
fn help_prints_product_name_and_exits_zero() {
    let assert = bin().arg("--help").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("llm-wikis"), "{stdout}");
}

// PRD 08-07-first-run-config-and-query-ux-fixes AC5: every clap-visible
// `///` doc comment is a short, user-facing one-liner -- the rationale
// prose that used to leak into `--help` (worst case: `config list`/
// `validate`'s full planning paragraphs) now lives only in `//` comments in
// the source, never in rendered help text. Checked across the root command
// and every subcommand, not just the two known offenders.
#[test]
fn help_output_never_leaks_internal_planning_markers() {
    let forbidden = ["PRD", "AC1", "D7", "spec §", "checklist", "OFF-"];
    let help_invocations: &[&[&str]] = &[
        &["--help"],
        &["list", "--help"],
        &["doctor", "--help"],
        &["query", "--help"],
        &["config", "--help"],
        &["config", "init", "--help"],
        &["config", "list", "--help"],
        &["config", "validate", "--help"],
    ];
    for args in help_invocations {
        let assert = bin().args(*args).assert().success();
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
        for marker in forbidden {
            assert!(
                !stdout.contains(marker),
                "{args:?} --help leaked internal marker {marker:?}:\n{stdout}"
            );
        }
    }
}

#[test]
fn unknown_subcommand_is_argument_invalid_not_a_panic() {
    let assert = bin().args(["skill", "foo"]).assert().failure().code(2);
    let out = assert.get_output();
    // PRD 08-06-pre-0-1-0-cli-refinements D2/AC3: human-mode error lines
    // move to stderr for every subcommand, including a top-level parse
    // failure that never reaches a specific subcommand handler — stdout
    // stays completely empty (`argument_invalid_query` sets `answer: None`)
    // and the error line lands on stderr instead, via the same
    // `eprint_error_line` rendering every other failure uses.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.is_empty(), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ARGUMENT_INVALID"), "{stderr}");
}

// OFF-170 (`cargo test --test cli_contract -- argument_invalid_exit`):
// every class of invalid/ambiguous CLI input maps to `ARGUMENT_INVALID` with
// exit code exactly 2 — spot-checked across a few different sources of
// argument invalidity (unrecognized subcommand, repeated `--wiki`, missing
// `--wiki`, a relative `--config` override).
#[test]
fn argument_invalid_exit_is_always_exactly_2() {
    bin().args(["skill", "foo"]).assert().failure().code(2);

    let reg = minimal_registry(None, "\"claude\"");
    bin()
        .args([
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--wiki",
            "demo",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    bin()
        .args([
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    bin()
        .args(["--config", "../relative/config.toml", "list"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn no_agent_context_subcommand_exists() {
    bin()
        .args(["agent-context", "foo"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn every_documented_subcommand_is_recognized() {
    // Each of these fails for a *different* reason than "unrecognized
    // subcommand" (missing --wiki, missing --wiki, an unresolvable config
    // path) — proving the four commands themselves parse. `config init`
    // alone (no --config) would write to *this host's* real platform config
    // path, which must never happen from a test, so it is exercised only
    // with an explicit `--config` elsewhere (`config_init_json_success_and_failure_shapes`).
    let (_tmp, missing) = nonexistent_config_path();
    let missing = missing.to_str().unwrap();
    bin().args(["--config", missing, "list"]).assert().code(2);
    bin().args(["--config", missing, "doctor"]).assert().code(2);
    bin()
        .args(["--config", missing, "query", "--", "x"])
        .assert()
        .code(2);
}

// ---------------------------------------------------------------------------
// Global `--config`/`--json` (spec §5.2)
// ---------------------------------------------------------------------------

#[test]
fn relative_config_override_rejected_before_anything_else() {
    let assert = bin()
        .args(["--json", "--config", "../relative/config.toml", "list"])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn config_init_json_success_and_failure_shapes() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("nested").join("config.toml");
    let target_str = target.display().to_string();

    let first = bin()
        .args(["--json", "--config", &target_str, "config", "init"])
        .assert()
        .success()
        .code(0);
    let v = stdout_json(&first);
    assert_eq!(v["ok"], true);
    assert_eq!(v["operation"], "config_init");
    assert_eq!(v["created"], true);
    assert!(target.exists());

    let second = bin()
        .args(["--json", "--config", &target_str, "config", "init"])
        .assert()
        .failure()
        .code(2);
    let v2 = stdout_json(&second);
    assert_eq!(v2["ok"], false);
    assert_eq!(v2["created"], false);
    assert_eq!(v2["error"]["code"], "CONFIG_EXISTS");
}

// ---------------------------------------------------------------------------
// `config list` / `config validate` (PRD 08-06-pre-0-1-0-cli-refinements
// AC1; subcommand renamed from `show` to `list` per D7). `doctor` is
// unchanged by this task (D1) and is exercised elsewhere. `config list` is
// distinct from the top-level `list` (wiki registry) subcommand exercised
// above -- clap's `config` prefix keeps the two unambiguous on the command
// line even though both derive to a bare `list` token at their own depth.
// ---------------------------------------------------------------------------

#[test]
fn config_list_json_success_and_failure_shapes() {
    let reg = minimal_registry(Some("claude"), "\"claude\", \"codex\"");

    let ok = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "config",
            "list",
        ])
        .assert()
        .success()
        .code(0);
    let v = stdout_json(&ok);
    assert_eq!(v["ok"], true);
    assert_eq!(v["operation"], "config_list");
    assert_eq!(v["config"]["config_version"], 1);
    assert_eq!(v["config"]["default_agent"], "claude");
    assert!(v["config"]["wikis"]["demo"].is_object());
    assert!(v["error"].is_null() || v.as_object().unwrap().get("error").is_none());

    let (_tmp, missing) = nonexistent_config_path();
    let failure = bin()
        .args([
            "--json",
            "--config",
            missing.to_str().unwrap(),
            "config",
            "list",
        ])
        .assert()
        .failure()
        .code(2);
    let vf = stdout_json(&failure);
    assert_eq!(vf["ok"], false);
    assert_eq!(vf["operation"], "config_list");
    assert!(vf.as_object().unwrap().get("config").is_none());
    assert_eq!(vf["error"]["code"], "CONFIG_INVALID");
}

#[test]
fn config_list_human_mode_prints_the_registry_without_starting_a_provider() {
    let reg = minimal_registry(Some("claude"), "\"claude\"");
    let assert = bin()
        .args([
            "--config",
            reg.config_path.to_str().unwrap(),
            "config",
            "list",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("config_version = 1"), "{stdout}");
    assert!(stdout.contains("default_agent = claude"), "{stdout}");
    assert!(stdout.contains("demo"), "{stdout}");
}

#[test]
fn config_list_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--config", missing.to_str().unwrap(), "config", "list"])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CONFIG_INVALID"), "{stderr}");
}

#[test]
fn config_validate_json_success_and_failure_shapes() {
    let reg = minimal_registry(None, "\"claude\"");

    let ok = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "config",
            "validate",
        ])
        .assert()
        .success()
        .code(0);
    let v = stdout_json(&ok);
    assert_eq!(v["ok"], true);
    assert_eq!(v["operation"], "config_validate");
    let top: BTreeSet<String> = v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        top,
        ["schema_version", "ok", "operation", "path"]
            .into_iter()
            .map(String::from)
            .collect(),
        "config validate's success shape must never echo the parsed config back"
    );

    let (_tmp, missing) = nonexistent_config_path();
    let failure = bin()
        .args([
            "--json",
            "--config",
            missing.to_str().unwrap(),
            "config",
            "validate",
        ])
        .assert()
        .failure()
        .code(2);
    let vf = stdout_json(&failure);
    assert_eq!(vf["ok"], false);
    assert_eq!(vf["operation"], "config_validate");
    assert_eq!(vf["error"]["code"], "CONFIG_INVALID");
}

#[test]
fn config_validate_never_mutates_the_target_file() {
    // AC1: "without side effects" — a failing `validate` call must not
    // touch the target file's bytes at all.
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    let tampered =
        b"config_version = 1\n# deliberately incomplete, no closing table\n[wikis.demo".to_vec();
    fs::write(&target, &tampered).unwrap();

    bin()
        .args([
            "--json",
            "--config",
            target.to_str().unwrap(),
            "config",
            "validate",
        ])
        .assert()
        .failure()
        .code(2);

    let after = fs::read(&target).unwrap();
    assert_eq!(
        after, tampered,
        "config validate must never modify its target"
    );
}

#[test]
fn config_validate_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--config", missing.to_str().unwrap(), "config", "validate"])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CONFIG_INVALID"), "{stderr}");
}

// OFF-020 (Windows platform default config path formula): with `--config`
// omitted entirely, the platform-native default (`crate::config::default_config_path`'s
// `%APPDATA%\llm-wikis\config.toml` formula) must be what actually gets
// written — verified here through the real binary (`config init` reports
// exactly where it wrote via its `path` field) rather than only at the
// `default_config_path` unit level (`tests/config_contract.rs`'s
// `windows_config_path_uses_appdata`). Linux/macOS's own formulas
// (OFF-021/022) are unreachable from this Windows host through a real
// binary run; they remain covered only at that same unit level.
#[cfg(windows)]
#[test]
fn default_config_path_on_windows_uses_appdata_with_config_omitted() {
    let appdata = tempfile::tempdir().unwrap();
    let expected_path = appdata.path().join("llm-wikis").join("config.toml");

    let assert = bin()
        .env("APPDATA", appdata.path())
        .args(["--json", "config", "init"])
        .assert()
        .success();
    let v = stdout_json(&assert);
    assert_eq!(v["ok"], true);
    assert_eq!(
        PathBuf::from(v["path"].as_str().unwrap()),
        expected_path,
        "config init must write to the platform-derived default path"
    );
    assert!(expected_path.exists());

    // `list` with the same overridden APPDATA and no `--config` must read
    // the file `config init` just wrote at that same derived path.
    let list_assert = bin()
        .env("APPDATA", appdata.path())
        .args(["--json", "list"])
        .assert()
        .success();
    assert_eq!(stdout_json(&list_assert)["wikis"], serde_json::json!([]));
}

#[test]
fn json_mode_emits_exactly_one_document_even_for_a_top_level_parse_failure() {
    let assert = bin()
        .args(["--json", "not-a-real-subcommand"])
        .assert()
        .code(2);
    let out = assert.get_output();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.matches('\n').count(),
        1,
        "exactly one line of output: {text:?}"
    );
    let v: serde_json::Value = serde_json::from_str(text.trim_end()).unwrap();
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    assert!(out.stderr.is_empty());
}

// ---------------------------------------------------------------------------
// `query` argument tests (spec §5.1; OFF-007/008/009/010/011/019)
// ---------------------------------------------------------------------------

#[test]
fn query_rejects_repeated_wiki() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--wiki",
            "demo",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    assert!(v["wiki"].is_null());
    assert!(v["agent"].is_null());
}

#[test]
fn query_wiki_all_is_argument_error_not_all_wikis() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "all",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn query_missing_wiki_is_argument_invalid() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

// PRD 08-07-first-run-config-and-query-ux-fixes D4/AC3: both traps a
// first-run user hits -- a bare positional question (missing the `--`
// separator) and a `query -- "q"` with no `--wiki` -- now name the correct
// invocation shape rather than leaving the caller with only clap's raw
// "unexpected argument" text or a bare "requires exactly one --wiki". The
// `--` requirement itself and the missing-`--wiki` rejection are unchanged
// (still `ARGUMENT_INVALID`, still exit 2); only the message text changed.
#[test]
fn query_without_separator_names_the_correct_invocation_shape() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "question text with no -- separator",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(r#"llm-wikis query --wiki <id> -- "<question>""#),
        "{message}"
    );
}

#[test]
fn query_missing_wiki_message_names_the_correct_invocation_shape() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(r#"llm-wikis query --wiki <id> -- "<question>""#),
        "{message}"
    );
}

#[test]
fn query_without_separator_still_emits_exactly_one_json_document() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "question text with no -- separator",
        ])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.matches('\n').count(),
        1,
        "exactly one line of output: {text:?}"
    );
    assert!(out.stderr.is_empty());
}

// PRD 08-07-first-run-config-and-query-ux-fixes D8: a first-run operator
// trying to check more than one agent in a single `doctor --live` (either a
// comma-separated `--agent claude,codex`, or a repeated `--agent claude
// --agent codex`) hits two different clap error kinds -- `InvalidValue`
// (comma-separated: `--agent` is a `ValueEnum`, so the whole joined string
// fails to match either variant) and `ArgumentConflict` (repeated: the flag
// itself isn't repeatable) -- and both now carry a reminder that `doctor
// --live` checks exactly one (wiki, agent) pair per run, rather than
// clap's bare "invalid value"/"cannot be used multiple times" text alone.
// Neither changes the `--live` one-pair-per-run requirement itself, the
// `ARGUMENT_INVALID` code, or the exit code (2) -- message text only. A
// bogus `--config` path is enough here: this is a pure clap parse failure,
// never reaching config resolution at all.
#[test]
fn doctor_comma_separated_agent_value_names_the_one_pair_per_run_rule() {
    let (_tmp, config_path) = nonexistent_config_path();
    let assert = bin()
        .args([
            "--json",
            "--config",
            config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude,codex",
            "--live",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("doctor runs one wiki/agent pair at a time"),
        "{message}"
    );
    assert!(
        message.contains("llm-wikis doctor --wiki <id> --agent claude --live"),
        "{message}"
    );
}

#[test]
fn doctor_repeated_agent_flag_names_the_one_pair_per_run_rule() {
    let (_tmp, config_path) = nonexistent_config_path();
    let assert = bin()
        .args([
            "--json",
            "--config",
            config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude",
            "--agent",
            "codex",
            "--live",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("doctor runs one wiki/agent pair at a time"),
        "{message}"
    );
}

/// `doctor --live` still emits exactly one JSON document for this error
/// shape too (same contract as `query_without_separator_still_emits_exactly_one_json_document`).
#[test]
fn doctor_comma_separated_agent_value_still_emits_exactly_one_json_document() {
    let (_tmp, config_path) = nonexistent_config_path();
    let assert = bin()
        .args([
            "--json",
            "--config",
            config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude,codex",
            "--live",
        ])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.matches('\n').count(),
        1,
        "exactly one line of output: {text:?}"
    );
    assert!(out.stderr.is_empty());
}

/// `query`'s own comma-separated `--agent` error is the same clap
/// `InvalidValue` kind as doctor's, sharing the exact `--agent` declaration
/// -- deliberately gets `query`'s own D4 usage hint (never doctor's), since
/// it is a `query`-scoped parse failure.
#[test]
fn query_comma_separated_agent_value_gets_the_query_usage_hint_not_doctors() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--agent",
            "claude,codex",
            "--",
            "q",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(r#"llm-wikis query --wiki <id> -- "<question>""#),
        "{message}"
    );
    assert!(
        !message.contains("doctor runs one wiki/agent pair at a time"),
        "{message}"
    );
}

#[test]
fn query_rejects_both_positional_and_stdin() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--",
            "x",
        ])
        .write_stdin("y\n")
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn query_rejects_empty_input_from_either_source() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
        ])
        .write_stdin("")
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn query_stdin_fallback_reads_the_complete_question() {
    // `max_question_bytes = 4`; a 5-byte stdin question crosses the boundary
    // only if the fallback actually read (and measured) the piped bytes.
    let reg = tiny_question_limit_registry();
    let too_large = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
        ])
        .write_stdin("abcde")
        .assert()
        .failure()
        .code(2);
    assert_eq!(
        stdout_json(&too_large)["error"]["code"],
        "QUESTION_TOO_LARGE"
    );

    // Exactly at the boundary (4 bytes) must *not* be too large — it proceeds
    // past question validation into path resolution instead.
    let at_boundary = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
        ])
        .write_stdin("abcd")
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&at_boundary);
    assert_ne!(v["error"]["code"], "QUESTION_TOO_LARGE");
    assert_ne!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn unicode_and_multiline_question_round_trips_byte_identical_via_stdin_and_positional() {
    // Traditional Chinese across three lines. `question.len()` is the exact
    // UTF-8 byte count (measured directly rather than hand-counted — the
    // point is proving the wrapper's own count matches it exactly). A
    // `max_question_bytes` one under that count must reject it as too
    // large; exactly that count must not — proving the multi-byte,
    // multi-line bytes were carried through unmodified and measured
    // exactly, not corrupted, re-encoded, truncated at a newline, or
    // truncated by codepoint count. Exercised through *both* input
    // channels (spec §5.1's positional-after-`--` and its stdin fallback).
    let question = "繁體中文\n多行問題\n第三行";
    let limit = question.len() as u64;
    assert!(limit > 20, "expected a multi-line multi-byte question");
    assert!(question.contains('\n'), "expected a multi-line question");

    let build = |max_question_bytes: u64| {
        let tmp = tempfile::tempdir().unwrap();
        let fake_root = tmp.path().join("does-not-exist");
        let config_path = tmp.path().join("config.toml");
        fs::write(
            &config_path,
            format!(
                r#"config_version = 1
default_agent = "claude"

[providers.claude]
executable = "claude"

[runtime]
max_question_bytes = {max_question_bytes}

[wikis.demo]
title = "Demo"
project_root = "{root}"
content_root = "{root}"
agents = ["claude"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"
"#,
                root = to_toml_string(&fake_root),
            ),
        )
        .unwrap();
        (tmp, config_path)
    };

    // --- via stdin (positional omitted) ---
    let (_tmp1, cfg_too_small) = build(limit - 1);
    let too_small_stdin = bin()
        .args([
            "--json",
            "--config",
            cfg_too_small.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
        ])
        .write_stdin(question)
        .assert()
        .code(2);
    assert_eq!(
        stdout_json(&too_small_stdin)["error"]["code"],
        "QUESTION_TOO_LARGE"
    );

    let (_tmp2, cfg_exact) = build(limit);
    let exact_stdin = bin()
        .args([
            "--json",
            "--config",
            cfg_exact.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
        ])
        .write_stdin(question)
        .assert()
        .code(2);
    assert_ne!(
        stdout_json(&exact_stdin)["error"]["code"],
        "QUESTION_TOO_LARGE"
    );

    // --- via the positional argument after `--` (same byte content) ---
    let (_tmp3, cfg_too_small_pos) = build(limit - 1);
    let too_small_positional = bin()
        .args([
            "--json",
            "--config",
            cfg_too_small_pos.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--",
            question,
        ])
        .assert()
        .code(2);
    assert_eq!(
        stdout_json(&too_small_positional)["error"]["code"],
        "QUESTION_TOO_LARGE"
    );

    let (_tmp4, cfg_exact_pos) = build(limit);
    let exact_positional = bin()
        .args([
            "--json",
            "--config",
            cfg_exact_pos.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--",
            question,
        ])
        .assert()
        .code(2);
    assert_ne!(
        stdout_json(&exact_positional)["error"]["code"],
        "QUESTION_TOO_LARGE"
    );
}

// OFF-011 (`cargo test --test cli_contract -- default_agent`): `--agent` is
// optional exactly when the wiki's derived `default_agent` (the same rule
// `list` uses) is non-null; otherwise omitting it is `ARGUMENT_INVALID`
// before any provider spawn. The two tests below cover both directions.

#[test]
fn query_omitted_agent_rejected_when_derived_default_agent_is_null() {
    // Global default_agent=claude, but this wiki enables only codex, so the
    // derived default_agent (same rule `list` uses) is null — omitting
    // --agent must be rejected before any provider spawn.
    let reg = minimal_registry(Some("claude"), "\"codex\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    assert!(v["wiki"].is_null());
    assert!(v["agent"].is_null());
}

#[test]
fn query_omitted_agent_proceeds_when_derived_default_agent_is_non_null() {
    // default_agent=claude and this wiki enables claude, so the derived
    // default is non-null: omitting --agent must *not* be rejected as an
    // argument error — it must proceed into real resolution, which then
    // fails for an unrelated reason (the fabricated path cannot resolve).
    let reg = minimal_registry(Some("claude"), "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--",
            "x",
        ])
        .assert()
        .failure();
    let v = stdout_json(&assert);
    assert_ne!(v["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn query_leading_dash_question_after_separator_is_not_reinterpreted_as_a_flag() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--agent",
            "claude",
            "--",
            "--leading-dash",
        ])
        .assert()
        .failure();
    // Proceeded past argument parsing (clap did not choke on the
    // leading-dash value) into real resolution.
    assert_ne!(stdout_json(&assert)["error"]["code"], "ARGUMENT_INVALID");
}

#[test]
fn shell_metacharacters_and_quotes_in_the_question_are_never_interpreted() {
    let reg = minimal_registry(None, "\"claude\"");
    let adversarial = "\"quotes\" `backticks` $() & | < > ^ % !";
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "demo",
            "--agent",
            "claude",
            "--",
            adversarial,
        ])
        .assert()
        .failure();
    assert_ne!(stdout_json(&assert)["error"]["code"], "ARGUMENT_INVALID");
}

// ---------------------------------------------------------------------------
// Output/exit contract (spec §13/§14; plan Task 12 Step 2)
// ---------------------------------------------------------------------------

#[test]
fn human_mode_never_prints_json_on_stdout_for_an_argument_failure() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "all",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(!String::from_utf8_lossy(&out.stdout).contains('{'));
    // PRD 08-06-pre-0-1-0-cli-refinements D2/AC3: the human-mode companion
    // of `json_mode_emits_exactly_one_document_even_for_a_top_level_parse_failure`
    // — stdout is fully empty and the error line lands on stderr instead.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.is_empty(), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ARGUMENT_INVALID"), "{stderr}");
}

// ---------------------------------------------------------------------------
// Human-mode error lines on stderr, end to end through the real binary, for
// every subcommand (PRD 08-06-pre-0-1-0-cli-refinements D2/AC3). The two
// tests above only exercise *pre-dispatch* clap parse failures; these four
// prove the same routing for a failure reached after a subcommand handler
// actually ran.
// ---------------------------------------------------------------------------

#[test]
fn list_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--config", missing.to_str().unwrap(), "list"])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CONFIG_INVALID"), "{stderr}");
}

#[test]
fn doctor_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--config", missing.to_str().unwrap(), "doctor"])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CONFIG_INVALID"), "{stderr}");
}

#[test]
fn config_init_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("config.toml");
    let target_str = target.display().to_string();
    bin()
        .args(["--config", &target_str, "config", "init"])
        .assert()
        .success();

    let assert = bin()
        .args(["--config", &target_str, "config", "init"])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("CONFIG_EXISTS"), "{stderr}");
}

#[test]
fn query_human_mode_failure_prints_the_error_line_to_stderr_only() {
    let reg = minimal_registry(None, "\"claude\"");
    let assert = bin()
        .args([
            "--config",
            reg.config_path.to_str().unwrap(),
            "query",
            "--wiki",
            "all",
            "--",
            "x",
        ])
        .assert()
        .failure()
        .code(2);
    let out = assert.get_output();
    assert!(String::from_utf8_lossy(&out.stdout).is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ARGUMENT_INVALID"), "{stderr}");
}

// ---------------------------------------------------------------------------
// `list` (spec §5.1, §5.3; OFF-025/028/029/033)
// ---------------------------------------------------------------------------

#[test]
fn list_config_failure_is_exit_2_with_empty_wikis_and_error() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--json", "--config", missing.to_str().unwrap(), "list"])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert_eq!(v["ok"], false);
    assert!(v["wikis"].as_array().unwrap().is_empty());
    assert_eq!(v["error"]["code"], "CONFIG_INVALID");
}

#[test]
fn list_json_exact_shape_and_exit_zero_for_a_zero_wiki_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("config.toml");
    bin()
        .args([
            "--json",
            "--config",
            config_path.to_str().unwrap(),
            "config",
            "init",
        ])
        .assert()
        .success();

    let assert = bin()
        .args(["--json", "--config", config_path.to_str().unwrap(), "list"])
        .assert()
        .success()
        .code(0);
    let v = stdout_json(&assert);
    let top: BTreeSet<String> = v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        top,
        ["schema_version", "ok", "operation", "wikis"]
            .into_iter()
            .map(String::from)
            .collect()
    );
    assert_eq!(v["operation"], "list");
    assert_eq!(v["wikis"], serde_json::json!([]));
}

#[test]
fn list_entry_shape_and_default_agent_derivation_through_the_real_binary() {
    // OFF-018: `default_agent` is the wiki's own value only when the global
    // default is enabled for that wiki, else JSON `null` — checked here
    // through the compiled binary for both directions, plus the per-wiki
    // entry key shape (OFF-025's `{id,title,default_agent,agents}`).
    let non_null = minimal_registry(Some("claude"), "\"claude\", \"codex\"");
    let assert = bin()
        .args([
            "--json",
            "--config",
            non_null.config_path.to_str().unwrap(),
            "list",
        ])
        .assert()
        .success();
    let v = stdout_json(&assert);
    let entry = &v["wikis"][0];
    let entry_keys: BTreeSet<String> = entry.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        entry_keys,
        ["id", "title", "default_agent", "agents"]
            .into_iter()
            .map(String::from)
            .collect()
    );
    assert_eq!(entry["default_agent"], "claude");

    let null_case = minimal_registry(Some("claude"), "\"codex\"");
    let assert2 = bin()
        .args([
            "--json",
            "--config",
            null_case.config_path.to_str().unwrap(),
            "list",
        ])
        .assert()
        .success();
    let v2 = stdout_json(&assert2);
    assert!(
        v2["wikis"][0]["default_agent"].is_null(),
        "global default_agent=claude is not enabled for this wiki: {v2:?}"
    );
}

// ---------------------------------------------------------------------------
// `doctor` (spec §5.1, §5.3, §15; OFF-015/016/017/026/230)
// ---------------------------------------------------------------------------

#[test]
fn doctor_default_matrix_covers_every_configured_wiki_agent_pair() {
    let reg = unresolvable_provider_registry();
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
        ])
        .assert()
        .code(3); // CLI_NOT_FOUND is the only failure class present.
    let v = stdout_json(&assert);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "one entry per (wiki, enabled-agent) pair");
    let agents: BTreeSet<String> = results
        .iter()
        .map(|r| r["agent"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        agents,
        ["claude", "codex"].into_iter().map(String::from).collect()
    );
}

#[test]
fn doctor_wiki_and_agent_selectors_narrow_the_matrix() {
    let reg = unresolvable_provider_registry();
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude",
        ])
        .assert()
        .code(3);
    let v = stdout_json(&assert);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["wiki"], "demo");
    assert_eq!(results[0]["agent"], "claude");
}

#[test]
fn doctor_live_requires_both_wiki_and_agent() {
    let reg = unresolvable_provider_registry();
    // Neither selector.
    let neither = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--live",
        ])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&neither);
    assert_eq!(v["error"]["code"], "ARGUMENT_INVALID");
    assert!(v["results"].as_array().unwrap().is_empty());

    // Only --wiki.
    bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--live",
            "--wiki",
            "demo",
        ])
        .assert()
        .failure()
        .code(2);
    // Only --agent.
    bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--live",
            "--agent",
            "claude",
        ])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn doctor_json_exact_top_level_shape() {
    let reg = unresolvable_provider_registry();
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude",
        ])
        .assert()
        .code(3);
    let v = stdout_json(&assert);
    let top: BTreeSet<String> = v.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        top,
        ["schema_version", "ok", "operation", "live", "results"]
            .into_iter()
            .map(String::from)
            .collect()
    );
    let result = &v["results"][0];
    let result_keys: BTreeSet<String> = result.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        result_keys,
        ["wiki", "agent", "ok", "checks"]
            .into_iter()
            .map(String::from)
            .collect()
    );
    let check = &result["checks"][0];
    let check_keys: BTreeSet<String> = check.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        check_keys,
        ["name", "status", "code", "message"]
            .into_iter()
            .map(String::from)
            .collect()
    );
}

#[test]
fn doctor_any_failing_check_makes_top_level_ok_false_with_matching_exit() {
    let reg = unresolvable_provider_registry();
    let assert = bin()
        .args([
            "--json",
            "--config",
            reg.config_path.to_str().unwrap(),
            "doctor",
            "--wiki",
            "demo",
            "--agent",
            "claude",
        ])
        .assert()
        .failure()
        .code(3); // CLI_NOT_FOUND's exit class (spec §14).
    let v = stdout_json(&assert);
    assert_eq!(v["ok"], false);
    assert_eq!(v["results"][0]["ok"], false);
    let checks = v["results"][0]["checks"].as_array().unwrap();
    let executable_check = checks
        .iter()
        .find(|c| c["name"] == "executable")
        .expect("executable check present");
    assert_eq!(executable_check["status"], "fail");
    assert_eq!(executable_check["code"], "CLI_NOT_FOUND");
}

#[test]
fn doctor_config_failure_is_empty_results_with_top_level_error() {
    let (_tmp, missing) = nonexistent_config_path();
    let assert = bin()
        .args(["--json", "--config", missing.to_str().unwrap(), "doctor"])
        .assert()
        .failure()
        .code(2);
    let v = stdout_json(&assert);
    assert!(v["results"].as_array().unwrap().is_empty());
    assert_eq!(v["error"]["code"], "CONFIG_INVALID");
}

// ---------------------------------------------------------------------------
// External-cwd tests (plan Task 12 Step 3; OFF-024)
// ---------------------------------------------------------------------------

#[test]
fn never_reads_or_discovers_from_the_callers_current_directory() {
    // The real config lives under `real_dir`, with a wiki whose paths are
    // *relative*, so they must resolve against the config file's own
    // directory rather than whatever the caller's cwd happens to be.
    let real_dir = tempfile::tempdir().unwrap();
    let wiki_dir = real_dir.path().join("wiki-content");
    fs::create_dir_all(&wiki_dir).unwrap();
    fs::write(wiki_dir.join("home.md"), b"# Home\n").unwrap();
    let real_config = real_dir.path().join("config.toml");
    fs::write(
        &real_config,
        r#"config_version = 1

[providers.claude]
executable = "llm-wikis-cli-contract-nonexistent-provider"

[wikis.real-wiki]
title = "Real Wiki"
project_root = "wiki-content"
content_root = "wiki-content"
agents = ["claude"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.real-wiki.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"
"#,
    )
    .unwrap();

    // An unrelated cwd containing its own *different* decoy config.toml.
    let cwd_dir = tempfile::tempdir().unwrap();
    fs::write(
        cwd_dir.path().join("config.toml"),
        r#"config_version = 1

[providers.claude]
executable = "claude"

[wikis.decoy-wiki]
title = "Decoy"
project_root = "."
content_root = "."
agents = ["claude"]
query_prompt = "Decoy prompt."

[wikis.decoy-wiki.claude]
load = "project_skill"
entrypoint = "/decoy"
skill_path = "SKILL.md"
"#,
    )
    .unwrap();

    let assert = bin()
        .current_dir(cwd_dir.path())
        .args(["--json", "--config", real_config.to_str().unwrap(), "list"])
        .assert()
        .success();
    let v = stdout_json(&assert);
    let ids: Vec<&str> = v["wikis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec!["real-wiki"],
        "the cwd-local decoy must never be read or merged"
    );

    // `doctor`'s `roots` check must resolve the relative paths against the
    // config file's directory, not the (unrelated) cwd.
    let doctor_assert = bin()
        .current_dir(cwd_dir.path())
        .args([
            "--json",
            "--config",
            real_config.to_str().unwrap(),
            "doctor",
            "--wiki",
            "real-wiki",
            "--agent",
            "claude",
        ])
        .assert();
    let dv = stdout_json(&doctor_assert);
    let roots_check = dv["results"][0]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "roots")
        .expect("roots check present");
    assert_eq!(
        roots_check["status"], "pass",
        "relative wiki paths must resolve against the config file's directory: {dv:?}"
    );
}

#[test]
fn unicode_config_and_wiki_paths_resolve_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("設定-config-😀");
    fs::create_dir_all(&config_dir).unwrap();
    let wiki_dir = config_dir.join("wiki-content");
    fs::create_dir_all(&wiki_dir).unwrap();
    fs::write(wiki_dir.join("home.md"), b"# Home\n").unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"config_version = 1

[providers.claude]
executable = "claude"

[wikis.demo]
title = "Demo"
project_root = "wiki-content"
content_root = "wiki-content"
agents = ["claude"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"
"#,
    )
    .unwrap();

    let assert = bin()
        .args(["--json", "--config", config_path.to_str().unwrap(), "list"])
        .assert()
        .success();
    let v = stdout_json(&assert);
    assert_eq!(v["wikis"][0]["id"], "demo");
}

// ---------------------------------------------------------------------------
// Windows-only end-to-end tests using a generated `.cmd` fixture provider
// (never a real claude/codex). OFF-232, OFF-197, OFF-231.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows_provider_fixture {
    use super::*;

    /// A real, on-disk wiki fixture plus a generated `.cmd` script standing
    /// in for the configured provider executable. The script:
    /// - `--version`  -> echoes a fixed version string;
    /// - `auth ...`   -> echoes `{"authenticated": <FAKE_AUTH env var>}`;
    /// - anything else (the real query/live invocation) -> echoes a fixed,
    ///   valid `wiki-query/v1` success document citing the fixture page.
    ///
    /// Never a real provider binary; `FAKE_AUTH` is set per-invocation via
    /// `Command::env`, letting one script serve both the logged-out
    /// (`AUTH_REQUIRED`) and authenticated paths.
    /// A disposable directory rooted under this crate's own `target/`,
    /// deliberately *not* under the system temp directory. `QueryService`'s
    /// generated-artifact temp root (spec §10.1) must be canonically
    /// disjoint from every configured `project_root`/`content_root`
    /// (`resolve_safe_temp_root`); since production `temp_base` is always
    /// `std::env::temp_dir()`, a wiki fixture built via `tempfile::tempdir()`
    /// (which also nests under the system temp directory) would always
    /// collide with that safety check. Removed on drop.
    struct LocalTempDir {
        path: PathBuf,
    }

    impl LocalTempDir {
        fn new(label: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("cli-contract-tmp")
                .join(format!("{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for LocalTempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct ProviderFixture {
        _tmp: LocalTempDir,
        _wiki_tmp: LocalTempDir,
        wiki_root: PathBuf,
        config_path: PathBuf,
        cache_dir: PathBuf,
    }

    fn build_provider_fixture(include_schema: bool) -> ProviderFixture {
        // `LocalTempDir` (rooted under this crate's own `target/`), not
        // `tempfile::tempdir()` (system temp): on a Windows account with a
        // long username, `%TEMP%` can resolve through an 8.3 short-name path
        // segment (observed on GitHub Actions Windows runners as
        // `C:\Users\RUNNER~1\...`), and this fixture's generated `.cmd`
        // script becomes the configured provider `executable` -- an
        // absolute path -- which `validate_executable` correctly rejects
        // if it contains `~` (one of the denylisted shell metacharacters,
        // spec §6.1). Rooting under `target/` avoids that short-name path
        // entirely, the same fix `wiki_tmp` below already applies for the
        // wiki root.
        let tmp = LocalTempDir::new("provider");
        let wiki_tmp = LocalTempDir::new("wiki");
        let wiki_root = wiki_tmp.path().join("wiki");
        let skill_dir = wiki_root.join(".claude").join("skills").join("wiki-query");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), b"skill body\n").unwrap();
        if include_schema {
            fs::write(wiki_root.join("SCHEMA.md"), b"# schema\n").unwrap();
        }
        fs::write(wiki_root.join("home.md"), b"# Home\nFixture content.\n").unwrap();

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let script_path = bin_dir.join("fake-claude.cmd");
        let script = r#"@echo off
if "%~1"=="--version" (
  echo 9.9.9
  exit /b 0
)
if "%~1"=="auth" (
  if "%FAKE_AUTH%"=="false" (
    echo {"authenticated":false}
  ) else (
    echo {"authenticated":true}
  )
  exit /b 0
)
echo {"subtype":"success","structured_output":{"contract":"wiki-query/v1","knowledge_status":"grounded","answer":"Fixture answer with a citation.","citations":["home"],"gaps":[],"warnings":[]}}
exit /b 0
"#;
        fs::write(&script_path, script).unwrap();

        let cache_dir = tmp.path().join("cache-root");
        fs::create_dir_all(&cache_dir).unwrap();

        let config_path = tmp.path().join("config.toml");
        fs::write(
            &config_path,
            format!(
                r#"config_version = 1
default_agent = "claude"

[providers.claude]
executable = "{script}"

[runtime]
timeout_seconds = 15

[wikis.demo]
title = "Demo"
project_root = "{root}"
content_root = "{root}"
agents = ["claude"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.demo.claude]
load = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"
"#,
                script = to_toml_string(&script_path),
                root = to_toml_string(&wiki_root),
            ),
        )
        .unwrap();

        ProviderFixture {
            _tmp: tmp,
            _wiki_tmp: wiki_tmp,
            wiki_root,
            config_path,
            cache_dir,
        }
    }

    #[test]
    fn auth_required_is_reachable_through_the_query_command_path() {
        let fixture = build_provider_fixture(true);
        let assert = bin()
            .env("FAKE_AUTH", "false")
            .env("LOCALAPPDATA", &fixture.cache_dir)
            .env("HOME", &fixture.cache_dir)
            .args([
                "--json",
                "--config",
                fixture.config_path.to_str().unwrap(),
                "query",
                "--wiki",
                "demo",
                "--agent",
                "claude",
                "--",
                "x",
            ])
            .assert()
            .failure()
            .code(3);
        let v = stdout_json(&assert);
        assert_eq!(v["error"]["code"], "AUTH_REQUIRED");
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn live_doctor_then_query_succeed_end_to_end_with_schema_absent_warning() {
        // No SCHEMA.md: proves OFF-197's WIKI_SCHEMA_ABSENT warning appears
        // in *both* doctor and a subsequent successful query, through the
        // real compiled binary.
        let fixture = build_provider_fixture(false);
        let common_env = |cmd: &mut Command| {
            cmd.env("LOCALAPPDATA", &fixture.cache_dir)
                .env("HOME", &fixture.cache_dir)
                .env("FAKE_AUTH", "true");
        };

        // --- doctor --live ---
        let mut doctor_cmd = bin();
        common_env(&mut doctor_cmd);
        let doctor_assert = doctor_cmd
            .args([
                "--json",
                "--config",
                fixture.config_path.to_str().unwrap(),
                "doctor",
                "--wiki",
                "demo",
                "--agent",
                "claude",
                "--live",
            ])
            .assert()
            .success()
            .code(0);
        let dv = stdout_json(&doctor_assert);
        assert_eq!(dv["ok"], true, "{dv:?}");
        let checks = dv["results"][0]["checks"].as_array().unwrap();
        let names: BTreeSet<&str> = checks.iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert!(names.contains("live_contract"), "{checks:?}");
        assert!(names.contains("mutation"), "{checks:?}");
        let wiki_structure = checks
            .iter()
            .find(|c| c["name"] == "wiki_structure")
            .unwrap();
        assert_eq!(wiki_structure["status"], "warn");
        assert_eq!(wiki_structure["code"], "WIKI_SCHEMA_ABSENT");

        // --- query, same cache dir: the just-published probe must satisfy
        // the Enforced-mode gate, so this succeeds rather than
        // ENTRYPOINT_UNVERIFIED. ---
        let mut query_cmd = bin();
        common_env(&mut query_cmd);
        let question = "繁體中文問題 with \"quotes\" and $() metacharacters";
        let query_assert = query_cmd
            .args([
                "--json",
                "--config",
                fixture.config_path.to_str().unwrap(),
                "query",
                "--wiki",
                "demo",
                "--agent",
                "claude",
                "--",
                question,
            ])
            .assert()
            .success()
            .code(0);
        let qv = stdout_json(&query_assert);
        assert_eq!(qv["ok"], true, "{qv:?}");
        assert_eq!(qv["answer"], "Fixture answer with a citation.");
        assert_eq!(qv["citations"][0]["slug"], "home");
        let warnings = qv["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w["code"] == "WIKI_SCHEMA_ABSENT"),
            "{warnings:?}"
        );

        // Human mode: answer, then (empty) gaps section skipped, then Warnings.
        let mut human_cmd = bin();
        common_env(&mut human_cmd);
        let human_assert = human_cmd
            .args([
                "--config",
                fixture.config_path.to_str().unwrap(),
                "query",
                "--wiki",
                "demo",
                "--agent",
                "claude",
                "--",
                question,
            ])
            .assert()
            .success();
        let human_out = String::from_utf8_lossy(&human_assert.get_output().stdout).into_owned();
        let answer_pos = human_out.find("Fixture answer with a citation.").unwrap();
        let warnings_pos = human_out.find("Warnings:").unwrap();
        assert!(
            answer_pos < warnings_pos,
            "answer must print before warnings: {human_out:?}"
        );
        // PRD 08-06-pre-0-1-0-cli-refinements AC2: `assert_cmd` always
        // captures stderr through an OS pipe, never a pty, so
        // `stderr.is_terminal()` is deterministically false here and the
        // spinner branch is never entered — stderr must be completely
        // empty on a successful human-mode run.
        assert!(
            human_assert.get_output().stderr.is_empty(),
            "no spinner bytes or diagnostics expected under a piped stderr: {:?}",
            String::from_utf8_lossy(&human_assert.get_output().stderr)
        );

        // Sanity: the wiki fixture's content root was never mutated by any
        // of the three real subprocess invocations above.
        let home_contents = fs::read(fixture.wiki_root.join("home.md")).unwrap();
        assert_eq!(home_contents, b"# Home\nFixture content.\n");
    }
}
