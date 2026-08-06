# Distill — 08-06-pre-0-1-0-cli-refinements

## Routed changes

| target | change | reason |
|---|---|---|
| `.trestle/workspace/ARCHITECTURE.md` (Key decisions) | Added: never add `Skill` to Claude `--tools` (live-verified regression of `/wiki-query` text expansion under `dontAsk`; research §3 / D4) | Load-bearing adapter constraint any future provider work must not undo |
| `.trestle/workspace/ARCHITECTURE.md` (Key decisions) | Added: directive channels are argv-level (`--append-system-prompt` / `-c developer_instructions=` with TOML round-trip caveat) and `--setting-sources project` complements `disableAllHooks` (D5/D6) | Encodes why these flags exist and why the R-27/R-28 `user` history does not apply, preventing a future "cleanup" revert |
| `.trestle/workspace/ARCHITECTURE.md` (Key decisions) | Added: CLI stream contract — stdout for answers/JSON, stderr for human errors + spinner, `--json` single-document invariant (D2/D3) | Cross-cutting output decision every future subcommand must follow |
| `AGENTS.md` (Working rules) | Added: `tests/process_supervisor.rs` deadline-race flakes (two named tests, baseline-confirmed 2026-08-07) — only a regression if `src/process.rs`/the test file changed; suite takes ~12-16 min | Prevents future rounds from burning time re-diagnosing a known environment flake |
| `AGENTS.md` (Working rules) | Added: checklist.md gate-command authoring pitfalls (`rg -F` needs `--` before leading-dash patterns; prose rows mis-executed by gates.ts) | Round-1 evaluation produced false gate blocks from exactly these two authoring mistakes |
| `AGENTS.md` (Working rules) | Added: Bash-tool `cargo` can resolve to a broken chocolatey shim on this machine | Evaluator lost a run to it; one line saves the next agent the same detour |

## Nothing to record

- PRODUCT.md: reviewed: prd.md Decisions D1-D6 and the phase-goal section
  of PRODUCT.md. why: the stream-contract UX norm is already routed to
  ARCHITECTURE.md's Key decisions (single home, avoids a near-duplicate
  across workspace files); the phase goal ("fix config issues, verify
  platforms, then v0.1.0") is unchanged by this task — it *advances* the
  goal, it doesn't alter it. No product-scope boundary moved.
- evolve proposal: reviewed: evidence/gates-round-1.json and the checklist
  G5/G6/G12-G15 rows. why: the failed-gate causes are checklist-authoring
  defects in this task's own checklist.md, not a reusable machine-checkable
  rule about the codebase; the durable half is captured as the AGENTS.md
  authoring rule above. No fixture generalizes beyond this task's files.
