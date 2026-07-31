# LLM Wikis Rust Query CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan revision:** 3 (2026-07-30). Written from rev 1, absorbing only corrections with a specification basis. rev 2 is superseded and must not be used. See **Provenance** below.

**Goal:** Build and release `llm-wikis` 0.1.0 as a read-only Rust CLI that queries exactly one configured wiki through Claude Code or Codex, with no Python runtime dependency.

**Architecture:** One synchronous Rust binary owns strict TOML configuration, canonical path resolution, platform process supervision, provider-specific argv/native-output parsing, `wiki-query/v1` normalization, citation validation, full-content mutation detection, doctor probes, and public output. The CLI calls an internal `QueryService`; future MCP and orchestration layers may call the same service but are not implemented here.

**Tech Stack:** Rust 1.97.1 and edition 2024, Cargo, `clap`, `serde`, `serde_json`, `toml`, `thiserror`, `sha2`, `tempfile`, a Phase-0-validated `process-wrap` standard-library frontend, Claude Code CLI, Codex CLI, POSIX shell, PowerShell, and GitHub Actions.

---

## Authoritative Inputs

- Specification: `docs/2026-07-28-llm-wikis-external-query-design.md` — **version 0.2.3**
- Plan: `docs/2026-07-28-llm-wikis-query-cli.md` (this file)
- Registered knowledge bases: `D:\Wikis\agents`, `D:\Wikis\harness-engineering`
- Comparison baseline: `docs/archive/2026-07-28-llm-wikis-query-cli.rev1.md`
- Installer reference: `https://raw.githubusercontent.com/gn00678465/apm-go/refs/heads/main/install.sh`

The intentionally removed `development-handoff.md` is not an input and must not be recreated.

This project **never writes to `D:\Wikis`**. There is no overlay, no skill deployment, no `agents/` directory in this repository, and no APM dependency.

## Provenance

rev 2 was written from a thirty-item gap list that an independent Codex pass later graded
**6 CONFIRMED, 12 REFUTED, 9 PARTIAL** — twelve of its claims were false positives against mechanisms
rev 1 or the specification already had. rev 3 therefore returns to rev 1 and absorbs only what has a
specification basis.

Absorbed from rev 2: corrected authoritative-input paths; corrected knowledge-base paths; the
Task 10 ↔ Task 11 dependency break (rev 1's Task 11 ↔ Task 12 cycle, broken by Task 9's
`ProbeReader`); the missing Target File Map entry; an executable `Run:` command
for the spike provider-contract step; `--config` trust-boundary documentation.

Added outside that list, each with a stated justification: **Task 0** (required by the no-Git
decision, which removed commit history as the record of environment and decisions) and repository
metadata deferred into the release task (it depends on undecided repository visibility and licence).

Everything else rev 2 added is reverted. Evidence for every disposition is in
`.scratch/spec-plan-correction/issues/`.

## Execution Governance

This implementation must not be performed by the main/orchestrating session. The main session may dispatch work, preserve scope, and route findings, but each production task is owned by a fresh implementation worker. Use `@subagent-driven-development`; if that is unavailable, start a separate `@executing-plans` session.

Before production code:

1. Task 0 records the environment and the standing decisions;
2. an independent review session creates the acceptance checklist in Task 1;
3. a separate spike worker completes Task 2;
4. any failed assumption enters the correction loop in Task 2's final step.

After production code, a separate verifier executes the checklist one row at a time. Implementers cannot author checklist requirements, mark their own work as passed, or edit failed rows into weaker assertions.

### Standing decisions

| # | Decision | Consequence for this plan |
|---|---|---|
| D1 | Two real knowledge bases, `D:\Wikis\agents` and `D:\Wikis\harness-engineering` | Offline tests use fixtures; live rows use the real wikis |
| D2 | This workspace is intentionally **not** a Git repository | Every task ends with a checkpoint, not a commit. Task 14 is blocked |

### Checkpoint protocol

Every task ends by appending one entry to `docs/verification/llm-wikis-execution.md`:

```text
## Task <N> — <title>
- completed_utc: <ISO-8601>
- worker: <session identifier>
- files: <path> <sha256>   (one line per created/modified file)
- commands: <exact command> -> exit <code>
- result: PASS | FAIL | PENDING | BLOCKED
- notes: <deviations, pending rows, follow-ups>
```

## Scope Locks

Version 0.1.0 includes only:

```text
llm-wikis --version
llm-wikis [--config <absolute-path>] [--json] config init
llm-wikis [--config <absolute-path>] [--json] list
llm-wikis [--config <absolute-path>] [--json] doctor [--wiki <id>] [--agent claude|codex] [--live]
llm-wikis [--config <absolute-path>] [--json] query --wiki <id> [--agent claude|codex] -- <question>
```

Five commands. Do not add MCP, orchestration, multi-wiki fan-out, direct retrieval APIs, answer saving, operation logging, index regeneration, index freshness checking, a `skill` subcommand, an `agent-context` subcommand, a configuration wizard, self-update, an uninstaller, package-manager manifests, Intel macOS, or platform code signing.

**Do not create, modify, or delete any file in any knowledge base.** No command may do so.

## Target File Map

| Path | Responsibility |
|---|---|
| `Cargo.toml` | Package metadata, dependencies, release profile, workspace exclusions |
| `Cargo.lock` | Reproducible dependency graph |
| `rust-toolchain.toml` | Pinned Rust 1.97.1 toolchain |
| `src/main.rs` | Minimal executable boundary and exit code |
| `src/lib.rs` | Internal module exports for integration tests |
| `src/cli.rs` | `clap` command model and dispatch |
| `src/config.rs` | Strict TOML schema, platform paths, initialization, resolution |
| `src/error.rs` | Stable error codes, exit classes, sanitized details |
| `src/model.rs` | Contract and public output types |
| `src/output.rs` | Human and exactly-one-document JSON rendering |
| `src/wiki.rs` | Content-root minimum structure and the schema-absent warning |
| `src/snapshot.rs` | Streaming SHA-256 snapshots of the content root |
| `src/citations.rs` | Citation grammar, resolution, strictness, namespacing |
| `src/process.rs` | Executable resolution, batch shim, bounded pipes, timeout, process-tree lifecycle |
| `src/providers/mod.rs` | Provider trait and shared request/result types |
| `src/providers/claude.rs` | Claude argv, JSON parser, version/auth probes |
| `src/providers/codex.rs` | Codex argv, JSONL parser, version/auth probes |
| `src/probes.rs` | Probe identity, fingerprints, `ProbeReader`/`ProbeWriter`, atomic cache |
| `src/query.rs` | Single-wiki Query Service workflow and verification mode |
| `src/doctor.rs` | Static/live doctor matrix and list operation |
| `tests/` | Rust integration, CLI, contract, security, and platform tests |
| `tests/fixtures/wiki-flat/` | Fixture content root, schema at root, pages in `wiki/pages/` |
| `tests/fixtures/wiki-typed/` | Fixture content root, schema at root, pages in type directories |
| `tests/fixtures/claude/`, `tests/fixtures/codex/` | Sanitized native-output fixtures |
| `tests/fixtures/process-helper/` | Separate child/grandchild process test crate |
| `spikes/` | Disposable pre-production capability experiments |
| `config.example.toml` | Documented configured-registry example |
| `install.ps1` | Windows x64 release installer |
| `install.sh` | Linux x64/macOS ARM64 release installer |
| `README.md`, `LICENSE`, `.gitignore` | Release prerequisites — created in Task 14 |
| `.github/workflows/ci.yml` | Offline quality and native platform tests |
| `.github/workflows/release.yml` | Tagged native builds, checksums, attestations, Release |
| `docs/llm-wikis.md` | Operator guide, install/config/query/security |
| `docs/verification/llm-wikis-preflight.md` | Sanitized Phase 0 results |
| `docs/verification/llm-wikis-execution.md` | Task checkpoints and test evidence |
| `docs/verification/llm-wikis-v0.1.0-checklist.md` | Independently authored acceptance checklist |
| `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json` | Independent immutable-column digest and row count |
| `docs/verification/evidence/llm-wikis-v0.1.0/` | Sanitized row-by-row verification evidence |

## Core Interfaces

Keep these boundaries stable unless Phase 0 proves them infeasible.

```rust
pub trait ProviderAdapter: Send + Sync {
    fn name(&self) -> Agent;
    fn version(&self, runner: &dyn ProcessRunner, executable: &ResolvedExecutable)
        -> Result<String, AppError>;
    fn auth_status(&self, runner: &dyn ProcessRunner, executable: &ResolvedExecutable)
        -> Result<AuthStatus, AppError>;
    fn invoke(&self, runner: &dyn ProcessRunner, request: ProviderRequest)
        -> Result<ModelResult, AppError>;
}

pub trait ProcessRunner: Send + Sync {
    fn run(&self, request: ProcessRequest) -> Result<ProcessOutcome, AppError>;
}

/// Read-only probe access. `QueryService` depends only on this, which is why the
/// Query Service task does not depend on the doctor task.
pub trait ProbeReader: Send + Sync {
    fn current_record(&self, key: &ProbeKey) -> Result<Option<ProbeRecord>, AppError>;
}

/// Write access. Used only by a successful live doctor.
pub trait ProbeWriter: Send + Sync {
    fn replace_for_key(&self, key: &ProbeKey, record: ProbeRecord) -> Result<(), AppError>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    /// Normal query. Requires exactly one matching current probe record.
    Enforced,
    /// Live doctor only. Skips the probe requirement; the caller publishes the
    /// record after validating output, citations, and zero mutation.
    Verification,
}

pub struct QueryService<R: ProcessRunner, P: ProbeReader> {
    runner: R,
    probes: P,
    providers: ProviderRegistry,
    clock: Box<dyn MonotonicClock>,
}

impl<R: ProcessRunner, P: ProbeReader> QueryService<R, P> {
    pub fn query(&self, request: QueryRequest, mode: QueryMode)
        -> Result<QueryEnvelope, AppError>;
}
```

The provider trait receives only validated, resolved values. CLI parsing, terminal rendering, TOML parsing, and direct process creation do not leak into `QueryService`.

**Dependency direction.** Task 9 defines `ProbeKey`, `ProbeRecord`, `ProbeReader`, `QueryMode`, and an in-memory test double in `src/probes.rs`. Task 10 consumes `ProbeReader` inside `QueryService`. Task 11 adds the on-disk atomic store and `ProbeWriter`, and its live doctor calls `QueryService::query(.., QueryMode::Verification)`. The chain is Task 9 → Task 10 → Task 11 with no back-edge; there is no cycle.

**Field ownership.**

| Public envelope field | Produced by |
|---|---|
| `schema_version` | `src/output.rs` constant `"1.0"` |
| `duration_ms` | `QueryService` monotonic clock, preflight through after-snapshot |
| `child_exit_code` | `ProcessOutcome::exit_code`, forwarded unchanged; `null` when no child ran |
| `raw_format` | Provider adapter, only after a native document or stream parses |
| `warnings` | `QueryService` wrapper codes in deterministic order, then provider strings |
| `citations` | `src/citations.rs`, namespaced by `QueryService` |

---

## Task 0: Record Environment, Prerequisites, and Standing Decisions

**Files:**

- Create: `docs/verification/llm-wikis-execution.md`
- Read: `docs/2026-07-28-llm-wikis-external-query-design.md`
- Read: `docs/2026-07-28-llm-wikis-query-cli.md`

> Justification: the no-Git decision removed commit history, which would otherwise have been the record of the environment, the standing decisions, and each task's outcome. Without this task the first checkpoint has nowhere to land.

- [ ] **Step 1: Record the standing decisions**

Open the execution record with a `## Decisions` section stating verbatim:

```text
D1 two real knowledge bases: D:\Wikis\agents and D:\Wikis\harness-engineering. Never written to.
D2 no Git repository: approved 2026-07-29. Checkpoints replace commits. Task 14 blocked.
Specification version in force: 0.2.3 (amended 2026-07-30, corrected 2026-07-31 after independent re-review; R-25/R-26 Codex corrections applied 2026-07-31 with user approval — see its Revision History).
```

- [ ] **Step 2: Record the Rust toolchain state**

```powershell
rustc --version
cargo --version
rustup toolchain list
```

Expected: Rust 1.97.1. If a shadowing installation resolves first — for example an orphaned package-manager shim ahead of `%USERPROFILE%\.cargo\bin` on the machine PATH — record the resolved path and the remedy applied. Do not proceed with a toolchain that cannot be invoked by bare `cargo`.

- [ ] **Step 3: Record provider CLI availability**

```powershell
(Get-Command claude -ErrorAction SilentlyContinue).Source
(Get-Command codex -ErrorAction SilentlyContinue).Source
```

Record the resolved absolute path and whether each is `.exe`, `.cmd`, or absent. A missing provider blocks only that provider's Task 2 rows and its live rows, which are recorded `PENDING`.

- [ ] **Step 4: Record the platform baseline**

OS name/version, architecture, shell, the platform config and cache directories, and whether native Linux and macOS ARM environments are reachable. Unreachable platforms are `PENDING`, never `FAIL`.

- [ ] **Step 5: Confirm the workspace matches the specification**

```powershell
Test-Path agents
Test-Path skills
rg -n "query_profiles|index_freshness|LINK_STYLE_UNSUPPORTED|overrides" docs
```

Expected: `agents` and `skills` are `False`; the removed identifiers appear only inside the specification's Revision History. Any other occurrence is a documentation defect to report before Task 1.

- [ ] **Step 6: Record the Task 0 checkpoint**

## Task 1: Create the Independent Acceptance Checklist

**Files:**

- Create by independent reviewer only: `docs/verification/llm-wikis-v0.1.0-checklist.md`
- Create by independent reviewer only: `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`
- Modify: `docs/verification/llm-wikis-execution.md`
- Read: `docs/2026-07-28-llm-wikis-external-query-design.md`
- Read: `docs/2026-07-28-llm-wikis-query-cli.md`

- [ ] **Step 1: Dispatch a checklist author that has no implementation role**

Give it only the two document paths and this instruction:

```text
Create an acceptance checklist before implementation. Map every normative requirement in the
specification to one independently executable check. Use one row per behavior with: ID,
requirement, platform, phase, exact command or inspection, expected result, status=PENDING,
evidence=empty.

The phase column uses exactly: Phase 0, Phase 1, Phase 2, Phase 3, as defined in specification
Section 19. Separate offline, process, installer, live-provider, and release checks.

Read Section 23 Revision History first. Sections 6.5 and 9 are tombstones: they define removed
behavior, and a checklist row must not require what they removed.

Do not implement code and do not weaken the specification.
```

- [ ] **Step 2: Confirm checklist provenance**

The file must identify the independent author/session and state that the main session and implementation workers may not edit requirement/expected-result columns. It must also reserve final status/evidence updates for a second independent verifier that is not the checklist author.

- [ ] **Step 3: Check coverage mechanically**

```powershell
rg -n "^\| [A-Z]+-[0-9]+" docs/verification/llm-wikis-v0.1.0-checklist.md
rg -n "PENDING|offline|installer|live|release|Windows|Linux|macOS" docs/verification/llm-wikis-v0.1.0-checklist.md
rg -n "Phase 0|Phase 1|Phase 2|Phase 3" docs/verification/llm-wikis-v0.1.0-checklist.md
```

Expected: rows exist, all begin `PENDING`, and every verification class, platform, and phase appears.

- [ ] **Step 4: Confirm the removed behavior is not required**

```powershell
rg -n "index_freshness|INDEX_STALE|INDEX_MAY_BE_STALE|link_style|query_profiles|overlay|skill install" docs/verification/llm-wikis-v0.1.0-checklist.md
```

Expected: no matches, or matches only in rows asserting the behavior is **absent**.

- [ ] **Step 5: Dispatch a different reviewer for checklist-to-spec coverage**

The reviewer may report omissions but must not implement. The checklist author fixes confirmed omissions before Task 2.

- [ ] **Step 6: Freeze the checklist requirements**

Canonicalize each row's immutable columns in this exact order: `ID`, `requirement`, `platform`, `phase`, `command_or_inspection`, `expected_result`, joined with UTF-8 LF and no trailing whitespace. Record their SHA-256 and row count in the baseline JSON, and record the author/session plus baseline path in the execution record. Later status/evidence edits are verifier-owned; immutable-column edits require user approval.

- [ ] **Step 7: Record the Task 1 checkpoint**

## Task 2: Run Disposable Pre-Implementation Capability Spikes

**Files:**

- Create: `spikes/Cargo.toml`
- Create: `spikes/src/main.rs`
- Create: `spikes/src/bin/process-tree-child.rs`
- Create: `spikes/README.md`
- Create: `docs/verification/llm-wikis-preflight.md`
- Do not create or modify production `src/` files
- Do not create or modify anything under `D:\Wikis`

> The native three-target smoke build and the Linux/macOS process-primitive rows are **not** in this task. They require CI that Task 14 defines. See specification Section 17.2.

- [ ] **Step 1: Scaffold the isolated spike crate**

Package name `llm-wikis-spikes`, edition 2024, dependencies:

```toml
process-wrap = { version = "9.1", features = ["std"] }
serde_json = "1"
sha2 = "0.10"
tempfile = "3"
```

The README must state that spike code is evidence only and cannot be copied into production.

- [ ] **Step 2: Confirm the `process-wrap` standard-library frontend**

```powershell
cargo tree --manifest-path spikes/Cargo.toml -p process-wrap
cargo doc --manifest-path spikes/Cargo.toml -p process-wrap --no-deps
```

Record the resolved version and the exact `std`-frontend types used for Unix process groups and Windows Job Objects. If the crate no longer exposes an equivalent synchronous composition, record `FAIL` and enter the correction loop; do not substitute an async runtime. Record `std::os::windows::process::CommandExt::creation_flags` as the fallback route if needed.

- [ ] **Step 3: Record provider capability surfaces**

```powershell
claude --version
claude --help
claude auth status --json
codex --version
codex --help
codex exec --help
codex login status
```

Record only versions, accepted flags, executable paths, and redacted auth state. Never record tokens, account IDs, prompts, or wiki prose.

For each provider record the minimal tool/capability set that both invokes a configured entrypoint and excludes Write, Edit, shell, web, MCP, subagents, session persistence, and interactive escalation. A flag substitution that restores a forbidden capability is a failed spike. Record the output-schema flag that constrains the result shape — `--json-schema` for Claude, `--output-schema` for Codex — because specification Section 7.2 item 9 depends on it.

If a provider does not expose the expected auth subcommand, record the actual non-billable readiness command it does expose and its exit behavior for authenticated and logged-out states. If none exists, record `FAIL` for that row — do not fall back to a paid model call.

An absent provider CLI makes this row `PENDING` for that provider.

- [ ] **Step 4: Prove stdin and argv separation**

The spike launches a fixture child with fixed argv and sends:

```text
繁體中文
--leading-dash
"quotes" `backticks` $() & | < > ^ % !
```

```powershell
cargo run --manifest-path spikes/Cargo.toml -- stdin-boundary
```

Expected: byte-for-byte stdin round trip; fixture argv contains no question substring.

- [ ] **Step 5: Prove Windows executable resolution and the batch-shim boundary**

Exercise the resolved native Claude `.exe` and Codex `.cmd`/`.exe` shape, plus a fixture shim under a path containing spaces. Test fixed path arguments containing `&`, `|`, `^`, `%VAR%`, `!DELAYED!`, `>`, `<`.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- windows-resolution
```

Expected: exact expected argv at the fixture, no shell expansion or variable substitution, explicit classification of `.exe` versus `.cmd`. **Record the exact quoting rule that made the `.cmd` path safe** — Task 8 reimplements it from this record.

- [ ] **Step 6: Prove platform config/cache directory resolution**

Inject environment/home/platform values into the spike resolver and verify all six config/cache results from specification Sections 5.2 and 15.1, including the macOS paths containing spaces.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- platform-dirs
```

Expected: exact Windows, Linux/XDG-fallback, and macOS paths with no cwd dependency.

- [ ] **Step 7: Prove bounded concurrent output**

The fixture alternates stdout/stderr beyond each limit and has a mode that fills one pipe while blocking on the other.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- bounded-pipes
```

Expected: no deadlock; correct stream and observed byte count; process tree terminated.

- [ ] **Step 8: Prove timeout and process-tree termination**

The helper spawns a grandchild and writes both PIDs to a temporary file. Exercise success, timeout, and forced overflow.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- process-tree
```

Expected: after every forced termination, neither PID remains alive. **Windows only in this task**; the Linux and macOS instances are `PENDING` rows owned by Task 14.

- [ ] **Step 9: Prove full-content mutation detection**

Create a temporary wiki, snapshot it, rewrite a file with the same byte length, restore its timestamp, and compare.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- mutation-hash
```

Expected: the changed relative path is detected despite equal size and timestamp.

- [ ] **Step 10: Prove temp-artifact safety**

Create the generated empty Claude MCP config, the Codex output-schema file, and any batch helper in a fresh system temp directory. Assert exclusive creation, user-only permissions where supported, canonical location outside every configured root, survival until child reap, and removal afterward. Include an injected unsafe temp root that must fail before spawn.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- temp-artifacts
```

- [ ] **Step 11: Query each registered wiki's real entrypoint**

This is the row that proves the whole envelope-carried contract. For each wiki, build the exact prompt from specification Section 7.1 — entrypoint, that wiki's `query_prompt`, then the `EXTERNAL_QUERY` object with `required_result` and `constraints` — and invoke the provider with the exact argv from Sections 10.2 and 10.3, including the output-schema flag.

```powershell
cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki agents --agent claude
cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki agents --agent codex
cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki harness --agent claude
cargo run --manifest-path spikes/Cargo.toml -- provider-contract --wiki harness --agent codex
```

Expected for each: a result satisfying `wiki-query/v1`, at least one citation resolving under the Section 11.2 rule, and **byte-identical before/after content-root hashes**. Record whether the skill attempted a forbidden action and failed, because that is the accepted residual risk of Section 7.2 and its frequency is worth knowing.

**These calls consume model quota and touch real knowledge bases read-only. Run only with explicit user authorization.** Without it, record `PENDING`.

- [ ] **Step 12: Write the preflight report**

For each assumption record `PASS`, `FAIL`, or `PENDING`, exact sanitized command, platform, tool versions, and evidence path in `docs/verification/llm-wikis-preflight.md`.

- [ ] **Step 13: Apply the per-row gate and the correction loop**

Task 2 has passed when every row reachable on this platform is `PASS`. Unreachable rows are `PENDING` with a reason and block only the release claim they support.

If any reachable row is `FAIL`, stop production work and run this loop:

1. record the failing assumption, the observed behavior, and the smallest specification statement it contradicts;
2. the main session reports it to the user with the affected section and a proposed correction — it does not apply the correction itself;
3. after user approval, the specification and this plan are corrected, and the correction is added to Section 23;
4. a fresh independent reviewer re-reviews the corrected documents;
5. if any Task 1 immutable column changes, the checklist author regenerates the baseline digest and the user approves the replacement;
6. the failed row is rerun to `PASS` before Task 3 starts.

- [ ] **Step 14: Record the Task 2 checkpoint**

## Task 3: Scaffold the Production Rust Crate

**Files:**

- Create: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`
- Create: `src/main.rs`, `src/lib.rs`, `src/cli.rs`
- Create: `tests/version_cli.rs`

- [ ] **Step 1: Write the failing version integration test**

```rust
#[test]
fn version_is_public_product_name_and_package_version() {
    let mut cmd = assert_cmd::Command::cargo_bin("llm-wikis").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout("llm-wikis 0.1.0\n");
}
```

- [ ] **Step 2: Create the manifest and pinned toolchain**

```toml
[package]
name = "llm-wikis"
version = "0.1.0"
edition = "2024"
rust-version = "1.97"

[workspace]
exclude = ["spikes", "tests/fixtures/process-helper"]

[dependencies]
clap = { version = "4.5", features = ["derive"] }
process-wrap = { version = "9.1", features = ["std"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
tempfile = "3"
thiserror = "2"
toml = "0.9"

[dev-dependencies]
assert_cmd = "2"
filetime = "0.2"
predicates = "3"

[profile.release]
codegen-units = 1
lto = "thin"
panic = "abort"
strip = "symbols"
```

`rust-toolchain.toml` pins `1.97.1` with `rustfmt` and `clippy`. Publication metadata — `description`, `license`, `repository`, `readme` — is added in Task 14, because it depends on repository visibility and licence choices that have not been made.

- [ ] **Step 3: Run the test and confirm failure**

```powershell
cargo test --test version_cli
```

- [ ] **Step 4: Implement the minimal executable boundary**

`src/cli.rs` provides the minimal `clap` product/version parser and `run()` needed for `--version`; later commands remain unimplemented. `src/main.rs` calls `llm_wikis::cli::run()` and exits with its returned code. `src/lib.rs` exports only the modules needed by integration tests.

- [ ] **Step 5: Run quality gates**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test version_cli
```

- [ ] **Step 6: Record the Task 3 checkpoint**

## Task 4: Implement Stable Models, Errors, Output, and the Drift Test

**Files:**

- Create: `src/error.rs`, `src/model.rs`, `src/output.rs`
- Create: `tests/model_contract.rs`, `tests/error_contract.rs`, `tests/output_contract.rs`, `tests/spec_drift.rs`

- [ ] **Step 1: Write failing model-contract tests**

```rust
pub enum KnowledgeStatus { Grounded, NoRelevantMaterial }

pub struct ModelResult {
    pub contract: String,
    pub knowledge_status: KnowledgeStatus,
    pub answer: String,
    pub citations: Vec<String>,
    pub gaps: Vec<String>,
    pub warnings: Vec<String>,
}
```

Reject unknown JSON fields, wrong `contract`, empty answer, grounded-without-citations, and no-material-with-citations-or-without-gap.

- [ ] **Step 2: Write failing error and exit-class tests**

Assert every row of specification Section 14 maps to its exact exit code — **27 codes**. Assert the removed codes do **not** exist: `LINK_STYLE_UNSUPPORTED`, `INDEX_STALE`, `QUERY_PROFILE_NOT_FOUND`, `PROVIDER_PROFILE_MISSING`, `CONTRACT_UNSUPPORTED`.

Assert the dominance rules: `READ_ONLY_VIOLATION` dominates exits `2`–`6` and preserves only a sanitized `secondary_error`; an incomplete integrity comparison becomes `INTERNAL_ERROR`. Assert the doctor precedence order `70, 7, 6, 5, 4, 3, 2, 0` as a pure function over a mixed failure set.

- [ ] **Step 3: Write failing closed-details tests**

`error.details` is closed to unknown keys per code:

- `OUTPUT_TOO_LARGE` → exactly `{stream, limit_bytes, observed_bytes}`; `stream` only `"stdout"` or `"stderr"`; `observed_bytes > limit_bytes`;
- `CITATION_AMBIGUOUS` → exactly `{slug, match_count}`; `match_count >= 2`; **no paths**;
- `READ_ONLY_VIOLATION` → exactly `{changed_paths, secondary_error?}`; sorted, unique, non-empty, relative, slash-separated; no content, absolute path, or traversal; `secondary_error` never nests `details`.

Assert JSON failures contain no traceback, prompt text, absolute protected path, hash, or wiki content.

- [ ] **Step 4: Write failing renderer tests**

Exactly one JSON document plus one trailing newline on stdout; diagnostics only on stderr. `schema_version` is the literal `"1.0"` on every operation envelope. Wrapper warning codes are exactly `WIKI_SCHEMA_ABSENT`, `CLAUDE_READ_SCOPE_BROAD`, `CODEX_READ_SCOPE_BROAD`; `INDEX_MAY_BE_STALE` must not exist. Model warnings normalize to `PROVIDER_WARNING` and follow wrapper warnings. `raw_format` is only `claude-json`, `codex-jsonl`, or null. `wiki` and `agent` serialize as `null` when argument failure precedes their resolution. Human mode prints answer, then gaps, then warnings.

- [ ] **Step 5: Write the failing specification drift test**

`tests/spec_drift.rs` parses `docs/2026-07-28-llm-wikis-external-query-design.md` and asserts, in both directions:

- the Section 14 error table equals the implemented error enum, code for code;
- each code's exit class in the table equals the implementation's;
- the Section 15 `checks[].name` vocabulary equals the implemented set — `config`, `roots`, `wiki_structure`, `entrypoint`, `executable`, `auth`, `read_scope`, `live_contract`, `mutation`;
- the Section 13 wrapper warning-code vocabulary equals the implemented set.

The specification stays the authority. The test reports disagreement without deciding which side is wrong. A parse failure is itself a signal that a normative table was restructured.

- [ ] **Step 6: Run tests and confirm failure**

```powershell
cargo test --test model_contract --test error_contract --test output_contract --test spec_drift
```

- [ ] **Step 7: Implement strict serde models and `AppError`**

Use `#[serde(deny_unknown_fields)]` on closed wire structures. Keep internal causes for diagnostics but expose only allowlisted sanitized details.

- [ ] **Step 8: Implement output rendering**

Rendering receives already-normalized envelopes and performs no provider parsing or path access.

- [ ] **Step 9: Run tests**

Expected: PASS.

- [ ] **Step 10: Record the Task 4 checkpoint**

## Task 5: Implement Strict Configuration and `config init`

**Files:**

- Create: `src/config.rs`, `config.example.toml`
- Create: `tests/config_contract.rs`, `tests/config_init.rs`

- [ ] **Step 1: Write failing platform-path tests**

Inject an environment abstraction rather than mutating process-global environment in parallel tests.

```text
Windows   %APPDATA%\llm-wikis\config.toml
Linux     ${XDG_CONFIG_HOME:-$HOME/.config}/llm-wikis/config.toml
macOS     $HOME/Library/Application Support/llm-wikis/config.toml
```

Also assert the three cache paths from Section 15.1.

- [ ] **Step 2: Write failing strict-schema tests for the 0.2 registry shape**

The registry has **no `[query_profiles]` table**. Each wiki carries `title`, `project_root`, `content_root`, `agents`, `query_prompt`, and one `[wikis.<id>.<agent>]` table per enabled agent containing `load`, `entrypoint`, and its load mode's fields.

Cover: zero wikis; the two-wiki registry from Section 6; unknown keys; wrong config version; invalid IDs; an enabled agent missing its global provider table (`PROVIDER_CONFIG_MISSING`); an enabled agent missing its per-wiki table (`CONFIG_INVALID`); provider command names; absolute executable paths; relative executable paths rejected; executable values containing arguments or shell syntax rejected; every forbidden key in Section 6.4; and the absence of `query_profiles`, `contract`, and `index_freshness` as recognized keys.

Assert `[runtime]` per-field defaults: with the table omitted entirely, and with each field omitted individually, the unspecified fields independently default to `timeout_seconds = 180`, `max_question_bytes = 65536`, `max_stdout_bytes = 1048576`, `max_stderr_bytes = 65536` (specification Section 6).

- [ ] **Step 3: Write failing `query_prompt` tests**

Required per wiki. Reject: missing; empty; containing `\n`, `\r`, or any other control character; longer than 500 UTF-8 bytes. Accept exactly 500 bytes. Assert it is stored verbatim, and that no code path lets it alter, override, or shadow an envelope field, provider flag, or tool restriction.

- [ ] **Step 4: Write failing path-resolution tests**

Config-relative project/content/plugin paths; Unicode and spaces; absolute-only `--config` override. **`content_root` equal to `project_root` is accepted**; `content_root` outside `project_root` is `PATH_OUTSIDE_ALLOWED_ROOT`.

Check special status on every component of each configured path. Recursively scan the complete content root and the selected skill/plugin artifact tree, **excluding any `.claude/` and `.agents/` immediately beneath `content_root`** (present there only when `content_root` equals `project_root` — specification Section 6.1), and not unrelated project-root subtrees. Unconditionally reject encountered symlinks, junctions, reparse points, mount points, and other special entries even when contained. Include a fixture with a symlink under `.claude/skills/` that must **not** trigger `UNSAFE_FILESYSTEM_ENTRY`, and the same symlink outside those directories that must. Symlink fixtures are constructed at test run time in a temporary directory — they are not committed under `tests/fixtures/` (symlinks do not survive portable checkout on Windows without a VCS).

- [ ] **Step 5: Write failing entrypoint-syntax tests**

Section 6.3 in full: Claude `/name`; Claude `/plugin:skill` with exactly one colon; Codex `$name`; deferred and rejected Codex `$plugin:skill`; the ASCII character set; rejection of whitespace, newlines, control characters, quotes, and shell metacharacters. Assert the wrapper never appends a namespace, renames, or infers the entrypoint from `skill_path`.

Also cover the statically addressable artifacts from specification Section 8.1 step 6: a `project_skill` whose resolved `skill_path` does not exist or is not a regular file, and a `local_plugin` whose `plugin_dir` or manifest is missing, each fail with `ENTRYPOINT_INVALID` before any provider invocation. Assert the passing case for both fixtures' real shapes.

- [ ] **Step 6: Write failing `config init` tests**

Parent creation; valid zero-wiki TOML; commented example; `create_new` semantics; unchanged bytes and `CONFIG_EXISTS`/exit 2 when the target exists; the exact `operation: "config_init"` contracts including `path` and `created`.

- [ ] **Step 7: Run tests and confirm failure**

```powershell
cargo test --test config_contract --test config_init
```

- [ ] **Step 8: Implement configuration structs**

Strict serde tables for `providers`, `runtime`, and `wikis`, with a nested per-provider table inside each wiki. Semantic validation only after syntactic deserialization.

- [ ] **Step 9: Implement platform paths and initialization**

`config init` uses an exclusive create operation. It never reads cwd for discovery and never overwrites or merges.

- [ ] **Step 10: Add the configured registry example**

`config.example.toml` reproduces Section 6's two-wiki registry verbatim, including both `query_prompt` values. Add a commented note that `content_root` may equal `project_root` and that relative paths resolve from the platform config directory, not the caller's project. The template generated by `config init` contains only a commented placeholder wiki block.

- [ ] **Step 11: Run tests**

Expected: PASS.

- [ ] **Step 12: Record the Task 5 checkpoint**

## Task 6: Implement Content-Root Preflight and Citation Rules

**Files:**

- Create: `src/wiki.rs`, `src/citations.rs`
- Create: `tests/fixtures/wiki-flat/`, `tests/fixtures/wiki-typed/`
- Create: `tests/wiki_preflight.rs`, `tests/citations.rs`

> The wrapper asserts no wiki layout. There is **no** index discovery, index parsing, freshness comparison, `SCHEMA.md` parsing, `link_style` selection, or `link_style_rules` resolution. Do not implement any of them.

- [ ] **Step 1: Build the two fixture content roots**

`tests/fixtures/wiki-flat/` mirrors the `agents` shape: `SCHEMA.md` at the root, `config/`, `bin/` with a hook, `raw/`, `assets/`, `wiki/index.md`, `wiki/overview.md`, `wiki/pages/*.md`.

`tests/fixtures/wiki-typed/` mirrors the `harness-engineering` shape: `SCHEMA.md` and `index.md` at the root, `concepts/`, `entities/`, `sources/`, `synthesis/`, and a `graph/` directory containing a small binary file.

Both are synthetic. No real wiki prose, no account identifiers, no absolute user paths, no special filesystem entries. Include in **each** fixture a page whose filename stem is not slug-shaped, and a Markdown file containing example wikilinks that resolve to nothing, so the citation-strictness assertions (Step 6) run against both layouts — the same dual-fixture property Steps 2–3 enforce for structure.

- [ ] **Step 2: Write failing minimum-structure tests**

Preflight requires only: `content_root` exists; is a directory; contains at least one regular `.md` at any depth. Cover a missing path, a file instead of a directory, an empty directory, a directory containing only non-Markdown files, and a directory whose only `.md` is several levels deep. Failure is `WIKI_INVALID`.

Assert **both** fixtures pass unchanged, which is the layout-agnostic property.

- [ ] **Step 3: Write failing schema-warning tests**

`WIKI_SCHEMA_ABSENT` is emitted under check name `wiki_structure` when no `SCHEMA.md` exists at the content root, with the exact Section 15 message. Assert it is a **warning**: `ok` stays true, exit stays 0, and query still succeeds. Assert it is not emitted for either fixture, and is emitted when the content root is set one level above a fixture.

- [ ] **Step 4: Write failing citation grammar tests**

Three forms recognized, slug always `[a-z0-9-]+`:

```text
[[slug]]              → slug
[[slug|label]]        → slug (text before the first `|`)
[[slug](any/path)]    → slug
```

Ignored, not rejected: `raw/<file>`, `assets/<file>`, HTTP(S) URLs, `[[raw/...]]`, `[[assets/...]]`, ordinary Markdown links, malformed and dangling wiki-link prose, uppercase or underscored slugs, and any form spanning a newline.

- [ ] **Step 5: Write failing resolution tests**

Exactly one regular `<slug>.md` anywhere beneath `content_root`, by exact ASCII filename stem. Zero matches → `CITATION_NOT_FOUND`; two or more → `CITATION_AMBIGUOUS` with `{slug, match_count}` and no paths. Assert Windows case folding does not authorize a case-mismatched citation. Assert resolution consumes a supplied file list rather than walking the tree itself, so it can reuse the snapshot's inventory.

- [ ] **Step 6: Write failing strictness tests**

| Source | Unsafe target | Unresolvable |
|---|---|---|
| explicit `citations` array | `CITATION_INVALID`, fail-fast | `CITATION_NOT_FOUND` / `CITATION_AMBIGUOUS`, fail-fast |
| inline slugs from the answer | ignored | **dropped silently** |

Assert an answer whose inline slugs are all unresolvable but whose array resolves is **accepted**. Assert an answer with no resolvable citation from either source is `CONTRACT_VIOLATION`, not a citation error. Assert `no_relevant_material` with any resolved citation, or without a non-empty gap, is `CONTRACT_VIOLATION`.

- [ ] **Step 7: Write failing ordering and namespacing tests**

Resolved inline slugs in answer order, then resolved array slugs in array order, deduplicated by first occurrence. The wiki ID is wrapper-supplied and a model-provided namespace is never trusted.

- [ ] **Step 8: Run tests and confirm failure**

```powershell
cargo test --test wiki_preflight --test citations
```

- [ ] **Step 9: Implement preflight and citations**

Do not use a general Markdown parser. Do not read any wiki file for configuration. Citation extraction is a bounded scanner over the answer string.

- [ ] **Step 10: Run tests**

Expected: PASS.

- [ ] **Step 11: Record the Task 6 checkpoint**

## Task 7: Implement Full-Content Mutation Snapshots

**Files:**

- Create: `src/snapshot.rs`
- Create: `tests/mutation_snapshot.rs`

- [ ] **Step 1: Write failing snapshot tests**

Use both Task 6 fixtures. Cover deterministic order, additions, removals, directory/file type changes, executable-helper changes, raw-source changes, byte changes, same-size rewrites with restored timestamps, binary files under `graph/`, unreadable files, and root escapes.

Assert the **only** excluded subtrees are `.claude/` and `.agents/` immediately beneath `content_root`, and that a change anywhere else — including `SCHEMA.md`, `config/`, `bin/`, `raw/`, `assets/`, and `graph/` — is detected.

- [ ] **Step 2: Define the snapshot shape**

```rust
pub struct SnapshotEntry {
    pub relative_path: String,
    pub kind: EntryKind,
    pub byte_len: Option<u64>,
    pub sha256: Option<[u8; 32]>,
}

pub struct WikiSnapshot { pub entries: Vec<SnapshotEntry> }
```

The entry list is also the input to citation resolution, so expose it without a second walk.

- [ ] **Step 3: Run tests and confirm failure**

```powershell
cargo test --test mutation_snapshot
```

- [ ] **Step 4: Implement streaming hashes**

Recursively enumerate the canonical `content_root`, skipping the two excluded directories at depth 1. Hash every regular file in bounded chunks; never load an entire file into memory solely for hashing. Normalize relative paths to `/`. Reject any symlink/junction/reparse/mount/special entry outside the excluded directories with `UNSAFE_FILESYSTEM_ENTRY`, and never descend into one.

- [ ] **Step 5: Implement comparison**

Return sorted, unique changed paths without bytes, hashes, or absolute roots.

- [ ] **Step 6: Run tests**

Expected: PASS.

- [ ] **Step 7: Record the Task 7 checkpoint**

## Task 8: Implement the Cross-Platform Process Supervisor

**Files:**

- Create: `src/process.rs`
- Create: `tests/process_supervisor.rs`, `tests/executable_resolution.rs`
- Create: `tests/fixtures/process-helper/Cargo.toml`, `tests/fixtures/process-helper/src/main.rs`

- [ ] **Step 1: Port the approved spike cases as failing production tests**

Do not copy spike implementation. Recreate tests for stdin/argv separation, dual-pipe pressure, stdout cap, stderr cap, timeout, non-zero exit, process-tree termination, complete reap, and `TERMINATION_FAILED` when a tree cannot be confirmed dead.

- [ ] **Step 2: Write failing executable-resolution tests**

Bare names via `PATH`/`PATHEXT`; absolute paths; missing files; rejected relative paths; directories; Windows `.exe`; Windows `.cmd`; spaces; metacharacters. Assert the canonical resolved path is reported for doctor.

- [ ] **Step 3: Write failing batch-shim adapter tests**

Using the quoting rule recorded in Task 2 Step 5, assert a `.cmd`/`.bat` shim receives the exact fixed provider argv with no batch expansion of `&`, `|`, `^`, `>`, `<`, `%VAR%`, or `!DELAYED!`; that paths containing spaces arrive as single arguments; and that the untrusted question never leaves stdin. Assert a `.exe` provider is launched directly with no shell.

- [ ] **Step 4: Define bounded request/outcome types**

```rust
pub struct ProcessRequest {
    pub executable: ResolvedExecutable,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub stdin: Vec<u8>,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

pub struct ProcessOutcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub elapsed: Duration,
    pub termination: TerminationReason,
}
```

- [ ] **Step 5: Run tests and confirm failure**

```powershell
cargo test --test process_supervisor --test executable_resolution
```

- [ ] **Step 6: Implement executable resolution and the batch shim**

Return a canonical classification (`Native` or Windows `BatchShim`). `BatchShim` uses the adapter proven in Task 2 and carries only the trusted fixed argv. Reject provider values containing raw arguments. Never accept the untrusted question in `args`.

- [ ] **Step 7: Implement concurrent bounded readers**

Start stdout and stderr reader threads before writing stdin. On overflow, signal the supervisor, terminate the tree, join both readers, and return the correct stream/limit/observed details.

- [ ] **Step 8: Implement process containment**

Use the exact Phase 0-approved `process-wrap` composition: Unix process group, Windows Job Object. Treat wrapper/spawn ordering as tested behavior, not an assumption.

- [ ] **Step 9: Implement deadline and cleanup**

Use `Instant`; close stdin, poll/wait, terminate on deadline, reap, and join readers on every return path.

All generated MCP config, Codex schema, and batch helper files use exclusive creation in a fresh system temp directory canonically outside every configured root, request user-only permissions, remain owned until spawn, and are removed only after reap. Claude's JSON schema stays an inline generated argument. Add a test for an injected unsafe temp root.

- [ ] **Step 10: Run native tests repeatedly**

```powershell
1..20 | ForEach-Object { cargo test --test process_supervisor -- --test-threads=1 }
cargo test --test executable_resolution
```

Expected: 20 clean passes with no surviving helper processes.

- [ ] **Step 11: Record the Task 8 checkpoint**

## Task 9: Implement Provider Adapters and the Prompt Envelope

**Files:**

- Create: `src/providers/mod.rs`, `src/providers/claude.rs`, `src/providers/codex.rs`
- Create: `src/probes.rs` — types and `ProbeReader` only
- Create: `tests/claude_adapter.rs`, `tests/codex_adapter.rs`, `tests/prompt_envelope.rs`
- Create: `tests/fixtures/claude/`, `tests/fixtures/codex/`

- [ ] **Step 1: Define probe types and the read-only gate**

Define `ProbeKey`, `ProbeRecord`, `ProbeReader`, `QueryMode`, and an in-memory `FakeProbeReader`. No filesystem access and no `ProbeWriter` in this task.

- [ ] **Step 2: Write failing prompt-envelope tests**

Assert the prompt is exactly: entrypoint token, blank line, `query_prompt`, blank line, `EXTERNAL_QUERY:` and the serialized object — in that order and nowhere else. Assert the object carries `contract`, `mode`, `wiki_id`, `content_root`, `question`, `required_result`, and `constraints` per Section 7.1. Assert the `constraints` array is the **provider-specific list from Section 7.1 (R-26)**: the Claude list states no shell tool exists; the Codex list permits read-only commands under the sandbox; the other five entries are byte-identical across providers. Assert the question is serialized by `serde_json` and never formatted into a string. Assert a `query_prompt` containing JSON-looking text cannot alter any envelope field.

- [ ] **Step 3: Write failing Claude argv tests**

The exact Section 10.2 vector; cwd is the selected `project_root`; `--add-dir <content_root>`; generated empty MCP config; `--json-schema` carrying the `wiki-query/v1` shape; optional `--plugin-dir`; and neither entrypoint, `query_prompt`, nor question in argv.

Reject compatibility changes that enable Write, Edit, shell, web, MCP, subagents, session persistence, or interactive escalation. Assert `CLAUDE_READ_SCOPE_BROAD` is emitted **only** when `content_root` is a strict subdirectory of `project_root`, and **not** when they are equal.

- [ ] **Step 4: Write failing Claude parser tests**

Valid structured output; error subtype; `is_error`; malformed JSON; missing structured result; non-zero exit; stderr warning; contract violation. Assert `raw_format` becomes `claude-json` only after a native document parses.

- [ ] **Step 5: Write failing Codex argv tests**

Top-level `--ask-for-approval never` before `exec`; `exec -C <project_root>`; `--sandbox read-only`; `--ephemeral`; `--skip-git-repo-check`; `--ignore-user-config`; `-c mcp_servers={}`; `--disable browser_use`; `--disable computer_use` (specification R-25 — `--ignore-user-config` cannot reach Codex's built-in `node_repl` MCP server); the temporary `--output-schema` file carrying the `wiki-query/v1` shape; `--json`; stdin `-`; and no Codex `--add-dir`.

Assert installed plugins are not supported under `--ignore-user-config`, a configured Codex `$plugin:skill` fails closed, and every Codex query carries `CODEX_READ_SCOPE_BROAD`.

- [ ] **Step 6: Write failing Codex parser tests**

Every non-empty line parsed as JSON; failed/error events; multiple messages; last completed agent message; `NO_FINAL_MESSAGE`; malformed JSONL; schema violation; bounded event diagnostics. Assert `raw_format` becomes `codex-jsonl` only after events parse.

Item-type discipline (specification §10.3, R-25/R-26): parse actual event items precisely — the top-level item `type` field only, never a recursive string scan. A `mcp_tool_call` item naming server `"codex"` with an introspection tool (e.g. `list_mcp_resources`) is benign and recorded in bounded diagnostics; assert that a fixture item naming **any other server** is recorded as a forbidden-capability signal, and the test asserts both behaviors.

- [ ] **Step 7: Write failing version/auth tests**

All probes use the configured resolved executable and the bounded supervisor, with the readiness commands confirmed in Task 2 Step 3. Cover authenticated; verified logged-out/nonzero mapped to `AUTH_REQUIRED`; malformed status mapped to `INVALID_NATIVE_OUTPUT`; ordinary process failures mapped normally. Fixtures contain no account identifiers or real wiki content.

- [ ] **Step 8: Run tests and confirm failure**

```powershell
cargo test --test prompt_envelope --test claude_adapter --test codex_adapter
```

- [ ] **Step 9: Implement the adapters**

Claude parses one native JSON document and prefers the schema-validated structured result. Codex parses bounded JSONL incrementally and selects the last valid completed agent message.

- [ ] **Step 10: Run tests**

Expected: PASS.

- [ ] **Step 11: Record the Task 9 checkpoint**

## Task 10: Implement the Single-Wiki Query Service

**Files:**

- Create: `src/query.rs`
- Create: `tests/query_service.rs`

- [ ] **Step 1: Write failing happy-path tests with fake providers**

Assert this order, which is specification Section 8.1 steps 3–18:

1. resolve exactly one allowlisted wiki ID and enabled provider;
2. validate the complete UTF-8 question against `max_question_bytes` **before any provider startup**;
3. canonicalize every configured path and verify containment, allowing root equality;
4. verify content-root minimum structure and emit `WIKI_SCHEMA_ABSENT` when applicable;
5. resolve the provider executable and run bounded version and non-billable auth probes;
6. fingerprint the selected artifact and, in `Enforced` mode, require exactly one matching current probe record; in `Verification` mode skip that gate;
7. build the prompt from entrypoint, `query_prompt`, and the `EXTERNAL_QUERY` object;
8. take the before snapshot of the content root and retain its entry list;
9. invoke the provider with separated stdin/stdout/stderr;
10. parse native output and validate `wiki-query/v1`;
11. extract, resolve, and namespace citations using the snapshot's entry list;
12. take the after snapshot in a guaranteed cleanup path;
13. reject mutation before returning any provider content;
14. build exactly one public envelope with `duration_ms`, `child_exit_code`, `raw_format`, and ordered warnings.

Assert no step reads, requires, or parses an index.

- [ ] **Step 2: Write failing question-validation tests**

Invalid stdin UTF-8 is `QUESTION_INVALID_UTF8`; over-limit is `QUESTION_TOO_LARGE`; both before executable resolution and with `child_exit_code: null`. Test the exact byte boundary and boundary+1 using multi-byte Traditional Chinese input.

- [ ] **Step 3: Write failing adversarial-input tests**

Traditional Chinese, multiline, leading dashes, quotes, backticks, `$()`, `&|<>^%!`, and JSON-looking text. Assert question bytes enter only serialized stdin and never appear in any recorded argv.

- [ ] **Step 4: Write failing failure-precedence tests**

Provider failure, timeout, oversized stream, malformed native output, invalid contract, invalid/missing/ambiguous citation, and `ENTRYPOINT_UNVERIFIED`, each combined with a mutation anywhere under `content_root`. `READ_ONLY_VIOLATION` wins over every exit `2`–`6` failure and preserves the displaced sanitized code/message as `secondary_error`; inability to recompute the snapshot becomes `INTERNAL_ERROR`.

- [ ] **Step 5: Write failing verification-mode tests**

`QueryMode::Verification` skips only the probe requirement and changes nothing else — same preflight, same snapshots, same contract and citation validation, same mutation dominance. It never writes a probe record itself.

- [ ] **Step 6: Run tests and confirm failure**

```powershell
cargo test --test query_service
```

- [ ] **Step 7: Implement `QueryService`**

Dependency-inject runner, providers, probe reader, and a monotonic clock. Do not print, parse CLI arguments, touch the probe cache file, or spawn processes directly.

- [ ] **Step 8: Implement sanitized diagnostics**

Warnings may include bounded stderr summaries, but never full prompts, wiki prose, tokens, or absolute protected paths.

- [ ] **Step 9: Run tests**

Expected: PASS.

- [ ] **Step 10: Record the Task 10 checkpoint**

## Task 11: Implement Probe Store, List, and Doctor

**Files:**

- Modify: `src/probes.rs`
- Create: `src/doctor.rs`
- Create: `tests/probes.rs`, `tests/doctor.rs`, `tests/list.rs`

- [ ] **Step 1: Write failing deterministic fingerprint tests**

Normalized UTF-8 path with `/` separators, one NUL byte, unsigned 64-bit big-endian length, raw bytes, for files sorted by ordinal relative path. Stored form is lowercase `sha256:` plus 64 hex digits. Reject escapes and executable plugin components.

**Assert `__pycache__/` directories and `*.pyc` / `*.pyo` files are excluded**, and that regenerating them does not change the fingerprint. Assert every other file in the skill directory does. Build the skill-directory fixture — including its `__pycache__/` content — at test run time in a temporary directory rather than under `tests/fixtures/`.

- [ ] **Step 2: Write failing probe-store tests**

All three platform cache paths; malformed and duplicate records; the exact logical key `(canonical content root, canonical project root, agent, load, entrypoint)`; and the separate current-verification tuple of executable path/version, skill fingerprint, and `compatibility_fingerprint`.

A successful live doctor deletes every record for the logical key and writes exactly one current record; historical fingerprints cannot accumulate or reactivate after rollback. No TTL. No prompt, answer, or wiki content stored. User-only permissions. Atomic temporary-file replacement in the same directory.

**Assert `query_prompt` participates in `compatibility_fingerprint`**: changing it invalidates the probe. Assert changing only timeout, byte limits, comments, or TOML key order does not.

- [ ] **Step 3: Write failing concurrency tests**

Two concurrent publishes for the same logical key leave exactly one valid record and never a truncated or interleaved file. A reader during a publish sees either the old or the new complete document.

- [ ] **Step 4: Write failing list tests**

Zero-wiki config returns an empty successful array with exit 0. Configured wikis return `id`, `title`, derived `default_agent`, and enabled `agents` without spawning a provider. `default_agent` is `null` when the global default is not enabled for that wiki, and omitting `--agent` from `query` is invalid exactly then. A config failure returns exit 2 with an empty `wikis` array and the top-level `error`.

- [ ] **Step 5: Write failing static-doctor tests**

Assert `checks[].name` is restricted to exactly nine values: `config`, `roots`, `wiki_structure`, `entrypoint`, `executable`, `auth`, `read_scope`, `live_contract`, `mutation`. Assert `profile` and `index_freshness` do not exist.

`checks[].code` is `null` for pass, a stable warning code for warn, or one Section 14 error code for fail. Static doctor covers every configured wiki/provider pair by default; `--wiki`/`--agent` narrow the matrix; it consumes zero model quota.

A warn-only matrix yields `ok: true` and exit 0. Any fail yields `ok: false` and the class from precedence `70, 7, 6, 5, 4, 3, 2, 0`. The `executable` check reports the canonical resolved provider path and recorded version without exposing unrelated environment values. Assert the exact `WIKI_SCHEMA_ABSENT`, `CLAUDE_READ_SCOPE_BROAD` (conditional), and `CODEX_READ_SCOPE_BROAD` messages; for Codex recommend only an OS sandbox or container, never a user permission profile disabled by `--ignore-user-config`.

- [ ] **Step 6: Write failing live-doctor tests**

`--live` requires both `--wiki` and `--agent`. All selected static checks run first. Live doctor calls `QueryService::query(.., QueryMode::Verification)` and publishes a probe **only** after valid native output, a valid result, resolvable citations, and identical before/after snapshots. A failure at any point publishes nothing and leaves an existing record untouched.

- [ ] **Step 7: Run tests and confirm failure**

```powershell
cargo test --test probes --test doctor --test list
```

- [ ] **Step 8: Implement fingerprints and the atomic probe store**

Implement `ProbeWriter` and the on-disk document. Query reads probes but never writes them.

- [ ] **Step 9: Implement list and doctor**

The fixed live question is:

```text
Verify external-readonly mode by reporting one fact from this wiki with a valid
citation. If the wiki has no pages, return no_relevant_material with a gap.
```

- [ ] **Step 10: Run tests**

Expected: PASS.

- [ ] **Step 11: Record the Task 11 checkpoint**

## Task 12: Complete the Public CLI

**Files:**

- Modify: `src/cli.rs`, `src/main.rs`
- Create: `tests/cli_contract.rs`

- [ ] **Step 1: Write failing argument tests**

Every public command; global `--config`/`--json`; rejection of relative `--config`; exactly one `--wiki`; rejection of repeated `--wiki` and of the literal `all`; nullable derived default agent — assert explicitly that `query` without `--agent` proceeds when the selected wiki's derived `default_agent` is non-null, and is rejected before any provider spawn when it is null (specification Section 5.1); positional question after `--`; stdin fallback when the positional is omitted; rejection when both are supplied; empty input; stable help and product name. Assert no `skill` or `agent-context` subcommand exists.

- [ ] **Step 2: Write failing output/exit tests**

Exactly one JSON document even for parse errors; stderr separation; human ordering; no panic or traceback on any path; every documented exit class; `wiki: null`/`agent: null` when argument failure precedes their resolution.

- [ ] **Step 3: Write failing external-cwd tests**

Run the compiled binary from an unrelated temporary directory with an explicit `--config`. Assert no cwd discovery, no walk of the caller's project, and correct resolution of config-relative wiki paths from the config file's own directory.

- [ ] **Step 4: Write failing Unicode and metacharacter tests**

Unicode config and wiki paths; Traditional Chinese questions via both the positional argument and stdin; leading dashes after `--`; quotes; backticks; shell metacharacters — none interpreted.

- [ ] **Step 5: Run tests and confirm failure**

```powershell
cargo test --test cli_contract
```

- [ ] **Step 6: Implement `clap` parsing and dispatch**

Keep `main.rs` minimal. Convert parse failures into `ARGUMENT_INVALID`; pre-detect the literal global `--json` only as far as needed to preserve the JSON error contract. Reading the question from stdin and enforcing `max_question_bytes` is delegated to `QueryService`; the CLI supplies the bytes and the source.

- [ ] **Step 7: Run all offline tests**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Expected: PASS.

- [ ] **Step 8: Record the Task 12 checkpoint**

## Task 13: Implement and Test Release Installers

**Files:**

- Create: `install.sh`, `install.ps1`
- Create: `tests/installers/verify-install-sh.sh`, `tests/installers/verify-install-ps1.ps1`

- [ ] **Step 1: Write installer contract tests**

Use a local fake Release server through a test-only override accepted only when an explicit `LLM_WIKIS_INSTALLER_TEST=1` guard is present. Production execution ignores alternate origins and uses the fixed GitHub repository. Do not download public assets in offline tests. Cover latest, pinned `LLM_WIKIS_VERSION`, checksum mismatch, missing checksum tool, smoke failure, unsupported architecture with explicit errors for Intel macOS and unsupported Linux architectures, temp cleanup on success and failure, and reinstall/upgrade/downgrade.

- [ ] **Step 2: Write PATH idempotency tests**

macOS zsh updates `~/.zprofile` once; Linux/bash/sh updates `~/.profile` once; unsupported shells print instructions without modifying profiles; Windows user PATH contains `%LOCALAPPDATA%\llm-wikis\bin` once.

- [ ] **Step 3: Run tests and confirm failure**

On Windows:

```powershell
pwsh -NoProfile -File tests/installers/verify-install-ps1.ps1
```

On Linux/macOS:

```sh
sh tests/installers/verify-install-sh.sh
```

Each platform runs only its own script; the other platform's row is `PENDING` until a native run or Task 14 CI covers it. Do not run the POSIX script through a Windows emulation layer and report it as native coverage.

- [ ] **Step 4: Implement `install.sh` from the apm-go flow**

`set -eu`, `mktemp -d`, a quoted cleanup trap, `curl -fsSL`, fail-closed SHA-256 verification via `sha256sum` or `shasum`, pre-install `--version`, atomic replacement within `~/.local/bin` where the filesystem permits.

- [ ] **Step 5: Implement `install.ps1`**

Strict error handling, `Invoke-WebRequest`, `Get-FileHash`, a unique temp directory, `--version`, `%LOCALAPPDATA%\llm-wikis\bin`, user-scope PATH update without process-wide string evaluation.

- [ ] **Step 6: Run installer tests twice**

Expected: both runs pass and the second adds no PATH or profile duplicate.

- [ ] **Step 7: Record the Task 13 checkpoint**

## Task 14: Add Repository Metadata, CI, and Release Automation

**Files:**

- Create: `README.md`, `LICENSE`, `.gitignore`
- Modify: `Cargo.toml`
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Create: `tests/release/verify-assets.ps1`, `tests/release/verify-assets.sh`

> **Blocked by D2.** This task requires a separate user decision to create a Git repository with a GitHub remote, Actions enabled, and Release/attestation permissions. Tasks 3–13 and 15 continue while it is blocked, but **no `v0.1.0` release may be claimed, and every native row owned here stays `PENDING`.** Do not initialize Git, create a remote, or push without that decision.
>
> This task also owns the rows moved out of Task 2: the three-target native smoke build, and the Linux and macOS process-tree termination instances.

- [ ] **Step 1: Create repository metadata**

`README.md` covers what the tool does, the three supported platforms, install one-liners, a minimal config, and a link to `docs/llm-wikis.md`. `LICENSE` records the chosen licence. `.gitignore` excludes `target/`, `spikes/target/`, and any local probe cache copied in for debugging.

Add `description`, `license`, `repository`, and `readme` to `Cargo.toml`, matching the decisions taken here.

- [ ] **Step 2: Add offline CI**

`windows-2025`, `ubuntu-24.04`, and ARM64 `macos-15`. Pin reviewed major action versions. Run format, Clippy, all Rust tests including the specification drift test, platform process tests, and installer tests. Never invoke Claude or Codex.

- [ ] **Step 3: Close the process rows moved from Task 2**

Run `tests/process_supervisor` natively on Ubuntu and macOS ARM64, including grandchild termination and reap. Record each as `PASS`, replacing its Task 2 `PENDING`.

- [ ] **Step 4: Add tag/version validation**

For tags matching `v*`, compare the tag without `v` to `cargo metadata --no-deps --format-version 1`. Mismatch fails before build.

- [ ] **Step 5: Add native release builds**

```text
x86_64-pc-windows-msvc
x86_64-unknown-linux-musl
aarch64-apple-darwin
```

Install `musl-tools` on Ubuntu. Rename outputs exactly:

```text
llm-wikis-windows-amd64.exe
llm-wikis-linux-amd64
llm-wikis-darwin-arm64
```

- [ ] **Step 6: Smoke-test each renamed asset natively**

Run `--version` and expect `llm-wikis 0.1.0` before uploading artifacts. This closes the target-smoke rows moved from Task 2.

- [ ] **Step 7: Generate and independently verify `SHA256SUMS`**

The manifest includes exactly the three binaries, `install.sh`, and `install.ps1`. Verification scripts reject missing, duplicate, or unexpected entries.

- [ ] **Step 8: Add artifact attestations**

GitHub's official build-provenance action with `id-token: write` and `contents: read`. Attest the executables and the checksum manifest; do not make `gh` a client installer prerequisite.

- [ ] **Step 9: Publish only after the full matrix succeeds**

One immutable GitHub Release per tag, attaching:

```text
llm-wikis-windows-amd64.exe
llm-wikis-linux-amd64
llm-wikis-darwin-arm64
install.ps1
install.sh
SHA256SUMS
```

A pending or failed native Windows, Linux, or macOS row blocks the entire version 0.1.0 release; never publish a partial three-asset set. Paid provider live rows are reported separately and do not block binary publication.

- [ ] **Step 10: Validate workflow syntax and scope**

Run available local workflow linting, then:

```powershell
rg -n "claude|codex" .github/workflows
```

Expected: provider names appear only in comments asserting they are not called, or not at all.

- [ ] **Step 11: Record the Task 14 checkpoint**

If still blocked, record `BLOCKED` with the exact external prerequisite and leave every release and native row `PENDING`.

## Task 15: Write Operator Documentation and Run Live Rows

**Files:**

- Create: `docs/llm-wikis.md`
- Modify: `docs/verification/llm-wikis-execution.md`, `docs/verification/llm-wikis-preflight.md`
- Add only sanitized fixtures under `tests/fixtures/claude/`, `tests/fixtures/codex/`

- [ ] **Step 1: Document installation**

Latest and pinned commands, asset names, the Windows and Unix install paths, PATH changes, checksum behavior, Apple Silicon-only support, the unsigned macOS Gatekeeper approval flow, and the Windows SmartScreen warning.

Document manual removal, since version 0.1.0 ships no uninstaller: delete the installed binary and remove the PATH entry from the user environment or from `~/.zprofile`/`~/.profile`.

- [ ] **Step 2: Document configuration**

All three config paths and all three cache paths. **`--config` as a trust boundary**: absolute-only, an operator and testing feature, never accepted from an untrusted caller, never discovered from the caller's project. Non-overwriting `config init`. Multiple wikis. Per-wiki provider tables. `query_prompt`, its constraints, and that changing it invalidates the live probe. Provider executable overrides. Project skills and Claude local plugins. Codex installed plugins deferred under `--ignore-user-config`.

State plainly that **`llm-wikis` never modifies a knowledge base**, that each wiki's own skill is used exactly as shipped, and that a changed skill fingerprint makes query fail `ENTRYPOINT_UNVERIFIED` until `doctor --live` is rerun.

- [ ] **Step 3: Document query and security contracts**

Commands, stdin transport, JSON envelopes, exit classes, the read-only enforcement layers, full-content mutation detection, probe cache paths, live doctor's model-quota cost, both read-scope warnings and their OS-sandbox recommendation, and troubleshooting per error class.

Document three things operators will otherwise get wrong:

- **The wrapper never reads or validates an index.** A wiki with a malformed, duplicated, or absent index still works. Do not "fix" a wiki's index to satisfy a rule that does not exist.
- **A generated index should still be regenerated before querying**, because a stale or missing index degrades the *skill's* navigation. This is advice, not an enforced gate.
- **`WIKI_SCHEMA_ABSENT` usually means `content_root` points one level too high**, which is expensive rather than fatal.

- [ ] **Step 4: Run complete offline gates**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
pwsh -NoProfile -File tests/installers/verify-install-ps1.ps1
```

Run POSIX installer tests on Ubuntu and macOS where available; record unavailable platforms as `PENDING`.

- [ ] **Step 5: Run static doctor from an unrelated directory**

Use a temporary config registering both real wikis and run both providers from a temp directory outside this repository. Expected: one JSON document per command, zero live model calls, `CLAUDE_READ_SCOPE_BROAD` present for `harness-engineering` and absent for `agents`, and no `WIKI_SCHEMA_ABSENT` for either.

- [ ] **Step 6: Run explicitly authorized live rows**

Execute specification Section 16.2's nine-row matrix. Each row records provider version, duration, raw format, warnings, citations, and before/after protected-tree digests.

Every row requires explicit user authorization because it consumes model quota and reads a real knowledge base. Unexecuted rows are `PENDING`. **Do not infer success for an unexecuted row and do not claim a platform, provider, or wiki capability whose row is pending.**

Record for each row whether the wiki's skill attempted a forbidden action and was blocked — the accepted residual risk of specification Section 7.2 — so its real frequency is documented rather than assumed.

- [ ] **Step 7: Verify the alternate load mode**

Use a temporary fixture root for a Claude local plugin, proving the entrypoint is configuration rather than a product constant. Assert a Codex installed-plugin configuration fails closed as out of scope.

- [ ] **Step 8: Sanitize fixtures and evidence**

Remove session and account identifiers, absolute user paths, full prompts, timings that identify accounts, and tokens. **Remove real wiki prose**: live-row evidence records digests, counts, and codes, not content.

- [ ] **Step 9: Record the Task 15 checkpoint**

## Task 16: Independent Row-by-Row Verification and Final Review

**Files:**

- Read: all implementation files
- Read: `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`
- Modify by independent verifier only: `docs/verification/llm-wikis-v0.1.0-checklist.md`
- Create by independent verifier only: `docs/verification/evidence/llm-wikis-v0.1.0/`
- Modify for confirmed fixes only: affected source/test files

- [ ] **Step 1: Freeze implementation for verification**

Without Git, record a SHA-256 manifest of every file under `src/`, `tests/`, `install.sh`, `install.ps1`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and `config.example.toml` in the execution record. Any change to a listed digest during verification restarts the affected rows.

- [ ] **Step 2: Dispatch an independent checklist verifier**

The verifier did not implement the feature and is not the Task 1 checklist author. It executes one row at a time and changes only `status` and `evidence`.

- [ ] **Step 3: Record exact evidence per row**

Each `PASS` includes command/inspection, exit status, platform, and a sanitized artifact under the evidence directory. Unavailable live, platform, and release checks remain `PENDING`.

- [ ] **Step 4: Revalidate the frozen checklist requirements**

Recreate the Task 1 canonical UTF-8/LF stream from the six immutable columns, recompute SHA-256 and row count, and compare both with the baseline. Any mismatch without a recorded user-approved baseline replacement fails the gate; status and evidence changes do not affect this digest.

- [ ] **Step 5: Route each failure through TDD**

For every `FAIL`, dispatch a fresh fix worker using `@systematic-debugging` and `@test-driven-development`. Add a failing regression test, implement the minimum fix, rerun the affected row, then rerun the full offline suite and refresh the Step 1 manifest.

- [ ] **Step 6: Use `@requesting-code-review`**

Review correctness, readability, architecture, security, and performance. Treat as blocking: shell interpolation, unbounded output, incomplete process-tree cleanup, mutation-check gaps, configuration trust expansion, contract drift, probe-gate bypass, release-asset mismatch, and **any code path that writes to a knowledge base**. Explicitly confirm no MCP or orchestration code exists.

- [ ] **Step 7: Apply findings with `@receiving-code-review`**

Technically verify every recommendation. Do not accept advisory style changes that expand scope.

- [ ] **Step 8: Repeat affected checklist rows**

Only the independent verifier may change a failed row to passed, with new evidence.

- [ ] **Step 9: Run the final complete gate**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Plus native installer, process, and release verification on Windows x64, Ubuntu x64, and macOS ARM64 where reachable.

- [ ] **Step 10: Use `@verification-before-completion`**

Report separately:

- offline Rust tests, including the specification drift test;
- Windows/Linux/macOS native binary and process tests;
- installer tests per platform;
- live rows LIVE-01 through LIVE-09 by platform, provider, and wiki;
- Claude local-plugin row status and Codex installed-plugin deferral;
- unsigned macOS and Windows warnings;
- GitHub Release status, including whether Task 14 remained blocked;
- **a positive statement that no file in either knowledge base was created, modified, or deleted**, backed by the live rows' before/after digests.

Do not claim completion while any required non-live row is failed or pending. Do not claim a live or platform capability whose row remains pending.

- [ ] **Step 11: Record the Task 16 checkpoint**

## Final Handoff Condition

Implementation is ready for release only when:

- Task 0 recorded the environment and standing decisions, Task 1 completed, and **Task 2 passed on the implementation platform** before production code began, with every unreachable row recorded `PENDING` and its reason;
- all required offline, process, installer, and native-binary checklist rows are independently `PASS`;
- every remaining `PENDING` row is an unreachable-platform or paid-provider row, is listed explicitly, and blocks only the specific support claim it underwrites;
- the plan/specification review loop is approved, including specification version 0.2.3 and its Revision History;
- no file in either knowledge base was created, modified, or deleted;
- `development-handoff.md` remains absent;
- MCP and orchestration remain unimplemented.

While Task 14 stays blocked by D2, the correct terminal state is **"implementation complete, release blocked on repository decision"** — not "released".
