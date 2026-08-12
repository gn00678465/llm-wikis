# Distill — 08-10-provider-model-effort

## Routed changes

| target | change | reason |
|---|---|---|
| AGENTS.md `## Working rules` | New bullet: the Codex adapter spawns with `--ignore-user-config`, so llm-wikis' registry is the ONLY channel that can influence Codex behavior — every new setting (model, reasoning effort, any future config key) must be turned into explicit invocation argv, never assumed settable in the operator's own Codex config. Same bullet records the newly proven fact that an argv value containing double quotes (`-c model_reasoning_effort="high"`) survives the Windows `.cmd` shim path verbatim, with the pinning test named; the older metacharacter tests cover `& \| ^ %VAR% !DELAYED!` but not quotes, which are Windows argv encoding's own delimiter. | Structural constraint a future config-key task would otherwise re-derive from scratch, plus a Windows-specific safety fact that was genuinely unproven before this task (the quote case had no coverage). |
| AGENTS.md `## Working rules` | New bullet: when the two `tests/process_supervisor.rs` deadline-race tests fail AND that file changed — so the pre-existing "only a regression if src/process.rs or that file changed" rule cannot clear them — decide by measurement: `git stash`, run just those two tests on the base commit, compare. Records the 2026-08-10 measurement (same binary, same session: ~10s green → ~846s with both failing, purely from host process-enumeration latency). | The existing rule has a real hole: any task that adds a test to that file loses its protection. This gives the next agent a decision procedure instead of an inference, and a concrete magnitude for how far host latency can swing. |

## Nothing to record

- reviewed: prd.md Findings F1-F10 and Decisions D1-D9, design.md §0-§6,
  checklist.md G1-G21, evidence/round-1.md (including the G4 environment
  exception write-up), and the independent Sonnet verification pass that
  re-checked all nine acceptance criteria and live-probed the installed
  `codex-cli 0.147.0`.
- why: the remaining candidates are already covered by permanent content or
  are task-scoped. The model/effort validation rules, applies-to matrix, and
  fingerprint-invalidation semantics belong to the product surface and were
  written into `docs/llm-wikis.md` and design spec §6.1/§15.1, where
  `tests/spec_drift.rs` and the exact-argv tests hold them; repeating them as
  agent working rules would create a second, drift-prone copy. The
  `PROVIDER_CONTRACT_VERSION` bump rule is documented on the constant itself,
  at the only place someone changing argv shape would look. The broken
  chocolatey cargo shim already has its own working rule and needed no
  tightening. No machine-checkable rule emerged that isn't already a gate, so
  no evolve.ts proposal.

## ARCHITECTURE.md / PRODUCT.md

No update: this task added two optional fields to an existing provider
declaration. No new component, no module-boundary change, no shift in product
positioning — and the argv-construction and probe-isolation boundaries it
touches were already recorded.
