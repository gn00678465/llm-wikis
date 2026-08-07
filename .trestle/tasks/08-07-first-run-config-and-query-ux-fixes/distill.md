# Distill — 08-07-first-run-config-and-query-ux-fixes

## Routed changes

| target | change | reason |
|---|---|---|
| `AGENTS.md` (Working rules) | Added: `///` on clap-derived items becomes user-facing `--help` text — rationale goes in `//`; the marker-leak test pins it | Root cause of user repro 4 (planning prose leaked into `config --help`); any agent writing cli.rs needs this at startup |

## Nothing to record

- ARCHITECTURE.md: reviewed: prd.md D1-D7 and the five implemented fixes.
  why: all five are guidance/message-text changes with no structural,
  layer, or dependency impact; the "guidance, not semantics" principle is
  task-scoped (D1) rather than a standing architectural rule, and the
  operator-facing knowledge (TOML quoting, doctor --live remedy) already
  lives in its permanent homes (INIT_TEMPLATE comment, error messages,
  docs/llm-wikis.md).
- PRODUCT.md: reviewed: phase-goal section vs this task's outcome. why:
  the task advances the existing "fix config issues before v0.1.0" goal
  without changing scope or boundaries; no product norm moved.
- evolve proposal: reviewed: evidence/gates-round-1.json false-fail
  entries. why: both false-fails are gates.ts/checklist-phrasing quirks
  already covered by the existing AGENTS.md checklist-authoring rule from
  the previous task; no new machine-checkable rule generalizes.
