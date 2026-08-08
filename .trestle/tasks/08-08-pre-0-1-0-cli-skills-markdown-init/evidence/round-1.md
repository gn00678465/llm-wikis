# Round 1 evaluation — 08-08-pre-0-1-0-cli-skills-markdown-init

Complex task, checklist.md present. Diff under review: `git diff HEAD`
(working tree, uncommitted) touching exactly the files listed in prd.md's
Expected Files: `Cargo.toml`, `Cargo.lock`, `README.md`,
`docs/2026-07-28-llm-wikis-external-query-design.md`, `docs/llm-wikis.md`,
`src/cli.rs`, `src/config.rs`, `src/output.rs`, `tests/cli_contract.rs`,
`tests/config_contract.rs`, `tests/output_contract.rs`, plus the new
`skills/llm-wikis-usage/**` directory.

## gates.ts invocations

Two invocations were run (`TRESTLE_CONTEXT_ID` set both times, `--round 1`):

1. First invocation used the default shell PATH — `cargo` resolves to a
   broken chocolatey shim (`AGENTS.md` working-rule, confirmed live:
   `which cargo` → `/c/ProgramData/chocolatey/bin/cargo`, invoking it
   errors `Cannot find file at '..\lib\rust-ms\tools\bin\cargo.exe'`).
   `verify-1..4`/`G1..G7` all report `exit 4294967295` (Windows ENOENT
   signature). Saved as `evidence/gates-round-1.1.json` (`rerun: false`).
2. Second invocation, `/c/Users/gn006/.cargo/bin` prepended to PATH
   (`cargo 1.97.1` resolves correctly). Ran in the background — the full
   suite takes ~12-16 min per `AGENTS.md`. Saved as
   `evidence/gates-round-1.json` (`rerun: true`, this is the authoritative
   capture — content identical to `.1` for every gate that doesn't shell
   out to `cargo`).

Final verdict from gates.ts: **block**. 47 gates total; failing:
`AC1`-`AC5` (prd.md checkbox gate), `verify-4`/`G4`
(`cargo test --all-targets --all-features`), `trace-audit`.

## AC1-AC5: prd.md Acceptance Criteria not checked (real gap, needs fixing)

gates.ts's AC parser (`gates.ts:447`) requires each `## Acceptance
Criteria` line to be `- [x] AC<n>: ... (evidence: ...)`. prd.md still has
all five as `- [ ] AC<n>: ...` with no evidence citations
(`prd.md`, `## Acceptance Criteria` section, all five lines). None of this
round's diff touches `prd.md`. This is a real, fixable gap: the
implementer needs to mark each AC done and cite evidence (e.g. file:line
of the relevant test/doc) before this can pass — not a gates.ts false-fail
and not something the evaluator can waive.

## G4 / verify-4: `cargo test --all-targets --all-features` — known-flaky exemption applies, not a regression

Ran the full suite directly (PATH-fixed, `--test-threads=1`) end to end
(770s for `process_supervisor` alone). Every test binary passed except
`tests/process_supervisor.rs`:

```
test grandchild_termination_kills_both_pids ... FAILED
test windows_job_object ... FAILED
thread 'grandchild_termination_kills_both_pids' (26024) panicked at tests\process_supervisor.rs:339:5:
thread 'windows_job_object' (33108) panicked at tests\process_supervisor.rs:402:5:
test result: FAILED. 24 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 770.01s
```

These are exactly the two deadline-race tests and line numbers
(339/402) `AGENTS.md`'s working rule documents as confirmed-flaky in
sandboxed environments on a clean `main` baseline, with the explicit
carve-out: "Treat a failure there as a regression only if `src/process.rs`
or that test file actually changed." Confirmed neither changed this round:
`git diff --stat HEAD -- tests/process_supervisor.rs src/process.rs` is
empty. Every other test binary in the same run was green, including the
three individually-gated ones (`G5 config_init`, `G6 cli_contract`,
`G7 spec_drift`, all `ok: true` in the corrected gates.ts run) and
`config_contract`/`output_contract` (the two other touched test files).
**Not attributable to this round's diff** — flagged per the file's own
documented exemption, not silently absorbed.

## trace-audit: real violation, not attributable to this diff

`trace-audit` failed: `suspicious-trace-start` — the trace file's first
event (`archive-verdict-waived` at `2026-08-07T13:31:24.187Z`,
`.trestle/.runtime/trace.jsonl:1`) is neither a preamble-bearing
session-start nor a trace-rotated event, while task lifecycle events are
present afterward. Read the referenced event directly: it documents that
the *previous* session, evaluating a different task
(`08-07-first-run-config-and-query-ux-fixes`), hit this exact same
`trace-audit` mechanism (a *different* violation pair,
`archive-verdict-not-gated`/`plan-drifted-without-rollback`, both about
task `08-06`, per that round's own evidence,
`.trestle/tasks/archive/2026-08/08-07-first-run-config-and-query-ux-fixes/evidence/round-2.md:157-199`)
and was resolved by a user-authorized trace rotation
(`trace.jsonl.pre-0.1.0.bak`) — but that rotation's own first written
event was a bare `archive-verdict-waived` record, not the
"trace-rotated"/"preamble-bearing session-start" shape `trace.ts`'s
integrity check requires a file to open with. This is a process-tooling
artifact from a prior task's remediation, not anything in this round's
`src/`/`tests/`/`skills/`/`docs/` diff — this task never writes to
`.trestle/.runtime/trace.jsonl` directly, and no file this round touches
is implicated in either violation. Per `gates.ts`'s own header comment,
this folding-in is deliberately non-waivable from inside a single
evaluation round (no escape hatch). Flagged as a process-level blocker for
the parent session to resolve (a legitimate `trace-rotated`/session-start
preamble event, or further investigation), not a code defect in this
round's diff.

## checklist.md gate G12: silently dropped by gates.ts (planning-format defect, not a diff defect)

`G12`'s row (`| G12 | \`rg --files-without-match "^(license|compatibility|...)"...\` | gate |`)
never appears in either gates.ts run's `gates` array at all (not
`ok: true`, not `ok: false` — absent). The command itself contains
literal `|` characters (the regex alternation) inside backticks, which
breaks a markdown table's `|`-delimited column parsing — the same family
of authoring hazard `AGENTS.md`'s working rule already documents for `rg
-F` patterns starting with `-`. Ran G12's command directly to confirm what
it *would* have reported:

```
$ rg --files-without-match "^(license|compatibility|metadata|allowed-tools|when_to_use|argument-hint|disable-model-invocation|user-invocable|context):" skills/llm-wikis-usage/SKILL.md
skills/llm-wikis-usage/SKILL.md
exit: 0
```

Passes — `SKILL.md`'s frontmatter contains only `name`/`description`, no
forbidden key. This is a `checklist.md` authoring defect (a `|`-containing
regex inside a markdown table cell), not an implementation defect;
verified manually in place of the dropped mechanical gate.

## Manual review of the checklist's other gates

- **G25** (line-141 JSON-envelope sentence byte-identical): confirmed —
  `docs/2026-07-28-llm-wikis-external-query-design.md:141` reads exactly
  `{ "schema_version": "1.0", "ok": true, "operation": "config_init",
  "path": "<absolute config path>", "created": true }`, same five keys as
  before, no new field for the wizard/overwrite behavior. Pass.
- **G26** (§3.2 no longer lists "an interactive configuration wizard" as a
  bare bullet): confirmed —
  `docs/2026-07-28-llm-wikis-external-query-design.md:97` now reads "an
  interactive wizard for registering wikis (`config add-wiki`) —
  `config init`'s own short wizard covers only `default_agent` and
  provider executables, never wiki registration", the narrower reworded
  boundary design.md §4 calls for, not a bare deletion and not left
  contradicting the shipped code. Pass.
- **G29** (SKILL.md command-grammar matches the shipped CLI surface):
  diffed `skills/llm-wikis-usage/SKILL.md`'s command block against
  `src/cli.rs`'s final `Cli`/`CliCommand`/`ConfigAction` definitions —
  `--plain` (on `Query`), `--yes`/`--force` (on `ConfigAction::Init`) are
  all present and correctly scoped in both. Pass.
- **G27/G28** (TTY-only manual wizard/rendering scenarios): **no
  transcript or recorded confirmation was found** anywhere in the task
  directory (`implement.jsonl` has only context-file entries, no manual
  verification log; no transcript file exists under the task directory).
  checklist.md is explicit these are "never auto-passed" and require a
  transcript or recorded confirmation. Per this task's own dispatch note,
  the implementer has stated this sandboxed environment cannot drive a
  real TTY. Recorded here as **manual-pending** — neither fabricated as
  passed nor treated as a code defect, since the diff correctly gates
  every `dialoguer`/TTY-render code path behind `IsTerminal` checks that
  are unreachable from this sandbox by construction (confirmed by reading
  `run_config_init`/`emit_query`'s branch structure in `src/cli.rs`, and
  corroborated by `tests/cli_contract.rs`'s explicit non-TTY-only test
  additions). A human with a real terminal must still run these before
  the task can be considered fully done for AC2-AC4's TTY-triggered
  clauses specifically.

## Implementation-quality spot checks (informational, not gate-deciding)

Read every touched source file's diff line-by-line against design.md:

- `src/cli.rs`: `emit_query`'s new rendering-routing branch and
  `run_config_init`/`run_config_init_interactive`/`run_config_init_wizard`
  match design.md §1.2/§2.2's decision matrices exactly; `dialoguer`
  `Select`/`Input`/`Confirm` are constructed only inside the
  TTY-gated branches, never unconditionally (confirmed by reading, not
  just testing, per implement.md Step 3.3's own instruction).
- `src/config.rs`: `init`/`init_envelope` signatures are byte-for-byte
  unchanged (`tests/config_init.rs` still imports and calls both directly,
  full suite green); `init_with_content`/`render_init_template` are
  additive; `render_init_template(Some(Agent::Claude), "claude", "codex")`
  is pinned byte-identical to `INIT_TEMPLATE` by a dedicated test
  (`tests/config_contract.rs::render_init_template_default_arguments_match_init_template_byte_for_byte`).
- `src/output.rs`: `render_markdown_ansi` is a pure `&str -> String`
  function (no I/O/TTY/env reads), matching design.md §1.3's contract;
  `render_human` itself is untouched.
- Skill content (`skills/llm-wikis-usage/SKILL.md`,
  `references/errors.md`) documents the actually-shipped surface
  (`--plain`/`--yes`/`--force`), written after Steps 2-3 per
  `implement.md`'s ordering rationale.
- Spec-doc edits (`docs/2026-07-28-llm-wikis-external-query-design.md`,
  `docs/llm-wikis.md`, `README.md`) cover every point in design.md §4:
  §3.1 In Scope, §3.2 Out of Scope, §5.1 command grammar/init
  paragraph/human-output sentence, `docs/llm-wikis.md` §2.3/§3.1/§3.2/§3.3,
  and `README.md`'s config-init description, command reference, and new
  skills-install section.
- `tests/spec_drift.rs` (`G7`) passed — none of the three features added
  an `ErrorCode` variant, doctor check name, or wrapper warning code, so
  the mechanically-checked tables stayed untouched, matching design.md §0.

No convergence-word (deferred/architectural/not-exploitable/done/
out-of-scope) claims found in the diff or in checklist.md/design.md
without an accompanying file:line + rationale — the one explicit
deferral (D6, version bump) already carries its cost estimate and
recorded user approval in prd.md, and its proving check (`DC1`) passed.
No anti-injection content found in the reviewed files.

## Conclusion

**Verdict: Block.** Driven primarily by `AC1`-`AC5` (prd.md's Acceptance
Criteria section was never marked done with evidence — a real,
implementer-actionable gap, not a tooling artifact) and by `trace-audit`
(a real but pre-existing, non-diff-attributable process-integrity finding
inherited from a prior task's remediation, non-waivable from inside this
evaluation per gates.ts's own design). The `G4`/`verify-4` cargo-test
failure is the two AGENTS.md-documented flaky `process_supervisor.rs`
tests only, confirmed by line number and by `src/process.rs`/that test
file being untouched — not a regression. `G12` was silently dropped by a
checklist.md table-formatting defect (verified manually instead: passes).
Every other custom/scope/deferral gate passed, and a line-by-line diff
review found the implementation faithfully matches design.md/implement.md
for every feature. `G27`/`G28` (TTY-only manual scenarios) remain
unverified pending a real-terminal transcript — required before the task
can be considered fully done on AC2-AC4's TTY-triggered clauses.

### Action items for the next round
1. Mark prd.md's AC1-AC5 checkboxes `[x]` with `(evidence: file:line)`
   citations.
2. Fix `checklist.md`'s G12 row so its `|`-containing regex doesn't break
   the table parser (e.g. escape/rewrite the pattern, or move it out of
   the table cell).
3. Get the parent session to resolve the `trace-audit` process-integrity
   finding out-of-band (a legitimate rotation/session-start event, per the
   same remedy 08-07 round 2 already flagged for a different violation
   pair).
4. Produce (or explicitly get user sign-off waiving) the G27/G28 real-TTY
   manual verification transcripts.
