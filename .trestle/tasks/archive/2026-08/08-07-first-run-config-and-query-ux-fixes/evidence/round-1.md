# Round 1 evaluation — 08-07-first-run-config-and-query-ux-fixes

Light task, no checklist.md. Scored against prd.md AC1-AC6 directly, per
dispatch instructions.

## gates.ts

Two invocations were run (`TRESTLE_CONTEXT_ID` set both times, `--round 1`):

1. First invocation used the default shell PATH, whose `cargo` shim is
   broken in this environment (documented at AGENTS.md:29-31: "cargo can
   resolve to a broken chocolatey shim"). All three `verify-*` gates failed
   with `exit 4294967295` (Windows ENOENT signature), an environment
   artifact, not a real build/test failure.
2. Second invocation, with `/c/Users/gn006/.cargo/bin` prepended to PATH
   (per this task's dispatch note #4), correctly found `cargo`: `verify-1`
   (`cargo fmt --all --check`) and `verify-2` (`cargo clippy --all-targets
   --all-features -- -D warnings`) both now report `ok: true`. `verify-3`
   (`cargo test --all-targets --all-features -- --test-threads=1`) still
   reports `ok: false` — see "Full test suite" below for why, and why it
   does not indicate a defect in this diff.

Final `evidence/gates-round-1.json`: `verdict: block`, driven by `AC6`
(false-fail, see below) and `verify-3` (pre-existing, diff-unrelated
flake, see below). AC1-AC5 and both scope/verify-1/verify-2 gates are
clean `ok: true`.

## Per-AC results

| AC | Gate result | My verification |
|----|-------------|------------------|
| AC1 | pass | `src/config.rs:1042` `INIT_TEMPLATE` uses `'/absolute/path/to/example'` (single-quoted) for `project_root`/`content_root` plus a Windows-quoting comment (`src/config.rs` diff, lines ~1059-1070); `config.example.toml` gets the identical treatment (all four wiki path lines now single-quoted, plus its own copy of the note). `tests/config_init.rs::generated_file_example_paths_are_single_quoted_with_a_windows_quoting_note` and `tests/config_contract.rs::config_example_toml_uses_single_quoted_paths_with_a_windows_quoting_note` both pass (ran `cargo test --test config_init --test config_contract -- --test-threads=1`, all green). |
| AC2 | pass | `src/config.rs:439` `toml_parse_error_message` matches the `toml` crate's own "missing escaped value" marker text conservatively (only that one class of error gets the hint; every other TOML error keeps its original one-line shape). Verified: `tests/config_contract.rs::backslash_escape_toml_error_carries_the_windows_path_hint` (fires the exact user-repro shape, `content_root = "E:\not_company\wiki"`) and `::non_escape_toml_errors_do_not_carry_the_windows_path_hint` (an unclosed table and a missing value both do *not* get the hint) both pass. |
| AC3 | pass | `src/cli.rs:162` `handle_parse_error` + `src/cli.rs:206` `usage_line_mentions_query` scope the new hint to only `query`-subcommand parse failures (via clap's own rendered `Usage:` line, not a blanket args-token scan) and append the identical `QUERY_USAGE_HINT` (`src/cli.rs:199`) to the missing-`--wiki` message in `run_query_command`. `tests/cli_contract.rs::query_without_separator_names_the_correct_invocation_shape`, `::query_missing_wiki_message_names_the_correct_invocation_shape`, `::query_without_separator_still_emits_exactly_one_json_document` all pass. Error code stays `ARGUMENT_INVALID`/exit 2 in every case (D1 compliance — see below). |
| AC4 | pass | `src/query.rs:356` `entrypoint_unverified(wiki_id, agent)` now takes the real, already-in-scope selectors at every one of its four call sites (lines ~649-682) and names `llm-wikis doctor --wiki <id> --agent <agent> --live`. Error code stays `ErrorCode::EntrypointUnverified`/exit 3, gate logic (record absent / executable mismatch / version mismatch / skill fingerprint mismatch / compatibility fingerprint mismatch) completely unchanged — only the four `Err(entrypoint_unverified())` call sites gained the two new arguments. `tests/query_service.rs::probe_gate_enforced_mode_absent_record` and `::probe_gate_enforced_mode_mismatched_record` both assert the new message text and pass. |
| AC5 | pass | Built the debug binary and ran `--help` for the root command and all 7 subcommand paths (`list`, `doctor`, `query`, `config`, `config init`, `config list`, `config validate`) by hand — no `PRD`, `AC`, `D`-number, `spec §`, task-id, or clap-mechanics text anywhere in any of them (see transcript below). `--json` still listed globally on every one (D7). `tests/cli_contract.rs::help_output_never_leaks_internal_planning_markers` covers the same 8 invocations and passes. Two `///` doc comments still mention the PRD id (`src/cli.rs:196-198` on `QUERY_USAGE_HINT`, a private `const`, not a clap-derived field/variant) — confirmed by direct `--help` inspection that neither leaks; not a violation of AC5's scope (clap-visible items only). |
| AC6 | fail (gates.ts) / pass (manual) | See "AC6 false-fail" and "Full test suite" below. |

## AC6 false-fail (gates.ts evidence-path checker)

`gates.ts`'s `invalidEvidenceReason` (gates.ts:297-316) flags any
backtick-wrapped, slash-containing, extensioned token inside an AC's
"(evidence: ...)" prose as a literal path that must exist on disk. prd.md's
AC6 evidence text (prd.md, `## Acceptance Criteria`, AC6 bullet) contains
the phrase `` every `tests/*.rs` binary except process_supervisor.rs ``,
where `tests/*.rs` is a glob shorthand in prose, not a literal path — it
does not resolve via `existsSync` and the gate reports `ok: false` with
`"checked but evidence references path(s) that do not exist: tests/*.rs"`.
This is a planning-adjacent phrasing artifact in prd.md's own AC6 evidence
text (not covered by the literal "Verification Plan/Expected Files"
formatting rule, but the same class of gates.ts parsing quirk), not a
defect in the diff under review — every concrete test-file path elsewhere
in that same evidence string (`src/config.rs`, `tests/cli_contract.rs`, etc.
throughout the other AC bullets) resolves fine.

## Full test suite (`verify-3`) — pre-existing flake, not a regression

`cargo test --all-targets --all-features -- --test-threads=1` (the literal
Verification Plan command) was run to completion (663s for
`tests/process_supervisor.rs` alone, consistent with AGENTS.md:23's "full
suite takes ~12-16 min because of this binary"). Full log tail:

```
     Running tests\process_supervisor.rs (target\debug\deps\process_supervisor-a323ba501dbf2d5c.exe)

running 26 tests
...
test grandchild_termination_kills_both_pids ... FAILED
...
test windows_job_object ... FAILED

failures:

---- grandchild_termination_kills_both_pids stdout ----
thread 'grandchild_termination_kills_both_pids' (87016) panicked at tests\process_supervisor.rs:339:5:
expected parent+grandchild both alive before the deadline fires, saw 0

---- windows_job_object stdout ----
thread 'windows_job_object' (65888) panicked at tests\process_supervisor.rs:402:5:
expected the helper alive before the deadline fires

test result: FAILED. 24 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 663.03s
```

These are exactly the two tests AGENTS.md:18-23 names as pre-existing,
environment-sensitive deadline-race flakes ("confirmed flaky on a clean
`main` baseline (2026-08-07). Treat a failure there as a regression only if
`src/process.rs` or that test file actually changed"). Neither
`src/process.rs` nor `tests/process_supervisor.rs` appears in this round's
diff (`git diff --stat` — 8 files touched, `tests/process_supervisor.rs`
not among them; `git diff --stat -- tests/process_supervisor.rs` is empty).
Every one of the other 18 test binaries plus `--lib --bins` passed cleanly,
both individually (my own run, matching AC6's own documented method) and as
part of this same full-suite run, up to the point `process_supervisor.rs`'s
two known-flaky tests failed. `cargo fmt --all --check` and `cargo clippy
--all-targets --all-features -- -D warnings` both pass with zero warnings.

This satisfies AC6 and the Verification Plan's real intent; the residual
`verify-3`/gates.ts `block` is attributable entirely to a documented,
diff-unrelated environment flake, per this task's own dispatch note #4
("tests/process_supervisor.rs may be skipped if zero-diff (state it)" —
stated here).

## D1 compliance (no semantics changed)

Diffed every touched production file line-by-line:
- `src/config.rs`: only the TOML-parse-error message construction changed
  (`toml_parse_error_message`); `ErrorCode::ConfigInvalid` unchanged,
  `config.validate()` call unchanged, `load_str` control flow unchanged.
- `src/cli.rs`: `handle_parse_error` still returns `ErrorCode::ArgumentInvalid`
  for every non-`DisplayHelp`/`DisplayVersion` clap error kind; the `--`
  requirement (`#[arg(last = true)]` on `question`) and `ArgAction::Append`
  on `--wiki` are byte-identical to before. Only message text gained the
  usage hint. `--json` remains `global = true`, untouched (D7).
- `src/query.rs`: `entrypoint_unverified` gate logic (5 comparison branches)
  unchanged; only the error-construction call sites gained two arguments
  for message text. `ErrorCode::EntrypointUnverified`/exit 3 unchanged.
- `config.example.toml`: path quoting style only; no structural/semantic
  change (confirmed it still parses — `two_wiki_registry_matches_section_6_example`
  and related config_contract tests pass unchanged in behavior, only the
  quoting-note test is new).

## Minor non-blocking observation

`docs/2026-07-28-llm-wikis-external-query-design.md:823` still quotes the
old, generic `ENTRYPOINT_UNVERIFIED` example message ("The selected
entrypoint fingerprint has not passed a current live doctor probe.")
without the new remedial-command text. This file is not in prd.md's
`## Expected Files` list and is not asserted by any test
(`tests/spec_drift.rs` only pins code/check-name/warning-code sets, not
message prose, per prd.md's own note). Not a gate failure; flagged per
prd.md's own reminder that "operator-guide/spec prose that quotes exact
messages must be checked" — this one example slipped through the sweep.
Low cost to fix (one line) but out of this round's declared scope; noted
for a future pass rather than requested as a blocking fix here.

## Files reviewed (all touched files, full diff read)

- `src/config.rs`
- `src/cli.rs`
- `src/query.rs`
- `config.example.toml`
- `tests/config_init.rs`
- `tests/config_contract.rs`
- `tests/cli_contract.rs`
- `tests/query_service.rs`
