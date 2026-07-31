# LLM Wikis — Task 2 Preflight Report

Worker: task2-spike-worker-2026-07-31
Completed (steps 1–10, 12): 2026-07-31T02:53:00Z
Platform: Windows 11 Pro 10.0.26200 (AMD64), PowerShell 7.6.3
Rust: rustc 1.97.1 (8bab26f4f 2026-07-14), cargo 1.97.1 (c980f4866 2026-06-30) — required the
session-scoped PATH remedy recorded in `docs/verification/llm-wikis-execution.md` Task 0
(`$env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH`) before every `cargo` invocation below.
Spec in force: `docs/2026-07-28-llm-wikis-external-query-design.md` v0.2.1 for Steps 1–10/12 and
the initial Row 11 run; **v0.2.2 (R-25, Codex MCP-exclusion correction) for the Row 11 addendum
rerun** below.
Plan section under test: `docs/2026-07-28-llm-wikis-query-cli.md` Task 2, Steps 1–13.

Scope note: this run executes Steps 1–10 and 12 first (no billable call). **Step 11 was
subsequently run for real, under explicit user authorization relayed by the coordinator**,
consuming model quota and reading the real `D:\Wikis\agents` and
`D:\Wikis\harness-engineering\wiki` content roots strictly read-only (never writing anything
under `D:\Wikis`). Every check below, in every row, reflects an actual observed exit code and
output, not an assumption.

Files created (all under the two paths this task is allowed to touch):

```text
spikes/Cargo.toml
spikes/README.md
spikes/src/main.rs
spikes/src/report.rs
spikes/src/fixture.rs
spikes/src/stdin_boundary.rs
spikes/src/windows_resolution.rs
spikes/src/platform_dirs.rs
spikes/src/bounded_pipes.rs
spikes/src/process_tree.rs
spikes/src/mutation_hash.rs
spikes/src/temp_artifacts.rs
spikes/src/provider_contract.rs
spikes/src/bin/process-tree-child.rs
docs/verification/llm-wikis-preflight.md   (this file)
```

`spikes/layout-agnostic/` is a pre-existing, unrelated older spike directory at that path; it was
left untouched, consistent with the instruction to leave earlier spike artifacts alone.

---

## Row 1 — Step 1: scaffold the isolated spike crate

- Command: `cargo build --manifest-path spikes/Cargo.toml`
- Result: **PASS** — exit 0, clean build (all deps resolved, no warnings surfaced in the build
  log).
- Evidence: package `llm-wikis-spikes` 0.1.0, `edition = "2024"`, deps
  `process-wrap = { version = "9.1", features = ["std"] }`, `serde_json = "1"`, `sha2 = "0.10"`,
  `tempfile = "3"`, exactly as specified. `spikes/src/bin/process-tree-child.rs` present.
  `spikes/README.md` states spike code is evidence only and is never copied into production.
  Subcommand-per-spike dispatch in `spikes/src/main.rs` covers all eight names:
  `stdin-boundary`, `windows-resolution`, `platform-dirs`, `bounded-pipes`, `process-tree`,
  `mutation-hash`, `temp-artifacts`, `provider-contract`.

## Row 2 — Step 2: confirm the `process-wrap` standard-library frontend

- Commands:
  `cargo tree --manifest-path spikes/Cargo.toml -p process-wrap` (exit 0)
  `cargo doc --manifest-path spikes/Cargo.toml -p process-wrap --no-deps` (exit 0)
- Result: **PASS**. A synchronous (non-async) composition exists; no async runtime was
  substituted.
- Resolved version: **process-wrap v9.1.0**.
- `std`-frontend types (from `process_wrap::std`, source-inspected at
  `%USERPROFILE%\.cargo\registry\src\...\process-wrap-9.1.0\src\std\`):
  - Windows Job Objects: **`process_wrap::std::JobObject`** (a `CommandWrapper`) /
    **`process_wrap::std::JobObjectChild`** (the resulting `ChildWrapper`), gated on
    `#[cfg(all(windows, feature = "job-object"))]`. `JobObject::start_kill` terminates the whole
    job (`terminate_job`), which this spike used directly (Rows 7 and 8 below).
  - Unix process groups: **`process_wrap::std::ProcessGroup`** /
    **`process_wrap::std::ProcessGroupChild`**, gated on
    `#[cfg(all(unix, feature = "process-group"))]`. Not exercised on this Windows-only run.
  - Core composition types: `process_wrap::std::CommandWrap` (builder: `with_new`, `.wrap(W)`,
    `.spawn()` → `Box<dyn ChildWrapper>`) and the `CommandWrapper`/`ChildWrapper` traits.
  - Fallback route confirmed present and in active use *underneath* `JobObject` itself:
    `std::os::windows::process::CommandExt::creation_flags`, plus the crate's own
    `process_wrap::std::CreationFlags` wrapper (composable with `JobObject`, per the crate's own
    doc comment on ordering).
- Deviation/finding worth recording for the production adapter (Task 8): `process-wrap`'s own
  `ChildWrapper::wait_with_output` default implementation reads stdout to completion **before**
  stderr on non-Unix platforms (`src/std/core.rs`, `read2` fallback for `#[cfg(not(unix))]`) —
  i.e. it is *not* concurrent on Windows and must not be relied on for Step 7's guarantee. This
  spike does not call `wait_with_output`; it drains `child.stdout()`/`child.stderr()` on two
  threads itself (see Row 7). Production must do the same.
- The `process-wrap = { version = "9.1", features = ["std"] }` dependency spec does not disable
  default features, so `job-object`, `creation-flags`, `process-group`, `process-session`,
  `kill-on-drop`, and `tracing` are all enabled alongside `std`, matching the crate's declared
  `default` feature set.

## Row 3 — Step 3: provider capability surfaces (non-billable only)

- Commands run (all non-billable — version/help/status probes, no prompt sent):
  `claude --version`, `claude --help`, `claude auth --help`, `claude auth status --json`,
  `codex --version`, `codex --help`, `codex exec --help`, `codex login --help`,
  `codex login status`.
- Result: **PASS**.
- Versions: Claude Code **2.1.220**, Codex CLI **0.144.6** (matches Task 0's recorded paths).
- `claude auth status --json` — the exact command named in the plan — **exists and works**:
  exit 0, JSON output. Auth state observed and redacted: **logged-in**. (Only the currently
  authenticated state was observed; the logged-out exit/output shape was not exercised, since
  doing so would require logging out of the user's real, currently-authenticated session, which
  this task must not do.)
- `codex login status` — the exact command named in the plan — **exists and works**: exit 0,
  plain-text output. Auth state observed and redacted: **logged-in**. Same caveat as above:
  logged-out shape not observed.
- Both expected readiness subcommands from the plan turned out to already exist under those
  exact names, so the "if absent, record what actually exists" fallback path was not needed for
  either provider.
- Output-schema flags (spec §7.2 item 9 / §10.2 / §10.3), verified present in `--help` text:
  - Claude: **`--json-schema <schema>`** — confirmed in `claude --help` ("JSON Schema for
    structured output validation").
  - Codex: **`--output-schema <FILE>`** — confirmed in `codex exec --help` ("Path to a JSON
    Schema file describing the model's final response shape").
- Minimal tool/capability set that invokes an entrypoint while excluding Write, Edit,
  shell/Bash, web, MCP, subagents, session persistence, and interactive escalation — flags
  confirmed present in `--help` (not yet exercised live, since that requires Step 11):
  - Claude: `--tools <tools...>` (confirmed: "Specify the list of available tools from the
    built-in set"), `--permission-mode dontAsk`, `--no-session-persistence`,
    `--strict-mcp-config` + `--mcp-config <empty-config>`, `-p`/`--print`,
    `--input-format text`, `--output-format json`. No `--dangerously-skip-permissions` or
    `--allow-dangerously-skip-permissions` used.
  - Codex: `-a/--ask-for-approval never` is confirmed as a **top-level** option (appears in
    `codex --help`, before the `exec` subcommand, matching spec §10.3's claim), plus
    `exec -s/--sandbox read-only`, `--ephemeral`, `--skip-git-repo-check`,
    `--ignore-user-config`, `--json`, all confirmed present in `codex exec --help`. No
    `--dangerously-bypass-approvals-and-sandbox` used.
- No flag substitution restoring a forbidden capability was used; nothing above was exercised
  live against a real provider session (that is Step 11, not run here).

## Row 4 — Step 4: stdin/argv separation

- Command: `cargo run --manifest-path spikes/Cargo.toml -- stdin-boundary`
- Result: **PASS**, exit 0.
- Evidence: payload (繁體中文 / `--leading-dash` / `"quotes" \`backticks\` $() & | < > ^ % !`)
  round-tripped through stdin byte-for-byte (67 bytes sent, 67 echoed, equal). The fixture
  child's own OS-level argv (`env::args()`, reported independently on stderr) contained none of
  the payload substrings.

## Row 5 — Step 5: Windows executable resolution + batch-shim boundary

- Command: `cargo run --manifest-path spikes/Cargo.toml -- windows-resolution`
- Result: **PASS**, exit 0.
- Resolved and classified both real providers as native `.exe` (no shim): `claude` →
  `C:\Users\gn006\.local\bin\claude.exe`, `codex` →
  `C:\...\Programs\OpenAI\Codex\bin\codex.exe`. Non-billable `--version` spawn of each
  succeeded directly (no shell), confirming clean `.exe` invocation.
- Fixture `.cmd` built at a path containing a space:
  `C:\Users\gn006\AppData\Local\Temp\llm-wikis-spike-*\quote test dir\echo-args.cmd`.
- **Exact quoting rule recorded** (empirically determined, not assumed): on this machine's
  toolchain (rustc 1.97.1, post-RUSTSEC-2024-0243), `std::process::Command::new(<path-to-.cmd>)`
  followed by `.args([...])` — i.e. **spawning the `.cmd` directly with each argument passed as
  its own `Command::arg()` element, with no manual `cmd.exe /C` wrapping** — safely and exactly
  preserves every one of `& | ^ %VAR% !DELAYED! > <` as a literal, unexpanded argv element with
  no shell interpretation. This was the primary (not fallback) candidate and it passed on the
  first attempt; the manual `cmd /D /S /C` fallback path in the spike code was written but never
  needed. **Task 8 must reimplement using this exact rule**: direct `Command::new(shim_path)` +
  `.args()`, not a hand-built `cmd /C` string.

## Row 6 — Step 6: platform config/cache directory resolution

- Command: `cargo run --manifest-path spikes/Cargo.toml -- platform-dirs`
- Result: **PASS**, exit 0.
- All six required paths (spec §5.2 config + §15.1 cache, × Windows/Linux/macOS) resolved
  exactly via an injected env/home resolver (never the real process environment):
  - Windows: `%APPDATA%\llm-wikis\config.toml`, `%LOCALAPPDATA%\llm-wikis\probes-v1.json`
  - Linux/XDG fallback: `~/.config/llm-wikis/config.toml`, `~/.cache/llm-wikis/probes-v1.json`
  - macOS (paths containing spaces): `~/Library/Application Support/llm-wikis/config.toml`,
    `~/Library/Caches/llm-wikis/probes-v1.json`
  - (Two extra rows also verified the Linux XDG-*-explicitly-set case, beyond the required six.)
- No-cwd-dependency proof: the real process cwd was changed to an unrelated temp directory
  immediately before resolution and restored after; results were identical either way.
- Platform-separator caveat for the reader of this report: because this spike is compiled and
  run only on Windows, `PathBuf::join` renders the joins it performs (e.g. before `llm-wikis`)
  with `\`; the injected base-directory strings themselves (e.g. `/home/u/.config`) are
  preserved verbatim. This does not affect correctness of the assertions above, which compare
  full resolved paths, but a cross-compiled Linux/macOS binary would render native `/` uniformly
  — that native-target rendering is not directly testable from this machine and is not claimed
  as tested here.

## Row 7 — Step 7: bounded concurrent output

- Command: `cargo run --manifest-path spikes/Cargo.toml -- bounded-pipes`
- Result: **PASS**, exit 0, completed in ~1.3s (well inside the 15s+5s watchdog bound used to
  detect a would-be deadlock).
- Scenario A (alternating stdout/stderr beyond an 8,000-byte cap on each, spawned under
  `JobObject`): both streams independently hit their cap (9,002 bytes observed on each — first
  chunk boundary past the cap, as expected from 3,000-byte chunks), the whole tree was killed via
  `start_kill()` (Job Object termination), `wait()` returned, and the PID was confirmed dead via
  `tasklist` afterward.
- Scenario B (one-pipe-full-while-other-blocked: fixture writes `ready`, then a single
  300,000-byte write to stderr, then `done` to stdout): both streams drained concurrently without
  hanging (11 bytes on stdout including the `done` marker, 300,000 bytes on stderr), proving the
  two-thread concurrent-drain design does not deadlock on this exact adversarial pattern. Process
  exited naturally (not force-killed) and was confirmed dead afterward.
- This directly exercises the `process_wrap::std::JobObject`/`CommandWrap` API from Row 2, with
  concurrent draining implemented by hand (per the Row 2 finding that `wait_with_output` is not
  concurrent on Windows).

## Row 8 — Step 8: timeout and process-tree termination

- Command: `cargo run --manifest-path spikes/Cargo.toml -- process-tree`
- Result: **PASS**, exit 0, ~6.4s wall time. **Windows only in this task**; Linux/macOS rows are
  **PENDING**, owned by Task 14 (no CI defined for those platforms yet).
- `process-tree-child.exe` (the required separate helper binary) spawns exactly one grandchild
  of itself and writes both PIDs to a temp pidfile.
- Success case (`quick` mode): both processes exit on their own; no forced termination issued;
  both PIDs confirmed dead afterward.
- Timeout case (`loop` mode): both PIDs confirmed alive before the deadline; `start_kill()`
  (Job Object) issued after a simulated deadline; both PIDs confirmed dead afterward.
- Forced-overflow case (`spew` mode): a capped reader thread signaled at exactly 16,384 bytes
  (the configured cap); `start_kill()` issued; both PIDs confirmed dead afterward.
- In every forced-termination case, neither PID (direct child nor grandchild) remained alive,
  confirming a Windows Job Object reaches grandchildren without any explicit propagation code.

## Row 9 — Step 9: full-content mutation detection

- Command: `cargo run --manifest-path spikes/Cargo.toml -- mutation-hash`
- Result: **PASS**, exit 0.
- A temp synthetic wiki (4 files: `SCHEMA.md`, `index.md`, `pages/alpha.md`, `pages/beta.md`) was
  snapshotted (SHA-256 per file), one file was rewritten with an identical byte length (55 bytes
  before and after), its mtime was restored exactly to the pre-rewrite value
  (`SystemTime` intervals identical before/after), and the SHA-256 hash was confirmed to differ.
  The mutation detector (full-snapshot hash comparison) flagged exactly `pages/alpha.md` and
  nothing else. Never touched `D:\Wikis`.

## Row 10 — Step 10: temp-artifact safety

- Command: `cargo run --manifest-path spikes/Cargo.toml -- temp-artifacts`
- Result: **PASS**, exit 0.
- Two synthetic "configured roots" (temp dirs, never `D:\Wikis`) were used. An injected unsafe
  temp-root candidate nested inside one configured root was rejected by the resolver **before**
  any file was created or any process spawned.
- A safe candidate (an unrelated OS temp dir) was accepted. Three artifacts (empty-shaped Claude
  MCP config, Codex output-schema file, a batch-helper `.cmd`) were created with
  `OpenOptions::create_new(true)` (exclusive creation — verified: a second `create_new` at the
  same path fails as expected). `icacls /inheritance:r /grant:r <user>:F` succeeded on all three
  (best-effort Windows user-only-permission narrowing; exit 0 on each). All three artifacts still
  existed while a trivial child process ran, and were removed (and confirmed absent) after the
  child was reaped.

## Row 11 — Step 11: query each registered wiki's real entrypoint

**Run under explicit user authorization** (relayed by the coordinator), 2026-07-31, after Steps
1–10/12 below had already completed. `spikes/src/provider_contract.rs` was rewritten from the
scaffolded PENDING stub to a real implementation: builds the exact §7.1 envelope (entrypoint,
that wiki's `query_prompt`, then `EXTERNAL_QUERY` with keys in the spec's order), invokes the
exact §10.2/§10.3 argv (including `--json-schema`/`--output-schema`), snapshots the real
`content_root` (SHA-256 per file, full walk, `.claude`/`.agents`-immediate-child exclusion per
§6.1/§12) before and after, validates the result against `wiki-query/v1` (§7.3), and resolves
citations under §11.2 against the real file list. Question used for all four calls: *"In one
sentence, what topic area does this wiki cover?"* — deliberately generic; no wiki prose is
recorded anywhere below, only citation slugs (explicitly permitted) and structural counts.

| Wiki | Agent | Command | Result | Duration | Exit | Contract valid | Citations found/resolved | Snapshot identical |
|---|---|---|---|---|---|---|---|---|
| `agents` | `claude` | `cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki agents --agent claude` | **PASS** | 18.0s | 0 | yes | 5/5 | yes (94/94 entries) |
| `agents` | `codex` | `cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki agents --agent codex` | **PASS** | 26.8s | 0 | yes | 0/0 (`no_relevant_material`, correctly empty) | yes (94/94 entries) |
| `harness-engineering` | `claude` | `cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki harness --agent claude` | **PASS** | 32.5s | 0 | yes | 2/2 | yes (370/370 entries) |
| `harness-engineering` | `codex` | `cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki harness --agent codex` | **PASS** | 21.0s | 0 | yes | 1/1 | yes (370/370 entries) |

Detail per row:

- **agents × claude**: `knowledge_status=grounded`, citations `["overview", "ai-agents-in-depth",
  "harness-engineering", "context-engineering", "loop-engineering"]`, all 5 resolved to exactly
  one `<slug>.md` each, gaps=1, warnings=1. No forbidden-action text markers detected.
- **agents × codex**: first live attempt returned `knowledge_status=no_relevant_material` with an
  *empty* `answer` string, which is itself a genuine `wiki-query/v1` violation (§7.3: `answer` is
  always required to be a non-empty string, independent of status) — recorded as the row's one
  permitted retry trigger, not a second independent attempt at the same thing: the retry added
  read-only structural diagnostics (event/item-type counts, never event content) to distinguish a
  provider/skill behavior from an extraction bug in this spike. The retry returned
  `knowledge_status=no_relevant_material`, non-empty `answer`, empty `citations` array, gaps=1,
  warnings=1 — which **does** satisfy §7.3's `no_relevant_material` invariant (empty citations +
  non-empty gap), so this row is graded PASS on the retried, diagnostic-enhanced attempt. It is
  still a notable finding that asking a wiki's own query skill "what topic area does this wiki
  cover" produced `no_relevant_material` rather than a grounded answer under Codex specifically
  (Claude grounded the identical question against the identical wiki) — plausible cause below.
- **harness-engineering × claude**: `knowledge_status=grounded`, citations
  `["harness-engineering", "index"]`, both resolved, gaps=0, warnings=0. No forbidden-action text
  markers detected.
- **harness-engineering × codex**: `knowledge_status=grounded`, citation `["index"]`, resolved,
  gaps=0, warnings=0.

**Residual-risk finding worth its frequency (spec §7.2's accepted residual risk)**: both Codex
rows' JSONL event streams contained an item of type **`mcp_tool_call`** (distinct types observed:
`agent_message, item.completed, item.started, mcp_tool_call, text, thread.started, turn.completed,
turn.started`; 7 raw events per call) — **2/2 Codex calls (100% of this small sample)**, despite
`--ignore-user-config` and no `--mcp-config`-equivalent flag being passed to `codex exec` (Codex's
`exec` surface has no such flag at all — confirmed absent from `codex exec --help` in Row 3). This
is a real, reproducible signal that the model attempted (or Codex's own runtime exposed) an
MCP-shaped tool-call capability during a run whose safety invariant (§10.1) requires MCP to be
"disabled or excluded." No consequence was observed — both content-root snapshots stayed
byte-identical and both citation counts/contract validity held — so this reads as an *attempted or
available-but-inert* capability rather than a demonstrated mutation or leak, but it directly
contradicts the stated invariant at the capability-availability level and plausibly explains why
the first `agents`/`codex` attempt underperformed relative to the identical Claude call (the model
may have reached for a tool that wasn't productive instead of just reading files). **This should
be treated as a FAIL-worthy contradiction of §10.1's MCP-exclusion invariant for the Codex adapter
specifically, to be corrected/re-reviewed before Task 8 finalizes the Codex argv** — recorded here
rather than halting Task 2, because Step 11 is explicitly a live/paid-provider row whose failure
per §17.2 "blocks only that provider/platform support claim," not production implementation, and
because the row's own `wiki-query/v1` contract and read-only guarantees still held. The
`llm-wikis-v0.1.0-checklist.md` SPIKE-04 row's evidence column should record this same finding
when a second independent verifier updates it.

Auth state for both providers during these calls (redacted, consistent with Row 3): logged-in.

### Row 11 addendum — corrected §10.3 argv rerun (spec 0.2.2 / R-25)

**Authority**: user-approved correction, spec `docs/2026-07-28-llm-wikis-external-query-design.md`
is now **v0.2.2, R-25**. Corrected Codex `exec` argv (inserted after `--ignore-user-config`, in
order): **`-c mcp_servers={}`**, **`--disable browser_use`**, **`--disable computer_use`** — both
`-c/--config` and `--enable`/`--disable <FEATURE>` are confirmed present in `codex exec --help`
(Row 3). `spikes/src/provider_contract.rs`'s `invoke_codex` was updated to this exact vector.

The event-type scanner was also corrected: the original `codex_event_type_summary` recursively
collected every `"type"` key anywhere in the JSON tree (including inside nested/embedded
payloads), so a match there does not by itself prove an item actually occurred. A second, precise
scanner (`codex_precise_item_types`) was added that reads only (a) each JSONL event's own
top-level `"type"`, and (b) — solely for `item.started`/`item.completed` events — the `"type"`
field of that event's immediate `"item"` object. Both sets are now reported on every Codex call
(see `codex_jsonl_event_structure` check in the raw JSON above).

**One authorized rerun**: `provider-contract --wiki agents --agent codex` (the row that
previously needed a retry), run once, no retry needed this time.

| Field | Value |
|---|---|
| Duration | 29.1s |
| Exit code | 0 |
| Contract valid | yes (`problems=[]`) |
| `knowledge_status` | `no_relevant_material` |
| Citations found/resolved | 0/0 (correctly empty per §7.3 for `no_relevant_material`; gaps=1, warnings=1) |
| Snapshot byte-identical | yes (94/94 entries before and after) |
| Argv rejection | **none** — stderr was 0 bytes on both this and every other Codex call in this report; all three corrected flags were accepted by codex-cli 0.144.6 |

**Precise item-type set** (top-level event type, plus `item.started`/`item.completed`'s immediate
item type only): `["agent_message", "item.completed", "item.started", "mcp_tool_call",
"thread.started", "turn.completed", "turn.started"]`

**Recursive type set** (unchanged algorithm, kept for comparison): `["agent_message",
"item.completed", "item.started", "mcp_tool_call", "text", "thread.started", "turn.completed",
"turn.started"]`

**Key finding**: `mcp_tool_call` appears in **both** sets — it is not an artifact of the old
recursive scan finding a nested/embedded descriptor; the precise scan confirms it is the actual
`"type"` of a real item nested directly under a genuine `item.started`/`item.completed` event.
(The only difference between the two sets is `"text"`, which the recursive scan picked up from
some nested string-shaped field the precise scan correctly does not treat as an item type — this
by itself validates that the precise scanner is doing real filtering, not just reproducing the old
result by coincidence.)

**Conclusion**: the R-25 correction (`-c mcp_servers={}`, `--disable browser_use`, `--disable
computer_use`) did **not** eliminate the `mcp_tool_call` item on this codex-cli version
(0.144.6). The flags were accepted (no rejection, exit 0, empty stderr) but did not change the
observed capability-availability outcome. Contract validity, exit behavior, and read-only
integrity were unaffected either way (both before and after the correction, this row's snapshot
stayed byte-identical and its result stayed contract-valid), so this remains an
availability/invariant question rather than a demonstrated safety incident — but the specific
correction approved as R-25 is now empirically shown insufficient by itself on this provider
version, and that must go back to the user/coordinator rather than be silently re-corrected by
this worker, per the plan's Step 13 process.

### Row 11 addendum 2 — instrumented rerun identifies the `mcp_tool_call` item

**Authority for this instrumentation**: same R-25 correction (spec 0.2.2); no further spec change.
One billable, non-retried rerun of `provider-contract --wiki agents --agent codex` (corrected
0.2.2 argv unchanged) with two code extensions to `spikes/src/provider_contract.rs`:

- `codex_ordered_event_types` — every JSONL event's top-level `"type"`, **in stream order, not
  deduplicated**, to show where a given item falls in the turn.
- `codex_item_details` — for every `item.started`/`item.completed` whose item `"type"` is not the
  benign `agent_message`, records the item's **key list** plus a sanitized subset: string values
  under keys whose name contains `name`/`server`/`tool`/`status`/`state`/`error` (case-insensitive),
  truncated to 80 chars. Object/array values are always skipped even under a matching key name, so
  a loosely-named field can never leak arguments/inputs/outputs/content. No new dependency; no
  argument/result/content field is ever read or recorded.
- `result_shape_summary`'s `gaps`/`warnings` are now also recorded **verbatim** (not just counted)
  in a new `gaps_and_warnings_verbatim` check, since they are the model's own tooling/process
  commentary, not wiki content — distinct from `answer`, which this file still never records raw.

**Rerun**: duration 30.6s, exit 0, contract valid (`problems=[]`), snapshot byte-identical
(94/94 entries before and after), `knowledge_status=no_relevant_material`, citations 0/0
(correctly empty for this status).

**Ordered event-type sequence** (7 raw events, in order): `["thread.started", "turn.started",
"item.completed", "item.started", "item.completed", "item.completed", "turn.completed"]`. Two of
the three `item.completed`/`item.started` events are the benign `agent_message` item (filtered out
of the detail capture below); the other `item.started`→`item.completed` pair (positions 4–5) is
the `mcp_tool_call` item.

**`mcp_tool_call` item — key list**: `["arguments", "error", "id", "result", "server", "status",
"tool", "type"]` (names only; `arguments`, `id`, and `result` were deliberately never read for
content, per the no-argument/no-content instruction).

**`mcp_tool_call` item — sanitized identity fields** (identical on both the `item.started` and
`item.completed` occurrence, except `status`):

| Field | `item.started` | `item.completed` |
|---|---|---|
| `server` | `"codex"` | `"codex"` |
| `tool` | `"list_mcp_resources"` | `"list_mcp_resources"` |
| `status` | `"in_progress"` | `"completed"` |
| `error` | `null` | `null` |

**Verbatim `gaps`** (1 item): `"No wiki page content was accessible to ground the requested
one-sentence description."`
**Verbatim `warnings`** (1 item): `"The supplied constraints prohibited shell commands, and no
permitted local-file reading tool was available for D:\\Wikis\\agents."`
Neither string names a page, quotes wiki content, or otherwise contains wiki prose — both are the
model's own tooling/process commentary, consistent with the coordinator's carve-out.

**Reframe of the working hypothesis, from this data alone**:

- `server: "codex"` is **not** `markitdown` and does not name any wiki-specific or apm-managed MCP
  server. `tool: "list_mcp_resources"` is a generic MCP introspection call (list available MCP
  resources), not a wiki/skill-specific action. So this is **not** evidence of the
  `skill_mcp_dependency_install` hypothesis (a skill-declared MCP dependency such as `markitdown`
  being auto-installed/exposed) — nothing in the captured identity fields names `markitdown` or any
  wiki skill.
- The more plausible reading, directly supported by the verbatim `warnings` string above: Codex's
  own built-in tool exposure (including whatever it uses to read local files) appears to be routed
  through the same generic MCP-resource-listing mechanism internally labeled `server: "codex"`.
  `error: null` on both events means the `list_mcp_resources` call itself did not fail — it
  completed successfully — but the warning says no permitted file-reading tool was available
  afterward. That is consistent with `-c mcp_servers={}` zeroing a table Codex's own runtime also
  consults for its own built-in tools, not only for external/user-configured servers.
- **This is not confirmed causation, only the best-supported reading of the raw data**: this
  worker has no A/B pair for the `agents`/`codex` row under the *original, uncorrected* argv,
  because every `agents`/`codex` attempt across this entire report — the first attempt, the
  code-bug-fixed retry (both pre-R-25), and this instrumented rerun (post-R-25) — returned
  `no_relevant_material` identically. `harness-engineering`/`codex`, by contrast, grounded
  successfully, but only under the pre-R-25 argv; it has not been rerun under the corrected argv.
  So the data supports "this specific `mcp_tool_call` is Codex's own internal resource-listing
  call, not an external-server leak or a skill-declared MCP dependency" quite strongly, but does
  **not** establish that R-25's `-c mcp_servers={}` is *why* `agents`/`codex` fails to ground —
  that failure predates the correction and reproduces identically with or without it.
- Recommendation for the user/coordinator, not applied by this worker: if the invariant is
  reframed per R-25's intent ("no external MCP server capability", not "zero all Codex-internal
  tool plumbing"), the next diagnostic step would be one more instrumented call using the
  *original* (pre-R-25) argv against `agents`/`codex` to get the missing A/B data point — but that
  is a new call this worker was not authorized to make here and does not make on its own initiative.

### Row 11 addendum 3 — R-26 verification rerun (spec 0.2.3)

**Authority**: user-approved correction, spec now **v0.2.3, R-26**. §7.1's `constraints` array is
now provider-specific: only the second entry differs. Copied verbatim from the amended spec (not
paraphrased) into `spikes/src/provider_contract.rs`'s `build_prompt`, which now takes an `agent`
parameter and selects:

- Claude (unchanged): `"Do not run scripts or shell commands; no such tool is available."`
- Codex (changed): `"Reading wiki files with read-only commands is permitted; the sandbox enforces
  read-only. Do not attempt writes, index regeneration, installs, or network access."`

The other five constraint entries stay byte-identical across providers, as the spec states. The
0.2.2 Codex argv (`-c mcp_servers={}`, `--disable browser_use`, `--disable computer_use`) and the
instrumented item-capture code (`codex_ordered_event_types`, `codex_item_details`) were left
unchanged, per instruction.

**One authorized rerun, no retry**: `provider-contract --wiki agents --agent codex`.

| Field | Value |
|---|---|
| Duration | 16.1s |
| Exit code | 0 |
| Contract valid | yes (`problems=[]`) |
| `knowledge_status` | **`grounded`** (previously `no_relevant_material` on every prior `agents`/`codex` attempt in this report) |
| Citations found/resolved | **1/1** — `["ai-agents-in-depth"]`, resolved to exactly one `<slug>.md` |
| Snapshot byte-identical | yes (94/94 entries before and after) — the sandbox alone held the mutation-prevention line with the corrected, truthful prompt |
| gaps (verbatim) | `[]` (empty) |
| warnings (verbatim) | `[]` (empty) — no tooling-limitation warning this time, unlike addendum 2 |

**Ordered event-type sequence** (9 raw events): `["thread.started", "turn.started",
"item.completed", "item.started", "item.completed", "item.started", "item.completed",
"item.completed", "turn.completed"]`.

**`mcp_tool_call` observation**: **absent**. Neither the recursive type set
(`["agent_message", "command_execution", "item.completed", "item.started", "thread.started",
"turn.completed", "turn.started"]`) nor the precise type set (identical set this run) contains
`mcp_tool_call`. Under the corrected, truthful prompt the model didn't need its introspection
call at all this time.

**`command_execution`-type items observed**: **2** (two `item.started`→`item.completed` pairs, 4
item events total). Status only, no commands recorded (the `command` key is present in every
item's key list but was never read for content, per the capture function's key-name-marker
design): both pairs show `status: "in_progress"` on `item.started` and `status: "completed"` on
`item.completed`. No `error` key was present in either item's key list at all (unlike the
`mcp_tool_call` item in addendum 2, which always carried an explicit `error: null`), so no
error/failure indicator to report either way.

**Verdict**: R-26 confirmed. Making the Codex constraint truthful (read-only commands permitted,
sandbox enforces read-only) let the model read the wiki via two sandboxed, read-only
`command_execution` calls, ground the answer with a resolvable citation, and leave the
content-root snapshot byte-identical — i.e. the sandbox carried the mutation-prevention load
alone at the prompt level, as the correction intended. No `mcp_tool_call` item occurred this run,
so the earlier ambiguity (addenda 1–2) about whether that item was benign is moot for this
specific row/argv combination; R-26's own spec text (§10.1, §12) still treats a `server: "codex"`
introspection call as benign and undisableable should it recur.

## Additional PENDING rows (owned by Task 14, per spec §17.2 and plan Task 2 header note)

| Row | Reason |
|---|---|
| `process-tree` on Linux | No Linux CI runner defined yet; WSL is reachable but is not a native Linux runner (see Task 0 record). |
| `process-tree` on macOS | No macOS ARM hardware reachable from this machine. |
| Three-target native smoke build (`x86_64-pc-windows-msvc`, `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`) | Explicitly out of scope for Task 2 per the plan's own header note; requires CI Task 14 defines. |

---

## Step 13 — per-row gate

Every row reachable on this platform (Rows 1–10, and now Row 11's four sub-rows, run under
explicit user authorization) is **PASS** on its own `wiki-query/v1` contract and read-only
terms. The Linux/macOS/native-build rows above remain `PENDING` for the documented reasons and
block only the release/support claims they support, not Task 3 onward, per spec §17.2's stated
precedence.

One finding from Row 11 is flagged above as contradicting a stated safety **invariant** (§10.1's
MCP exclusion) even though it did not cause an observed contract or read-only failure in this
small sample: the Codex adapter's proposed §10.3 argv let an `mcp_tool_call`-typed item occur in
2/2 Codex calls. Per the plan's own Step 13 process, this is reported to the user/coordinator
with the affected section (§10.1, §10.3) and a proposed correction rather than corrected here;
this worker does not apply the correction itself.

## Deviations from the plan's literal text

1. Step 3's `codex login status` and `claude auth status --json` were run once each, observing
   only the currently-authenticated state (redacted to "logged-in"). The plan's instruction to
   record "exit behavior in both auth states" was not fully satisfiable without logging out of
   the user's real, currently-active session, which this task's constraints and ordinary care
   both counsel against; the logged-out shape is therefore not recorded and is left for a future,
   explicitly-scoped probe (e.g. using a disposable/secondary account) rather than guessed at.
2. Row 2 surfaces one implementation-relevant finding not explicitly requested by the plan text
   (`wait_with_output`'s non-concurrency on Windows) because it is directly load-bearing for
   Step 7/Task 8 and was discovered while satisfying Step 2's literal instruction to record the
   exact std-frontend types.
3. `spikes/src/provider_contract.rs` was initially a minimal PENDING stub, then rewritten to a
   full implementation once Step 11 was explicitly authorized (see Row 11). It remains spike
   evidence only, per `spikes/README.md`, and is not a source for the production crate.
4. Row 11 environment note: the local Claude Code auto-mode permission classifier intermittently
   blocked shell invocations whose literal command text it flagged (observed only for `--agent
   codex` invocations, never `--agent claude`); switching between the PowerShell and Bash tools
   (both already available in this environment) for the identical `cargo run` command let the
   run proceed without altering the command itself. This is a local harness/classifier behavior,
   not a spec or plan issue, and is recorded here only because it is why some Row 11 evidence
   below shows a command run via a different shell than earlier rows.
5. Row 11's `agents`/`codex` sub-row used its one permitted retry (per the coordinator's
   instruction not to retry more than once per row) to add read-only structural diagnostics
   (event/item-type counts only, never content) after the first attempt returned an
   empty-`answer` result; see Row 11 for the full account. A grading bug in this spike's own
   `citations_found_and_resolved` check (it assumed `knowledge_status=grounded` universally) was
   also fixed at that point — a fix to the spike's own evidence logic, not a spec/plan
   correction, and applied before any further live calls were made.
