# Implementation plan — pre-0.1.0 CLI refinements

All work happens on `feat/pre-0.1.0-cli-refinements`, branched from `main`.
Steps are ordered so each one lands (commits) only after its own tests pass,
giving four independent rollback points — a broken later step can be
`git revert`ed without unwinding earlier, already-verified ones. Run the
full verification set (§ Verification Plan) after every step, not only at
the end, so a regression is caught at the step that introduced it.

## Step 0 — branch and baseline

1. `git checkout -b feat/pre-0.1.0-cli-refinements`
2. Run the full verification set once before any change, to confirm the
   baseline is green (a failing baseline is a pre-existing condition, not
   something this task introduces — stop and report if this fails).

**Rollback point**: nothing committed yet; `git checkout main` abandons the
branch cleanly if Step 0 itself finds a broken baseline.

## Step 1 — error lines to stderr (Feature 3 / AC3)

Design reference: `design.md` §1.

1. `src/cli.rs`: add `eprint_error_line(err: &AppError)`.
2. `src/cli.rs`: convert the three `println!("error: ...")` sites
   (`run_config_init`, `print_list_human`, `print_doctor_human`) to call it.
3. `src/cli.rs`: restructure `emit_query` per design.md §1.3 (error branch
   calls `eprint_error_line` directly instead of delegating to
   `render_human`).
4. `src/output.rs`: remove `render_human`'s now-unreachable-from-production
   `else if let Some(err) = &envelope.error` branch; update its doc comment
   to state error rendering is `cli.rs`'s responsibility.
5. `tests/cli_contract.rs`:
   - Flip `unknown_subcommand_is_argument_invalid_not_a_panic`'s stdout/stderr
     assertions (design.md §1.4).
   - Strengthen `human_mode_never_prints_json_on_stdout_for_an_argument_failure`
     with the stdout-empty/stderr-contains-code assertions.
   - Add one new human-mode failure test per subcommand (`list`, `doctor`,
     `config init`, `query`) proving the error line is on stderr and stdout
     is empty, using existing fixtures (`nonexistent_config_path()`,
     `minimal_registry`).
6. Run the verification set. Fix forward until green.

**Rollback point**: `git add -A && git commit -m "cli: route human-mode error lines to stderr"`
(exact message per the `commit-message` skill's conventional-commits
convention at actual commit time). If verification cannot be made green,
`git checkout -- src/cli.rs src/output.rs tests/cli_contract.rs` and stop —
do not proceed to Step 2 with a broken Step 1.

## Step 2 — `config show` / `config validate` (Feature 1 / AC1)

Design reference: `design.md` §2. Depends on Step 1's `eprint_error_line`.

1. `src/config.rs`: add `Serialize` to the six derive lists enumerated in
   design.md §2.1.
2. `src/config.rs`: add `ConfigShowEnvelope`/`config_show_envelope`/
   `config_show_error_envelope` and the `ConfigValidateEnvelope` trio,
   alongside `ConfigInitEnvelope`.
3. `src/cli.rs`: extend `ConfigAction` with `Show`/`Validate`; extend the
   `dispatch` match arm; add `run_config_show`/`run_config_validate`,
   `print_config_show_human`/`print_config_validate_human`, and (if not
   already `pub(crate)`) bump `doctor.rs::agent_key`'s visibility or add a
   two-line local equivalent — design.md §2.3 leaves either acceptable.
4. `tests/config_contract.rs`: add envelope unit tests (design.md §2.4).
5. `tests/cli_contract.rs`: add the `config show`/`config validate` CLI
   section (design.md §2.4), including the no-side-effects byte-comparison
   test for `validate`'s failure path.
6. Run the verification set. Fix forward until green.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- src/config.rs src/cli.rs tests/config_contract.rs tests/cli_contract.rs`
and stop before Step 3.

## Step 3 — query spinner (Feature 2 / AC2)

Design reference: `design.md` §3. Independent of Steps 1-2's internals
(only calls the already-existing `emit_query`/`run_query_command`).

1. `Cargo.toml`: add `indicatif = "0.17"` under `[dependencies]` (default
   features).
2. `src/cli.rs`: wrap the `service.query(...)` call in `run_query_command`
   with the spinner start/`finish_and_clear` pair from design.md §3.2.
3. `tests/cli_contract.rs`: add the `stderr.is_empty()` assertion to the
   Windows-fixture human-mode success sub-test (design.md §3.3).
4. Run the verification set — including a **manual** interactive smoke
   check outside the automated suite: run `llm-wikis query --wiki <id>
   --agent <agent> -- "<question>"` directly in an interactive terminal
   against a real registered wiki and confirm the spinner is visible on
   screen and disappears before the answer prints, then run the same
   command with `| cat` (or `> file.txt`) and confirm no spinner
   characters appear in the captured output. This manual check is not
   automatable through `assert_cmd` (design.md §3.3) and is recorded as
   Hard Gate row (manual) in `checklist.md`.

**Rollback point**: commit as its own atomic change (`Cargo.lock` changes
alongside `Cargo.toml`). On failure, `git checkout -- Cargo.toml Cargo.lock
src/cli.rs tests/cli_contract.rs` and stop before Step 4.

## Step 4 — provider argv additions (Feature 4 / AC4, AC5)

Design reference: `design.md` §4. Most test-invasive step; do last.

1. `src/providers/mod.rs`: add `NON_INTERACTIVE_SYSTEM_DIRECTIVES`.
2. `src/providers/claude.rs`: add the four new argv elements to
   `build_argv`; update the R-27/R-28 doc comment to add R-34 (design.md
   §4.2).
3. `src/providers/codex.rs`: add the two new argv elements to `build_argv`.
4. `tests/claude_adapter.rs`: update `exact_argv` and
   `hook_neutralization_settings_flag_present_without_excluding_setting_sources`
   exactly as design.md §4.4 specifies. Leave every other argv test as-is
   (verify each one individually rather than assuming — re-run the full
   `claude_adapter` test binary and read every failure, not just the two
   expected ones).
5. `tests/codex_adapter.rs`: update `exact_argv`; extend `capability_exclusion`
   with the `developer_instructions=` adjacency check.
6. `docs/2026-07-28-llm-wikis-external-query-design.md`: apply every edit
   listed in design.md §4.5 (command grammar, `config show`/`validate`
   prose, stderr sentence, §10.2/§10.3 target-invocation blocks, R-27/R-28
   paragraph correction).
7. `docs/llm-wikis.md`: apply the operator-guide edits listed in design.md
   §4.5.
8. Before committing, explicitly re-verify the TOML-round-trip caveat
   (D5/research §4): confirm `NON_INTERACTIVE_SYSTEM_DIRECTIVES` contains no
   `"`, `\`, or newline character (a one-line `grep -c` on the constant's
   literal, or eyeball it — it is a single short `const`). This is Hard
   Gate G13.
9. Run the verification set. Fix forward until green.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- src/providers/mod.rs src/providers/claude.rs src/providers/codex.rs tests/claude_adapter.rs tests/codex_adapter.rs docs/2026-07-28-llm-wikis-external-query-design.md docs/llm-wikis.md`
and stop — Steps 1-3 remain intact and mergeable independently of Step 4 if
Step 4 cannot be made green in time.

## Step 5 — final full-suite pass

1. Run the verification set one more time against the fully merged branch
   (all four steps applied together), not just each step in isolation —
   catches any cross-step interaction the per-step runs missed (e.g. a
   `config show` human-mode failure test from Step 2 combined with Step 1's
   stderr change, exercised together for the first time here).
2. Confirm `cargo tree --duplicates` (informational, not a gate) does not
   show an unexpected duplicate major version pulled in by `indicatif` —
   acceptable if it does (no gate blocks on it), but worth a look given the
   "keep the addition minimal" instruction.

## Verification Plan

```
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

## Expected Files

- `Cargo.toml`
- `Cargo.lock`
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
