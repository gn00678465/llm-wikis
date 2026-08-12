# llm-wikis

<!-- TRESTLE:START -->
# Trestle Instructions
- .trestle/workflow.md — phases, current-step routing (read first)
- .trestle/workspace/ — PRODUCT.md, ARCHITECTURE.md, journals
- .trestle/tasks/ — active tasks (prd/design/implement/checklist)
- Task CLI: node <plugin>/scripts/task.ts <cmd>
If your platform has trestle commands (/trestle:*), use them.
Otherwise invoke the matching skill: trestle-init, trestle-brainstorm,
trestle-plan, trestle-continue, trestle-handoff, trestle-finish-work.
No hook support? Read workflow.md and follow the phase for the current
task status manually — statuses and next steps are all defined there.
<!-- TRESTLE:END -->

## Working rules

- `tests/process_supervisor.rs` has two deadline-race tests
  (`grandchild_termination_kills_both_pids`, `windows_job_object`, ~lines
  339/402) that fail intermittently in sandboxed/high-latency environments
  — confirmed flaky on a clean `main` baseline (2026-08-07). Treat a
  failure there as a regression only if `src/process.rs` or that test file
  actually changed. The full suite takes ~12-16 min because of this binary.
- When authoring trestle `checklist.md` gate commands: `rg -F` patterns
  that start with `-` need a `--` separator or ripgrep parses them as
  flags; prose-only checklist rows get mis-executed by gates.ts's literal
  command extractor — keep every gate row either a runnable command or
  clearly non-command prose outside the command column. Same family: in
  prd.md AC evidence text, never write backticked glob-like paths (e.g.
  a `tests/*.rs` shorthand) — gates.ts's evidence-path checker reads them
  as literal file references and false-blocks; spell paths out or use
  plain prose. Two more confirmed instances (task 08-08): a literal `|`
  inside a backticked gate command splits the markdown table row and
  gates.ts drops the gate — write rg alternation as `\x7c` (hex escape,
  rg-verified); and AC evidence must not glue a test name onto its file
  path (`tests/foo.rs::test_name`) — the path checker chokes on the fused
  token; keep the path ending at `.rs` and name the test separately.
  Two more (task 08-12): a prose-only gate row containing ANY backticked
  token gets that token extracted and executed as a command (a row mentioning
  `--json` ran `--json` and failed) — prose rows must carry no backticks at
  all; and a `## Deferral Checks` section, once present, must be a
  `| id | check | approved-by |` pipe table — writing "no deferrals" as prose
  is judged unparseable and blocks the round, so delete the section instead.
- Trestle trace-audit's `suspicious-trace-start` (trace.jsonl's first
  event is neither a preamble session-start nor `trace-rotated`, e.g.
  after a prior task's archive-time waive) false-blocks every later
  round; the sanctioned fix is `trace.ts rotate --reason "<why>"` — it
  starts a fresh trace with a `trace-rotated` head and preserves the old
  file under the active task's `evidence/`. (Task 08-08, round 1.)
- In the Bash tool environment on this machine, `cargo` can resolve to a
  broken chocolatey shim; if cargo commands fail oddly, check `which cargo`
  / correct PATH (or use PowerShell) before debugging the build itself.
- The Codex adapter spawns with `--ignore-user-config`, so llm-wikis'
  registry is the ONLY channel that can influence Codex behavior — any new
  setting (model, reasoning effort, future config keys) has to be turned into
  explicit invocation argv; never assume the operator can set it in their own
  Codex config. Related: an argv value containing double quotes
  (`-c model_reasoning_effort="high"`) is now proven to survive the Windows
  `.cmd` shim path verbatim
  (`tests/process_supervisor.rs`, test
  `batch_shim_preserves_quoted_config_override_argument`);
  the older metacharacter tests cover `& | ^ %VAR% !DELAYED!` but not quotes,
  which are Windows argv encoding's own delimiter.
- When the two `tests/process_supervisor.rs` deadline-race tests fail AND that
  file changed (so the rule above cannot clear them on its own), decide it by
  measurement, not inference: `git stash`, run just those two tests on the base
  commit, compare. Confirmed on 2026-08-10 that the same binary went from ~10s
  green to ~846s with both tests failing inside one session, purely from host
  process-enumeration latency.
- Two collection chores that block a round if skipped, both discovered in
  task 08-12: prd.md's `## Expected Files` is the scope gate's allowlist, so
  every file the implementation turns out to touch (test binaries especially —
  planning cannot predict them all) has to be added before running gates; and
  every `- [ ]` acceptance criterion must become `- [x]` or each one reports
  "not checked" and the verdict is block. Note the ordering trap: distill
  writes to AGENTS.md and .trestle/workspace/*, so running gates *after*
  distill (rather than before) needs those files in Expected Files too.
- Any configuration default that reaches out to the host environment must be
  switched off explicitly in test fixtures' `Config` literals rather than left
  to `Default`. Task 08-12's `[viewer].backend` defaults to spawning an
  external binary found on `PATH`; fixtures that inherited the default would
  pass on a machine with it installed and warn on one without, making the
  suite depend on what the host has installed.
- On clap-derived items in src/cli.rs, `///` doc comments BECOME the
  user-facing `--help` text. Rationale, spec citations, and task/decision
  references go in `//` comments; keep `///` to a short imperative
  one-liner. `tests/cli_contract.rs::help_output_never_leaks_internal_planning_markers`
  pins this.

## Historical archives

The pre-trestle agent conventions (`.scratch/` markdown issue tracker,
triage labels, the CONTEXT.md/ADR domain-doc scheme from
`/setup-matt-pocock-skills`) were retired on 2026-08-06 and their concerns
migrated into `.trestle/` (tasks, workspace docs). `.scratch/` remains as a
read-only historical archive of past planning artifacts — don't create new
issues there.
