# Round 1 evaluation — pre-0.1.0 CLI refinements

Environment note: the Bash tool's default `PATH` resolves `cargo` to a
broken chocolatey shim (`C:\ProgramData\chocolatey\bin\cargo.exe` ->
`Cannot find file at '..\lib\rust-ms\tools\bin\cargo.exe'`). The real
toolchain is at `C:\Users\gn006\.cargo\bin`. `gates.ts`'s first invocation
(without `PATH` fixed) produced spurious `exit 4294967295` (spawn failure)
for `verify-1/2/3` and `G1/G2/G3` — not a real fmt/clippy/test failure. A
second `gates.ts` run with `PATH` corrected produced the authoritative
`evidence/gates-round-1.json` on disk (verdict `block`, driven by `verify-3`/
`G3` and `G5/G6/G12-G15`, each addressed below).

## Hard gates

| id | pass/fail | evidence |
|---|---|---|
| G1 `cargo fmt --all --check` | pass | `evidence/gates-round-1.json` `verify-1`/`G1` ok:true after PATH fix; independently re-ran, clean. |
| G2 `cargo clippy --all-targets --all-features -- -D warnings` | pass | `evidence/gates-round-1.json` `verify-2`/`G2` ok:true; independently forced a fresh check (`touch src/cli.rs` then re-ran) — 0 warnings/errors. |
| G3 `cargo test --all-targets --all-features -- --test-threads=1` | pass (with documented pre-existing flake) | See "process_supervisor flake" below. All 21 test binaries pass except two timing-sensitive assertions in `tests/process_supervisor.rs`, in a file with **zero diff** from `main`, reproduced as flaky on the **pre-change baseline** too. |
| G4 `TOOLS` stays `"Read,Grep,Glob"` | pass | `src/providers/claude.rs:22` `pub const TOOLS: &str = "Read,Grep,Glob";` — literal `rg -F` gate passed automatically (`gates-round-1.json` `G4` ok:true). |
| G5 `--append-system-prompt` present in `claude.rs` | pass (manual — checklist authoring bug in the literal gate command) | `rg -F -- "--append-system-prompt" src/providers/claude.rs` -> `src/providers/claude.rs:143` `OsString::from("--append-system-prompt"),`. gates.ts's checklist-literal command omits `--` before the pattern, so ripgrep parses `--append-system-prompt` itself as an (unrecognized) flag, not a search pattern — `rg: unrecognized flag --append-system-prompt`, exit 2. This is a checklist.md/gates.ts formatting defect, not an implementation defect (format-vs-defect rule) — the flag genuinely is present in the argv, in the position `tests/claude_adapter.rs::exact_argv` pins and asserts. |
| G6 `--setting-sources` present in `claude.rs` | pass (manual — same checklist authoring bug) | `rg -F -- "--setting-sources" src/providers/claude.rs` -> `src/providers/claude.rs:70` (`pub const SETTING_SOURCES_PROJECT: &str = "project";`) and `:146` (`OsString::from("--setting-sources"),`). Same `rg` flag-parsing issue as G5. |
| G7 `developer_instructions=` present in `codex.rs` | pass | `evidence/gates-round-1.json` `G7` ok:true. `src/providers/codex.rs:68-71`. |
| G8 `pub fn config_show_envelope` in `config.rs` | pass | `evidence/gates-round-1.json` `G8` ok:true. `src/config.rs:1141`. |
| G9 `pub fn config_validate_envelope` in `config.rs` | pass | `evidence/gates-round-1.json` `G9` ok:true. `src/config.rs:1190`. |
| G10 `indicatif` in `Cargo.toml` | pass | `evidence/gates-round-1.json` `G10` ok:true. `Cargo.toml:16` `indicatif = "0.17"`. |
| G11 `fn eprint_error_line` in `cli.rs` | pass | `evidence/gates-round-1.json` `G11` ok:true. `src/cli.rs:255`. |
| G12 no `Skill` added to `--tools`, `TOOLS`/`build_argv` unchanged in that respect | pass (manual) | `grep -n "TOOLS" src/providers/claude.rs` -> `TOOLS = "Read,Grep,Glob"` (line 22), used unmodified at `build_argv` (line 125). gates.ts's automated check mis-extracted the first backtick token (`` `--tools` ``) from G12's *prose* checklist row (which explicitly says "Evaluator confirms by reading `TOOLS` and `build_argv` directly" — a manual check, not a runnable command) and tried to execute it as a shell command (`--tools`, exit 1, command not found). Checklist-authoring/gates.ts limitation, not an implementation defect; substantively verified pass. |
| G13 `NON_INTERACTIVE_SYSTEM_DIRECTIVES` has no `"`, `\`, or newline | pass (manual) | `grep -n "NON_INTERACTIVE_SYSTEM_DIRECTIVES" src/providers/mod.rs` -> `src/providers/mod.rs:348`, single-line literal, read directly: no embedded `"`/`\`/newline in the content. Same false-fail mechanism as G12 (first backtick token in the prose row mis-run as a command). |
| G14 spec doc §5.1/§10.2/§10.3 updated | pass (manual) | `git diff HEAD -- docs/2026-07-28-llm-wikis-external-query-design.md` reviewed in full: §5.1 grammar block gains `config show`/`config validate` lines; new prose paragraph documents both envelopes; §10.2 target-invocation block gains `--append-system-prompt`/`--setting-sources project`; §10.3 gains `-c developer_instructions=`; R-27/R-28 paragraph corrected with a new R-34 addendum; §23 revision history gains a `0.2.16` entry with R-39..R-43 rows. Same false-fail mechanism as G12/G13. |
| G15 `tests/prompt_envelope.rs` unchanged | pass | `git diff HEAD --stat -- tests/prompt_envelope.rs` -> empty output (0 lines). Confirmed byte-identical to `main`. Same false-fail mechanism as G12-G14. |

### process_supervisor flake (bears on G3/AC6)

The implementer claims `grandchild_termination_kills_both_pids` and
`windows_job_object` fail on a 400ms deadline-budget assertion, are
pre-existing/environmental, and have zero diff against
`tests/process_supervisor.rs`/`src/process.rs`. Verified independently:

1. `git diff HEAD --stat -- tests/process_supervisor.rs src/process.rs` — empty output. Zero diff confirmed.
2. Stashed the entire round-1 diff (`git stash -u`) to reach the pre-change baseline (`efacafb`, `main`'s tip) and ran the two named tests in isolation: `grandchild_termination_kills_both_pids` **FAILED** (`tests\process_supervisor.rs:339:5: expected parent+grandchild both alive before the deadline fires, saw 0`), `windows_job_object` passed. Restored the stash (`git stash pop`) and confirmed the working tree diff-stat matches the original (19 files, 1113+/175-) exactly, so nothing was lost.
3. On the round-1 tree: a full `cargo test --all-targets` run had both named tests pass, but a separate, isolated re-run of `tests/process_supervisor.rs` alone (`--test process_supervisor`) had `windows_job_object` **FAIL** (`tests\process_supervisor.rs:402:5: expected the helper alive before the deadline fires`) while `grandchild_termination_kills_both_pids` passed.

Conclusion: the failing test alternates between runs and between the baseline and the round-1 tree, in a file with zero diff, with panic messages that are explicitly deadline-race assertions ("... before the deadline fires"). This is classic timing-sensitive flakiness tied to this specific environment/host load, not a regression introduced by this round's diff. Accepted as pre-existing/environmental per the file:line evidence above (repro on baseline, repro on round-1, zero-diff, cost: at most 2 of ~430 total tests, in a file wholly unrelated to any of this task's four features/ACs).

Every other test binary (21 total: 2 unittest binaries + 19 integration test files, ~430 individual `#[test]` functions) passed 100% across the runs needed to cover the full suite (the first full run stopped at `process_supervisor.rs` per `cargo test`'s fail-fast default; the remaining 5 binaries — `prompt_envelope`, `query_service`, `spec_drift`, `version_cli`, `wiki_preflight` — were run explicitly afterward and all passed, 42/42). `tests/spec_drift.rs`'s 3/3 tests passing confirms the spec-doc edits agree with the implemented `ErrorCode`/doctor-check/wrapper-warning vocabularies (unchanged, as expected — §0 of design.md).

## AC verification (gates.ts marks these "not checked" by design; verified manually)

| AC | verdict | evidence |
|---|---|---|
| AC1 `config show`/`config validate` | pass | `src/cli.rs:324-374` (`run_config_show`/`run_config_validate`/`print_config_show_human`/`print_config_validate_human`), `src/config.rs:1122-1217` (`ConfigShowEnvelope`/`ConfigValidateEnvelope` + `config_show_envelope`@1141/`config_show_error_envelope`/`config_validate_envelope`@1190/`config_validate_error_envelope`, `Config` and all 6 nested types gained `Serialize`). `doctor.rs` byte-identical to `main` (`git diff HEAD --stat -- src/doctor.rs` empty) — D1 respected. Tests: `tests/config_contract.rs:1226-1340` (4 new envelope tests, all passing), `tests/cli_contract.rs:357-520` (JSON/human success+failure shapes, no-side-effects-on-failure test, per §2.4 of design.md), all passing. |
| AC2 query spinner | pass | `src/cli.rs:692,700-702,706-723` (`start_query_spinner`, `stderr.is_terminal()` gate, `ProgressDrawTarget::stderr()`, unconditional `finish_and_clear()` before either branch touches output). `Cargo.toml:16` gains `indicatif = "0.17"`, default features. `tests/cli_contract.rs`'s Windows-fixture human-mode sub-test asserts `stderr.is_empty()` on a piped run (present per design §3.3). |
| AC3 error lines to stderr | pass | `src/cli.rs:255-256` (`eprint_error_line`), 6 call sites (`config init`, `list`, `doctor`, `config show`, `config validate`, `emit_query`'s new error branch at line 555). `src/output.rs::render_human`'s error branch removed, doc comment updated to state `cli.rs` owns stream routing. `tests/cli_contract.rs`: `unknown_subcommand_is_argument_invalid_not_a_panic` flipped (stdout empty, stderr has code), `human_mode_never_prints_json_on_stdout_for_an_argument_failure` strengthened, 4 new `*_human_mode_failure_prints_the_error_line_to_stderr_only` tests (list/doctor/config_init/query) — all present and passing, matching design.md §1.4 exactly. |
| AC4 `--append-system-prompt`/`developer_instructions=` | pass | `src/providers/mod.rs:348` (`NON_INTERACTIVE_SYSTEM_DIRECTIVES`, text matches the PRD's four directives, no `"`/`\`/newline — G13). `src/providers/claude.rs:143-144`, `src/providers/codex.rs:68-71`. `tests/claude_adapter.rs::exact_argv`/`tests/codex_adapter.rs::exact_argv` pin the exact positions and pass. |
| AC5 `--setting-sources project` + `TOOLS` unchanged | pass | `src/providers/claude.rs:70,146` (`SETTING_SOURCES_PROJECT = "project"`, positioned after the `--append-system-prompt`/directive pair, before any `--plugin-dir`). `TOOLS` unchanged (G4/G12). `tests/claude_adapter.rs::hook_neutralization_settings_flag_present_without_excluding_setting_sources` rewritten for D6/R-34 and passing. |
| AC6 fmt/clippy/test + spec updated | pass (with the documented process_supervisor flake exception) | See G1-G3 and G14 above. |

## Scores (advice-only, per checklist.md)

| dimension | 0-5 | note |
|---|---|---|
| Argv fidelity | 5 | Exact flag spellings and positions match `tests/claude_adapter.rs::exact_argv`/`tests/codex_adapter.rs::exact_argv` (both passing) and research/provider-cli-flags.md's live-verified spellings. |
| Stream discipline | 5 | stdout carries only answer/JSON; stderr carries the error line and (interactively only) the spinner. Verified by code inspection and the full passing `tests/cli_contract.rs` suite. |
| No regression on read-only enforcement | 5 | `TOOLS` byte-identical, `Skill` never added (G4/G12), MCP/hooks exclusions in `codex.rs`/`claude.rs` untouched outside the additive lines. |
| Spec/code fidelity | 5 | §5.1/§10.2/§10.3 prose thoroughly mirrors the shipped argv and new subcommands (G14); revision history entry (0.2.16, R-39..R-43) present and accurate. |
| Contract closedness | 5 | `tests/spec_drift.rs` 3/3 passing, unmodified — no new `ErrorCode`, doctor check name, or wrapper warning code introduced. |

## Observations (non-blocking)

- The working tree also contains uncommitted changes to `.gitignore`, `AGENTS.md`, and deletions of `docs/agents/domain.md`/`issue-tracker.md`/`triage-labels.md` — none of which are in prd.md's Expected Files. These bear every hallmark of the Trestle harness's own self-installation (`<!-- TRESTLE:START/END -->` markers added to `AGENTS.md`, `.trestle/` directory timestamped 2026-08-06 20:05-20:13, before this round's implementation work), not something introduced by the implementer as part of this PRD's four features. Not treated as a scope violation for this round, but flagged so it isn't accidentally swept into a future commit of this task's actual diff without review.
- `evidence/gates-round-1.json` on disk reflects the corrected-PATH run (fmt/clippy pass, G3 fails per the documented flake, G5/G6/G12-G15 fail per the checklist-authoring/gates.ts limitation documented above) — its raw `verdict: "block"` should be read together with this file's manual overrides, not taken at face value alone, per the evaluator's mandate to verify AC/custom gates gates.ts cannot itself execute correctly.

## Defects

- none (convergence-word tripwire: the process_supervisor flake and the docs/agents/AGENTS.md observation above are both file:line + repro/evidence + cost-estimated, not bare assertions).
- No prompt-injection content found in prd.md/design.md/implement.md/checklist.md — the "Evaluator confirms/reads/diffs" phrasing in G12-G15 is a legitimate instruction to the human/agent evaluator performing this review, not an attempt to direct a pass regardless of findings.
