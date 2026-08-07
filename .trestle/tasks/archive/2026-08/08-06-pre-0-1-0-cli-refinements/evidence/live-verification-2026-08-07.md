# Live end-to-end verification — Windows, real providers (2026-08-07)

User-added checkpoint after the round-2 commits. Binary: release build of
`feat/pre-0.1.0-cli-refinements` (78638bf). Providers: claude 2.1.223
(`ANTHROPIC_MODEL=haiku` for speed, per user), codex-cli 0.146.0. Fixture:
`target/llm-wikis-verify/` (gitignored) — throwaway wiki with two pages and
a `wiki-query` project skill for both providers.

## Results (all pass)

| step | result |
|---|---|
| `config validate` | ok on stdout, stderr empty, exit 0 |
| `config list` human + `--json` | resolved registry rendered; JSON: `operation:"config_list"`, full config serialized |
| `doctor --wiki verify --agent claude` static | ok (7 checks; expected WIKI_SCHEMA_ABSENT / READ_SCOPE warnings) |
| `doctor ... --agent claude --live` | Pass incl. live_contract (valid wiki-query/v1, resolvable citations) + mutation clean |
| `doctor ... --agent codex --live` | Pass, same checks |
| `query` claude, human | grounded answer from page content ("axolotl named Quill"), stdout only, **stderr 0 bytes** — SessionEnd-hook pollution confirmed gone |
| `query` claude, `--json` | single JSON doc on stdout, `ok:true`, `knowledge_status:"grounded"`, citation `{wiki:"verify",slug:"facts"}`, stderr 0 bytes |
| `query` codex, human | grounded answer ("Helix"), stdout only, stderr 0 bytes |
| error path (`--wiki nope`, human) | stdout 0 bytes, `error: WIKI_NOT_ALLOWED (...)` on **stderr**, exit 2 |
| spinner in non-TTY | zero spinner bytes in every piped/redirected capture (answer streams byte-clean) |

## Findings along the way

1. **Temp-root disjointness guard fired correctly**: first fixture attempt
   lived under `%TEMP%`; `doctor --live` failed with the documented
   "temp root ... not canonically disjoint" error (exit 70). Guard works;
   fixture moved to `target/llm-wikis-verify/`.
2. **stdin-EOF blocking is real but by design**: with a positional question
   AND a non-terminal stdin that never closes (this harness's Bash tool),
   `resolve_question_bytes` (src/cli.rs) blocks reading stdin to EOF before
   any provider work — looked like a hang until diagnosed. `</dev/null`
   resolves it. Operator-relevant only for exotic callers whose stdin never
   EOFs; the documented stdin-fallback contract itself is intact.
3. **Wiki-authoring requirement surfaced**: with `--permission-mode
   dontAsk`, a skill-turn's tool use is governed by the SKILL.md
   `allowed-tools` frontmatter. A wiki skill WITHOUT
   `allowed-tools: Read, Grep, Glob` gets its reads denied during
   `/wiki-query` expansion (both haiku and the default model reproduced
   this; adding the frontmatter line fixed it immediately). Verified NOT a
   D6 regression: the operator's `~/.claude/settings.json` carries zero
   `permissions.allow` rules, so the pre-D6 argv had no user-level allow
   rules to lose — behavior is identical before/after this task. Candidate
   doc note for the operator guide's wiki-authoring guidance (not yet
   written — needs a scope decision).
4. Interactive-TTY spinner display was NOT exercisable in this harness (no
   TTY); its absence in non-TTY streams is what the checkpoint could and
   did verify. The indicatif TTY path remains covered by unit-level
   IsTerminal gating only.
