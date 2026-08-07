# pre-0.1.0 CLI refinements

Four refinements requested before the v0.1.0 stable release:

1. New subcommands: `config show` / `config validate`; environment
   verification via `doctor`.
2. UX: a query-in-progress spinner on interactive terminals only; answer
   stays on stdout, progress/errors on stderr so pipes/redirects/CI stay
   clean.
3. Via `--append-system-prompt`(-equivalent), instruct Claude/Codex to:
   answer directly and stop; never ask about saving; never update
   wiki/index/frontmatter/log; never ask follow-up questions a
   non-interactive CLI cannot answer.
4. Tool-layer read-only enforcement: allow only Read/Glob/Grep/Skill;
   forbid Bash/Edit/Write/Agent/interactive prompts and all MCP tools.
   Load only Claude's `project` setting source, excluding user/local
   plugins and hooks — fixes "SessionEnd hook ... Hook cancelled"
   polluting query results.

## Findings

- `doctor` subcommand already exists with static checks and `--live`
  (src/cli.rs:65-72, src/cli.rs:321-365, src/doctor.rs). Item 1's doctor
  ask may already be satisfied — confirmed scope pending.
- `config init` exists (src/cli.rs:92-95, 242-283); `config show` and
  `config validate` do not exist.
- Claude argv already enforces a read-only tool set `--tools Read,Grep,Glob`
  (src/providers/claude.rs:21, build_argv at :91-123) — `Skill` is NOT in
  the list. Also already present: `--permission-mode dontAsk`,
  `--strict-mcp-config --mcp-config <empty>` (src/providers/claude.rs:128),
  `--settings {"disableAllHooks":true}` (src/providers/claude.rs:55).
- History: `--setting-sources user` was tried (R-27) and reverted (R-28)
  because it excluded the `project` source that project-skill discovery
  depends on (src/providers/claude.rs:69-73). `--setting-sources project`
  (the current ask) is different and keeps project skills working.
- Codex argv already read-only: `--sandbox read-only`, `--ephemeral`,
  `--ignore-user-config`, `-c mcp_servers={}`, `--disable browser_use/
  computer_use` (src/providers/codex.rs:38-61). No system-prompt-append
  flag currently used.
- No spinner exists; human-readable errors currently print to stdout via
  `println!` (src/cli.rs:279-281, 308-310, 397-400). `--json` contract:
  exactly one JSON document on stdout even on failure (src/cli.rs:6-8).
- tests/spec_drift.rs guards code↔spec consistency — argv/contract changes
  must update docs/2026-07-28-llm-wikis-external-query-design.md and
  related spec text.
- Existing contract tests likely to be touched: tests/cli_contract.rs,
  tests/output_contract.rs, tests/claude_adapter.rs, tests/codex_adapter.rs,
  tests/prompt_envelope.rs.
- `Config` (src/config.rs:371-383) derives only `Deserialize`, not
  `Serialize` — `config show`'s `--json` mode needs the latter added to
  `Config` and every nested type (`RuntimeConfig`, `ProvidersConfig`,
  `ProviderConfig`, `LoadMode`, `ProviderWikiConfig`, `WikiConfig`).
- `render_human` (src/output.rs:162-183) formats the error line into the
  same string as the answer/gaps/warnings and `cli.rs::emit_query`
  `print!()`s that whole string to stdout — the error-to-stderr move (D2)
  requires splitting this, not just swapping `println!`/`eprintln!` at the
  three call sites already found.
- `tests/cli_contract.rs::unknown_subcommand_is_argument_invalid_not_a_panic`
  (line 221) and `::json_mode_emits_exactly_one_document_even_for_a_top_level_parse_failure`
  (line 388) both assert today's stdout-only behavior (`stderr.is_empty()`,
  error text found in stdout) — the first must flip under D2, the second
  (pure `--json` mode) stays correct unchanged.
- `tests/claude_adapter.rs::exact_argv` (line 138) and
  `::hook_neutralization_settings_flag_present_without_excluding_setting_sources`
  (line 204, which currently asserts `--setting-sources` is *never* present)
  both hard-fail once D6's `--setting-sources project` is added — the
  second test's own doc comment documents the R-27/R-28 history D6
  deliberately reverses part of.
- research/provider-cli-flags.md §3's live repro: adding `Skill` to
  `--tools` produced `"permission_denials":[{"tool_name":"Skill",...}]` and
  a denied `/echotest` invocation under `--permission-mode dontAsk`;
  omitting it (current `TOOLS`) produced zero denials on the identical
  fixture — this is the direct evidence behind D4.

## Decisions

- D1: Existing `doctor` already satisfies the environment-verification ask;
  item 1 reduces to adding `config show` and `config validate` only. No
  doctor changes this task. (User, interview 2026-08-06.)
- D2: Human-readable error lines move to stderr for ALL subcommands
  (query/list/doctor/config), not just query — `println!("error: ...")`
  sites become `eprintln!` (src/cli.rs:279-281, 308-310, 397-400 and any
  peers). `--json` contract unchanged: exactly one JSON document on stdout
  even on failure. Spec + contract tests updated in step. (User, interview
  2026-08-06.)
- D3: Spinner is implemented with the `indicatif` crate (user accepted the
  added dependency tree over a hand-rolled minimal spinner). Shown only for
  `query`, only when stderr is an interactive terminal (`IsTerminal`);
  cleared before the answer is printed. (User, interview 2026-08-06.)
- D4: `Skill` is NOT added to Claude's `--tools` — research §3 showed it
  reproducibly regresses the `/wiki-query` project-skill entrypoint under
  `--permission-mode dontAsk` (Skill tool call denied, no fallback to text
  expansion), while the current `TOOLS = "Read,Grep,Glob"` works with zero
  denials. Item 4's other parts proceed. (User, interview 2026-08-06,
  overriding the original item-4 wording.)
- D5: System-prompt directives delivered via `--append-system-prompt` for
  Claude (research §1, live-verified in `-p` mode) and
  `-c developer_instructions='<text>'` for Codex (research §4,
  live-verified on codex-cli 0.146.0; text must round-trip TOML parsing).
- D6: Add `--setting-sources project` to Claude argv, keeping `--settings
  {"disableAllHooks":true}` — complementary, live-verified together with
  successful project-skill expansion (research §2). This is NOT the
  reverted R-27/R-28 `user` value; spec text must document the
  admin-managed-settings exception remains.
- D7: The new config-display subcommand is named `config list`, not
  `config show` — user rename request at the round-1 commit gate
  (2026-08-07), before any commit landed. Applies everywhere the round-1
  implementation used `show`: CLI grammar, envelope `operation` string,
  tests, both spec docs. `config validate` name unchanged.
- D8: Add a wiki-authoring note to the operator guide: under
  `--permission-mode dontAsk`, a Claude wiki skill's tool use during the
  `/wiki-query` turn is governed by SKILL.md `allowed-tools` frontmatter —
  wikis must declare `allowed-tools: Read, Grep, Glob` or skill-turn reads
  are denied. Live-verified 2026-08-07 (evidence/live-verification-
  2026-08-07.md finding 3); confirmed NOT a D6 regression (operator user
  settings carry zero allow rules). Docs-only, round 3. (User, 2026-08-07.)

## Research findings (research/provider-cli-flags.md, verified on claude 2.1.223 / codex-cli 0.146.0)

- `--append-system-prompt` exists, works in `-p` mode, live-verified to
  suppress follow-up questions (§1).
- `--setting-sources project` is the exact spelling; excludes user/local
  sources (where user-level `enabledPlugins`/hooks live — the SessionEnd
  "Hook cancelled" pollution source) while keeping the `project` source
  skill discovery needs; live-verified compatible with
  `--settings {"disableAllHooks":true}` (§2). Does NOT reach
  admin-managed/enterprise-policy settings (documented exception).
- `Skill` is a valid built-in tool name, and a skill's `allowed-tools`
  frontmatter can NOT transitively unlock tools absent from `--tools`
  (empirically verified) — BUT adding `Skill` to `--tools` reproducibly
  BROKE the `/name` project-skill entrypoint under `--permission-mode
  dontAsk` (model calls the Skill tool instead of relying on CLI-side text
  expansion; the call is denied; no fallback). Omitting `Skill` (current
  production TOOLS) works with zero denials (§3).
- Codex: no dedicated flag; `-c developer_instructions='<text>'` is the
  additive equivalent, live-verified working under `codex exec` (§4).
  Caveat: `-c` values are TOML-parsed — instruction text must round-trip.
- Both CLIs already deny/lack interactive-question paths (`dontAsk` denies
  AskUserQuestion unconditionally; codex exec has no such tool) — the
  system-prompt addition reinforces existing behavior (§5).

## Acceptance Criteria

- [ ] AC1: `config list` and `config validate` subcommands exist. `list`
  loads and prints the resolved configuration; `validate` loads the config
  and reports ok/errors without side effects. Both honor `--config` and
  `--json` (one JSON document on stdout, matching the existing envelope
  conventions) and map errors to the existing error-code/exit-code scheme.
  `doctor` is unchanged.
- [ ] AC2: `llm-wikis query` shows an indicatif spinner while the provider
  call runs, only when stderr is an interactive terminal; the spinner is
  written to stderr and cleared before output. stdout carries only the
  answer (human) or the single JSON document (`--json`). Piped/redirected/
  CI runs produce byte-identical stdout to today (minus item-3 prompt
  effects) and no spinner bytes anywhere.
- [ ] AC3: Human-readable error lines for ALL subcommands print to stderr
  (`eprintln!`); `--json` failure envelopes stay on stdout. Spec and
  contract tests updated accordingly.
- [ ] AC4: Claude argv includes `--append-system-prompt "<directives>"`;
  Codex argv includes `-c developer_instructions=<directives>` with text
  that round-trips TOML parsing. Directives: answer directly and stop; no
  save/persist questions; no updates to wiki/index/frontmatter/log; no
  follow-up questions unanswerable in a non-interactive CLI.
- [ ] AC5: Claude argv includes `--setting-sources project` alongside the
  existing `--settings {"disableAllHooks":true}`. `TOOLS` stays
  `Read,Grep,Glob` (no `Skill`).
- [ ] AC6: `cargo fmt --check`, `cargo clippy --all-targets --all-features
  -- -D warnings`, and `cargo test --all-targets --all-features --
  --test-threads=1` all pass, including updated spec_drift and contract
  tests; the design spec is updated wherever argv/output contracts changed.

## Verification Plan

```
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

## Expected Files

- `Cargo.toml`
- `src/cli.rs`
- `src/config.rs`
- `src/output.rs`
- `src/providers/mod.rs`
- `src/providers/claude.rs`
- `src/providers/codex.rs`
- `tests/cli_contract.rs`
- `tests/config_contract.rs`
- `tests/claude_adapter.rs`
- `tests/codex_adapter.rs`
- `docs/2026-07-28-llm-wikis-external-query-design.md`
- `docs/llm-wikis.md`
- `config.example.toml`
