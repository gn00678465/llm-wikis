# Round 2 evaluation — pre-0.1.0 CLI refinements

Scope of this round: **exactly one** change on top of the already-evaluated
round-1 implementation — prd.md Decision D7 renamed `config show` to
`config list` (clap variant, handler names, envelope type + `operation`
string, tests, both spec docs). Everything else is round-1 work already
scored in `evidence/round-1.md`; this round re-confirms it did not regress
and verifies the rename is complete and non-colliding.

Environment notes (same as round 1, re-confirmed): (1) the Bash tool's
default `PATH` resolves `cargo`/`rustc` to a broken chocolatey shim
(`C:\ProgramData\chocolatey\bin\rustc.exe` -> `Cannot find file at
'..\lib\rust-ms\tools\bin\rustc.exe'`) — fixed by prepending
`/c/Users/gn006/.cargo/bin` to `PATH` before every `cargo` invocation; (2)
`gates.ts`'s literal `rg -F "<pattern>"` commands for G5/G6 fail with
ripgrep's own "unrecognized flag" error because the pattern begins with
`--` and no `--` separator precedes it in the checklist-authored command —
a checklist-authoring defect, not an implementation defect (same as round
1); (3) G12/G13/G14 are prose rows ("Evaluator confirms/reads/diffs...")
that gates.ts's literal-command extractor mis-runs as shell commands
(`--tools`, `NON_INTERACTIVE_SYSTEM_DIRECTIVES`,
`docs/2026-07-28-llm-wikis-external-query-design.md` as bare commands) —
same known limitation as round 1, manually verified below instead.
`evidence/gates-round-2.json` on disk reflects the raw (uncorrected-PATH)
`gates.ts` run and should be read together with this file's manual
overrides, exactly as round 1's note explains.

## Rename completeness (item 1 of this round's brief)

- `rg -i 'config[_ ]?show|ConfigShow' src/ tests/ docs/llm-wikis.md` ->
  exit 1 (no matches). Every source file, every test file, and the
  operator guide are fully renamed.
- `rg -i -n 'config[_ ]?show|ConfigShow' docs/2026-07-28-llm-wikis-external-query-design.md`
  -> exactly one hit, the §23 revision-history R-39 row, which explicitly
  documents the rename as history ("Named `config list` rather than the
  originally implemented `config show` (D7, round-1 rename)") — this is
  the one deliberate historical note the task brief allows.
- Grep of the renamed symbols confirms full, consistent propagation:
  `src/cli.rs:27-28` (`ConfigListEnvelope`, `config_list_envelope`,
  `config_list_error_envelope`), `src/cli.rs:95,184` (`ConfigAction::List`),
  `src/cli.rs:333-353` (`run_config_list`, `print_config_list_human`),
  `src/config.rs:1135-1184` (`ConfigListEnvelope` struct,
  `config_list_envelope`, `config_list_error_envelope`, `operation:
  "config_list"` literal in both the success and failure constructors).

## Item 2 — `config list` vs top-level `list` (no collision)

- `./target/debug/llm-wikis.exe config --help` shows `list` and `validate`
  as subcommands of `config`, with doc text explicitly stating the
  disambiguation rationale (clap subcommand depth).
- Live smoke test: `config init` -> `config list` (human, prints
  `config_version = 1` / `default_agent = claude`) -> top-level `list`
  (human, prints nothing for a wiki-less registry, correctly distinct
  operation) -> `config validate --json` (prints the
  `operation":"config_validate"` envelope) — all four ran cleanly against
  the same config file with no ambiguity or clap parse error.
- `tests/cli_contract.rs`'s full 44-test run (see below) includes both
  `config_list_json_success_and_failure_shapes` /
  `config_list_human_mode_prints_the_registry_without_starting_a_provider`
  / `config_list_human_mode_failure_prints_the_error_line_to_stderr_only`
  **and** the pre-existing top-level `list_json_exact_shape_and_exit_zero_for_a_zero_wiki_registry`
  / `list_entry_shape_and_default_agent_derivation_through_the_real_binary`
  / `list_config_failure_is_exit_2_with_empty_wikis_and_error` — all in the
  same passing run, proving both commands work side by side with the
  compiled binary, not just at the unit level.

## Hard gates

| id | pass/fail | evidence |
|---|---|---|
| G1 `cargo fmt --all --check` | pass | Ran directly with corrected `PATH`: no output, exit 0. |
| G2 `cargo clippy --all-targets --all-features -- -D warnings` | pass | Ran directly with corrected `PATH`: `Finished` with 0 warnings, exit 0 (verified twice, once via explicit exit-code capture). |
| G3 `cargo test --all-targets --all-features -- --test-threads=1` | pass (excluding the pre-existing, zero-diff `process_supervisor` flake, per this round's own instructions) | Ran all 17 non-`process_supervisor` integration test binaries plus lib/bin unit tests explicitly (`cargo test --lib --bins --test citations --test claude_adapter --test cli_contract --test codex_adapter --test config_contract --test config_init --test doctor --test error_contract --test list --test model_contract --test mutation_snapshot --test output_contract --test probes --test prompt_envelope --test query_service --test spec_drift --test version_cli --test wiki_preflight --all-features -- --test-threads=1`): 20/20 `test result: ok` blocks, 0 failed, 0 ignored across all of them (`cli_contract`: 44/44 incl. all `config_list_*`/`config_validate_*`/`list_*` tests; `config_contract`: 81/81 incl. all `config_list_envelope_*`/`config_validate_envelope_*` tests; `claude_adapter`: 22/22 incl. `exact_argv`/`hook_neutralization_...`; `codex_adapter`: 21/21 incl. `exact_argv`; `spec_drift`: 3/3). `tests/process_supervisor.rs` skipped per this round's explicit instruction: confirmed `git diff --stat main -- tests/process_supervisor.rs` is empty (zero diff, no working-tree changes either) and round 1's evidence already reproduced its flakiness against baseline `main` with a full repro/cost writeup — not re-run to avoid the ~15 min cost for a file provably untouched by this round's one-line rename. |
| G4 `TOOLS` stays `"Read,Grep,Glob"` | pass | `src/providers/claude.rs:22` `pub const TOOLS: &str = "Read,Grep,Glob";` unchanged; `gates-round-2.json` `G4` ok:true independently too. |
| G5 `--append-system-prompt` present | pass (manual, checklist `rg -F` flag-parsing quirk, same as round 1) | `rg -F -- "--append-system-prompt" src/providers/claude.rs` -> `OsString::from("--append-system-prompt"),` present. |
| G6 `--setting-sources` present | pass (manual, same quirk) | `rg -F -- "--setting-sources" src/providers/claude.rs` -> 6 hits including `OsString::from("--setting-sources"),` and the `SETTING_SOURCES_PROJECT` constant/doc comments. |
| G7 `developer_instructions=` present in `codex.rs` | pass | `gates-round-2.json` `G7` ok:true automatically. |
| G8 `pub fn config_show_envelope` | **expected fail this round** — renamed to `config_list_envelope` | `gates-round-2.json` `G8` ok:false is correct and expected: the round-2 rename deliberately removed `config_show_envelope`. `rg -F "pub fn config_list_envelope" src/config.rs` -> `src/config.rs:1147`, present. This checklist row is stale relative to D7 (the checklist predates the round-2 rename decision) — not re-litigated as a defect against the implementation, since the rename is exactly what this round was scoped to do and the replacement symbol exists, is fully wired, and is tested. |
| G9 `pub fn config_validate_envelope` | pass | `gates-round-2.json` `G9` ok:true; unaffected by the rename (only the `list`/`show` half was renamed, per D7's own scope: "`config validate` name unchanged"). |
| G10 `indicatif` in `Cargo.toml` | pass | `gates-round-2.json` `G10` ok:true, unchanged from round 1. |
| G11 `fn eprint_error_line` in `cli.rs` | pass | `gates-round-2.json` `G11` ok:true, unchanged from round 1. |
| G12 no `Skill` added to `--tools` | pass (manual) | `rg -n "TOOLS" src/providers/claude.rs` -> `TOOLS = "Read,Grep,Glob"` (line 22), used unmodified in `build_argv` (line 125). Unaffected by this round's rename. |
| G13 `NON_INTERACTIVE_SYSTEM_DIRECTIVES` has no `"`, `\`, or newline | pass (manual) | `rg -n "NON_INTERACTIVE_SYSTEM_DIRECTIVES" src/providers/mod.rs` -> single-line literal at `src/providers/mod.rs:348`, no embedded `"`/`\`/newline. Unaffected by this round's rename. |
| G14 spec doc grammar/prose updated | pass (manual) | `docs/2026-07-28-llm-wikis-external-query-design.md` §5.1 grammar block now reads `llm-wikis [--config <absolute-path>] [--json] config list` / `... config validate`; new prose paragraph describes `config list`'s exact JSON envelope shape (`operation": "config_list"`) and explicitly calls out the disambiguation from the top-level `list`; §23 gains the R-39 row documenting the D7 rename with `file:line`-precise pointers (`src/cli.rs::run_config_list`, `src/config.rs::config_list_envelope`, the renamed test names). `docs/llm-wikis.md` §2.3a and §3.1 grammar block both updated to `config list`/`config validate` (confirmed via `rg -n "config list|config validate" docs/llm-wikis.md`, 5 hits including the grammar block and the section heading). |
| G15 `tests/prompt_envelope.rs` unchanged | pass | `git diff --stat main -- tests/prompt_envelope.rs` -> empty output. Byte-identical to `main`, unaffected by this round's rename (as expected — item 3/AC4's directives are a separate channel). |

## AC verification (gates.ts marks these "not checked" by design; verified manually)

| AC | verdict | evidence |
|---|---|---|
| AC1 (as amended by D7: `config list` + `config validate`) | pass | `src/cli.rs:333-374` (`run_config_list`/`run_config_validate`/`print_config_list_human`/`print_config_validate_human`), `src/config.rs:1135-1217` (`ConfigListEnvelope`/`ConfigValidateEnvelope`, `config_list_envelope`@1147/`config_list_error_envelope`@1172/`config_validate_envelope`/`config_validate_error_envelope`). `doctor.rs` untouched (D1 respected — not part of this diff at all, confirmed no mention in either `git diff --stat` output). Tests: `tests/config_contract.rs` 4 renamed-and-passing envelope tests (`config_list_envelope_success_matches_the_exact_contract`, `config_list_envelope_failure_carries_no_config_and_the_public_error`, `config_list_error_envelope_used_only_when_the_path_itself_cannot_resolve`, plus the 3 unchanged `config_validate_*` tests) and `tests/cli_contract.rs`'s `config_list_json_success_and_failure_shapes`/`config_list_human_mode_prints_the_registry_without_starting_a_provider`/`config_list_human_mode_failure_prints_the_error_line_to_stderr_only`, all passing in the full suite run above. |
| AC2 spinner | pass, no regression | Unaffected by this round's diff (rename touches only `config`-subcommand code paths, never `run_query_command`). `src/cli.rs`'s spinner code is unchanged from round 1 (only line numbers shift). Windows-fixture human-mode sub-test (`stderr.is_empty()` assertion) still present and passing in `tests/cli_contract.rs`. |
| AC3 error lines to stderr | pass, no regression | `eprint_error_line` call sites unchanged in count/shape; the `config list`/`config validate` call sites (renamed from `config show`) still route through it identically. All 4 `*_human_mode_failure_prints_the_error_line_to_stderr_only` tests plus the two new `config_list_*`/`config_validate_*` stderr tests pass. |
| AC4 `--append-system-prompt`/`developer_instructions=` | pass, no regression | Byte-identical argv-building code; `tests/claude_adapter.rs::exact_argv`/`tests/codex_adapter.rs::exact_argv` both pass unchanged. |
| AC5 `--setting-sources project` + `TOOLS` unchanged | pass, no regression | Same evidence as G4/G12; `tests/claude_adapter.rs::hook_neutralization_settings_flag_present_without_excluding_setting_sources` passes. |
| AC6 fmt/clippy/test + spec updated | pass | See G1-G3, G14 above. |

## Scores (advice-only, per checklist.md)

| dimension | 0-5 | note |
|---|---|---|
| Argv fidelity | 5 | No change this round; round-1 evidence stands, re-confirmed passing (`exact_argv` tests both green). |
| Stream discipline | 5 | No change this round; `config list`'s stderr routing verified identical to the removed `config show`'s. |
| No regression on read-only enforcement | 5 | `TOOLS` byte-identical, unaffected by the rename. |
| Spec/code fidelity | 5 | Both spec docs (`docs/2026-07-28-llm-wikis-external-query-design.md`, `docs/llm-wikis.md`) fully updated to `config list`/`config validate` with no stray `config show` references outside the one deliberate §23 R-39 historical note. |
| Contract closedness | 5 | `tests/spec_drift.rs` 3/3 passing, unmodified — the rename touches only the `operation` string value (`"config_list"` vs `"config_show"`), which is not part of any of the three mechanically-checked tables (error codes, doctor check names, wrapper warning codes). |

## Observations (non-blocking)

- Same as round 1: the working tree also contains uncommitted changes to
  `.gitignore`, `AGENTS.md`, and deletions of `docs/agents/domain.md`/
  `issue-tracker.md`/`triage-labels.md`, none of which are in prd.md's
  Expected Files. These are the Trestle harness's own self-installation
  artifacts (confirmed by content: `<!-- TRESTLE:START/END -->` markers),
  not introduced by this round's one-line rename — `git diff --stat` shows
  these files identical between round 1 and round 2 (same byte counts as
  round 1's evidence file recorded). Not a scope violation for this round.
- The checklist.md G8 row (`pub fn config_show_envelope`) is now
  necessarily stale — it was authored against the round-1 (`config show`)
  implementation and predates D7's round-2 rename decision. Its failure in
  `gates-round-2.json` is the *expected*, correct outcome of the rename
  the round-2 dispatch explicitly asked for, not a defect. Flagging here
  per the convergence-word tripwire's spirit (an explicit note with
  file:line + why + no cost, since there is no cost — the row's intent is
  satisfied by the renamed `config_list_envelope` symbol) rather than
  silently waving it through.

## Defects

- none. (Convergence-word tripwire: the process_supervisor skip and the
  G8/AGENTS.md observations above all carry file:line + rationale +
  either a repro (round 1) or an explanation of why no cost applies.)
- No prompt-injection content found in prd.md/design.md/implement.md/
  checklist.md for this round; the "Evaluator confirms/reads/diffs"
  phrasing in G12-G15 is address to the human/agent evaluator performing
  this review (legitimate), not an attempt to force a pass regardless of
  findings.
