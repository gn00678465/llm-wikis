# Round 2 evaluation — 08-08-pre-0-1-0-cli-skills-markdown-init

Diff under review: `git diff HEAD` (working tree, uncommitted), same file set as
Round 1 (`Cargo.toml`, `Cargo.lock`, `README.md`,
`docs/2026-07-28-llm-wikis-external-query-design.md`, `docs/llm-wikis.md`,
`src/cli.rs`, `src/config.rs`, `src/output.rs`, `tests/cli_contract.rs`,
`tests/config_contract.rs`, `tests/output_contract.rs`,
`skills/llm-wikis-usage/**`), plus this round's remediation: `prd.md`'s
AC1-AC5 marked `[x]` with evidence, `checklist.md` G12's regex escaped with
`\x7c`, and `.trestle/.runtime/trace.jsonl` rotated via `trace.ts rotate`.

## gates.ts invocations

Ran `gates.ts run ... --round 2` twice (`TRESTLE_CONTEXT_ID` set both times):

1. First invocation, default shell PATH — `cargo` resolves to the broken
   chocolatey shim (`AGENTS.md` working-rule, same as Round 1). `verify-1..4`
   and `G1/G4-G7` all report `exit 4294967295` (Windows ENOENT signature).
2. Second invocation, `/c/Users/gn006/.cargo/bin` prepended to PATH
   (`cargo 1.97.1`), run in the background (~20+ min: build+fmt+clippy+full
   test+three filtered test runs). This is the authoritative capture,
   `evidence/gates-round-2.json` (`rerun: true`).

Final verdict from gates.ts (authoritative, PATH-fixed run): **block**.
Failing: `AC2`, `AC3`, `AC4`, `verify-4`/`G4`. Everything else — `AC1`,
`AC5`, `verify-1..3`/`G1..G3`, `G5..G24`, every `scope-*` gate, `DC1` — is
`ok: true`. `trace-audit` does not appear in the gates array at all this
round (gates.ts only emits it when `audit()` finds violations>0) — confirms
the Round 1 `suspicious-trace-start` finding is resolved by the rotation.

## AC2/AC3/AC4: gates.ts evidence-path false positive (planning-format defect, not a diff defect)

`prd.md:86`, `prd.md:94`, `prd.md:100` (the `(evidence: ...)` lines for
AC2/AC3/AC4) each open with a compound citation of the form
`tests/cli_contract.rs::some_test_name` — a file path glued directly to
`::test_name` with **no separating space or comma**. `gates.ts`'s
`invalidEvidenceReason` (gates.ts:383-403) splits evidence text on
whitespace/commas first, then on `PATH_ANNOTATION_SPLIT_RE`
(`/[()「」『』\`;]/`) — neither split point fires on `::`, so the whole
glued string (`tests/cli_contract.rs::live_doctor_then_query_succeed_...`)
becomes one candidate. It contains `/`, its last segment contains a `.`
(from `.rs`), so it must resolve to a real file — and of course it doesn't,
since no file is literally named that. Hence "evidence references path(s)
that do not exist" for exactly one glued token per AC (the *first* citation
in each evidence line only — every subsequent `::test_name` reference in
the same line, written without a repeated file prefix, e.g. `::config_init_generated_file_passes_config_validate_regardless_of_which_path_produced_it`,
correctly has no `/` and is never flagged).

Verified directly (not just trusting the prd.md prose) that every cited test
actually exists and says what the AC evidence claims:

- `tests/cli_contract.rs::live_doctor_then_query_succeed_end_to_end_with_schema_absent_warning`
  exists (grep-confirmed) and its `--plain` byte-identity assertion sits at
  `tests/cli_contract.rs:2164-2168` as cited.
- `tests/output_contract.rs::render_markdown_ansi_produces_escape_bytes_for_a_heading_and_bold_text`
  (line 170) and `::render_markdown_ansi_round_trips_plain_text_without_markdown_syntax`
  (line 181) both exist and assert what AC2 claims (ANSI escape bytes for a
  heading/bold; plain-text round-trip).
- `tests/cli_contract.rs::config_init_yes_produces_byte_identical_output_to_the_default_template`
  (line 441) and `::config_init_generated_file_passes_config_validate_regardless_of_which_path_produced_it`
  (line 469) exist and match AC3's claim.
- `tests/cli_contract.rs::config_init_force_overwrites_an_existing_file_without_a_prompt`
  (line 389), `::config_init_without_force_still_refuses_to_overwrite_an_existing_file`
  (line 421), `tests/config_init.rs::success_envelope_matches_the_exact_contract`
  (line 100), `::failure_envelope_keeps_created_false_and_carries_the_public_error`
  (line 124) all exist and match AC4's claim.

This is the same family of authoring hazard `AGENTS.md`'s working rule
already documents for `prd.md` AC evidence text (backticked glob-like paths
false-blocking the evidence checker) — a different specific pattern
(`file.rs::test_name` glued with no separator, vs. a glob shorthand) hitting
the identical mechanism in `gates.ts:383-403`. **Not an implementation
defect** — the diff's tests genuinely exist and prove the ACs; this is a
`prd.md`-authoring format issue. Cost estimate to fix: trivial, three
one-line edits (insert a space, comma, or backtick boundary before each
leading `::` at `prd.md:86`, `prd.md:94`, `prd.md:100` — e.g.
"`tests/cli_contract.rs`, test `live_doctor_...`" or
"tests/cli_contract.rs :: live_doctor_..."), on the order of a minute; no
design.md/implement.md change and no code rework required. Per this
evaluation's own hard-constraint rules (gates.ts false-fails from
formatting are planning defects, not implementation defects), this is
flagged as such — but per this round's own workflow rule ("any hard gate
failing... means Block, regardless of scores"), the round verdict is still
**Block** until `prd.md`'s citation format is fixed and gates.ts is
re-run.

## verify-4 / G4: `cargo test --all-targets --all-features` — known-flaky exemption applies, not a regression

Ran the full suite directly (PATH-fixed, `--test-threads=1`) end to end
(~420s for `process_supervisor` binary alone this run):

```
test grandchild_termination_kills_both_pids ... FAILED
thread 'grandchild_termination_kills_both_pids' (24368) panicked at tests\process_supervisor.rs:339:5:
test result: FAILED. 25 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 419.99s
EXIT: 101
```

Exactly the AGENTS.md-documented flaky test at the documented line (339).
(`windows_job_object`, the sibling flaky test named in AGENTS.md, passed
this run — consistent with these being non-deterministic deadline races,
not a stable failure.) Confirmed neither `tests/process_supervisor.rs` nor
`src/process.rs` changed this round: `git diff --stat HEAD -- tests/process_supervisor.rs src/process.rs`
and `git status --porcelain -- tests/process_supervisor.rs src/process.rs`
are both empty. Every other test binary in the same run was green,
including the three individually-gated ones (`G5 config_init`,
`G6 cli_contract`, `G7 spec_drift`, all `ok: true` in the gates.ts run) and
`config_contract`/`output_contract`. **Not attributable to this round's
diff** — flagged per the file's own documented exemption, not silently
absorbed.

## Manual gates (G25-G29) — verified directly, not just re-cited from Round 1

- **G25** (line-141 JSON-envelope sentence byte-identical): confirmed —
  `docs/2026-07-28-llm-wikis-external-query-design.md` §5.1 still reads
  exactly `{ "schema_version": "1.0", "ok": true, "operation": "config_init",
  "path": "<absolute config path>", "created": true }`, same five keys, no
  new field. Pass.
- **G26** (§3.2 no longer lists "an interactive configuration wizard" as a
  bare bullet): confirmed — §3.2 now reads "an interactive wizard for
  registering wikis (`config add-wiki`) — `config init`'s own short wizard
  covers only `default_agent` and provider executables, never wiki
  registration", the narrower reworded boundary design.md §4 calls for.
  Pass.
- **G27/G28** (TTY-only manual scenarios): `evidence/manual-tty-verification.md`
  now exists (it did not in Round 1) with the verifier identified (task/repo
  owner), a real-terminal binary path, and per-scenario confirmations for
  all four G27 rendering scenarios and all four G28 wizard/overwrite
  scenarios, including one verbatim terminal transcript (scenario 2, the
  declined-overwrite `CONFIG_EXISTS`/exit-2 case) and explicit
  scenario-by-scenario "Confirmed" statements for the rest, each mapped to
  the specific row of design.md §1.2/§2.2's matrices. Meets checklist.md's
  "transcript or a recorded confirmation... never auto-passed" bar. Pass.
- **G29** (SKILL.md command-grammar matches the shipped CLI surface):
  `skills/llm-wikis-usage/SKILL.md` contains `[--yes] [--force]` on the
  `config init` line, `[--plain]` on the `query` line, and prose describing
  the wizard trigger, `--yes`, `--force`, and `--plain` — all present and
  correctly scoped against `src/cli.rs`'s final `ConfigAction::Init`/`Query`
  fields (`plain: bool`, `yes: bool`, `force: bool`, all confirmed present
  via `rg`). Pass.

## Deferral / scope / dependency gates

- `DC1` (version-bump deferral, D6): `Cargo.toml` still reads
  `version = "0.1.0-beta.2"` — the bump was not snuck into this diff.
  Approval already recorded in prd.md's Decisions (D6). Pass.
- Every `scope-*` gate passes — the diff touches exactly prd.md's Expected
  Files, no more.
- `G8`/`G9` (`termimad`/`dialoguer` in Cargo.toml): pass.

## Implementation-quality spot checks (informational, not gate-deciding)

Reused Round 1's line-by-line diff review (unchanged this round — no source
files changed since Round 1, only `prd.md`, `checklist.md`, and trace
housekeeping) and independently re-verified the manual-gate claims above by
reading the actual doc/skill/test files rather than only trusting Round 1's
citations. No discrepancy found.

No convergence-word (deferred/architectural/not-exploitable/done/
out-of-scope) claims found in the diff or in checklist.md/design.md without
an accompanying file:line + rationale — D6 (the one explicit deferral)
already carries its cost estimate and recorded user approval, and its
proving check (`DC1`) passed. No anti-injection content found in the
reviewed files. No screenshot-baseline changes in this diff.

## Conclusion

**Verdict: Block.** Driven by gates.ts's own `AC2`/`AC3`/`AC4` (ok: false)
and `verify-4`/`G4` (ok: false) results. On inspection:

- `verify-4`/`G4` is the single AGENTS.md-documented flaky
  `process_supervisor.rs` test (`grandchild_termination_kills_both_pids`,
  line 339) — confirmed by line number and by `src/process.rs`/that test
  file being untouched this round. Not a regression.
- `AC2`/`AC3`/`AC4` fail only because `prd.md`'s evidence citations at
  lines 86, 94, and 100 glue a file path directly to `::test_name` with no
  separator, tripping `gates.ts`'s literal-path existence checker on a
  compound token that was never meant to resolve as one file. Every cited
  test was independently verified to exist and to prove the AC it's cited
  for. This is a `prd.md`-authoring format defect (trivial, ~1-minute,
  3-line fix), not an implementation defect.

Both root causes trace to the plan/process layer, not the diff — but per
this evaluation's own hard-constraint rule, any gates.ts `ok: false` still
forces a Block verdict this round regardless. Every other gate (custom,
scope, deferral, and all five manual gates) passed on direct
re-verification.

### Action items for the next round

1. Edit `prd.md`'s AC2/AC3/AC4 evidence lines (86, 94, 100) so the leading
   `file.rs::test_name` citation isn't one glued token — insert a space,
   comma, or other separator before the first `::` in each (the pattern
   already used for every *subsequent* `::test_name` reference in the same
   lines is already safe and needs no change).
2. No code, test, or design change required — G4's failure is the
   documented flaky exemption; re-running gates.ts after item 1 should be
   sufficient to reach `pass` (assuming the same non-deterministic
   `process_supervisor.rs` race doesn't recur, which is a pre-existing,
   task-independent condition, not something this task can control).
