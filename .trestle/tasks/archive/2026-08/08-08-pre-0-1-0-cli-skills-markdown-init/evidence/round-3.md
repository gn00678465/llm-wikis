# Round 3 evaluation — 08-08-pre-0-1-0-cli-skills-markdown-init

Diff under review: `git diff HEAD` (working tree, uncommitted), same file set
as Round 1/2 (`Cargo.toml`, `Cargo.lock`, `README.md`,
`docs/2026-07-28-llm-wikis-external-query-design.md`, `docs/llm-wikis.md`,
`src/cli.rs`, `src/config.rs`, `src/output.rs`, `tests/cli_contract.rs`,
`tests/config_contract.rs`, `tests/output_contract.rs`,
`skills/llm-wikis-usage/**`), plus this round's sole remediation:
`prd.md`'s AC2/AC3/AC4 evidence lines (86/94/100) rewritten so the leading
`file.rs::test_name` citation is no longer one glued token (now
`tests/cli_contract.rs, test \`name\`` etc.), per Round 2's action item 1.

## gates.ts invocation

`TRESTLE_CONTEXT_ID` set, `/c/Users/gn006/.cargo/bin` prepended to PATH
(same fix as Rounds 1/2 — default PATH resolves `cargo` to a broken
chocolatey shim, per `AGENTS.md`'s working rule). Ran once, in the
background (~15 min: build+fmt+clippy+full test+three filtered test runs).
Output: `evidence/gates-round-3.json` (`rerun: false`).

Result: **AC2/AC3/AC4 now `ok: true`** — the Round 2 evidence-citation fix
worked exactly as diagnosed: gates.ts's `invalidEvidenceReason` no longer
flags a glued `file.rs::test_name` token because the file path and the
backticked test name are now separated by `, test `. Verified the fix
directly by reading `prd.md`'s current AC2/AC3/AC4 evidence lines (all now
read `tests/cli_contract.rs, test \`name\`` / `tests/config_init.rs, test
\`name\`` rather than `tests/cli_contract.rs::name`) and by re-checking
every cited test still exists at its claimed line number (unchanged from
Round 2 — re-verified independently this round rather than only trusting
the prior citation):

- `tests/cli_contract.rs:389` `config_init_force_overwrites_an_existing_file_without_a_prompt` — confirmed.
- `tests/cli_contract.rs:421` `config_init_without_force_still_refuses_to_overwrite_an_existing_file` — confirmed (`fn` at line 420, `#[test]` at 419).
- `tests/cli_contract.rs:441` `config_init_yes_produces_byte_identical_output_to_the_default_template` — confirmed.
- `tests/cli_contract.rs:469` `config_init_generated_file_passes_config_validate_regardless_of_which_path_produced_it` — confirmed.
- `tests/config_init.rs:100` `success_envelope_matches_the_exact_contract` — confirmed.
- `tests/config_init.rs:124` `failure_envelope_keeps_created_false_and_carries_the_public_error` — confirmed.
- `tests/output_contract.rs:170` `render_markdown_ansi_produces_escape_bytes_for_a_heading_and_bold_text` — confirmed.
- `tests/output_contract.rs:181` `render_markdown_ansi_round_trips_plain_text_without_markdown_syntax` — confirmed.

Every other AC/gate that passed in Round 2 (`AC1`, `AC5`, `verify-1..3`/`G1..G3`,
`G5..G24`, every `scope-*` gate, `DC1`) still passes this round, re-confirmed
directly rather than only carried forward:

- `G25` (line-141 JSON-envelope sentence byte-identical): read the line
  directly — still exactly `{ "schema_version": "1.0", "ok": true,
  "operation": "config_init", "path": "<absolute config path>", "created":
  true }`. Pass.
- `G26` (§3.2 no longer lists "an interactive configuration wizard" as a
  bare bullet): read the section directly — now reads "an interactive
  wizard for registering wikis (`config add-wiki`) — `config init`'s own
  short wizard covers only `default_agent` and provider executables, never
  wiki registration". Pass.
- `G27`/`G28` (manual TTY scenarios): `evidence/manual-tty-verification.md`
  still present, unchanged since Round 2, still carries a verifier
  identity, real-terminal binary path, a verbatim transcript for the
  declined-overwrite scenario, and explicit per-scenario confirmations for
  all four G27 and all four G28 rows, each mapped to the matching row of
  design.md §1.2/§2.2. Pass.
- `G29` (SKILL.md matches shipped CLI surface): read `skills/llm-wikis-usage/SKILL.md`
  directly this round — its `## Commands` block shows `[--yes] [--force]`
  on the `config init` line and `[--plain]` on the `query` line, plus body
  prose describing the wizard trigger, `--yes`, `--force`, `--plain`; cross-
  checked against `src/cli.rs`'s actual `plain: bool`/`yes: bool`/`force:
  bool` fields (all present via `rg`). Pass.
- `G8`/`G9` (`termimad`/`dialoguer` in `Cargo.toml`), `G10-G24` (skill
  frontmatter shape, flag-name greps, doc-sync greps): re-ran every `rg`
  command from the Verification Plan directly — all pass, matching
  gates.ts's own JSON.
- `DC1` (version-bump deferral): `Cargo.toml` still reads `version =
  "0.1.0-beta.2"` — confirmed directly, not snuck into this diff.
- Every `scope-*` gate: `ok: true` in gates.ts's own output — diff touches
  exactly prd.md's Expected Files, no more.

## G4 (`cargo test --all-targets --all-features -- --test-threads=1`): known-flaky exemption applies, not a regression

Within this single round's `gates.ts` invocation, `verify-4` (`ok: true`)
and `G4` (`ok: false`, `exit 101`) ran the **exact same command** as two
separate invocations (the Verification Plan runner and the checklist
custom-gate runner are independent processes in gates.ts) and got
**different results** — itself direct evidence of non-determinism, not a
deterministic regression in the diff.

To confirm which specific test caused `G4`'s `exit 101` and whether it is
the AGENTS.md-documented flaky pair, ran the identical command
(`cargo test --all-targets --all-features -- --test-threads=1`,
PATH-fixed) a third time, independently, redirecting full output to a log
file (not piped through anything that could truncate it):

```
test grandchild_termination_kills_both_pids ... ok
test windows_job_object ... ok
...
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
EXIT:0
```

All 21 test binaries in this independent run reported `test result: ok`
with `0 failed` (checked via `grep -c "test result: ok"` = 21, and `grep
FAILED|panicked` = no matches) — including both AGENTS.md-documented
flaky tests (`grandchild_termination_kills_both_pids`,
`windows_job_object`) passing this time. This is consistent with
`gates.ts`'s own `verify-4`/`G4` split result: the same command
non-deterministically passes or fails depending on timing, exactly the
deadline-race behavior `AGENTS.md`'s working rule documents for these two
specific tests.

Confirmed the file-level precondition for the exemption still holds this
round:

```
$ git diff --stat -- tests/process_supervisor.rs src/process.rs
(no output)
$ git status --porcelain -- tests/process_supervisor.rs src/process.rs
(no output)
```

Neither `tests/process_supervisor.rs` nor `src/process.rs` has any change
in this task's diff or working tree — the exemption's precondition ("a
failure there only counts as a regression if `src/process.rs` or that test
file actually changed") is met. **Not attributable to this round's diff.**

## Manual re-verification of the manual gates (not just carried forward)

Re-read `evidence/manual-tty-verification.md` in full this round (not just
citing its existence): it records a verifier identity (Madao, matching
`gitStatus`'s git user), a date, the exact binary path, and explicitly
notes it is a real interactive terminal, not `assert_cmd`. All four G27
rendering scenarios and all four G28 wizard/overwrite scenarios have
explicit "Confirmed" statements mapped to specific rows of design.md
§1.2/§2.2's matrices, including one verbatim terminal transcript
(scenario 2, the declined-overwrite `CONFIG_EXISTS`/exit-2 case). Meets
`checklist.md`'s "transcript or a recorded confirmation... never
auto-passed" bar. No new manual verification was required this round since
nothing in Round 2→3's diff touched the rendering or wizard code paths
(only `prd.md`'s evidence-citation text changed) — re-confirmed by reading
the evidence file's content directly rather than assuming it still applies.

## Deferral / scope / dependency gates

Unchanged from Round 2, re-verified directly: `DC1` passes (`Cargo.toml`
still `0.1.0-beta.2`, approval recorded in prd.md's Decisions D6, cost
estimate present); every `scope-*` gate passes; `G8`/`G9` pass.

## Convergence-word / injection / baseline-poisoning check

No convergence-word (deferred/architectural/not-exploitable/done/
out-of-scope) claim found in the diff, `prd.md`, `design.md`, or
`checklist.md` without an accompanying `file:line` + rationale + cost
estimate — the one explicit deferral (D6, version-bump) already carries
both, and its proving check (`DC1`) passed. No content in the reviewed
files addresses the evaluator directly or attempts to instruct a pass. No
screenshot-baseline (`*-snapshots/*.png` or similar) changes in this diff.

## Conclusion

**Verdict: Block** — driven mechanically by `gates.ts`'s own `G4` result
(`ok: false`), per this workflow's hard-constraint rule that any failing
gate blocks regardless of root-cause analysis or scores.

On inspection, this is the single remaining failure and it traces to
environment/flakiness, not the implementation:

- `verify-4` and `G4` are the identical command, run twice in the same
  `gates.ts` invocation, with different results (pass then fail) —
  itself proof of non-determinism rather than a deterministic break.
- An independent third run of the same command, immediately after,
  passed cleanly across all 21 test binaries, including both
  AGENTS.md-documented flaky tests by name
  (`grandchild_termination_kills_both_pids`, `windows_job_object`).
- `git diff --stat`/`git status --porcelain` against `tests/process_supervisor.rs`
  and `src/process.rs` are both empty — this task's diff never touches
  either file, meeting the exemption's stated precondition exactly.

All previously-blocking `AC2`/`AC3`/`AC4` gates now pass — the Round 2
`prd.md` evidence-citation fix (separating the glued `file.rs::test_name`
tokens) resolved that false-fail exactly as diagnosed, with no further
code, test, or design change involved.

This is the basis for the user to decide whether to waive `G4` given the
above evidence (per the dispatching agent's own framing of this round);
the evaluator's own verdict per the mechanical gate rule remains Block
until that decision is made or the non-deterministic race stops
recurring.

### Score dimensions (advice-only, does not affect verdict)

| dimension | score | note |
|---|---|---|
| Rendering fidelity | 5 | `emit_query`'s routing branch (`src/cli.rs:822-850`) matches design.md §1.2's matrix exactly; non-TTY rows automated (`tests/cli_contract.rs`, `tests/output_contract.rs`), TTY rows manually confirmed (G27). |
| Init safety | 5 | `run_config_init`/`run_config_init_interactive`/`write_template_init` (`src/cli.rs:469-569`) match design.md §2.2's matrix row-for-row; neither `dialoguer` prompt type is constructed outside the TTY-gated branch (confirmed by reading branch structure, not just testing). |
| Envelope stability | 5 | `init`/`init_envelope` signatures unchanged (G17/G18); `ConfigInitEnvelope`'s five-key shape unchanged, confirmed at `tests/config_init.rs:100,124`. |
| Contract closedness | 5 | `tests/spec_drift.rs` (G7) passes; no new `ErrorCode`/check-name/warning-code introduced. |
| Skill correctness | 5 | `SKILL.md` frontmatter is exactly `name`+`description` (G10-G12); body documents `--plain`/`--yes`/`--force` accurately against the shipped `src/cli.rs` surface (G29). |
| Spec/doc fidelity | 5 | Spec and operator-guide prose edits (G19-G26) all confirmed directly; line-141 JSON envelope sentence untouched (G25); §3.2 out-of-scope bullet correctly reworded (G26). |

## Files created/modified by this evaluation

- `D:\Projects\llm-wikis\.trestle\tasks\08-08-pre-0-1-0-cli-skills-markdown-init\evidence\gates-round-3.json` (written by gates.ts)
- `D:\Projects\llm-wikis\.trestle\tasks\08-08-pre-0-1-0-cli-skills-markdown-init\evidence\round-3.md` (this file)
