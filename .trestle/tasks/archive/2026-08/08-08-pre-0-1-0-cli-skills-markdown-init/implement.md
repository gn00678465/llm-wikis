# Implementation plan — pre-0.1.0 CLI 強化：skills 目錄、markdown 終端渲染、init 防覆寫與互動設定

Branch: `feat/pre-0.1.0-cli-refinements` (already active per gitStatus). Order:
dependencies/flag skeleton → rendering → init overwrite+wizard → skills
content → spec docs → full regression. Rendering and init are independent of
each other but both depend on Step 1's flag skeleton; skills content (Step 4)
depends on Steps 2-3 landing first so it documents the shipped surface, not
the pre-task one. Run the full Verification Plan after every step, not only
at the end.

## Step 0 — baseline

1. Run the Verification Plan once before any change to confirm the branch is
   green already. A failing baseline is a pre-existing condition — stop and
   report if this fails, do not attribute it to this task. (Known exception,
   not a stop condition: the two `tests/process_supervisor.rs` deadline-race
   tests, `grandchild_termination_kills_both_pids`/`windows_job_object`, are
   documented flaky in sandboxed environments per `AGENTS.md` — a failure
   there only counts as a regression if `src/process.rs` or that test file
   changed, and neither does in this task.)

**Rollback point**: nothing committed yet.

## Step 1 — dependencies and flag skeleton

Design reference: `design.md` §1.1, §1.4, §2.1, §2.5.

1. `Cargo.toml`: add `termimad = "0.35"` (default features) and
   `dialoguer = { version = "0.12", default-features = false }` under
   `[dependencies]`.
2. `cargo build` immediately after the `Cargo.toml` edit, before writing
   any calling code — confirms both crates resolve and that `dialoguer`'s
   `Confirm`/`Select`/`Input` types are usable with `default-features =
   false` (design.md §2.5's fallback: if not, set `default-features =
   true` and move on, this saving is not an AC).
3. `src/cli.rs`: add `#[arg(long)] plain: bool` to the `Query` variant
   (`src/cli.rs:76-93`); change `ConfigAction::Init` from a bare unit
   variant to `Init { #[arg(long)] yes: bool, #[arg(long)] force: bool }`
   (`src/cli.rs:96-114`) — wire the two new fields through `dispatch`'s
   existing match arm into `run_config_init`'s signature, and thread
   `plain` from the `Query` arm into `run_query_command`, without changing
   any behavior yet (flags parsed and passed through, not yet consumed).
4. Run the Verification Plan. `cargo build`/`cargo test` should stay green
   — this step is additive-only (new unused-until-now parameters), no
   existing test's assertions should need to change.

**Rollback point**: `git add -A && git commit` (message per the
`commit-message` skill's conventional-commits convention). On failure,
`git checkout -- Cargo.toml Cargo.lock src/cli.rs` and stop before Step 2.

## Step 2 — markdown rendering (Feature 2 / AC2)

Design reference: `design.md` §1.

1. `src/output.rs`: add `render_markdown_ansi(markdown: &str, width:
   Option<usize>) -> String` using `termimad::MadSkin::default()`. Pure
   function, no I/O.
2. `src/cli.rs::emit_query` (`src/cli.rs:654-672`): replace the third
   branch (`print!("{}", render_human(&envelope))`) with the routing logic
   from design.md §1.2 — `--plain` forces raw; else `stdout.is_terminal()
   && env::var_os("NO_COLOR").is_none()` selects the rendered path,
   otherwise raw. `emit_query`'s signature needs the `plain: bool` value
   threaded in from Step 1's wiring (and it must read `stdout.is_terminal()`
   and `NO_COLOR` itself — no new crate for either, per `design.md` §0/§1.2
   and the existing `IsTerminal` precedent at `src/cli.rs:698,828`).
3. `tests/output_contract.rs` (or a new `#[cfg(test)]` module in
   `src/output.rs`): unit test `render_markdown_ansi` with a fixed `width`
   — assert a heading/bold construct produces ANSI escape bytes, assert
   plain text without markdown syntax still round-trips readably.
4. `tests/cli_contract.rs`: add a `--plain` regression case — piped
   `assert_cmd::Command` output with `--plain` is byte-identical to the
   same invocation without it (both hit the non-TTY branch under
   `assert_cmd`, so this guards against a future refactor putting the
   `--plain` check on the wrong side of the TTY check, not against a
   behavior difference visible today).
5. Run the Verification Plan. Fix forward until green.
6. **Manual verification** (not part of the automated gate, but required
   before calling AC2 done — see `checklist.md`'s manual Hard Gate row):
   run `llm-wikis query --wiki <id> --agent <agent> -- "<question>"` in a
   real terminal against a registered wiki; confirm ANSI rendering; repeat
   piped through `cat`, with `--plain`, and with `NO_COLOR=1` set, and
   confirm each of those three produces plain markdown.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- src/output.rs src/cli.rs tests/output_contract.rs
tests/cli_contract.rs` and stop before Step 3.

## Step 3 — `config init` overwrite guard + wizard (Feature 3 / AC3, AC4)

Design reference: `design.md` §2. Independent of Step 2's internals.

1. `src/config.rs`: add the content-parameterized init entry point and the
   template-rendering function (design.md §2.3), sharing the
   exclusive-create-vs-truncate file-write logic with the existing `init()`
   via an internal helper. **Do not change `init()`'s or
   `init_envelope()`'s existing signature or behavior** —
   `tests/config_init.rs` calls both directly and must keep passing
   unmodified.
2. Add a test asserting the new template-rendering function's
   default-argument output (`Some(Agent::Claude), "claude", "codex"`) is
   byte-identical to today's `INIT_TEMPLATE` constant (design.md §2.3) —
   write this test *before* wiring the CLI, so a template-generation bug
   is caught at the unit level first.
3. `src/cli.rs::run_config_init`: implement the branching from
   design.md §2.2's matrix — TTY check (`stdin.is_terminal() &&
   stdout.is_terminal()`), `--yes`/`--json` short-circuits, the
   `dialoguer::Confirm` for overwrite (gated: only constructed when file
   exists, TTY, and `!force`), and the `dialoguer::Select`/`Input` wizard
   (gated: only constructed when TTY, `!json`, `!yes`). Neither `dialoguer`
   type is ever instantiated on a non-TTY or `--yes`/`--json` code path —
   verify this by reading the branch structure, not just testing it,
   since an accidentally-always-constructed (but never-`.interact()`-ed)
   prompt object would still compile and might still pass tests while
   violating the "never even constructs the object" precedent
   (`src/cli.rs:820-826`).
4. `tests/config_init.rs`/`tests/cli_contract.rs`: add the non-TTY-only
   cases enumerated in design.md §2.6 — `--force` overwrite (content
   changes, exit 0), `--force`-absent-still-`CONFIG_EXISTS` regression,
   `--yes` byte-identical-to-default regression, `config validate`
   accepting the generated file. Do not attempt to simulate the TTY-gated
   wizard/confirm paths through `assert_cmd` — they are structurally
   unreachable there (design.md §2.6); write the manual script instead
   (next item).
5. Run the Verification Plan. Fix forward until green.
6. **Manual verification** (see `checklist.md`'s manual Hard Gate row): run
   `llm-wikis config init` in a real terminal against a fresh path, accept
   every default with Enter, diff the result against the byte-identical
   assertion from Step 3.2; re-run against the same path, decline the
   `Overwrite?` prompt, confirm the file is untouched and exit code is 2;
   re-run and accept, confirm the wizard runs again and the file changes;
   run `llm-wikis config init --force` against an existing file with
   stdout piped and confirm no prompt appears.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- src/config.rs src/cli.rs tests/config_init.rs
tests/cli_contract.rs` and stop before Step 4.

## Step 4 — `skills/llm-wikis-usage/` (Feature 1 / AC1)

Design reference: `design.md` §3. Deliberately after Steps 2-3 so the
skill documents the actually-shipped `--plain`/`--yes`/`--force` surface.

1. Create `skills/llm-wikis-usage/SKILL.md` with the frontmatter from
   design.md §3.2 (exactly `name`+`description`, nothing else) and the
   body content mapped in design.md §3.3, including the now-current
   command grammar (with `--plain`/`--yes`/`--force`).
2. Create `skills/llm-wikis-usage/references/errors.md` per design.md §3.4.
3. `README.md`: add the install section (manual copy to
   `~/.claude/skills/`/`.agents/skills/`, plus the `npx skills add`
   one-liner) per design.md §3.1.
4. Run the `rg`-based structural gates from `checklist.md` (frontmatter
   shape, forbidden-key absence) locally before moving on.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- skills README.md` and stop before Step 5 (or `git clean -f
skills` if the directory was never tracked yet).

## Step 5 — spec-document sync (D2, AC5)

Design reference: `design.md` §4. Prose-only edits; do last so every
sentence describes the final, already-tested behavior from Steps 1-4
rather than an intermediate state.

1. `docs/2026-07-28-llm-wikis-external-query-design.md`: apply every edit
   listed in design.md §4 — §3.1 line 72, §3.2 line 97 (remove the
   contradicted out-of-scope bullet, add the narrower still-true one),
   §5.1 command grammar (lines 127-135), §5.1 config-init paragraph
   (line 139), §5.1 human-output sentence (line 151). Leave the line-141
   JSON-envelope sentence untouched (design.md §4 is explicit: no content
   change there).
2. `docs/llm-wikis.md`: apply the operator-guide edits from design.md §4 —
   §2.3 (lines 186-198, including its own heading), §3.1 command grammar
   (lines 428-438), the `--plain` addition near lines 454-474, and the
   optional TTY-conditional-rendering note near lines 479-480.
3. `README.md`: update the `config init` one-line description (line 60)
   and the command reference block (lines 100-109) with the new flags.
4. Run `cargo test --test spec_drift` specifically to confirm the three
   mechanically-checked tables are still intact (they should be untouched
   by any of the prose edits above — a failure here means a table got
   accidentally edited, not just prose around it).
5. Run the Verification Plan. Fix forward until green.

**Rollback point**: commit as its own atomic change. On failure,
`git checkout -- docs/2026-07-28-llm-wikis-external-query-design.md
docs/llm-wikis.md README.md` and stop before Step 6 — Steps 1-4 remain
intact and independently mergeable.

## Step 6 — final full-suite pass

1. Run the Verification Plan once more against the fully merged branch (all
   five steps applied together), not just each step in isolation.
2. Re-run every manual verification script from Steps 2 and 3 back-to-back
   in the same terminal session, since they were previously verified in
   isolation against intermediate states.
3. Confirm `cargo tree --duplicates` (informational, not a gate) does not
   show an unexpected duplicate major version pulled in by `termimad`'s
   `crossterm` or `dialoguer` — acceptable if it does, no gate blocks on
   it, but worth a look given two new terminal-backend-adjacent
   dependencies landed in one task.

## Verification Plan

```
cargo build --all-targets --all-features
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
- `tests/config_init.rs`
- `tests/cli_contract.rs`
- `tests/output_contract.rs`
- `tests/config_contract.rs`
- `skills/llm-wikis-usage/**`
- `docs/2026-07-28-llm-wikis-external-query-design.md`
- `docs/llm-wikis.md`
- `README.md`
