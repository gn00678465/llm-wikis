# Distill — 08-08-pre-0-1-0-cli-skills-markdown-init

## Routed changes

| target | change | reason |
|---|---|---|
| AGENTS.md `## Working rules` | Extended the existing gates.ts-formatting bullet with two newly confirmed hazard instances: literal `\|` inside a backticked gate command splits the checklist table row (fix: rg `\x7c` hex escape, verified in round 2/3 G12), and glued `tests/foo.rs::test_name` tokens in prd.md AC evidence trip the evidence-path checker (round 2 AC2-AC4 false-block, fixed round 3). Merged into the same bullet, not appended as a near-duplicate — same defect family. | Cross-task operational pitfall; both cost a full evaluation round each. |
| AGENTS.md `## Working rules` | New bullet: trace-audit `suspicious-trace-start` after an archive-time waive false-blocks later rounds; sanctioned remediation is `trace.ts rotate --reason`, old trace preserved under the task's evidence dir. | Round 1 was blocked by exactly this inherited state; next occurrence should cost minutes, not a round. |
| .trestle/workspace/ARCHITECTURE.md `## Key decisions` | Recorded the presentation-layer split (termimad/dialoguer live in cli.rs behind `IsTerminal` gates, `render_human` stays pure — spinner precedent extended) and the dependency rationale (dialoguer = console-rs family reuse; termimad accepted with its crossterm cost). Recorded the `skills/` cross-tool frontmatter subset decision and the marketplace rejection. | Durable structure/dependency decisions future tasks must not accidentally reverse. |
| .trestle/workspace/PRODUCT.md | Corrected stale version line (beta.1 → beta.2, commit 682ddfc); added the agent-safety UX invariant (TTY-only interactivity, `--plain`/`NO_COLOR`/`--yes`/`--force` escape hatches, piped/`--json` byte-identical). | The invariant is the product boundary every future CLI-surface task must preserve; version line was factually wrong. |

## Nothing to record

- reviewed: prd.md Findings F1-F10 / Decisions D1-D6, checklist.md Deferral
  Checks (DC1), evidence/round-1.md through round-3.md, the three research
  files, and the surprises log (broken cargo shim, flaky G4).
- why: the remaining candidates are already covered by existing permanent
  content — the chocolatey cargo shim and the flaky
  tests/process_supervisor.rs pair each already have an AGENTS.md working
  rule (tightening was not needed; round evaluators applied both rules
  as written), the spec-doc-sync obligation is enforced mechanically by
  tests/spec_drift.rs and now documented in the revised spec itself, and
  the D6 version-bump deferral is task-scoped (release chore, not durable
  knowledge). No machine-checkable rule emerged that isn't already a gate,
  so no evolve.ts proposal.
