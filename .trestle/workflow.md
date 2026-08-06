# Trestle Workflow

Single source of truth for phases, routing, and per-turn status breadcrumbs.
Hooks parse this file; they carry no embedded phase text of their own. If a
status has no matching tag block below, the breadcrumb hook falls back to a
visible line pointing back at this file rather than guessing.

Tag blocks: `[workflow-state:STATUS]` ... `[/workflow-state:STATUS]`, STATUS
matching `[A-Za-z0-9_-]+`. Required one-time steps carry `[required · once]`
(or `[required · once per round]`) plus `{enforced: <slug>}` — that slug must
appear in the tag block(s) reachable while the step is due.

Gate discipline: blocking questions are few and load-bearing — the
artifact-review start approval and the batched-commit plan, normally
nothing else. Everything in between reports or reminds; do not add
reflexive ok-prompts, they train rubber-stamping and weaken the real gates.

## Phase Index

### Phase 1 — Plan

1. `trestle task create "<title>"` — creates the task dir, prd.md, seed jsonl. [required · once] {enforced: task-create}
2. Research `[optional · repeatable]` — an explicit user research instruction (research a third-party SDK/API/library, study sample data, prior-art lookup, "research first") takes precedence over the vague/clear classification below: dispatch `trestle-research` first (any tool — MCP, web search, fetched docs), and persist its findings to `<task>/research/` before the plan converges.
3. Vague intent → run the `trestle-brainstorm` skill (explore, propose 2-3 directions, hand off). Clear intent → run `trestle-plan` (grill details one question at a time, write prd.md). [required · once] {enforced: prd-drafted}
4. Curate `implement.jsonl` / `check.jsonl` via `task.ts add-context` — each needs at least one real entry. [required · once] {enforced: jsonl-curated}
5. Complex tasks only: also write `design.md` + `implement.md` + `checklist.md` before starting.
6. `trestle task start <name>` — flips status to `in_progress`, gated on steps 3-5. [required · once] {enforced: task-start}

### Active Task Routing

Ad-hoc intents inside an active task route immediately, ahead of the
phase-scoped breadcrumb below — route first, then fall back to the phase
step if still needed:

- Research intent (an explicit research request, a third-party SDK/API/
  library question, prior-art lookup, "research first") → dispatch
  `trestle-research` (any tool — MCP, web search, fetched docs), findings to
  `<task>/research/`.
- Repeated debugging (the same bug resurfaces, or "we keep hitting this") →
  dispatch `trestle-retro`.

[workflow-state:no_task]
No active task for this session. Ask the user whether this turn should
create a task (step: task-create) before doing broad implementation work.
Task creation precedes research persistence: even an explicit research-first
request needs a task created first, since `trestle-research` findings
persist under `<task>/research/`. Once the task exists, an explicit
research-first request means: dispatch `trestle-research` before running
brainstorm or plan — research-first precedence survives into planning.
[/workflow-state:no_task]

[workflow-state:planning]
Task exists, not started yet. An explicit user research-first request takes
precedence over the vague/clear classification below: dispatch
`trestle-research` before running brainstorm or plan, and persist its
findings to `<task>/research/` before the plan converges. Then confirm the
plan is drafted (step: prd-drafted) and implement.jsonl/check.jsonl each
carry a real entry (step: jsonl-curated). Then ask the user, in your own
words, whether to start the task, and tell them to reply `start task` to
approve. Only that exact reply runs `trestle task start <name>` (step:
task-start). Approving individual planning questions is not start
approval.
[/workflow-state:planning]

### Phase 2 — Execute

1. `trestle task round start <name>` — opens round N (cap 3); refused while the previous round's `evidence/gates-round-<N-1>.json` is missing, so rounds cannot be counter-bumped retroactively. [required · once per round] {enforced: round-open}
2. Dispatch `trestle-implement`. [required · once per round] {enforced: implement-round}
3. Dispatch `trestle-evaluator` to run hard gates + scores — the hook denies this dispatch while no round is open or the open round already has its gates file; every `gates.ts run` must name `--round <n>`. [required · once per round] {enforced: evaluate-round}
4. Any hard gate fails → re-implement. After 3 failed evaluate rounds: halt and report to the user instead of continuing silently.
5. Evaluator claims a plan defect → rollback to the planner, only with the evidence triple (file:line + why-PRD-not-diff + cost estimate), max once per task.
6. Need more research mid-implementation → dispatch `trestle-research` (same research step as Phase 1), findings to `<task>/research/`, then resume implementation — not a plan-defect rollback, and not limited to once per task.
7. User-added checkpoints: when the user inserts a verification or review step mid-task (e.g. "have codex verify"), report its result before acting on it. A result that only prescribes a mechanical fix is applied and reported — mechanical means the fix preserves approved scope and behavior and leaves no choice about implementation; any change to behavior, interfaces, data, dependencies, policy, or approved scope is a real decision and stops for the user. Commit, push, and finish never absorb an unreported checkpoint result.
8. All hard gates pass → move to Phase 3.

### Phase 3 — Finish

1. `trestle-distill` — distill and route durable learnings across ARCHITECTURE.md / PRODUCT.md / AGENTS.md / `.trestle/spec/`, recording it in distill.md. [required · once] {enforced: distill}
2. Batched commit — present ONE plan in fixed format: numbered commit messages, each with its file list, plus an "Unrecognized dirty files" section for anything this session didn't touch; unrecognized files go into NO commit unless the user explicitly names them for inclusion. Wait for the user's one-shot confirmation — ask, in your own words, whether to commit the plan as presented: agreement executes it as presented; message-wording edits are applied and re-confirmed once before executing; rejecting the grouping means manual mode — stop, do not offer a second plan. Never amend. Never push in this step — push is not part of any phase and runs only on an explicit user instruction naming this specific push; a declared chain or a prior task's approval is not push authorization. Standing approvals do not carry across tasks: ask once per task, every task. [required · once] {enforced: batched-commit}
3. `trestle-finish-work` — after commits land, remind the user that /trestle:finish-work (archive + journal) is ready; run it when the user invokes it or asks for it. Never auto-run it as part of a chain, and do not turn the reminder into an ok-prompt. When it runs, it commits its own bookkeeping (archive move + journal/index) in one chore commit without asking — invoking finish-work is that approval; its outputs only, never task code, never push. [required · once] {enforced: finish-work}

[workflow-state:in_progress]
Executing. Each round: open it with `task.ts round start <name>`
(step: round-open) — refused until the previous round's gates file exists —
then implement (step: implement-round), then evaluate by dispatching
`trestle-evaluator` (step: evaluate-round); that dispatch is hook-denied
while no round is open or the open round already holds its
`evidence/gates-round-<n>.json`, and every `gates.ts run` names
`--round <n>`. Gate failure re-implements; after 3 rounds, halt and
report rather than continue silently. A plan-defect rollback needs the
evidence triple and is allowed once per task. Need more research mid-task →
dispatch `trestle-research`, findings to `<task>/research/`, then resume —
not a plan-defect rollback and not limited to once per task. Repeated
debugging (the same bug keeps resurfacing) → dispatch `trestle-retro`.
User-added mid-task
verification steps are checkpoints: report the result before acting on
it — a fix is mechanical only if it preserves approved scope and behavior
and leaves no implementation choice; anything touching behavior,
interfaces, data, dependencies, policy, or scope stops for the user, and
commit/push/finish never absorb an unreported result. Once every hard gate passes: distill and route learnings
(step: distill); present ONE batched-commit plan — numbered messages with
file lists plus an "Unrecognized dirty files" section; unrecognized files
enter no commit unless the user explicitly names them for inclusion —
then wait for this task's
one-shot confirmation — ask whether to commit the plan as presented:
agreement executes it as presented; message-wording edits
are applied and re-confirmed once before executing; grouping rejection
means manual mode (stop, no second plan). Standing approvals do not carry
across tasks; never amend and never push here: push runs only on an
explicit user instruction naming this specific push (step: batched-commit). After commits
land, remind the user that finish-work (archive + journal) is ready; run it
only when the user asks — never auto-run it, and the reminder is not an
ok-prompt; once invoked, finish-work makes its own bookkeeping commit
(archive move + journal/index only) without asking — invoking it is that
approval; never task code, never push (step: finish-work).
[/workflow-state:in_progress]

[workflow-state:replanning]
Plan defect flagged by a round rollback (evidence triple: file:line + why-PRD
+ cost estimate). Do not run `task.ts round start` yet — fix prd.md/design.md/
implement.md first. `task.ts round start` refuses to open a new round while
this state holds, until at least one plan file's content actually differs
from its snapshot at rollback time; once a real edit lands, the state clears
automatically on the next successful round start and normal in_progress
execution resumes.
[/workflow-state:replanning]

[workflow-state:completed]
<!-- Currently unreachable: task.ts archive is the only writer of "completed"
     and moves the task dir out from under the active-task pointer in the
     same call, so this status is never read back for an active task. Block
     kept for a future explicit in_progress -> completed breadcrumb. -->
Task archived. Nothing further required for this task.
[/workflow-state:completed]

[workflow-state:stale_session]
This session's active-task pointer references a task directory that no
longer exists at that path (the task was archived, moved, or deleted out
from under the pointer). Recover by clearing or re-binding the pointer, not
by editing the missing task: run `task.ts current` to confirm the stale
path, then either `task.ts create`/`task.ts start` the task fresh, or —if
the work is actually still in progress under a different task id— locate
and resume that one instead.
[/workflow-state:stale_session]

[workflow-state:broken_task]
This session's active-task pointer references a task directory that DOES
exist, but its `task.json` is missing or unreadable — the task's own
metadata is broken, not the pointer. Recover by inspecting/restoring
`task.json` (e.g. from git history) so the task can resume normally; if the
task is genuinely dead, remove or archive the directory manually rather than
re-binding the pointer elsewhere — a broken task's history should not be
silently abandoned by pointing somewhere else.
[/workflow-state:broken_task]
