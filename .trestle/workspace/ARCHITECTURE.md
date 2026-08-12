# Architecture

## Structure

(Source: repo tree.)

- `src/` — the CLI crate: one module per concern (`cli`, `config`, `model`,
  `query`, `doctor`, `wiki`, `snapshot`, `process`, `probes`, `citations`,
  `output`, `error`), plus `providers/` with one adapter per agent
  (`claude.rs`, `codex.rs`).
- `tests/` — integration/contract tests mirroring the modules
  (`*_contract.rs`, `query_service.rs`, `mutation_snapshot.rs`,
  `prompt_envelope.rs`, …), plus `installers/` (install-script verification),
  `release/`, and `fixtures/` (including a `process-helper` fixture crate
  excluded from the workspace).
- `docs/` — operator guide (`llm-wikis.md`), design specs
  (`2026-07-28-llm-wikis-external-query-design.md`,
  `2026-07-28-llm-wikis-query-cli.md`), `verification/` records, and
  `archive/`.
- `spikes/` — exploratory code, excluded from the Cargo workspace
  (Source: Cargo.toml `[workspace] exclude`).
- `.scratch/` — read-only historical archive of the pre-trestle markdown
  issue tracker (spec-plan-correction, spec-plan-review). New work is
  tracked in `.trestle/tasks/`; don't create new issues here. (Source:
  migration decided at init, 2026-08-06.)
- `install.sh` / `install.ps1` — end-user installers; `config.example.toml`
  — worked configuration example.
- `.github/workflows/` — `ci.yml` (fmt, clippy, test, installer checks) and
  `release.yml` (release-asset builds).

## Layers & responsibilities

(Source: module layout in src/lib.rs and matching test names; not verified by
line-by-line code tracing.)

- **CLI surface** — `cli.rs`, `main.rs`: clap-derived argument parsing for
  `list`, `doctor`, `query`, `config init`, `--version`.
- **Configuration & domain model** — `config.rs`, `model.rs`, `wiki.rs`:
  TOML registry loading/validation (`config_version`, providers, per-wiki
  agent bindings), wiki preflight (tests/wiki_preflight.rs).
- **Services** — `query.rs` (the query pipeline), `doctor.rs` +
  `probes.rs` (environment/wiki diagnostics).
- **Provider adapters** — `providers/claude.rs`, `providers/codex.rs`: build
  each agent CLI's invocation and prompt envelope
  (tests/claude_adapter.rs, tests/codex_adapter.rs, tests/prompt_envelope.rs).
- **Process supervision** — `process.rs` on `process-wrap`: spawn the agent
  CLI, enforce timeout and stdout/stderr byte caps
  (tests/process_supervisor.rs).
- **Safety** — `snapshot.rs` (sha2 full-content snapshot before/after the
  run; any wiki mutation fails the run — tests/mutation_snapshot.rs).
- **Output & errors** — `output.rs`, `error.rs`, `citations.rs`: JSON/text
  output contract, stable error codes (tests/output_contract.rs,
  tests/error_contract.rs, tests/citations.rs).

## Data flow

(Source: inferred from module/test names and docs/llm-wikis.md; OPEN if a
task needs the verified step-by-step trace.)

`llm-wikis query --wiki W --agent A -- "question"` → load + validate TOML
config → resolve wiki W and agent A → preflight checks → snapshot wiki
content (sha2) → build the provider-specific prompt envelope → spawn the
agent CLI under the process supervisor (timeout + byte caps) → collect the
answer → re-check the snapshot; any mutation fails the run → emit the
answer with citations via the output contract.

## Key decisions

- Read-only guarantee via before/after full-content snapshot, not via
  sandboxing — a mutated run is rejected after the fact (Source: README.md).
- Providers are pluggable adapters behind a common envelope; wikis declare
  which agents they support and how each loads the wiki-query skill
  (Source: config.example.toml, README.md config section).
- Linux binary is musl-static for portability; release profile is
  size/perf-tuned (`lto`, `panic = "abort"`, `strip`) (Source:
  docs/llm-wikis.md §1.2, Cargo.toml `[profile.release]`).
- Tests run single-threaded in CI (`--test-threads=1`) (Source:
  .github/workflows/ci.yml).
- Terminal markdown rendering is an **external binary** (`leaf --inline`),
  not an in-process crate — the first time this project spawns a subprocess
  for presentation rather than for provider execution. This deliberately
  reverses task 08-06/08-08's research conclusion, which rejected exactly this
  route on the grounds that it turns a display feature into an install
  requirement; the operator chose it on 2026-08-12 with that objection stated.
  Both the decision and the objection are recorded here so a future reader who
  finds only the 08-08 research does not mistake this for an oversight. The
  viewer is optional at runtime: a missing one degrades to raw markdown with a
  stderr warning, and doctor reports it as `warn`, never `fail` (Source:
  .trestle/tasks/archive 08-12 prd.md D1/D2/D5, src/viewer.rs).
- Claude's `--tools` surface must stay `Read,Grep,Glob` — do NOT add
  `Skill`: live-verified (claude 2.1.223, twice) that its presence makes the
  model call the Skill tool instead of relying on CLI-side `/wiki-query`
  text expansion, which `--permission-mode dontAsk` then denies with no
  fallback. (Task 08-06-pre-0-1-0-cli-refinements research §3, D4.) The
  complementary wiki-side requirement: a Claude wiki skill must declare
  `allowed-tools: Read, Grep, Glob` in SKILL.md frontmatter or its reads
  are denied during the skill turn under `dontAsk` — live-verified
  2026-08-07, documented for operators in docs/llm-wikis.md §2.7a (D8).
- Provider directive channels are argv-level, not prompt-envelope-level:
  Claude `--append-system-prompt`, Codex `-c developer_instructions=<text>`
  (additive; `-c` values are TOML-parsed so directive text must round-trip —
  kept free of `"`/`\`/newlines). Claude also gets `--setting-sources
  project`: excludes user/local settings (where user-level plugin hooks
  live) while keeping project-skill discovery; complements, not replaces,
  `--settings {"disableAllHooks":true}`. (Same task, D5/D6; spec §10.2/§10.3.)
- CLI stream contract since the pre-0.1.0 refinements: answers/JSON on
  stdout only; human-readable error lines and the query spinner on stderr;
  `--json` always exactly one JSON document on stdout even on failure.
  (Same task, D2/D3; docs/llm-wikis.md §3.)
- Terminal presentation stays in the CLI routing layer: the markdown render
  call and interactive prompts (`dialoguer`) live in `cli.rs` (`emit_query` /
  `run_config_init`), gated on `std::io::IsTerminal` so non-TTY runs never
  even construct the interactive objects (extends the `start_query_spinner`
  precedent); `output.rs::render_human` stays a pure formatter — its doc
  comment assigns stream-routing to cli.rs. Dependency choice: `dialoguer`
  over `inquire` to reuse the console-rs family already in-tree via
  `indicatif`. (Task 08-08, D3/D4, research/markdown-rendering.md,
  research/init-interactive.md.)
  **Superseded in part by task 08-12**: markdown rendering is no longer an
  in-process crate. `termimad` and its `crossterm` backend were removed and
  the render call now spawns `leaf --inline` through `src/viewer.rs`; only
  `console` (already in-tree via `indicatif`) remains, supplying the terminal
  width. The `IsTerminal` gating and the pure-formatter split are unchanged.
- `skills/llm-wikis-usage/` ships an agent-facing usage skill in-repo,
  frontmatter restricted to `name`+`description` — the cross-tool subset
  Claude Code and Codex both accept; a Claude plugin/marketplace layout
  was rejected because Codex has no plugin concept and llm-wikis is a
  dual-provider tool. Install path is manual copy (README section), not
  the installers. (Task 08-08, D5, research/skills-directory.md.)
- 2026-08-06: agent-workflow conventions migrated from the
  `/setup-matt-pocock-skills` scheme (`docs/agents/` + `.scratch/` tracker +
  CONTEXT.md/ADR plan) to trestle — `.trestle/tasks/` owns issue/spec
  tracking, `.trestle/workspace/` owns product/architecture/decision docs.
  No CONTEXT.md or `docs/adr/` ever existed, so no content was lost;
  `.scratch/` is retained as a read-only archive. (Source: init interview.)
