# Round 2 evaluation — 08-07-first-run-config-and-query-ux-fixes

Light task, no checklist.md. Round 2 delivers exactly one change: prd.md
D8 (doctor-scoped usage hints for multi-agent `--agent` attempts), per
this round's dispatch. Touched files this round: `src/cli.rs`,
`tests/cli_contract.rs` (plus `prd.md`/`task.json` admin updates — not
code, not in Expected Files scope).

## gates.ts

Two invocations were run (`TRESTLE_CONTEXT_ID` set both times, `--round 2`):

1. First invocation used the default shell PATH (broken `cargo` shim,
   AGENTS.md:29-31) — `verify-1`/`verify-2`/`verify-3` all report
   `exit 4294967295` (Windows ENOENT signature), same environment
   artifact round 1 documented. Saved as
   `evidence/gates-round-2.1.json`.
2. Second invocation, `/c/Users/gn006/.cargo/bin` prepended to PATH,
   launched in the background because `verify-3` (`cargo test
   --all-targets --all-features -- --test-threads=1`) takes ~12-16 min
   (AGENTS.md:23, `tests/process_supervisor.rs`). Its result was not
   back by the time this evidence was finalized, but is immaterial to
   the verdict below since two other gates (`AC6`, `trace-audit`) already
   force `block` regardless of `verify-1/2/3`'s outcome, and this task's
   own manual reproduction of every `Verification Plan` command (below)
   already gives an authoritative answer independent of gates.ts's own
   subprocess wrapper.

Raw `gates.ts` output (`evidence/gates-round-2.json`, the broken-PATH
capture — content identical for AC1-AC6/scope/trace-audit regardless of
PATH, since those gates don't shell out to `cargo`):

```
verdict: block
gateCount: 11, failingCount: 4
AC1  ok
AC2  ok
AC3  ok
AC4  ok
AC5  ok
AC6  fail (false-fail, see below)
verify-1  fail (env/PATH artifact in gates.ts's own subprocess, see below)
verify-2  fail (env/PATH artifact, see below)
verify-3  fail (env/PATH artifact, see below)
scope-src/cli.rs           ok
scope-tests/cli_contract.rs ok
trace-audit  fail (real, but unrelated to this diff, see below)
```

## D8 verification (this round's actual change)

### Mechanism read (src/cli.rs)

`handle_parse_error` now calls `hint_subcommand(e, args)` instead of the
round-1 `usage_line_mentions_query`. `hint_subcommand`:
1. Checks clap's own rendered `Usage:` line for ` query ` / ` doctor `
   (precise when it fires — `ArgumentConflict`, i.e. the repeated
   `--agent` case, renders one).
2. Falls back to `subcommand_from_args` — a raw-argv walk — for error
   kinds that render no `Usage:` line at all (`InvalidValue`, i.e. the
   comma-separated `--agent claude,codex` case; confirmed in the code's
   own doc comment that clap's rendered message is byte-identical between
   `query`/`doctor` for this shape, carrying no subcommand information).

`subcommand_from_args` (src/cli.rs:261-279) walks argv skipping `--config`
and its one following value (the only global flag besides the boolean
`--json` — confirmed against `Cli`'s field list, src/cli.rs:46-59), and
returns the first bare token if it is literally `"doctor"` or `"query"`,
else `None`.

### Adversarial probing for false positives (done live against the release binary)

All of the following were run against `./target/release/llm-wikis.exe`
(built from this round's diff) to hunt for the fallback misfiring on a
subcommand it didn't actually parse:

| Invocation | Result | Verdict |
|---|---|---|
| `--json doctor --wiki demo --agent claude,codex --live` (global flag before subcommand) | doctor hint attached | correct |
| `--json --config /tmp/doctor_pathtest/doctor/x.toml list --agent claude,codex` (config **path value contains the literal word "doctor"**) | `unexpected argument '--agent' found`, **no hint** | correct — proves the `--config`-value skip works; the word "doctor" inside the config path never leaks into the walk |
| `--json list --agent claude,codex` (bare `--agent` on a subcommand that doesn't declare it) | no hint | correct |
| `--json --config doctor query --agent claude,codex -- "q"` (config value is literally the word `doctor`) | query hint (not doctor) attached | correct — the literal token immediately after `--config` is consumed as its value, not matched as a subcommand |
| `--json --agent claude,codex --config /tmp/x.toml doctor --wiki demo --live` (`--agent` given before `--config`/subcommand — clap error becomes `unexpected argument '--agent'` at the *root* parser, not `InvalidValue`) | no hint | not a false positive (under-triggers on an unrealistic argument order; doc comment explicitly scopes the fallback to the real invocation shape `[--json] [--config PATH] <subcommand> ...`) |
| `--json config validate --agent claude,codex` (config subcommand, no `--agent`) | no hint | correct |

No false-positive hint misfire was found in any probed shape, including
the specific attack the dispatch flagged (`--config` value containing the
word "doctor").

### Automated tests

`cargo test --test cli_contract --all-features -- --test-threads=1`:
**52 passed, 0 failed** (48 pre-existing + 4 new: `doctor_comma_separated_agent_value_names_the_one_pair_per_run_rule`,
`doctor_repeated_agent_flag_names_the_one_pair_per_run_rule`,
`doctor_comma_separated_agent_value_still_emits_exactly_one_json_document`,
`query_comma_separated_agent_value_gets_the_query_usage_hint_not_doctors`).

Minor inaccuracy in the round-2 dispatch summary handed to me: it states
"tests/cli_contract.rs gained 5 tests" but the diff (`git diff --stat` /
direct read) shows exactly 4 new `#[test]` functions. Not a defect —
prose miscount in the dispatch note, not a claim made by the implementer
in prd.md itself.

### D1 compliance (no semantics change)

Diffed `src/cli.rs` line-by-line: `hint_subcommand`/`subcommand_from_args`/
`usage_line_mentions` only ever *append* text to an already-constructed
`message: String` that still becomes `ErrorCode::ArgumentInvalid` exactly
as before; no change to `CliCommand::Doctor`'s `agent: Option<AgentArg>`
declaration (still not repeatable → `ArgumentConflict` on repeat, still a
plain `ValueEnum` → `InvalidValue` on a comma-joined value), no change to
`--live`'s own requirement that both `--wiki` and `--agent` be present
(`run_doctor_command`, untouched by this diff). Exit code stays 2
(`ArgumentInvalid::exit_code()`, untouched) in every test above. Confirms
prd.md D8's own claim ("the one-pair-per-run semantics themselves stay
exactly as specified... no semantics change").

## Manual full Verification Plan (authoritative, PATH fixed directly in-shell)

- `cargo fmt --all --check` → exit 0, no output.
- `cargo clippy --all-targets --all-features -- -D warnings` → exit 0,
  "Finished" with zero warnings.
- Every `tests/*.rs` binary run individually with `--test-threads=1`,
  **all green**: citations(22), claude_adapter(22), codex_adapter(21),
  config_contract(84), config_init(8), doctor(31), error_contract(13),
  list(7), model_contract(9), mutation_snapshot(20), output_contract(7),
  probes(18), prompt_envelope(6), query_service(20), spec_drift(3),
  version_cli(1), wiki_preflight(12), **cli_contract(52)** — 356 tests
  total, 0 failed. `cargo test --lib --bins --all-features` → 4 passed
  (lib), 0 (bin). `tests/process_supervisor.rs` skipped: zero-diff this
  round (`git diff --stat -- tests/process_supervisor.rs` empty), same
  standing exemption round 1 used (AGENTS.md:18-23's documented flake).

This satisfies AC6/the Verification Plan's real intent exactly as round
1's evidence established.

## AC6 gates.ts false-fail (recurs from round 1, unchanged root cause)

Same as round 1: `gates.ts`'s evidence-path checker treats the prose
phrase `` `tests/*.rs` `` inside AC6's `(evidence: ...)` text as a literal
path and fails because it doesn't `existsSync`. This is prd.md's own
prose glob-shorthand, not a diff defect — unchanged from round 1's
documented finding (`evidence/round-1.md` lines 39-54). prd.md was not
touched in this respect between rounds (only the D8 paragraph was added),
so this false-fail persists identically.

## verify-1/2/3 gates.ts false-fail (recurs from round 1)

Same root cause as round 1: gates.ts's own subprocess inherits whatever
PATH the harness's default shell has, and in this environment that
resolves `cargo` to a broken chocolatey shim unless
`/c/Users/gn006/.cargo/bin` is prepended (AGENTS.md:29-31). With the
correct PATH, the same three commands were run directly (not through
gates.ts's wrapper) and all pass/behave exactly as this evidence's
"Manual full Verification Plan" section shows. Not a diff defect.

## trace-audit gate (real violation, but not attributable to this diff)

`trace-audit` failed with two violations, both about a **different,
already-merged task**, `08-06-pre-0-1-0-cli-refinements`:

- `archive-verdict-not-gated`: that task was archived
  (`.trestle/.runtime/trace.jsonl`, `task-archive` event,
  `2026-08-07T05:19:41.639Z`) with **no** matching passing `gates-run`
  event and no `archive-verdict-waived` event. Inspected the check's own
  implementation (`trace.ts` `checkArchiveVerdictGated`, ~line 520): for
  a `task-archive` line with no `round` field it falls back to "any
  `gates-run` with `verdict === 'pass'` anywhere in the trace" — and
  confirmed by grepping the whole trace file that **no** task, in this
  entire session's history, has ever recorded a passing `gates-run`
  (08-06 rounds 1-3: `block`/`block`/`block`; 08-07 round 1: `block`;
  08-07 round 2, this evaluation: `block`). The violation is real, not a
  parsing artifact.
- `plan-drifted-without-rollback`: 08-06's round 2 `design.md`/
  `implement.md` hashes differ from round 1's with no intervening
  `round-rollback` event — again, entirely about task 08-06's own
  history, unrelated to 08-07.

Both are about a task this evaluation has no visibility into, cannot
modify (out of scope: writes are restricted to this task's own
`evidence/` directory; no git commit; no checklist.md/prd.md edits), and
cannot fix retroactively (the only sanctioned remedy per `trace.ts`'s own
doc comment is an `archive-verdict-waived` event or investigation into
why 08-06 shipped without ever passing gates — neither of which is
this round's job). `gates.ts`'s own header comment is explicit that this
folding-in is by design and has "no waive/force escape hatch (D3)" — i.e.
this is intentionally non-negotiable from inside a single evaluation
round, not a bug I can reason around.

This is flagged, not silently absorbed: it is a genuine, currently
unresolved process-integrity finding that blocks **every** `gates.ts`
run in this session (for any task, not just 08-07) until resolved
out-of-band — most likely by whoever owns task-archival/waiver tooling
investigating why `08-06-pre-0-1-0-cli-refinements` was archived despite
three consecutive `block` verdicts, and either producing a legitimate
`archive-verdict-waived` record or reopening that task. This is squarely
outside 08-07 round 2's own diff and outside what a "planner rollback"
for *this* task could address (rolling back 08-07's own plan would not
touch 08-06's trace history at all).

## Conclusion

D8's own implementation is correct, adversarially probed for the exact
false-positive class the dispatch asked about (a `--config` path value
containing the word "doctor") with no misfire found, fully tested (4 new
tests, all passing), and D1-compliant (message text only, verified
line-by-line). AC1-AC5 (round 1's work, re-verified unchanged this round)
and the Verification Plan's three commands all pass when run directly.

The verdict is nonetheless **Block**, driven by the `trace-audit` gate
(a real, currently-failing, by-design-non-waivable hard gate) plus the
recurring `AC6` prose-glob false-fail. Per this role's mandate ("Hard
gates decide Pass or Block... any hard gate failing... verdict is Block,
regardless of scores"), gates.ts's `trace-audit` failure cannot be
overridden by evaluator judgment even though its root cause sits entirely
outside this round's diff. This is a process-level blocker, not a code
defect in `src/cli.rs`/`tests/cli_contract.rs` — the parent session
should treat 08-06's unresolved archive-without-passing-gates state as
the actual action item, separately from continuing 08-07.
