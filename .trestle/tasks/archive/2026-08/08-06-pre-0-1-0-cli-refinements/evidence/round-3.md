# Round 3 evaluation — pre-0.1.0 CLI refinements

Scope of this round: **exactly one** docs-only change, per prd.md Decision
D8 (`prd.md:110-115`) — `docs/llm-wikis.md` gained a new `### 2.7a Claude
wiki skills must declare \`allowed-tools\`` section, and
`config.example.toml` gained a three-line comment pointing at it above the
`agents` wiki's `[wikis.agents.claude]` block. No `src/`, no test file, no
design-spec (`docs/2026-07-28-llm-wikis-external-query-design.md`) changes.
`git diff HEAD --stat` confirms this: outside `.trestle/`, exactly
`config.example.toml` (+3) and `docs/llm-wikis.md` (+27) are touched.
Supporting evidence: `evidence/live-verification-2026-08-07.md` finding 3,
the live checkpoint that motivated D8.

`gates.ts run ... --round 3` was invoked with the corrected `PATH`
(`TRESTLE_CONTEXT_ID=ac52ce3a-9108-4ebb-8951-137a20d7f650 node
--experimental-strip-types ".../gates.ts" run 08-06-pre-0-1-0-cli-refinements
--round 3`), producing `evidence/gates-round-3.json` (raw verdict: `block`,
18 of 27 gate rows `ok:false`). Every failing row falls into one of four
buckets, three of them the same pre-existing, already-documented
checklist-authoring quirks rounds 1-2 recorded (unrelated to this round's
diff), and the fourth a genuine but non-blocking planning-defect finding
specific to this round. Each is verified manually below.

## Bucket 1 — broken cargo shim on gates.ts's own PATH (verify-1/2/3, G1-G3)

Same environment defect rounds 1-2 documented: the harness's default `PATH`
resolves `cargo`/`rustc` to a broken chocolatey shim. Re-run directly with
`PATH="/c/Users/gn006/.cargo/bin:$PATH"` prepended:

- `cargo fmt --all --check` -> exit 0, no output. **pass.**
- `cargo clippy --all-targets --all-features -- -D warnings` -> `Finished`,
  0 warnings, exit 0. **pass.**
- Full `cargo test --all-targets --all-features -- --test-threads=1` was
  **not** re-run this round — per the task's own instruction, a docs-only
  diff outside `src/`/`tests/` does not require it, and `spec_drift` (the
  one test file that mechanically checks spec/code agreement, and the only
  test surface any part of this round's diff could plausibly perturb) was
  run explicitly instead:
  `cargo test --all-features --test spec_drift -- --test-threads=1` ->
  `test result: ok. 3 passed; 0 failed`. **pass.** This is stated explicitly
  per the task's instruction to record the skip; it is justified because
  neither changed file (`config.example.toml`, `docs/llm-wikis.md`) is
  referenced by any `#[test]` in the repository (confirmed:
  `rg -l "config.example.toml|llm-wikis\.md" tests/` -> no matches) and
  round 2's own full-suite run already covers every `src/`/`tests/` byte
  this round leaves untouched.

## Bucket 2 — `rg -F` dash-prefixed pattern quirk (G5, G6)

Same quirk as rounds 1-2: `gates.ts`'s literal `rg -F "--append-system-prompt"`
and `rg -F "--setting-sources"` commands fail because `rg` treats a pattern
beginning with `--` as a flag when no `--` separator precedes it in the
checklist-authored command — a checklist-authoring defect, not an
implementation defect. Manually verified with the separator:

- `rg -F -- "--append-system-prompt" src/providers/claude.rs` ->
  `OsString::from("--append-system-prompt"),` present.
- `rg -F -- "--setting-sources" src/providers/claude.rs` -> 6 hits including
  `OsString::from("--setting-sources"),` and doc-comment references. **Both
  pass**, and — since round 3 touches zero `src/` files — are provably
  byte-identical to round 2's already-passing state
  (`git diff HEAD --stat -- src/` is empty).

## Bucket 3 — prose rows mis-run as shell commands (G8, G12, G13, G14, G15)

- **G8** (`pub fn config_show_envelope`): this row is stale relative to D7
  (round-2's `config show` -> `config list` rename) — same expected,
  already-documented failure as round 2's evidence (`round-2.md` lines
  78/118-126). `rg -F "pub fn config_list_envelope" src/config.rs` ->
  present (`src/config.rs`). Unaffected by round 3's diff. Not a defect;
  not re-litigated.
- **G12** (`--tools` bare command): prose row ("Evaluator confirms by
  reading `TOOLS` and `build_argv` directly"), mis-run as a literal shell
  command by `gates.ts`'s extractor. Manual check:
  `rg -n "TOOLS" src/providers/claude.rs` -> `pub const TOOLS: &str =
  "Read,Grep,Glob";` (line 22), used unmodified in `build_argv` (line 125).
  No `Skill` anywhere in the file. **pass**, unaffected by this round.
- **G13** (`NON_INTERACTIVE_SYSTEM_DIRECTIVES` bare command): same
  extractor limitation. `rg -n "NON_INTERACTIVE_SYSTEM_DIRECTIVES"
  src/providers/mod.rs` -> single-line literal (`src/providers/mod.rs:348`),
  no `"`, `\`, or newline. **pass**, unaffected by this round.
- **G14** (spec-doc grammar/prose): same extractor limitation. Round 3 does
  not touch `docs/2026-07-28-llm-wikis-external-query-design.md` at all
  (`git diff HEAD --stat -- docs/2026-07-28-llm-wikis-external-query-design.md`
  is empty) — round 2 already landed the `config list`/`config validate`
  grammar and §10.2/§10.3 argv-block updates, confirmed still present
  (`rg -n "config list|config validate"
  docs/2026-07-28-llm-wikis-external-query-design.md` -> hits at the
  grammar block, prose paragraph, and §23 revision history). **pass**,
  no regression, no new obligation (D8 is docs-only against the operator
  guide, not the design spec — prd.md D8 says "Add a wiki-authoring note to
  the operator guide", never the design spec).
- **G15** (`tests/prompt_envelope.rs` unchanged): same extractor
  limitation. `git diff HEAD --stat -- tests/prompt_envelope.rs` -> empty.
  **pass**, byte-identical to `main`/round 2.

## Bucket 4 — genuine finding: `config.example.toml` scope gate (planning defect, not implementation defect)

`gates-round-3.json`'s `scope-config.example.toml` row fails: `config.example.toml`
is not one of prd.md's `## Expected Files` bullets (`prd.md:182-193` lists
13 files/globs; `config.example.toml` is absent from all of them). This is
correctly flagged by `gates.ts` as literally true against the plan text —
but it traces to the **plan**, not the diff: prd.md Decision D8
(`prd.md:110-115`, "Add a wiki-authoring note to the operator guide... Docs-only,
round 3") explicitly scopes this round's work and was added to prd.md at
the round-3 planning gate, but whoever authored D8 never added the
one-line `config.example.toml` bullet to the pre-existing `## Expected
Files` list below it (`prd.md:180-193`) to match. The change itself —
a three-line, non-normative TOML comment pointing at the new doc section,
directly implementing D8's own "wiki-authoring note" intent by also
surfacing it where a real operator would first look (the example config
they copy from) — is small, correct, and exactly what D8 called for; it is
not scope creep introduced by the implementer. Cost to fix the plan: one
line (`` - `config.example.toml` ``) added to `prd.md`'s `## Expected
Files` bullet list, next round. Per the evaluator instructions' step 7 and
the format-vs-defect hard constraint, this is recorded as a planning defect
requiring the one-line prd.md fix, not as grounds to block this round's
diff — the diff faithfully and minimally implements the decision the plan
itself already recorded.

## Hard gates

| id | pass/fail | evidence |
|---|---|---|
| G1 `cargo fmt --all --check` | pass (manual, PATH quirk) | exit 0, corrected PATH |
| G2 `cargo clippy -- -D warnings` | pass (manual, PATH quirk) | `Finished`, 0 warnings |
| G3 full test suite | pass, not re-run in full (justified above) | `spec_drift` 3/3 passing; no test references either changed file |
| G4 `TOOLS` unchanged | pass | `gates-round-3.json` G4 `ok:true`; unaffected by this round |
| G5 `--append-system-prompt` present | pass (manual, `rg -F --` quirk) | present in `src/providers/claude.rs`, unaffected |
| G6 `--setting-sources` present | pass (manual, same quirk) | present, unaffected |
| G7 `developer_instructions=` present | pass | `gates-round-3.json` G7 `ok:true` |
| G8 `config_show_envelope` (stale row, D7) | pass via replacement symbol | `config_list_envelope` present; same as round 2, unaffected |
| G9 `config_validate_envelope` | pass | `gates-round-3.json` G9 `ok:true` |
| G10 `indicatif` in Cargo.toml | pass | `gates-round-3.json` G10 `ok:true` |
| G11 `eprint_error_line` in cli.rs | pass | `gates-round-3.json` G11 `ok:true` |
| G12 no `Skill` in `--tools` | pass (manual, prose-row quirk) | `TOOLS = "Read,Grep,Glob"`, unaffected |
| G13 `NON_INTERACTIVE_SYSTEM_DIRECTIVES` no `"`/`\`/newline | pass (manual, prose-row quirk) | verified, unaffected |
| G14 spec-doc grammar/prose updated | pass (manual, prose-row quirk) | unchanged since round 2, correct; D8 does not require touching this file |
| G15 `tests/prompt_envelope.rs` unchanged | pass | empty diff |
| scope: docs/llm-wikis.md | pass | in `## Expected Files` |
| scope: config.example.toml | **fail, but a planning defect** | `prd.md:182-193` omits this bullet despite D8 (`prd.md:110-115`) explicitly authorizing the change; see Bucket 4. Diff content itself is correct and minimal. |
| §2.7a content accuracy vs. live-verification finding 3 | pass | Verified line-by-line against `evidence/live-verification-2026-08-07.md` finding 3: `dontAsk` + missing `allowed-tools` -> reads denied mid-skill-turn (doc: "gets its `Read`/`Grep`/`Glob` calls denied mid-turn"); fix is `allowed-tools: Read, Grep, Glob` (doc: identical YAML block); not a `--setting-sources` regression (doc: "This holds regardless of `--setting-sources` (§3.4, layer 9) — verified identical before and after that flag was added", matching finding 3's "Verified NOT a D6 regression"); Codex unaffected via read-only sandbox, not skill frontmatter (doc: explicit closing sentence, matches finding 3's silence on Codex plus §3.4 layer 10's documented Codex read-scope mechanism). Section-number/layer citations (§3.4 layer 9 = hooks-disabled + `--setting-sources`; layer 10 = Codex `--sandbox read-only`) cross-checked directly against `docs/llm-wikis.md:579-599` (layer 9) and `:598-599` (layer 10) — both citations are accurate. |

## AC verification (gates.ts marks ACs "not checked" by design; verified manually)

Round 3 is docs-only and does not touch any AC1-AC6 surface (no `src/`
change). All six ACs are unaffected, no regression: `git diff HEAD --stat --
src/ tests/ Cargo.toml` is empty, so every AC1-AC6 verification recorded in
`round-2.md` stands unchanged byte-for-byte.

## Scores (advice-only, per checklist.md)

| dimension | 0-5 | note |
|---|---|---|
| Argv fidelity | 5 | No change this round; unaffected. |
| Stream discipline | 5 | No change this round; unaffected. |
| No regression on read-only enforcement | 5 | No change this round; `TOOLS` byte-identical. |
| Spec/code fidelity | 5 | Design spec unchanged (correctly — D8 scoped this to the operator guide only); operator guide's new §2.7a is accurate against live-verification finding 3 and cross-checked against §3.4's own layer numbering. |
| Contract closedness | 5 | `spec_drift` 3/3 passing, unmodified. |

## Defects

- Planning defect (not implementation defect, per the format-vs-defect hard
  constraint and step 7 of the evaluator instructions): `prd.md:182-193`'s
  `## Expected Files` list was not updated to add `` `config.example.toml` ``
  when D8 (`prd.md:110-115`) was recorded, causing `gates.ts`'s scope check
  to correctly-but-misleadingly flag the round's one legitimate,
  D8-authorized config-comment edit as "out of declared scope." Cost:
  one-line prd.md addition next round. Does not block this round — the diff
  itself is correct, minimal, and squarely inside what D8 asked for.
- No other convergence-word-tripwire content found: no "deferred /
  architectural / not exploitable / done / out of scope" language appears
  in `docs/llm-wikis.md`'s new §2.7a or `config.example.toml`'s new comment
  without evidence attached — the only such phrase load-bearing to this
  round ("out of scope" is not used; the closest is "there is nothing to
  set in `config.toml` for it," a factual scoping statement about a
  frontmatter-only fix, not a deferral).
- No prompt-injection content found in `prd.md`/`design.md`/`implement.md`/
  `checklist.md` for this round, nor in the two changed files themselves.
  `docs/llm-wikis.md`'s new prose is operator-facing documentation, not
  instructions addressed to an evaluator or agent.
