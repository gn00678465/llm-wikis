# Checklist — pre-0.1.0 CLI 強化：skills 目錄、markdown 終端渲染、init 防覆寫與互動設定

## Hard Gates

| id | check | type |
|---|---|---|
| G1 | `cargo build --all-targets --all-features` | gate |
| G2 | `cargo fmt --all --check` | gate |
| G3 | `cargo clippy --all-targets --all-features -- -D warnings` | gate |
| G4 | `cargo test --all-targets --all-features -- --test-threads=1` | gate |
| G5 | `cargo test --test config_init -- --test-threads=1` | gate |
| G6 | `cargo test --test cli_contract -- --test-threads=1` | gate |
| G7 | `cargo test --test spec_drift -- --test-threads=1` | gate |
| G8 | `rg -F "termimad" Cargo.toml` | gate |
| G9 | `rg -F "dialoguer" Cargo.toml` | gate |
| G10 | `rg -n "^name:" skills/llm-wikis-usage/SKILL.md` | gate |
| G11 | `rg -n "^description:" skills/llm-wikis-usage/SKILL.md` | gate |
| G12 | `rg --files-without-match "^(license\x7ccompatibility\x7cmetadata\x7callowed-tools\x7cwhen_to_use\x7cargument-hint\x7cdisable-model-invocation\x7cuser-invocable\x7ccontext):" skills/llm-wikis-usage/SKILL.md` | gate |
| G13 | `rg -l "" skills/llm-wikis-usage/references/errors.md` | gate |
| G14 | `rg -F "plain: bool" src/cli.rs` | gate |
| G15 | `rg -F "yes: bool" src/cli.rs` | gate |
| G16 | `rg -F "force: bool" src/cli.rs` | gate |
| G17 | `rg -F "pub fn init(path: &Path) -> Result<ConfigInitOutcome, AppError>" src/config.rs` | gate |
| G18 | `rg -F "pub fn init_envelope(path: &Path) -> ConfigInitEnvelope" src/config.rs` | gate |
| G19 | `rg -n -- "--force" docs/2026-07-28-llm-wikis-external-query-design.md` | gate |
| G20 | `rg -n -- "--yes" docs/2026-07-28-llm-wikis-external-query-design.md` | gate |
| G21 | `rg -n -- "--plain" docs/2026-07-28-llm-wikis-external-query-design.md` | gate |
| G22 | `rg -n -- "--force" docs/llm-wikis.md` | gate |
| G23 | `rg -n -- "--plain" docs/llm-wikis.md` | gate |
| G24 | `rg -n "llm-wikis-usage" README.md` | gate |
| G25 | The line-141 JSON-envelope sentence in `docs/2026-07-28-llm-wikis-external-query-design.md` (`{ "schema_version": "1.0", "ok": true, "operation": "config_init", "path": "<absolute config path>", "created": true }`) is byte-identical to today's wording — no new field added for the wizard/overwrite behavior (design.md §4, F10/D4). Evaluator reads that line directly. | gate |
| G26 | `docs/2026-07-28-llm-wikis-external-query-design.md`'s §3.2 Explicitly Out of Scope no longer lists "an interactive configuration wizard" verbatim as a bare bullet (it directly contradicted the shipped TTY-triggered wizard) — evaluator confirms the sentence was reworded to the narrower, still-true `config add-wiki`-focused boundary (design.md §4) rather than merely deleted or left contradicting the code. | gate |
| G27 | Manual: the four TTY-rendering scenarios from `implement.md` Step 2.6 (real-terminal rendered output, piped raw output, `--plain` raw output in a TTY, `NO_COLOR=1` raw output in a TTY) were each run once against a real registered wiki and matched the expected raw/rendered split in design.md §1.2's matrix. Evaluator asks for the transcript or a recorded confirmation of each of the four runs; never auto-passed. | gate |
| G28 | Manual: the four TTY-wizard/overwrite scenarios from `implement.md` Step 3.6 (accept-all-defaults wizard write, decline overwrite leaves file untouched at exit 2, accept overwrite re-runs the wizard, `--force` with piped stdout shows no prompt) were each run once in a real terminal and matched design.md §2.2's matrix. Evaluator asks for the transcript or a recorded confirmation of each of the four runs; never auto-passed. | gate |
| G29 | `skills/llm-wikis-usage/SKILL.md`'s command-grammar/flag content matches the CLI surface actually shipped by this task (including `--plain`, `--yes`, `--force`) rather than the pre-task surface — evaluator diffs the skill's command examples against `src/cli.rs`'s final `Cli`/`CliCommand`/`ConfigAction` definitions. | gate |

## Deferral Checks

D6 (prd.md's Decisions) defers the `0.1.0-beta.3` version/`chore(release)`
bump to a follow-up task, mirroring the `beta.2` precedent (commit
`682ddfc`). Cost estimate: trivial — a one-line `Cargo.toml` version bump
plus a `chore(release)` commit, on the order of minutes, following an
already-executed template; the deferral exists purely for release
sequencing (batching the version bump after this task's three features
land together), not because of unresolved risk or missing evidence. User
approval for this deferral is already recorded in prd.md's Decisions (D6,
"使用者 2026-08-08 選定") — no further approval is required before landing
this task, but the proving check below must still pass to confirm the
deferral was actually honored (i.e., this task's own diff did not sneak a
version bump in).

| id | check | approved-by |
|---|---|---|
| DC1 | `rg -n "^version = \"0.1.0-beta.2\"" Cargo.toml` | user (2026-08-08) |

## Scores

| dimension | what to check | source |
|---|---|---|
| Rendering fidelity | `--json`/piped/`--plain`/`NO_COLOR` all still produce byte-identical raw markdown; TTY-rendered path uses termimad and matches design.md §1.2's matrix | `tests/cli_contract.rs` (non-TTY rows), G27 (TTY rows, manual) |
| Init safety | No overwrite happens without `--force` or an accepted `Confirm`; every non-TTY/`--yes`/`--json` path stays byte-identical to pre-task behavior (agent-safety requirement) | `tests/config_init.rs`, `tests/cli_contract.rs`, G28 (TTY rows, manual) |
| Envelope stability | `config_init` JSON envelope keeps exactly its five keys (`schema_version`, `ok`, `operation`, `path`, `created`, plus optional `error`) across every new code path | `tests/config_init.rs::success_envelope_matches_the_exact_contract`, `tests/config_init.rs::failure_envelope_keeps_created_false_and_carries_the_public_error` (G5) |
| Contract closedness | No new `ErrorCode` variant, doctor check name, or wrapper warning code introduced by any of the three features | `tests/spec_drift.rs` (G7) |
| Skill correctness | `SKILL.md` frontmatter is exactly `name`+`description`; body/`references/errors.md` describe the CLI surface this task actually shipped | G10-G13, G29 |
| Spec/doc fidelity | Design spec, operator guide, and README all reflect `--plain`/`--yes`/`--force` and the new init/rendering behavior; the JSON-envelope sentence and the three `spec_drift`-checked tables stay untouched | G19-G26 |
