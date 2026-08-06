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
  clearly non-command prose outside the command column.
- In the Bash tool environment on this machine, `cargo` can resolve to a
  broken chocolatey shim; if cargo commands fail oddly, check `which cargo`
  / correct PATH (or use PowerShell) before debugging the build itself.

## Historical archives

The pre-trestle agent conventions (`.scratch/` markdown issue tracker,
triage labels, the CONTEXT.md/ADR domain-doc scheme from
`/setup-matt-pocock-skills`) were retired on 2026-08-06 and their concerns
migrated into `.trestle/` (tasks, workspace docs). `.scratch/` remains as a
read-only historical archive of past planning artifacts — don't create new
issues there.
