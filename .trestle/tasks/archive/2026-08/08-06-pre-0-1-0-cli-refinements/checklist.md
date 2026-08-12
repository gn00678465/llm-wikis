# Checklist — pre-0.1.0 CLI refinements

## Hard Gates

| id | check | type |
|---|---|---|
| G1 | `cargo fmt --all --check` | gate |
| G2 | `cargo clippy --all-targets --all-features -- -D warnings` | gate |
| G3 | `cargo test --all-targets --all-features -- --test-threads=1` | gate |
| G4 | `rg -F "pub const TOOLS: &str = \"Read,Grep,Glob\";" src/providers/claude.rs` | gate |
| G5 | `rg -F "--append-system-prompt" src/providers/claude.rs` | gate |
| G6 | `rg -F "--setting-sources" src/providers/claude.rs` | gate |
| G7 | `rg -F "developer_instructions=" src/providers/codex.rs` | gate |
| G8 | `rg -F "pub fn config_show_envelope" src/config.rs` | gate |
| G9 | `rg -F "pub fn config_validate_envelope" src/config.rs` | gate |
| G10 | `rg -F "indicatif" Cargo.toml` | gate |
| G11 | `rg -F "fn eprint_error_line" src/cli.rs` | gate |
| G12 | Claude's `--tools` value never gained `Skill` anywhere in `src/providers/claude.rs` (D4 is a fixed constraint reaffirmed by research §3's live repro of the resulting `permission_denials` regression — not re-litigated by this task). Evaluator confirms by reading `TOOLS` and `build_argv` directly. | gate |
| G13 | `NON_INTERACTIVE_SYSTEM_DIRECTIVES` (`src/providers/mod.rs`) contains no `"`, `\`, or newline character (D5's TOML-round-trip caveat, research/provider-cli-flags.md §4). Evaluator reads the constant's literal text directly. | gate |
| G14 | `docs/2026-07-28-llm-wikis-external-query-design.md`'s §5.1 command grammar and §10.2/§10.3 target-invocation blocks were updated to include `config show`/`config validate` and the new argv flags (spec_drift.rs does not mechanically check this prose; evaluator diffs the relevant sections against `src/cli.rs`/`src/providers/claude.rs`/`src/providers/codex.rs`). | gate |
| G15 | `tests/prompt_envelope.rs` is unchanged (byte-diff against `main`) — item 3/AC4's directives ride a separate channel (`--append-system-prompt`/`developer_instructions=`) from the closed, spec-§7.1 `constraints` array this file tests. | gate |

## Deferral Checks

| id | check | approved-by |
|---|---|---|

No deferrals in this task. D4 (excluding `Skill` from `--tools`) reads like
a deferral but is not one under the tripwire definition: it has `file:line`
evidence (research/provider-cli-flags.md §3, live-verified reproducible
regression), a threat model/repro (the exact `permission_denials` transcript
quoted there), and user approval already recorded in prd.md's Decisions
(D4, "User, interview 2026-08-06, overriding the original item-4 wording")
— it is a closed decision with evidence attached at the point of decision,
not a cost/approval gap being carried forward.

## Scores

| dimension | what to check | source |
|---|---|---|
| Argv fidelity | New Claude/Codex argv elements match research/provider-cli-flags.md's live-verified flag spellings exactly (`--append-system-prompt`, `--setting-sources project`, `-c developer_instructions=`) | `tests/claude_adapter.rs::exact_argv`, `tests/codex_adapter.rs::exact_argv` |
| Stream discipline | stdout carries only the answer/JSON envelope; stderr carries only the human-mode error line and (interactively) the spinner | `tests/cli_contract.rs` stdout/stderr assertions (§1.4, §3.3 of design.md) |
| No regression on read-only enforcement | `TOOLS` unchanged, `Skill` never added, MCP/hooks exclusions untouched | G4, G12 |
| Spec/code fidelity | §5.1/§10.2/§10.3 prose mirrors the shipped argv and new subcommands | G14 |
| Contract closedness | No new `ErrorCode`, doctor check name, or wrapper warning code introduced | `tests/spec_drift.rs` (unmodified, still passing) |
