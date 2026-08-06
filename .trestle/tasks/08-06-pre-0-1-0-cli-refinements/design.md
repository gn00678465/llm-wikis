# Design — pre-0.1.0 CLI refinements

Branch: `feat/pre-0.1.0-cli-refinements`. All work happens in the four areas
below, in the order Feature 3 → Feature 1 → Feature 2 → Feature 4 (see
`implement.md` for why that order and where the rollback points sit).

## 0. Cross-cutting invariants that must survive every feature

- `tests/spec_drift.rs` parses exactly three things out of
  `docs/2026-07-28-llm-wikis-external-query-design.md`: the Section 14 error
  table (`ErrorCode::ALL`), the Section 15 `checks[].name` sentence
  (`DOCTOR_CHECK_NAMES`), and the Section 13 wrapper-warning-code sentence
  (`WrapperWarningCode::ALL`). None of the four features add an `ErrorCode`
  variant, a doctor check name, or a wrapper warning code, so all three
  `spec_drift` tests stay green without edits to those specific tables. The
  *other* spec text this task touches (§5.1 command grammar, §10.2/§10.3
  target-invocation blocks) is **not** mechanically checked by
  `spec_drift.rs` — it is a manual-fidelity requirement from the task brief,
  tracked as Hard Gate G14 in `checklist.md`, not a test.
- `TOOLS` (`src/providers/claude.rs:21`) stays the literal `"Read,Grep,Glob"`
  — D4/research §3 is a fixed constraint, not re-opened by this task. No
  step below touches that constant.
- No change in this task adds, removes, or renames a public JSON field on
  `QueryEnvelope`, `ListEnvelope`, or `DoctorEnvelope` — only two *new*
  envelope shapes are added (`ConfigShowEnvelope`, `ConfigValidateEnvelope`),
  and only the *destination stream* of human-mode error text changes.

## 1. Feature 3 — human-readable error lines move to stderr (AC3)

This is done first because Feature 1's new subcommands are written to reuse
the resulting helper rather than duplicating the old inline pattern.

### 1.1 Shared helper (`src/cli.rs`)

```rust
/// The one human-readable rendering of a top-level command failure (spec
/// §5.1/§5.3, D2): always stderr, never stdout, so a pipe/redirect/CI
/// consumer of stdout never sees error text mixed into whatever partial
/// human output preceded it. `--json` mode never calls this — the failing
/// envelope's `error` object is already part of the single JSON document
/// `emit_*`'s `if json` branch writes to stdout instead.
fn eprint_error_line(err: &AppError) {
    eprintln!("error: {} ({})", err.code.as_str(), err.message);
}
```

This is the **single source of truth** for the format string, replacing five
independent `println!("error: {} ({})", ...)` call sites (matches the
codebase's existing "one function, not duplicated logic" convention, e.g.
`check_claude_wiki_settings_surface`).

### 1.2 Call sites converted

| Site | Today | After |
|---|---|---|
| `run_config_init` (`src/cli.rs:279-281`) | `println!("error: ...")` in the `else if let Some(err)` branch | `eprint_error_line(err)` |
| `print_list_human` (`src/cli.rs:308-310`) | `println!("error: ...")` | `eprint_error_line(err)` |
| `print_doctor_human` (`src/cli.rs:397-400`) | `println!("error: ...")` | `eprint_error_line(err)` (the per-check `[Fail] entrypoint: ...` lines directly below stay on stdout — they are the doctor report body, not the top-level command-failure line; AC3 only moves the single top-level `error` object's rendering) |
| `emit_query` (`src/cli.rs:451-463`) | `print!("{}", render_human(&envelope))`, which internally formats the error line onto the same stdout string (`src/output.rs:167-169`) | restructured — see 1.3 |
| new `run_config_show`/`run_config_validate` (Feature 1) | n/a (new code) | written directly against `eprint_error_line` from the start |

### 1.3 `emit_query` / `render_human` split

`render_human` (`src/output.rs:162-183`) is a **public, independently tested**
function (`tests/output_contract.rs::human_mode_prints_answer_then_gaps_then_warnings_in_order`
exercises only the success path — no test exercises its `error` branch). Its
documented job is "answer, then gaps, then warnings" (`src/output.rs:160-161`)
and its own module doc says rendering "performs no provider parsing or path
access" — it is a pure formatter, not a stream-routing decision. Stream
routing belongs in `cli.rs`, so `emit_query` — not `render_human` — decides
where the error line goes:

```rust
fn emit_query(json: bool, envelope: QueryEnvelope) -> i32 {
    let exit = envelope.error.as_ref().map(|e| e.code.exit_code()).unwrap_or(0);
    if json {
        print!("{}", render_json(&envelope));
    } else if let Some(err) = &envelope.error {
        eprint_error_line(err);
    } else {
        print!("{}", render_human(&envelope));
    }
    exit as i32
}
```

`render_human`'s `else if let Some(err) = &envelope.error` branch
(`src/output.rs:167-169`) becomes dead code from `cli.rs`'s perspective (a
`QueryEnvelope` with `error: Some(_)` never reaches `render_human` through
`emit_query` any more). Remove that branch from `render_human` itself —
`answer` is `None` exactly when `error` is `Some` for every envelope this
codebase constructs (see `query_failure_envelope`, `src/cli.rs:419-441`, and
`QueryService`'s own success/failure split), so the branch was already only
reachable via a deliberately-constructed test envelope, and no such test
exists. Leaving it in would be a second, now-unreachable-from-production copy
of the error-formatting logic — exactly the kind of drift risk this
codebase's comments elsewhere warn about. Update `render_human`'s doc comment
to state plainly that error rendering is `cli.rs`'s responsibility now.

### 1.4 Test fallout (enumerated exactly)

- `tests/cli_contract.rs::unknown_subcommand_is_argument_invalid_not_a_panic`
  (`tests/cli_contract.rs:221-232`): today asserts `out.stderr.is_empty()`
  and `stdout` contains `"ARGUMENT_INVALID"`. Flip both: assert `stdout` is
  now empty (`argument_invalid_query` sets `answer: None`, so nothing prints
  to stdout in human mode) and `stderr` contains `"ARGUMENT_INVALID"`. Update
  the stale doc comment ("this wrapper never writes to stderr outside
  `--help`/`--version`") to describe the new behavior.
- `tests/cli_contract.rs::json_mode_emits_exactly_one_document_even_for_a_top_level_parse_failure`
  (`tests/cli_contract.rs:388-403`): **no change** — this test already only
  exercises `--json` mode, whose `stderr.is_empty()` assertion stays true
  under the new `emit_query` (the `eprint_error_line` branch is `else if`,
  reached only when `json` is false).
- `tests/cli_contract.rs::human_mode_never_prints_json_on_stdout_for_an_argument_failure`
  (`tests/cli_contract.rs:791-809`): existing assertion (`stdout` contains no
  `{`) still holds (stdout is now empty, a fortiori no `{`). Strengthen it:
  additionally assert `stdout` is fully empty and `stderr` contains
  `"ARGUMENT_INVALID"`, turning this test into the human-mode companion of
  the JSON-mode test above.
- New test: `list`/`doctor`/`config init`/`query` each need one human-mode
  failure case proving the new stderr routing end to end through the real
  binary (not just the two existing argument-parse-time cases above, which
  both happen to be *pre-dispatch* clap failures). Use `minimal_registry`
  fixtures already in the file (e.g. a `list` against `nonexistent_config_path()`
  in human mode: assert `stdout` empty, `stderr` contains `"CONFIG_INVALID"`).
- `tests/output_contract.rs`: no change required (its one `render_human` test
  never touches the error branch being removed).

## 2. Feature 1 — `config show` / `config validate` (AC1)

`doctor` is explicitly unchanged (D1). Both new subcommands reuse
`resolve_config_path` (`src/cli.rs:195-208`) and `Config::load`
(`src/config.rs:413-417`) exactly the way `run_list_command` already does —
no new path-resolution logic.

### 2.1 `Config` gains `Serialize`

`config show`'s JSON mode must emit the resolved configuration, which means
`Config` and every type it is built from need `serde::Serialize` in addition
to the `Deserialize` they already have. Add `Serialize` to the derive list
of, in `src/config.rs`:

- `RuntimeConfig` (`src/config.rs:158-159`)
- `ProvidersConfig` (`src/config.rs:213-214`)
- `ProviderConfig` (`src/config.rs:231-232`)
- `LoadMode` (`src/config.rs:239-240`) — its existing
  `#[serde(rename_all = "snake_case")]` applies identically to serialization,
  so `project_skill`/`local_plugin` round-trip byte-identical.
- `ProviderWikiConfig` (`src/config.rs:248-249`)
- `WikiConfig` (`src/config.rs:285-286`)
- `Config` (`src/config.rs:371-372`)

`#[serde(deny_unknown_fields)]` is a deserialize-only attribute and is inert
(and harmless) on the serialize side, so it stays untouched everywhere it
appears. `Agent` (`src/output.rs:32-37`) already derives both.

"Resolved configuration" (AC1's wording) means: the parsed `Config` after
serde's own field-level defaulting (`RuntimeConfig`'s four
`#[serde(default = "...")]` fields, `default_agent`/`providers`/`wikis`'s
`#[serde(default)]`) — i.e. exactly what `Config::load` already produces.
This task does **not** additionally canonicalize `project_root`/
`content_root` per wiki (that is `resolve_wiki_roots`, a separate,
per-wiki-fallible operation `doctor`/`query` already own) — `config show`
prints the configuration as declared-plus-defaulted, not as
filesystem-resolved. This scope boundary keeps `config show` a pure,
side-effect-free config-load operation, matching AC1's "without side
effects" wording (shared with `validate`).

### 2.2 New envelopes (`src/config.rs`, alongside `ConfigInitEnvelope`)

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ConfigShowEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str, // "config_show"
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

pub fn config_show_envelope(path: &Path) -> ConfigShowEnvelope {
    match Config::load(path) {
        Ok(config) => ConfigShowEnvelope {
            schema_version: SCHEMA_VERSION, ok: true, operation: "config_show",
            path: path.display().to_string(), config: Some(config), error: None,
        },
        Err(err) => ConfigShowEnvelope {
            schema_version: SCHEMA_VERSION, ok: false, operation: "config_show",
            path: path.display().to_string(), config: None, error: Some(err),
        },
    }
}

/// Used only when the config *path itself* could not even be resolved
/// (relative `--config`, unresolvable platform default) — mirrors
/// `list_error_envelope`'s role for `list` (`src/doctor.rs:94-102`).
pub fn config_show_error_envelope(err: AppError) -> ConfigShowEnvelope {
    ConfigShowEnvelope {
        schema_version: SCHEMA_VERSION, ok: false, operation: "config_show",
        path: String::new(), config: None, error: Some(err),
    }
}
```

`ConfigValidateEnvelope` is the identical shape minus the `config` field
(AC1: "validate loads the config and reports ok/errors without side
effects" — no need to echo the parsed document back):

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ConfigValidateEnvelope {
    pub schema_version: &'static str,
    pub ok: bool,
    pub operation: &'static str, // "config_validate"
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

pub fn config_validate_envelope(path: &Path) -> ConfigValidateEnvelope { /* same Config::load match, no `config` field */ }
pub fn config_validate_error_envelope(err: AppError) -> ConfigValidateEnvelope { /* mirrors config_show_error_envelope */ }
```

Both reuse `Config::load`'s existing error-code mapping unchanged
(`CONFIG_INVALID` for malformed/semantically-invalid TOML,
`PROVIDER_CONFIG_MISSING`/`ENTRYPOINT_INVALID`/etc. for the semantic
`validate()` failures it already performs) — **no new error codes**, keeping
`tests/spec_drift.rs`'s Section 14 count at 27.

### 2.3 `src/cli.rs` wiring

```rust
#[derive(Subcommand)]
enum ConfigAction {
    Init,
    Show,
    Validate,
}
```

(clap's default kebab-case rule maps these to `show`/`validate` — no
`#[command(name = ...)]` needed, matching `AgentArg`'s existing precedent.)

```rust
CliCommand::Config { action } => match action {
    ConfigAction::Init => run_config_init(json, config_override),
    ConfigAction::Show => run_config_show(json, config_override),
    ConfigAction::Validate => run_config_validate(json, config_override),
},
```

```rust
fn run_config_show(json: bool, override_path: Option<&Path>) -> i32 {
    let envelope = match resolve_config_path(override_path) {
        Ok(p) => config_show_envelope(&p),
        Err(e) => config_show_error_envelope(e),
    };
    let exit = envelope.error.as_ref().map(|e| e.code.exit_code()).unwrap_or(0);
    if json {
        print!("{}", render_json_generic(&envelope));
    } else if envelope.ok {
        print_config_show_human(&envelope);
    } else if let Some(err) = &envelope.error {
        eprint_error_line(err);
    }
    exit as i32
}

fn print_config_show_human(envelope: &ConfigShowEnvelope) {
    let Some(config) = &envelope.config else { return };
    println!("config_version = {}", config.config_version);
    match config.default_agent {
        Some(a) => println!("default_agent = {}", agent_lowercase(a)),
        None => println!("default_agent = (none)"),
    }
    for (id, wiki) in &config.wikis {
        let agents: Vec<String> = wiki.agents.iter().copied().map(agent_lowercase).collect();
        println!("{id} - {} [{}]", wiki.title, agents.join(","));
    }
}
```

(`run_config_validate`/`print_config_validate_human` are the same shape,
printing `"Configuration at {path} is valid."` on success and nothing extra
— `eprint_error_line` covers the failure case identically.) `agent_lowercase`
is a one-line `Agent -> &'static str` helper (`Agent::Claude => "claude"`,
`Agent::Codex => "codex"`) — `doctor.rs::agent_key` already has this exact
mapping (`src/doctor.rs:361-366`) under a private name; either reuse it via a
newly-`pub(crate)` visibility bump or duplicate the two-line match — the
duplication is small enough that either is acceptable, prefer reuse if the
visibility bump is a one-line diff.

### 2.4 Test fallout

- `tests/config_contract.rs`: add `config_show_envelope`/`config_validate_envelope`
  unit tests (success + `CONFIG_INVALID` failure), following
  `tests/config_init.rs`'s existing envelope-shape-assertion style
  (`success_envelope_matches_the_exact_contract` at
  `tests/config_init.rs:73-95` is the template: assert the exact top-level
  key set via `BTreeSet`).
- `tests/cli_contract.rs`: add a `config show`/`config validate` section
  (after the existing `config init` tests) mirroring
  `config_init_json_success_and_failure_shapes`
  (`tests/cli_contract.rs:320-346`): `config init` to create a file, then
  `config show --json` / `config validate --json` against it (success), then
  against a missing path (failure, exit `2`, `CONFIG_INVALID`). Add one
  human-mode failure case per subcommand (stdout empty, stderr contains the
  error code) per §1.4 above. Assert `config validate`'s failure case does
  not mutate the target file's bytes (no side effects), reusing
  `config_init.rs`'s tamper-and-compare idiom
  (`tests/config_init.rs:56-70`).

## 3. Feature 2 — query spinner (AC2)

### 3.1 Dependency

`Cargo.toml`: add `indicatif = "0.17"` under `[dependencies]`, default
features only (no `rayon`, `tokio`, or `improved_unicode` — none of those
are needed for a stderr-only steady-tick spinner). This is the workspace's
ninth dependency (was 8).

### 3.2 Placement

Wrap only the one blocking call in `run_query_command`
(`src/cli.rs:594-598`, `service.query(request, QueryMode::Enforced)`) — the
only place a provider process actually runs synchronously on the calling
thread:

```rust
let spinner_enabled = std::io::stderr().is_terminal();
let spinner = spinner_enabled.then(|| {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr());
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner} querying \"{wiki}\"...")
            .expect("static template is well-formed")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
});
let envelope = match service.query(request, QueryMode::Enforced) {
    Ok(envelope) => envelope,
    Err(e) => query_failure_envelope(None, None, e),
};
if let Some(pb) = spinner {
    pb.finish_and_clear();
}
emit_query(json, envelope)
```

(`{wiki}` in the template needs a `pb.set_message`/custom field rather than a
literal token if indicatif's template syntax doesn't support interpolating
an external string directly into `with_template` — the simpler, safer
template is just `"{spinner} querying..."` with no wiki-id interpolation;
prefer that unless a quick indicatif-docs check confirms
`pb.set_message(wiki_id.clone())` + `"{spinner} {msg}"` is trivial, in which
case use that instead. Either is acceptable; exact wording is not
contract-tested.)

`is_terminal()` gates the check on **stderr**, not stdin (which `cli.rs`
already checks elsewhere for a different purpose, `src/cli.rs:489`) — the
spinner's own draw target. `finish_and_clear()` runs unconditionally right
after the blocking call, before either failure or success branches touch
stdout/stderr, satisfying AC2's "cleared before output" for both outcomes.

### 3.3 Why no unit test forces `is_terminal() == true`

`assert_cmd::Command` (used throughout `tests/cli_contract.rs`) always
captures stdout/stderr through OS pipes, never a pty, so
`stderr.is_terminal()` is deterministically `false` in every existing and
new CLI-level test — the spinner branch is simply never entered by any
`assert_cmd` invocation. This is not a test gap to work around; it is
exactly the behavior AC2 requires ("Piped/redirected/CI runs produce
byte-identical stdout... and no spinner bytes anywhere") and it is provable
for free: every existing `tests/cli_contract.rs` success/failure assertion
on `stdout`/`stderr` content continues to hold unchanged specifically
*because* the spinner code path is unreachable under `assert_cmd`. Add one
explicit assertion (not previously present) that reinforces this rather than
leaving it implicit: in the Windows fixture's
`live_doctor_then_query_succeed_end_to_end_with_schema_absent_warning`
human-mode sub-test (`tests/cli_contract.rs:1492-1514`), assert
`human_assert.get_output().stderr.is_empty()` alongside the existing stdout
ordering assertions.

## 4. Feature 4 — provider argv additions (AC4, AC5)

### 4.1 Shared directive text (`src/providers/mod.rs`)

```rust
/// Item 3's non-interactive directives (PRD 08-06-pre-0-1-0-cli-refinements,
/// D5): delivered to each provider via its own system-prompt-append
/// mechanism (Claude's `--append-system-prompt`, Codex's `-c
/// developer_instructions=`), **not** via the `constraints` array in
/// [`build_prompt`] above — that array is a separate, closed, spec-§7.1
/// contract (byte-identical across providers, tested by
/// `tests/prompt_envelope.rs`) that this task does not touch. No `"`, `\`,
/// or newline characters appear in this text: Codex's `-c` value is
/// TOML-parsed first and falls back to a raw-string literal only when TOML
/// parsing fails (research/provider-cli-flags.md §4) — a quote or backslash
/// here could parse as valid (and different) TOML instead of falling
/// through to the intended literal text, live-verified only for the
/// quote/backslash-free case.
pub const NON_INTERACTIVE_SYSTEM_DIRECTIVES: &str = "Answer directly and stop. Do not ask whether to save the answer. Do not update the wiki, its index, frontmatter, or any log. Do not ask a follow-up question; this is a non-interactive CLI with no one present to answer it.";
```

### 4.2 Claude (`src/providers/claude.rs`)

Add, to `build_argv`'s base `args` vec, immediately after the existing
`OsString::from(DISABLE_ALL_HOOKS_SETTINGS)` element and before the
`if let Some(dir) = plugin_dir` block:

```rust
OsString::from("--append-system-prompt"),
OsString::from(NON_INTERACTIVE_SYSTEM_DIRECTIVES),
OsString::from("--setting-sources"),
OsString::from("project"),
```

Import `NON_INTERACTIVE_SYSTEM_DIRECTIVES` via the existing
`use super::{...}` block (`src/providers/claude.rs:14-17`).

**R-34** (next sequential R-number after R-33, confirmed as the current
maximum via `grep -rn "R-3[0-9]" src/`): this is a *deliberate, narrower*
reversal of the "`--setting-sources` is not used here at all any more"
framing left over from R-27/R-28
(`src/providers/claude.rs:69-73`'s doc comment, and the near-identical spec
prose at `docs/2026-07-28-llm-wikis-external-query-design.md:601`). R-27's
original `--setting-sources user` excluded the `project` source and broke
skill discovery (R-28); this task's `--setting-sources project` is the
opposite exclusion (drops `user`/`local`, keeps `project`) and was
live-verified compatible with `--settings {"disableAllHooks":true}` and
successful `/echotest` skill expansion, zero `permission_denials`
(research/provider-cli-flags.md §2, §3 first test). Update both doc comments
(`src/providers/claude.rs:69-73` and the spec's §10.2 R-27/R-28 paragraph,
`docs/2026-07-28-llm-wikis-external-query-design.md:601`) to state the R-34
addition plainly rather than leaving the old "not used" sentence
contradicted by the new argv — this is exactly the kind of doc/code drift
`tests/spec_drift.rs`'s existence is meant to prevent catching *silently*,
even though this specific sentence isn't one of the three mechanically
checked tables.

### 4.3 Codex (`src/providers/codex.rs`)

Add, to `build_argv`'s returned vec, immediately after the existing
`OsString::from("mcp_servers={}")` element and before
`OsString::from("--disable")`:

```rust
OsString::from("-c"),
OsString::from(format!("developer_instructions={NON_INTERACTIVE_SYSTEM_DIRECTIVES}")),
```

Grouping both `-c` overrides adjacently (`mcp_servers={}` then
`developer_instructions=...`) is a readability choice, not a functional
requirement — `codex exec`'s `-c` flag is order-independent among
overrides. Import the constant the same way as Claude's adapter.

### 4.4 Test fallout (enumerated exactly)

**`tests/claude_adapter.rs`**:
- `exact_argv` (`tests/claude_adapter.rs:138-175`): append the four new
  `OsString` elements to `expected`, in the position specified in §4.2,
  before the final `assert_eq!`.
- `hook_neutralization_settings_flag_present_without_excluding_setting_sources`
  (`tests/claude_adapter.rs:204-238`): the `schema_pos+2`/`schema_pos+3`
  assertions (settings pair directly follows `--json-schema` pair) are
  unaffected and stay. Replace the
  `assert!(!args.iter().any(|a| a == "--setting-sources"), ...)` assertion
  (now false) with the opposite: assert `--setting-sources` is present with
  value `"project"`, and that it (and `--append-system-prompt`) appear after
  the `--settings` pair and before `--plugin-dir` when one is present
  (extend the existing `settings_pos < plugin_pos` check at the bottom of
  the test to a `setting_sources_pos < plugin_pos` check too). Rewrite the
  test's doc comment to describe R-34 rather than asserting the flag's
  permanent absence.
- `capability_exclusion`, `tool_restriction`, `no_session_persistence`,
  `plugin_dir_flag`, `json_schema_inline`, `no_prompt_in_argv`: no change —
  none of the new tokens (`--append-system-prompt`, the directive text,
  `--setting-sources`, `project`) collide with any forbidden-token check or
  positional assumption those tests make. Spot check `no_prompt_in_argv`
  specifically: it asserts the argv contains neither `/wiki-query` nor "Use
  the wiki-query skill" — the directive text contains neither substring.

**`tests/codex_adapter.rs`**:
- `exact_argv` (`tests/codex_adapter.rs:153-181`): insert the two new
  `OsString` elements per §4.3 into `expected`.
- `capability_exclusion` (`tests/codex_adapter.rs:221-253`): the
  `args.windows(2).any(|w| w[0] == "-c" && w[1] == "mcp_servers={}")` check
  is adjacency-based, not position-based, and stays true regardless of what
  follows. No change needed, but add a parallel
  `args.windows(2).any(|w| w[0] == "-c" && w[1].to_string_lossy().starts_with("developer_instructions="))`
  assertion here for symmetry/coverage.
- `flag_order`, `sandbox_flag`, `no_add_dir`, `no_session_persistence`,
  `no_prompt_in_argv`, `installed_plugin_fails_closed`: no change.

**`tests/spec_drift.rs`**: no change (§0 above).

**`tests/prompt_envelope.rs`**: no change — `constraints_for`/`CONSTRAINT_*`
are untouched (§4.1).

### 4.5 Spec doc updates (manual, Hard Gate G14)

`docs/2026-07-28-llm-wikis-external-query-design.md`:
- §5.1 command grammar block (`docs/2026-07-28-llm-wikis-external-query-design.md:127-133`):
  add `llm-wikis [--config <absolute-path>] [--json] config show` and
  `... config validate` lines.
- §5.1 prose (`docs/2026-07-28-llm-wikis-external-query-design.md:137-139`):
  add a paragraph for `config show`/`config validate` mirroring the existing
  `config init` paragraph — envelope shape, exit-code mapping, "without side
  effects" for `validate`.
- §5.1's "Diagnostics go to stderr" sentence
  (`docs/2026-07-28-llm-wikis-external-query-design.md:147`): extend to
  state explicitly that the human-mode error line itself (not just child
  diagnostics) is stderr-only as of this task, for every subcommand.
- §10.2 target invocation block
  (`docs/2026-07-28-llm-wikis-external-query-design.md:562-575`): add the
  `--append-system-prompt <directives>` and `--setting-sources project`
  lines in argv order.
- §10.2 R-27/R-28 paragraph
  (`docs/2026-07-28-llm-wikis-external-query-design.md:601`): correct per
  §4.2 above (R-34).
- §10.3 target invocation block
  (`docs/2026-07-28-llm-wikis-external-query-design.md:617-631`): add the
  `-c developer_instructions=<directives>` line.

`docs/llm-wikis.md` (operator guide): update the §3.1 command grammar block
(`docs/llm-wikis.md:383-389`) the same way as spec §5.1, and confirm/adjust
the existing "with diagnostics on stderr" sentence
(`docs/llm-wikis.md:426`) now that it is literally true rather than
aspirational.
