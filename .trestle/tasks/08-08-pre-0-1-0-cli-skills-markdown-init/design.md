# Design — pre-0.1.0 CLI 強化：skills 目錄、markdown 終端渲染、init 防覆寫與互動設定

Branch: `feat/pre-0.1.0-cli-refinements` (already checked out per gitStatus).
Three independent features touching disjoint code paths; `implement.md`
orders them dependencies → rendering → init → skills → docs → full regression
so each has its own rollback point.

## 0. Cross-cutting invariants that must survive every feature

- `tests/spec_drift.rs` mechanically parses only three things out of
  `docs/2026-07-28-llm-wikis-external-query-design.md`: the Section 14 error
  table (`ErrorCode::ALL`, 27 codes), the Section 15 `checks[].name`
  sentence, and the Section 13 wrapper warning-code sentence. None of the
  three features in this task add an `ErrorCode` variant, a doctor check
  name, or a wrapper warning code (D4's overwrite/wizard paths reuse
  `ErrorCode::ConfigExists` unchanged; F10/D4 keep the `config_init`
  envelope's JSON shape locked) — all three mechanical checks stay green
  without table edits. The *other* spec prose this task touches (§3.1/§5.1
  command grammar, the config-init paragraph, the Out-of-Scope list) is
  **not** mechanically checked — it is a manual-fidelity requirement,
  tracked as Hard Gates in `checklist.md`, not a test.
- No change in this task adds, removes, or renames a public JSON field on
  `QueryEnvelope` or `ConfigInitEnvelope` (`src/config.rs:1116-1124`) — the
  wizard and the overwrite-confirm both run entirely before any envelope is
  constructed; the envelope constructors' code paths are otherwise
  unchanged.
- `src/cli.rs`'s existing `///` vs `//` doc-comment convention (user-facing
  `--help` text vs internal rationale) applies to every new `#[arg(...)]`
  field added below — `tests/cli_contract.rs::help_output_never_leaks_internal_planning_markers`
  (`tests/cli_contract.rs:228-248`) already exercises `config init --help`
  and `query --help`, so a rationale comment accidentally written as `///`
  on the new flags fails that test immediately.

## 1. Feature 2 — markdown rendering (AC2)

### 1.1 Flag

New `#[arg(long)]` bool field on the `Query` variant of `CliCommand`
(`src/cli.rs:76-93`), named `plain`. Scoped to `query` only (per task
instructions) — not a global flag, since no other subcommand's human-mode
output is markdown.

### 1.2 Decision matrix (evaluated inside `emit_query`, `src/cli.rs:654-672`)

| `json` | `envelope.error` | `--plain` | stdout `is_terminal()` | `NO_COLOR` set | Output |
|---|---|---|---|---|---|
| true | — | — | — | — | JSON envelope on stdout (unchanged) |
| false | Some | — | — | — | `eprint_error_line` on stderr (unchanged) |
| false | None | true | — | — | raw `render_human(&envelope)` on stdout |
| false | None | false | false | — | raw `render_human(&envelope)` on stdout (unchanged — piped/redirected) |
| false | None | false | true | true (any value) | raw `render_human(&envelope)` on stdout (no-color.org: presence, not content, disables color) |
| false | None | false | true | false/unset | `render_human(&envelope)` piped through the termimad conversion, ANSI text on stdout |

The `json`/error branches are evaluated first and are byte-identical to
today — only the success/human branch (`src/cli.rs:669`,
`print!("{}", render_human(&envelope))`) gains the routing logic above.

### 1.3 Module split

- `src/output.rs` gains one new **pure** function, e.g.
  `render_markdown_ansi(markdown: &str, width: Option<usize>) -> String`,
  using `termimad::MadSkin::default()` to convert. No I/O, no env/TTY
  reads — consistent with `render_human`'s own doc comment
  (`src/output.rs:160-171`) that this module does formatting, not
  stream-routing. `width: Option<usize>` lets a unit test pass a fixed
  value for deterministic output instead of depending on the real terminal
  size; the CLI call site passes `None` (or the real detected width) to
  let termimad auto-detect at runtime.
- `src/cli.rs::emit_query` gains the routing branch described in §1.2 —
  it, not `render_human` or the new `output.rs` function, decides *whether*
  to call the conversion, mirroring how it already decides *whether* to
  call `eprint_error_line` vs `render_human` today.
- `render_human` itself (`src/output.rs:172-191`) is untouched.

### 1.4 Dependency

`Cargo.toml`: add `termimad = "0.35"` under `[dependencies]`, default
features. Research (`research/markdown-rendering.md` §3) confirms this is a
single-purpose crate already scoped to exactly this need (rich markdown →
terminal ANSI) — no feature trimming identified as beneficial, unlike
`dialoguer` below. Introduces `crossterm`/`minimad` as new transitive
dependencies; `indicatif`'s existing `console` backend is untouched and
coexists (two terminal abstractions in the tree, an accepted cost per
research §3).

### 1.5 Test strategy

- **Automatable**: `render_markdown_ansi` with a fixed `width` is a pure
  string→string function — unit test in a new or existing `output.rs`
  test module / `tests/output_contract.rs`, asserting known markdown
  constructs (a heading, `**bold**`) produce ANSI escape bytes and that
  plain text without markdown syntax round-trips recognizably. Also
  automatable: every existing `tests/cli_contract.rs` query assertion on
  stdout bytes continues to pass **unmodified** specifically because
  `assert_cmd::Command` never provides a controlling terminal — `stdout
  .is_terminal()` is deterministically `false` under every CLI-level test,
  so the new rendering branch is provably unreachable from the automated
  suite, exactly like `start_query_spinner` (`src/cli.rs:820-826`) already
  demonstrates for stderr. This is not a coverage gap to work around; it is
  the mechanism that proves AC2's "byte-for-byte identical when
  piped/redirected" clause for free — add one new explicit `--plain`
  regression test asserting its output is identical to the flag-omitted
  case under a piped `assert_cmd::Command` (both hit the same "not a
  terminal" branch, so this mainly guards against a future refactor that
  moves the TTY check to the wrong side of the `--plain` check).
- **Manual-only (CI-unreachable, no pty in this repo's test harness)**: run
  `llm-wikis query --wiki <id> --agent <agent> -- "<question>"` directly in
  a real interactive terminal against a registered wiki and confirm ANSI
  styling appears (headings/bold visibly rendered); repeat with `| cat` (or
  `> file.txt`) and confirm the captured bytes are plain markdown with no
  escape codes; repeat with `--plain` in the interactive terminal and
  confirm plain markdown despite the TTY; repeat with `NO_COLOR=1` in the
  interactive terminal and confirm plain markdown. Recorded as a Hard Gate
  (manual, prose) row in `checklist.md`.

## 2. Feature 3 — `config init` overwrite guard + wizard (AC3, AC4)

### 2.1 New flags

Two new `#[arg(long)]` bool fields on `ConfigAction::Init`
(`src/cli.rs:96-114`, currently a bare unit variant — becomes a struct
variant): `yes` and `force`. No `-i`/`--interactive` flag — D4 (final,
user-selected) supersedes the `-i`-based draft matrix in
`research/init-interactive.md` §4: the wizard triggers automatically on a
TTY instead of requiring an explicit opt-in flag.

### 2.2 Behavior matrix (authoritative; final per D4, not the research draft)

TTY condition = `std::io::stdin().is_terminal() && std::io::stdout().is_terminal()`
(both, since the wizard both prompts on the terminal and needs to read a
response — mirrors `resolve_question_bytes`'s existing `stdin`-only check
extended to also require `stdout`, since this is an *output*-driven prompt
UI, not just an input-source decision).

| `--json` | file exists | `--force` | `--yes` | TTY | Behavior |
|---|---|---|---|---|---|
| true | any | any | any | any | Unchanged: template write or `CONFIG_EXISTS`/overwrite per `--force`, never interactive |
| false | false | — | true | any | Skip wizard, write template (today's fixed defaults) — `--yes` always short-circuits |
| false | false | — | false | false (non-TTY) | Skip wizard, write template — identical to today's behavior, safe for agent callers |
| false | false | — | false | true | Run wizard (default_agent Select, two executable Text prompts), then write the customized template |
| false | true | false | any | false (non-TTY) | Unchanged: `CONFIG_EXISTS`, exit 2, no prompt |
| false | true | false | true | true | `--yes` skips the overwrite confirm too — behaves like non-TTY: `CONFIG_EXISTS` (yes only skips the *wizard*, it is not a blanket overwrite consent; `--force` is the only overwrite consent, matching D4's "帶 --force → 不問直接覆寫...為 agent 唯一覆寫路徑") |
| false | true | false | false | true | `dialoguer::Confirm` ("configuration already exists at `<path>`. Overwrite?", default `false`); Yes → proceed to wizard (since `--yes` absent) → overwrite write; No/Esc → `CONFIG_EXISTS`, exit 2, no write |
| false | true | true | true | any | Skip confirm, skip wizard, overwrite with the fixed template directly |
| false | true | true | false | false (non-TTY) | Skip confirm (can't prompt anyway), skip wizard (non-TTY), overwrite with the fixed template |
| false | true | true | false | true | Skip confirm (`--force` is explicit consent), run wizard, overwrite with the customized template |

Row 6 is the one cell not spelled out verbatim in D4's four bullets; it is
the direct consequence of the D4 sentence "`--force` 是...唯一覆寫路徑" (only
`--force` authorizes an overwrite) combined with "`--yes`：跳過精靈直接寫樣板" — `--yes`'s
documented job is skipping the *wizard*, not the overwrite guard.
Implementer should confirm this reading against the live PRD owner if it
surfaces as ambiguous during implementation; it is not a new deferral (no
cost/evidence gap — it is a direct logical composition of two already-
approved D4 clauses), so no `checklist.md` Deferral Checks row is needed for it.

### 2.3 `src/config.rs` changes

- **Keep `init(path: &Path) -> Result<ConfigInitOutcome, AppError>` and
  `init_envelope(path: &Path) -> ConfigInitEnvelope` exactly as they are
  today** (signature and behavior unchanged) — `tests/config_init.rs`
  imports and calls both directly (`use llm_wikis::config::{Config, init,
  init_envelope};`), and every non-interactive/default-template code path
  in the new CLI flow (the `--yes`/non-TTY/`--json` rows above) should keep
  calling exactly this pair, so those rows stay provably byte-identical to
  today's behavior without new code.
- Add new entry points, layered on a shared internal write helper so the
  exclusive-create-vs-truncate logic is not duplicated:
  - A content-parameterized init for the wizard path, e.g.
    `pub fn init_with_content(path: &Path, content: &str, force: bool) -> Result<ConfigInitOutcome, AppError>`
    — `force: false` behaves exactly like today's `create_new(true)`;
    `force: true` opens with `write(true).truncate(true).create(true)`
    instead, then writes `content`.
  - A template-rendering function parameterized by the three wizard
    answers, e.g. `render_init_template(default_agent: Option<Agent>, claude_executable: &str, codex_executable: &str) -> String`,
    replacing the `default_agent = "claude"` / `executable = "claude"` /
    `executable = "codex"` lines of the current `INIT_TEMPLATE` const
    (`src/config.rs:1042-1078`) with the supplied values while leaving the
    `[runtime]` block and the commented example wiki untouched (F9: wiki
    registration stays out of the wizard). The non-interactive default
    path must call this with `(Some(Agent::Claude), "claude", "codex")`
    and the result must be **byte-identical** to today's `INIT_TEMPLATE`
    constant — add a test asserting exactly that (guards against silent
    template drift when the function is introduced).
  - `run_config_init` in `src/cli.rs` becomes the place that decides which
    entry point to call, based on the matrix in §2.2 — not `config.rs`.
- No new `ErrorCode` variant. `ConfigExists` (`src/error.rs:14,78`) keeps
  its existing meaning (decline-to-overwrite, whether from a non-TTY
  invocation, a missing `--force`, or a declined `Confirm`).

### 2.4 `src/cli.rs` wiring

- `ConfigAction::Init` becomes `Init { #[arg(long)] yes: bool, #[arg(long)] force: bool }`.
  `///` doc comments on both are short imperative help text (e.g. "Skip the
  interactive setup wizard and write the default template." /
  "Overwrite an existing configuration file without prompting."); any
  rationale referencing D4/AC3/AC4 goes in `//` per the repo convention
  (§0 above).
- `run_config_init` gains the branching in §2.2. The wizard itself
  (`default_agent` `dialoguer::Select` with items `["claude", "codex",
  "none"]`; two `dialoguer::Input`/`Text`-equivalent prompts for
  `providers.claude.executable`/`providers.codex.executable`, each with
  `.default("claude".into())`/`.default("codex".into())` so Enter accepts
  it) is only ever constructed inside the TTY-gated branch — no
  `dialoguer` type is instantiated on any other code path, mirroring
  `start_query_spinner`'s "non-interactive run never even constructs an
  indicatif handle" precedent (`src/cli.rs:820-826`).
- The overwrite `Confirm` (`dialoguer::Confirm::new().with_prompt(...).default(false).interact()`
  or `.interact_opt()` to also accept Esc-as-No) similarly only constructs
  inside the TTY-gated, `--force`-absent branch.

### 2.5 Dependency

`Cargo.toml`: add `dialoguer = { version = "0.12", default-features =
false }` under `[dependencies]`. Research (`research/init-interactive.md`
§2.4) recommends `dialoguer` over `inquire` specifically because it shares
the already-present `console` backend (via `indicatif`,
`Cargo.lock:176-178`) instead of introducing a second terminal backend
(`inquire`'s default `crossterm` feature) — note this is now a *second*
terminal backend regardless, because §1.4 above independently adds
`crossterm` via `termimad`; the `dialoguer`-over-`inquire` choice still
avoids a *third* backend, which is the actual saving. Disabling default
features drops the `editor` (needs `tempfile` — already a dependency
either way) and `password` (needs `zeroize`) features, neither of which
this task's `Confirm`/`Select`/`Input` usage needs. First implementation
step must confirm with `cargo build` that `Confirm`/`Select`/`Input` remain
available with `default-features = false`; if the build fails because one
of them is gated behind a feature this research did not identify, fall
back to `default-features = true` rather than spending implementation time
reverse-engineering dialoguer's feature graph — the dependency-size
saving is a nice-to-have, not an AC.

### 2.6 Test strategy

- **Automatable**: every non-TTY row of the §2.2 matrix (`--json` any,
  `--yes`, or plain non-TTY) is fully exercisable through `assert_cmd`
  exactly as `tests/config_init.rs`/`tests/cli_contract.rs` already do —
  add cases for: `--force` overwriting an existing file (content changes,
  `created: true`, exit 0); `--force` absent still returns `CONFIG_EXISTS`
  (regression); `--yes` produces byte-identical output to the pre-task
  default (regression against `INIT_TEMPLATE`); the wizard-parameterized
  template renderer's default-argument output matches `INIT_TEMPLATE`
  byte-for-byte (§2.3); `config validate` accepts the generated file
  regardless of which path produced it (AC3's "產出的 config 通過
  `config validate`" clause). None of these require a TTY — they test the
  *non-interactive* branches, which is most of the matrix's cells.
- **Manual-only (CI-unreachable)**: the wizard prompts and the overwrite
  `Confirm` themselves need a real terminal (`dialoguer` requires an actual
  tty to interact with, and this repo's test harness has no pty). Manual
  script: run `llm-wikis config init` in a real terminal against a fresh
  path, accept every default with Enter, confirm the written file matches
  the byte-identical-default assertion from the automated test; re-run
  against the same path, confirm the `Overwrite?` prompt appears, decline
  it, confirm the file is untouched and exit code is 2; re-run a third time
  and accept, confirm the wizard runs again and the file changes; run
  `llm-wikis config init --force` against an existing file with `stdout`
  piped (`| cat`) and confirm no prompt appears (force always skips
  regardless of the pipe, since force short-circuits before the TTY check
  even matters for the confirm — though the wizard is still skipped here
  too, since a piped stdout is not a TTY). Recorded as a Hard Gate (manual,
  prose) row in `checklist.md`.

## 3. Feature 1 — `skills/llm-wikis-usage/` (AC1)

### 3.1 Layout

```
skills/
└── llm-wikis-usage/
    ├── SKILL.md
    └── references/
        └── errors.md
```

Per D5: pure directory, no `.claude-plugin/` marketplace manifest, no
install script changes. `README.md` gains an install section explaining
manual copy to `~/.claude/skills/` (Claude Code, personal scope) or
`.agents/skills/` (Codex CLI, project or user scope per
`research/skills-directory.md` §2.2) and mentioning `npx skills add
<repo-url>` as the one-line alternative (§3.1(a) of that research, not a
new install-script dependency this project maintains).

### 3.2 `SKILL.md` frontmatter

```yaml
---
name: llm-wikis-usage
description: <one to two sentences the model uses to decide when to load this — trigger phrases like "query a wiki with llm-wikis", "check why llm-wikis doctor failed", "what does llm-wikis --json return">
---
```

Exactly these two keys (D5) — no `allowed-tools`, `license`,
`compatibility`, `metadata`, `when_to_use`, `argument-hint`, or any other
Claude Code-specific or Agent-Skills-standard field, even though the
broader 6-field cross-tool-safe set (F1) would permit more. This is a
narrower, deliberate choice (D5), not an oversight — do not "helpfully"
add `allowed-tools` back in during implementation.

### 3.3 `SKILL.md` body — content map (source: `docs/llm-wikis.md`)

| Section of `SKILL.md` | Sourced from |
|---|---|
| Command overview (`config init/list/validate`, `list`, `doctor`, `query`) | `docs/llm-wikis.md:428-438` (§3.1 command grammar) |
| Question input (positional after `--` vs stdin, not both) | `docs/llm-wikis.md:454-469` (§3.2) |
| `--json` envelope shape + exit-code table | `docs/llm-wikis.md:476-506` (§3.3) |
| `doctor` static vs `--live` (both selectors required, quota cost) | `docs/llm-wikis.md` §3.7 area (doctor checks) |
| `ENTRYPOINT_UNVERIFIED` / live-probe fingerprinting | `docs/llm-wikis.md` §2.9 area |
| Read-only contract, one-paragraph summary (not the full 17-layer list) | `docs/llm-wikis.md:508-` (§3.4), condensed |
| "Where to look when something fails" pointer into `references/errors.md` | new prose, not copied verbatim |

Keep `SKILL.md` itself under roughly 500 lines per the Agent Skills
convention cited in research (`research/skills-directory.md` §1.3) — the
full error-code table and the full read-only-layer list belong in
`references/errors.md`, not the main body. **`SKILL.md` and
`references/errors.md` must reflect the flags/behavior this task actually
ships** (`--plain`, `--yes`, `--force`) — write this content *after*
Features 2 and 3 land (see `implement.md` ordering), not from the
pre-task command surface, or the skill will document a CLI that no longer
matches reality on day one.

### 3.4 `references/errors.md`

The full error-code table from `docs/llm-wikis.md` §3.8 (or the
equivalent numbered section — confirm exact heading during
implementation, since this design was drafted from the Findings' §3.8
citation without re-reading that section verbatim), reformatted for an
agent troubleshooting a single failing invocation: code → likely cause →
what to check next, not the operator-guide's full prose.

### 3.5 Test strategy

Not `cargo test`-covered (it is documentation, not code) — `checklist.md`
carries `rg`-based structural gates instead: frontmatter contains `name:`
and `description:` and nothing else key-wise (§0's exact gate commands are
in `checklist.md`), and the file exists at the expected path. Content
*accuracy* (does the skill correctly describe the shipped CLI) is a
manual-review gate, not automatable — recorded as prose in
`checklist.md`.

## 4. Spec-document revision points (D2, F7)

All of the following are **prose edits**, not mechanically checked by
`tests/spec_drift.rs` (§0) — each needs a human-verified, not just an
`rg`-found-a-substring, review, though `checklist.md`'s `rg` gates catch
the coarse "was this section touched at all" signal.

`docs/2026-07-28-llm-wikis-external-query-design.md`:
- §3.1 In Scope, `docs/2026-07-28-llm-wikis-external-query-design.md:72`:
  "a non-interactive, non-overwriting `llm-wikis config init`" no longer
  describes the shipped behavior (it is TTY-interactive by default, and
  overwrites under `--force`/confirmed `Confirm`) — reword to state the
  actual default-agent-and-provider-executable wizard scope and the
  overwrite-guard behavior, while keeping the "no `config add-wiki`" scope
  boundary (F9) explicit.
- §3.2 Explicitly Out of Scope,
  `docs/2026-07-28-llm-wikis-external-query-design.md:97`: "an interactive
  configuration wizard" must be removed from this out-of-scope list (it
  directly contradicts the shipped behavior) — replace with a narrower,
  still-true out-of-scope statement, e.g. "an interactive wizard for
  registering wikis (`config add-wiki`)" to preserve F9's actual boundary.
- §5.1 command grammar block
  (`docs/2026-07-28-llm-wikis-external-query-design.md:127-135`): add
  `[--yes] [--force]` to the `config init` line and `[--plain]` to the
  `query` line.
- §5.1 `config init` paragraph
  (`docs/2026-07-28-llm-wikis-external-query-design.md:139`): replace "A
  future wizard or `config add-wiki` command is outside version 0.1.0."
  with a description of the shipped wizard/overwrite behavior (§2.2's
  matrix, summarized in prose) — retain "`config add-wiki` is outside
  version 0.1.0" as still true (F9/D4).
- §5.1 JSON envelope sentence
  (`docs/2026-07-28-llm-wikis-external-query-design.md:141`): **no content
  change** — the envelope shape is locked (F10/D4) and stays exactly `{
  schema_version, ok, operation, path, created, error? }`; do not add an
  `overwritten`/`interactive` field here (would be a second, unapproved
  spec-drift point).
- §5.1 human-output sentence
  (`docs/2026-07-28-llm-wikis-external-query-design.md:151`): "Normal
  human output prints the answer followed by gaps and warnings." needs an
  addendum for the new rendering behavior (TTY auto-render via termimad,
  `--plain`/`NO_COLOR`/piped fall back to raw markdown).

`docs/llm-wikis.md` (operator guide):
- §2.3 "`config init` never overwrites"
  (`docs/llm-wikis.md:186-198`): rewrite to describe the wizard trigger
  condition, `--yes`, the overwrite `Confirm`, and `--force` — this
  section's title itself ("never overwrites") is no longer accurate and
  should change along with the body.
- §3.1 command grammar block (`docs/llm-wikis.md:428-438`): same flag
  additions as the spec doc's §5.1 block above.
- §3.2/new subsection: document `--plain` next to the existing stdin/spinner
  prose (`docs/llm-wikis.md:454-474`).
- §3.3 (`docs/llm-wikis.md:476-506`): no JSON-shape change (mirrors the
  spec doc's §5.1 note above); optionally note that human-mode rendering
  is TTY-conditional, next to the existing "human mode prints the answer,
  then gaps, then warnings" sentence (`docs/llm-wikis.md:479-480`).
- README.md's own copies of the `config init` description (`README.md:60`)
  and command reference (`README.md:100-109`) get the same flag mentions,
  plus the new skills-install section (§3.1 above).

## 5. Summary: what is and is not automatable

| Behavior | Automatable in this repo's CI? |
|---|---|
| `--json`/non-TTY/`--plain`/`NO_COLOR` markdown paths | Yes — `assert_cmd`, byte comparison |
| Actual TTY-triggered termimad ANSI rendering | No — no pty in the test harness; manual |
| `--yes`/non-TTY/`--force` init paths (template, overwrite, `CONFIG_EXISTS`) | Yes — `assert_cmd`, byte comparison |
| Actual TTY-triggered wizard prompts and overwrite `Confirm` | No — `dialoguer` requires a real tty; manual |
| Template-renderer default-argument output vs `INIT_TEMPLATE` | Yes — pure function unit test |
| `SKILL.md` frontmatter shape (exactly `name`+`description`) | Yes — `rg`-based structural gate |
| `SKILL.md`/`references/errors.md` content accuracy | No — manual review against shipped CLI |
| Spec-doc prose fidelity (§4 above) | No — manual review; `rg` only proves "section was touched" |
