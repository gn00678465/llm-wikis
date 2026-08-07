# first-run config and query UX fixes

Two first-run usability failures reported by the user during real-machine
verification (2026-08-07), after the pre-0.1.0 CLI refinements task
shipped. Part of the "fix and optimize config issues before v0.1.0" phase
goal; the user will keep verifying until 0.1.0 criteria are met.

## Findings

- User repro 1: `doctor` with a hand-edited config containing
  `content_root = "E:\not_company\..."` fails CONFIG_INVALID with a raw
  TOML parse error ("missing escaped value") — in TOML basic (double-
  quoted) strings `\` is an escape character. This is TOML-spec behavior,
  not liftable by the parser; legal spellings are single-quoted literal
  strings `'E:\path'`, doubled backslashes `"E:\\path"`, or forward
  slashes `"E:/path"` (Windows accepts them).
- Root cause of the trap: `config init`'s INIT_TEMPLATE (src/config.rs,
  const near the `config init` section) shows the example wiki paths in
  DOUBLE quotes (`"/absolute/path/to/example"`) — a Windows user who
  replaces them with backslash paths hits the escape error naturally.
  config.example.toml uses the same double-quoted style.
- User repro 2: `llm-wikis query "question"` (no `--`, no `--wiki`) fails
  with raw clap text: `error: ARGUMENT_INVALID (error: unexpected argument
  '...' found)` — no guidance toward the correct shape. The `--`
  requirement itself is deliberate (spec §5.1, clap `last = true` at
  src/cli.rs Query.question) and stays.
- Both error paths flow through src/cli.rs::handle_parse_error (clap parse
  errors → first_line of clap message) or the CONFIG_INVALID construction
  in src/config.rs::Config::load's TOML-parse error mapping.
- tests/spec_drift.rs only pins error-code/check-name/warning-code sets —
  message-text changes do not require spec §13/14/15 table changes, but
  operator-guide/spec prose that quotes exact messages must be checked.
- User repro 4: `llm-wikis config --help` leaks internal planning prose
  into user-facing help — clap derives subcommand help text from `///` doc
  comments, and the round-2 rename wrote full rationale paragraphs (PRD
  id, AC1, D7, clap-collision reasoning) on `ConfigAction::List`/
  `Validate` variants. Any clap-visible `///` comment on Cli/CliCommand/
  ConfigAction and their args becomes help output; a sweep is needed for
  other internal-reference leaks (e.g. `--config`'s doc comment cites
  "spec §5.2").
- `--json` is a global flag (`global = true`, src/cli.rs) and is
  FUNCTIONAL on every subcommand: list/doctor/config init/config list/
  config validate all render JSON envelopes, and the spec's one-JSON-
  document failure contract applies to all of them (live-verified for
  config list/validate on 2026-08-07). Its appearance in every
  subcommand's help reflects real behavior.

## Decisions

- D1: Do NOT change parsing behavior or the `--` requirement — both traps
  get better guidance, not different semantics. (User-endorsed
  recommendation, 2026-08-07.)
- D2: INIT_TEMPLATE example paths switch to single-quoted TOML literal
  strings with a comment explaining Windows path quoting (single quotes /
  doubled backslashes / forward slashes). config.example.toml gets the
  same treatment.
- D3: CONFIG_INVALID for TOML parse failures of the escape class appends a
  hint about the three legal Windows-path spellings.
- D4: The clap unexpected-argument path for `query` (and the
  missing-`--wiki` message) gains a usage hint showing
  `llm-wikis query --wiki <id> -- "question"`.
- D5: ENTRYPOINT_UNVERIFIED's message (src/query.rs::entrypoint_unverified,
  ~line 348) gains the remedial command: run
  `llm-wikis doctor --wiki <id> --agent <agent> --live` first. The live
  fingerprint gate itself stays exactly as specified (spec §8.1 step 8) —
  guidance only, no semantics change. User repro 3, 2026-08-07: static
  doctor all-pass followed by query ENTRYPOINT_UNVERIFIED with no next-step
  hint. If the selectors are in scope at the error site, include the
  actual wiki/agent values in the hint; otherwise the generic form.

- D7: `--json` stays a global flag, unchanged — it is functional on every
  subcommand and the help listing reflects real behavior; only the
  help-text leak (D6) is fixed. (User, interview 2026-08-07.)
- D8: Multi-agent doctor attempts get guidance (round 2; user repro 5,
  2026-08-07): `--agent claude,codex` (invalid value) and repeated
  `--agent` (cannot be used multiple times) both emit raw clap text. Fix:
  the doctor-scoped clap parse errors of those two shapes gain a hint that
  `doctor --live` accepts one (wiki, agent) pair per run — run it once per
  agent. Same mechanism as the D4 query hint (usage-line detection); the
  one-pair-per-run semantics themselves stay exactly as specified (live
  probes consume quota). AC: both messages carry the hint; a test pins
  each; no semantics change.
- D6: Help-text hygiene — every clap-visible doc comment becomes a short,
  user-facing one-liner (imperative, no PRD/AC/D-number/task/spec-section
  references); the rationale prose moves to regular `//` comments so the
  code keeps its decision history without leaking it to `--help`. Sweep
  ALL derive-clap items in src/cli.rs (commands, subcommands, args), not
  just `config list`/`validate`. Add a test pinning that `--help` output
  contains no internal markers (e.g. "PRD", "AC1", "D7", "spec §").

## Acceptance Criteria

- [x] AC1: `config init` template and config.example.toml show
  single-quoted example paths plus a Windows-quoting comment; existing
  tests that pin template content updated. (evidence: src/config.rs:1057-1078
  INIT_TEMPLATE; config.example.toml:17-22,43-44,65-66;
  tests/config_init.rs generated_file_example_paths_are_single_quoted_with_a_windows_quoting_note;
  tests/config_contract.rs config_example_toml_uses_single_quoted_paths_with_a_windows_quoting_note
  — `cargo test --test config_init --test config_contract -- --test-threads=1` passed)
- [x] AC2: A config whose TOML parse error is backslash-escape-related
  yields CONFIG_INVALID whose message includes the three legal spellings
  hint; other TOML errors keep their current message shape. (evidence:
  src/config.rs toml_parse_error_message; tests/config_contract.rs
  backslash_escape_toml_error_carries_the_windows_path_hint and
  non_escape_toml_errors_do_not_carry_the_windows_path_hint — `cargo test
  --test config_contract -- --test-threads=1` passed; manual smoke:
  `llm-wikis config validate` against a config with
  `project_root = "E:\not_company\wiki"` prints "... -- Windows paths in
  double-quoted TOML strings must escape `\`: use a single-quoted literal
  string ('C:\path'), double the backslashes in a double-quoted string
  (\"C:\\path\"), or use forward slashes (\"C:/path\")")
- [x] AC3: `llm-wikis query "question"` (no `--`) and `query -- "q"` (no
  `--wiki`) both emit ARGUMENT_INVALID messages that include the correct
  invocation shape `query --wiki <id> -- "<question>"`; `--json` still
  emits exactly one JSON document. (evidence: src/cli.rs handle_parse_error,
  usage_line_mentions_query, QUERY_USAGE_HINT, run_query_command;
  tests/cli_contract.rs query_without_separator_names_the_correct_invocation_shape,
  query_missing_wiki_message_names_the_correct_invocation_shape,
  query_without_separator_still_emits_exactly_one_json_document — `cargo
  test --test cli_contract -- --test-threads=1` passed)
- [x] AC4: The ENTRYPOINT_UNVERIFIED message names the remedial command
  (`doctor ... --live`); tests pinning that message updated; the gate's
  behavior itself unchanged. (evidence: src/query.rs:356-364
  entrypoint_unverified; tests/query_service.rs
  probe_gate_enforced_mode_absent_record and
  probe_gate_enforced_mode_mismatched_record — `cargo test --test
  query_service -- --test-threads=1` passed)
- [x] AC5: `--help` output for the root command and every subcommand
  (list, doctor, query, config init/list/validate) contains only short
  user-facing descriptions — no PRD/AC/D-number/task-id/spec-section
  markers; a test pins this. `--json` remains listed globally (D7).
  (evidence: src/cli.rs Cli/CliCommand/ConfigAction/AgentArg doc comments;
  tests/cli_contract.rs help_output_never_leaks_internal_planning_markers —
  `cargo test --test cli_contract -- --test-threads=1` passed)
- [x] AC6: `cargo fmt --all --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, and `cargo test --all-targets
  --all-features -- --test-threads=1` pass (process_supervisor caveat per
  AGENTS.md Working rules). (evidence: `cargo fmt --all --check` exit 0;
  `cargo clippy --all-targets --all-features -- -D warnings` exit 0; `cargo
  test --lib --bins --all-features` plus every integration-test binary
  except process_supervisor.rs run individually with `--test-threads=1`,
  all passed)

## Verification Plan

```
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

## Expected Files

- `src/config.rs`
- `src/cli.rs`
- `config.example.toml`
- `tests/config_init.rs`
- `tests/config_contract.rs`
- `tests/cli_contract.rs`
- `docs/llm-wikis.md`
- `src/query.rs`
- `tests/query_service.rs`
