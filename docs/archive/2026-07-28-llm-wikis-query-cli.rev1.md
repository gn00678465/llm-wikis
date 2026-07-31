# LLM Wikis Rust Query CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and release `llm-wikis` 0.1.0 as a read-only Rust CLI that queries exactly one configured wiki through Claude Code or Codex, with no Python runtime dependency.

**Architecture:** One synchronous Rust binary owns strict TOML configuration, canonical path resolution, platform process supervision, provider-specific argv/native-output parsing, `wiki-query/v1` normalization, citation validation, full-content mutation detection, doctor probes, and public output. The CLI calls an internal `QueryService`; future MCP and orchestration layers may call the same service but are not implemented here.

**Tech Stack:** Rust 1.97.1 and edition 2024, Cargo, `clap`, `serde`, `serde_json`, `toml`, `thiserror`, `sha2`, `tempfile`, a Phase-0-validated `process-wrap` standard-library frontend, Claude Code CLI, Codex CLI, POSIX shell, PowerShell, and GitHub Actions.

---

## Authoritative Inputs

- Specification: `docs/superpowers/specs/2026-07-28-llm-wikis-external-query-design.md`
- Plan: `docs/superpowers/plans/2026-07-28-llm-wikis-query-cli.md`
- Upstream interactive baseline: `agents/apm_modules/kfchou/wiki-skills/skills/wiki-query/SKILL.md`
- Project-owned external-query overlay: `agents/overrides/wiki-query/SKILL.md`
- Deployed Codex skill: `agents/.agents/skills/wiki-query/SKILL.md`
- Deployed Claude skill: `agents/.claude/skills/wiki-query/SKILL.md`
- Existing wiki helper tests: `agents/apm_modules/kfchou/wiki-skills/tests/`
- Installer reference: `https://raw.githubusercontent.com/gn00678465/apm-go/refs/heads/main/install.sh`

The intentionally removed `development-handoff.md` is not an input and must not be recreated.

## Execution Governance

This implementation must not be performed by the main/orchestrating session. The main session may dispatch work, preserve scope, and route findings, but each production task is owned by a fresh implementation worker. Use `@subagent-driven-development`; if that is unavailable, start a separate `@executing-plans` session.

Before production code:

1. an independent review session creates the acceptance checklist in Task 1;
2. a separate spike worker completes Task 2;
3. any failed assumption returns the spec and plan to review.

After production code, a separate verifier executes the checklist one row at a time. Implementers cannot author checklist requirements, mark their own work as passed, or edit failed rows into weaker assertions.

This workspace is currently not a Git repository. Before implementation, ask the user whether to initialize/use Git or work without commits. Do not initialize Git, create a worktree, or change repository state without that answer. When Git is approved, use `@using-git-worktrees` before Task 3 and make the commit checkpoints below. Without Git, record equivalent task checkpoints in `docs/verification/llm-wikis-execution.md` and skip commit commands.

## Scope Locks

Version 0.1.0 includes only:

```text
llm-wikis --version
llm-wikis [--config <absolute-path>] [--json] config init
llm-wikis [--config <absolute-path>] [--json] list
llm-wikis [--config <absolute-path>] [--json] doctor [--wiki <id>] [--agent claude|codex] [--live]
llm-wikis [--config <absolute-path>] [--json] query --wiki <id> [--agent claude|codex] -- <question>
```

Do not add MCP, orchestration, multi-wiki fan-out, direct retrieval APIs, answer saving, operation logging, index regeneration, a configuration wizard, self-update, an uninstaller, package-manager manifests, Intel macOS, or platform code signing.

## Target File Map

| Path | Responsibility |
|---|---|
| `Cargo.toml` | Package metadata, dependencies, release profile, spike exclusion |
| `Cargo.lock` | Reproducible dependency graph |
| `rust-toolchain.toml` | Pinned Rust 1.97.1 toolchain |
| `src/main.rs` | Minimal executable boundary and exit code |
| `src/lib.rs` | Internal module exports for integration tests |
| `src/cli.rs` | `clap` command model and dispatch |
| `src/config.rs` | Strict TOML schema, platform paths, initialization, resolution |
| `src/error.rs` | Stable error codes, exit classes, sanitized details |
| `src/model.rs` | Contract and public output types |
| `src/output.rs` | Human and exactly-one-document JSON rendering |
| `src/wiki.rs` | Wiki structure, SCHEMA/link-style parsing, freshness |
| `src/snapshot.rs` | Streaming SHA-256 snapshots of the complete content root |
| `src/citations.rs` | Native citation extraction and validation |
| `src/process.rs` | Executable resolution, bounded pipes, timeout, process-tree lifecycle |
| `src/providers/mod.rs` | Provider trait and shared request/result types |
| `src/providers/claude.rs` | Claude argv, JSON parser, version/auth probes |
| `src/providers/codex.rs` | Codex argv, JSONL parser, version/auth probes |
| `src/query.rs` | Single-wiki Query Service workflow |
| `src/probes.rs` | Probe identity, fingerprints, atomic cache |
| `src/doctor.rs` | Static/live doctor matrix and list operation |
| `tests/` | Rust integration, CLI, contract, security, and platform tests |
| `tests/fixtures/process-helper/` | Separate child/grandchild process test crate |
| `spikes/` | Disposable pre-production capability experiments |
| `config.example.toml` | Documented configured-registry example |
| `install.ps1` | Windows x64 release installer |
| `install.sh` | Linux x64/macOS ARM64 release installer |
| `agents/bin/deploy-wiki-query-overlay.ps1` | Restore the project-owned query overlay after APM sync on Windows |
| `agents/bin/deploy-wiki-query-overlay.sh` | Restore the project-owned query overlay after APM sync on POSIX |
| `.github/workflows/ci.yml` | Offline quality and native platform tests |
| `.github/workflows/release.yml` | Tagged native builds, checksums, attestations, Release |
| `docs/llm-wikis.md` | Operator guide, install/config/query/security |
| `docs/verification/llm-wikis-preflight.md` | Sanitized Phase 0 results |
| `docs/verification/llm-wikis-execution.md` | Task checkpoints and test evidence |
| `docs/verification/llm-wikis-v0.1.0-checklist.md` | Independently authored acceptance checklist |
| `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json` | Independent immutable-column digest and row count |
| `docs/verification/evidence/llm-wikis-v0.1.0/` | Sanitized row-by-row verification evidence |

## Core Interfaces

Keep these boundaries stable unless Phase 0 proves them infeasible:

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

pub struct QueryService<R: ProcessRunner> {
    runner: R,
    providers: ProviderRegistry,
}

impl<R: ProcessRunner> QueryService<R> {
    pub fn query(&self, request: QueryRequest) -> Result<QueryEnvelope, AppError>;
}
```

The provider trait receives only validated, resolved values. CLI parsing, terminal rendering, TOML parsing, and direct process creation do not leak into `QueryService`.

## Task 1: Create the Independent Acceptance Checklist

**Files:**

- Create by independent reviewer only: `docs/verification/llm-wikis-v0.1.0-checklist.md`
- Create by independent reviewer only: `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`
- Create: `docs/verification/llm-wikis-execution.md`
- Read: `docs/superpowers/specs/2026-07-28-llm-wikis-external-query-design.md`
- Read: `docs/superpowers/plans/2026-07-28-llm-wikis-query-cli.md`

- [ ] **Step 1: Dispatch a checklist author that has no implementation role**

Give it only the spec path, plan path, and this instruction:

```text
Create an acceptance checklist before implementation. Map every normative spec
requirement to one independently executable check. Use one row per behavior with:
ID, requirement, platform, phase, exact command or inspection, expected result,
status=PENDING, evidence=empty. Separate offline, process, installer, live-provider,
and release checks. Do not implement code and do not weaken the spec.
```

- [ ] **Step 2: Confirm checklist provenance**

The file must identify the independent author/session and state that the main session and implementation workers may not edit requirement/expected-result columns. It must also reserve final status/evidence updates for a second independent verifier that is not the checklist author.

- [ ] **Step 3: Check coverage mechanically**

Run:

```powershell
rg -n "^\\| [A-Z]+-[0-9]+" docs/verification/llm-wikis-v0.1.0-checklist.md
rg -n "PENDING|offline|installer|live|release|Windows|Linux|macOS" docs/verification/llm-wikis-v0.1.0-checklist.md
```

Expected: checklist rows exist, all begin `PENDING`, and every verification class/platform appears.

- [ ] **Step 4: Dispatch a different reviewer for checklist-to-spec coverage**

The reviewer may report omissions but must not implement. The checklist author fixes confirmed omissions before Task 2.

- [ ] **Step 5: Freeze the checklist requirements**

Canonicalize each checklist row's immutable columns in this exact order: `ID`, `requirement`, `platform`, `phase`, `command_or_inspection`, `expected_result`, joined with UTF-8 LF and no trailing whitespace. Record their SHA-256 and row count in `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`, and record the author/session plus baseline path in `docs/verification/llm-wikis-execution.md`. Later status/evidence edits are verifier-owned; immutable-column edits require user approval.

## Task 2: Run Disposable Pre-Implementation Capability Spikes

**Files:**

- Create: `spikes/Cargo.toml`
- Create: `spikes/src/main.rs`
- Create: `spikes/src/bin/process-tree-child.rs`
- Create: `spikes/fixtures/project/.claude/skills/spike-query/SKILL.md`
- Create: `spikes/fixtures/project/.agents/skills/spike-query/SKILL.md`
- Create: `spikes/fixtures/project/wiki-data/SCHEMA.md`
- Create: `spikes/fixtures/project/wiki-data/wiki/index.md`
- Create: `spikes/fixtures/project/wiki-data/wiki/pages/spike-page.md`
- Create: `spikes/README.md`
- Create: `docs/verification/llm-wikis-preflight.md`
- Do not create or modify production `src/` files

- [ ] **Step 1: Establish the Rust prerequisite**

Run:

```powershell
rustc --version
cargo --version
```

Expected: Rust 1.97.1. If Rust is absent, the execution operator installs Rust 1.97.1 with `rustup` before continuing; do not silently install it from the main session.

- [ ] **Step 2: Scaffold the isolated spike crate**

Use package name `llm-wikis-spikes`, edition 2024, and dependencies:

```toml
process-wrap = { version = "9.1", features = ["std"] }
serde_json = "1"
sha2 = "0.10"
tempfile = "3"
```

The README must state that spike code is evidence only and cannot be copied into production.

- [ ] **Step 3: Record provider capability surfaces**

Run:

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

For each provider, record the minimal tool/capability set that both invokes the configured skill and excludes Write, Edit, shell, web, MCP, subagents, session persistence, and interactive permission escalation. A flag substitution that restores a forbidden capability is a failed spike.

- [ ] **Step 4: Prove stdin and argv separation**

The spike launches a fixture child with fixed argv and sends:

```text
繁體中文
--leading-dash
"quotes" `backticks` $() & | < > ^ % !
```

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- stdin-boundary
```

Expected: byte-for-byte stdin round trip; fixture argv contains no question substring.

- [ ] **Step 5: Prove Windows executable resolution**

On Windows, exercise the resolved native Claude `.exe` and Codex `.cmd`/`.exe` shape, plus a fixture shim under a path containing spaces. Test fixed path arguments containing representative batch metacharacters.

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- windows-resolution
```

Expected: exact expected argv at the fixture, no shell expansion, and explicit classification of `.exe` versus `.cmd`.

- [ ] **Step 6: Prove platform config/cache directory resolution**

Inject environment/home/platform values into the spike resolver and verify all six config/cache results from spec Sections 5.2 and 15.1, including the macOS paths containing spaces.

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- platform-dirs
```

Expected: exact Windows, Linux/XDG-fallback, and macOS config/cache paths with no cwd dependency.

- [ ] **Step 7: Prove bounded concurrent output**

The fixture alternates stdout/stderr output beyond each configured limit and has a mode that fills one pipe while blocking on the other.

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- bounded-pipes
```

Expected: no deadlock; correct stream and observed byte count reported; process tree terminated.

- [ ] **Step 8: Prove timeout and process-tree termination**

The helper spawns a grandchild and writes both PIDs to a temporary file. Exercise success, timeout, and forced overflow.

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- process-tree
```

Expected: after every forced termination, neither PID remains alive. Repeat natively on Windows, Ubuntu, and macOS ARM runners before production process code.

- [ ] **Step 9: Prove full-content mutation detection**

Create a temporary wiki, snapshot it, rewrite a file with the same byte length, restore its timestamp, and compare.

Run:

```powershell
cargo run --manifest-path spikes/Cargo.toml -- mutation-hash
```

Expected: changed relative path is detected despite equal size and timestamp.

- [ ] **Step 10: Query a disposable contract fixture through each installed provider**

Use `spikes/fixtures/project` as `project_root` and its contained `wiki-data` directory as the distinct `content_root`, with the exact proposed argv. The disposable fixture skills implement the minimal read-only `wiki-query/v1` behavior without importing or modifying the future production overlay. Capture sanitized Claude JSON and Codex JSONL shapes and hash the disposable content tree before/after.

Expected: valid `wiki-query/v1`, at least one resolvable citation, and identical protected-tree hashes. These calls consume quota and must be run deliberately.

- [ ] **Step 11: Verify native target smoke builds**

Run native spike CI on:

```text
windows-2025          x86_64-pc-windows-msvc
ubuntu-24.04          x86_64-unknown-linux-musl
macos-15 (ARM64)      aarch64-apple-darwin
```

Expected: each binary runs its version smoke command on the same architecture that built it. When no user-authorized GitHub repository/Actions runner exists yet, record these rows `PENDING`; Task 15 must satisfy every row before the all-platform release.

- [ ] **Step 12: Write the preflight report**

For each assumption record `PASS`, `FAIL`, or `PENDING`, exact sanitized command, platform, tool versions, and evidence path in `docs/verification/llm-wikis-preflight.md`.

- [ ] **Step 13: Enforce the stop gate**

If any required local/provider assumption is `FAIL`, stop. Correct the spec and this plan, then repeat independent review. Windows is the current local implementation gate. Unavailable Linux/macOS rows remain explicitly `PENDING`; native GitHub runners may complete their process/target-smoke rows before release, while paid live-provider rows remain separately pending until explicitly run. Pending native rows do not block core implementation but block the entire three-asset version 0.1.0 release; pending paid live-provider rows block only their explicit support claims.

## Task 3: Scaffold the Production Rust Crate

**Files:**

- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `rust-toolchain.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/cli.rs`
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

Use:

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

`rust-toolchain.toml` pins `1.97.1` with `rustfmt` and `clippy`.

- [ ] **Step 3: Run the test and confirm failure**

Run:

```powershell
cargo test --test version_cli
```

Expected: FAIL because the executable boundary is not implemented.

- [ ] **Step 4: Implement the minimal executable boundary**

`src/cli.rs` provides the minimal `clap` product/version parser and `run()` needed for `--version`; later commands remain unimplemented. `src/main.rs` calls `llm_wikis::cli::run()` and exits with its returned code. `src/lib.rs` exports only the modules needed by integration tests.

- [ ] **Step 5: Run quality gates**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test version_cli
```

Expected: all exit 0.

- [ ] **Step 6: Commit if Git is available**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml src tests/version_cli.rs
git commit -m "feat: scaffold llm-wikis rust cli"
```

## Task 4: Implement Stable Models, Errors, and Output

**Files:**

- Create: `src/error.rs`
- Create: `src/model.rs`
- Create: `src/output.rs`
- Create: `tests/model_contract.rs`
- Create: `tests/output_contract.rs`

- [ ] **Step 1: Write failing model-contract tests**

Cover exactly:

```rust
pub enum KnowledgeStatus {
    Grounded,
    NoRelevantMaterial,
}

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

- [ ] **Step 2: Write failing error and envelope tests**

Assert every row in the spec's complete error table maps to its exact exit code, including question encoding/size, missing provider profile, entrypoint invalid/unverified, no final message, termination failure, citation failures, and integrity failure. Test that `READ_ONLY_VIOLATION` dominates exits `2`–`6`, preserves only a sanitized `secondary_error`, and that incomplete integrity comparison becomes `INTERNAL_ERROR`. JSON failures contain no traceback, prompt, absolute protected path, hash, or wiki content.

- [ ] **Step 3: Write failing renderer tests**

Assert JSON mode writes exactly one document plus one trailing newline to stdout and diagnostics only to stderr. Public warnings are closed `{source, code, message}` objects: `INDEX_MAY_BE_STALE`, `CLAUDE_READ_SCOPE_BROAD`, and `CODEX_READ_SCOPE_BROAD` retain stable codes and precede model strings normalized as `PROVIDER_WARNING`. Assert `raw_format` is only `claude-json`, `codex-jsonl`, or null. Human mode prints answer, then gaps, then warnings.

- [ ] **Step 4: Run tests and confirm failure**

```powershell
cargo test --test model_contract --test output_contract
```

Expected: FAIL with missing types/functions.

- [ ] **Step 5: Implement strict serde models and `AppError`**

Use `#[serde(deny_unknown_fields)]` on closed wire structures. Keep internal causes for diagnostics but expose only allowlisted sanitized details.

- [ ] **Step 6: Implement output rendering**

Rendering receives already-normalized envelopes. It performs no provider parsing or path access.

- [ ] **Step 7: Run tests**

```powershell
cargo test --test model_contract --test output_contract
```

Expected: PASS.

- [ ] **Step 8: Commit if Git is available**

```bash
git add src/error.rs src/model.rs src/output.rs tests/model_contract.rs tests/output_contract.rs
git commit -m "feat: define query result and error contracts"
```

## Task 5: Implement Strict Configuration and `config init`

**Files:**

- Create: `src/config.rs`
- Create: `config.example.toml`
- Create: `tests/config_contract.rs`
- Create: `tests/config_init.rs`

- [ ] **Step 1: Write failing platform-path tests**

Inject an environment abstraction rather than mutating process-global environment in parallel tests. Assert:

```text
Windows   %APPDATA%\llm-wikis\config.toml
Linux     ${XDG_CONFIG_HOME:-$HOME/.config}/llm-wikis/config.toml
macOS     $HOME/Library/Application Support/llm-wikis/config.toml
```

Also assert the cache paths from the spec.

- [ ] **Step 2: Write failing strict-schema tests**

Cover zero wikis, multiple wikis, profile reuse, positive `max_question_bytes`, provider command names, absolute executable paths, unknown keys, wrong config version, invalid IDs, missing profile, an enabled provider missing its global provider table, an enabled provider missing from its profile, unsupported contract, raw args, shell syntax, relative executable paths, and invalid entrypoints. Test every required/default rule from spec Section 6, including optional runtime fields and default executable names.

- [ ] **Step 3: Write failing path-resolution tests**

Cover config-relative project/content/plugin paths, containment, Unicode and spaces, and an absolute-only `--config` override. Check special status on components of each configured path. Recursively scan only the complete content root and selected skill/local-plugin artifact tree, not unrelated project-root subtrees. Unconditionally reject encountered symlinks, junctions, reparse points, mount points, and other special entries even when contained; use `PATH_OUTSIDE_ALLOWED_ROOT` only for canonical regular paths outside the root.

- [ ] **Step 4: Write failing `config init` tests**

Assert parent creation, valid zero-wiki TOML, commented example, `create_new` semantics, unchanged bytes and `CONFIG_EXISTS`/exit 2 when the target exists, and the exact `operation: "config_init"` human/JSON success/failure contracts.

- [ ] **Step 5: Run tests and confirm failure**

```powershell
cargo test --test config_contract --test config_init
```

Expected: FAIL.

- [ ] **Step 6: Implement configuration structs**

Use strict serde tables for `providers`, `runtime`, `query_profiles`, and `wikis`. Apply semantic validation only after syntactic deserialization.

- [ ] **Step 7: Implement platform paths and initialization**

`config init` uses an exclusive create operation. It never reads cwd for discovery and never overwrites or merges.

- [ ] **Step 8: Add the configured registry example**

`config.example.toml` contains the complete Agents example from the spec. The generated template contains an explicitly absolute placeholder wiki block only as comments and explains that relative paths resolve from the platform config directory.

- [ ] **Step 9: Run tests**

```powershell
cargo test --test config_contract --test config_init
```

Expected: PASS.

- [ ] **Step 10: Commit if Git is available**

```bash
git add src/config.rs config.example.toml tests/config_contract.rs tests/config_init.rs
git commit -m "feat: add trusted llm-wikis configuration"
```

## Task 6: Implement Wiki Preflight, Freshness, and Citation Rules

**Files:**

- Create: `src/wiki.rs`
- Create: `src/citations.rs`
- Create: `tests/wiki_preflight.rs`
- Create: `tests/citations.rs`

- [ ] **Step 1: Write failing wiki-structure tests**

Require `SCHEMA.md`, the exact `## Cross-References` machine subsection, optional contained link rules, `wiki/index.md`, and `wiki/pages/`. Test absent `link_style` defaults to `obsidian` and every unsupported style fails with `LINK_STYLE_UNSUPPORTED`.

- [ ] **Step 2: Write failing freshness tests**

Cover missing index, newer page, equal timestamp, coarse timestamp, an older-timestamp page missing from the index, duplicate/extra/case-mismatched generated-list entries, the implementation-owned `audit-*.md` exclusion on both compared sets, and incidental wikilinks on non-entry lines. Accept only exact built-in `- <page-link> — ...` entry shapes. Cover `warn`/`error` and assert query never invokes `generate-index.py`.

- [ ] **Step 3: Write failing citation tests**

Cover exact built-in `obsidian` and `markdown` page-reference parsers, inline/array merge order, outer delimiter removal, unsafe explicit-array targets, missing recognized page slugs, deduplication, and wrapper-owned wiki namespace. Prove that raw/assets/URL text, `[[raw/...]]`, `[[assets/...]]`, ordinary Markdown links, display-label wiki links, and malformed/dangling inline forms are ignored rather than rejected. Such text cannot be the sole evidence for `grounded`.

- [ ] **Step 4: Run tests and confirm failure**

```powershell
cargo test --test wiki_preflight --test citations
```

Expected: FAIL.

- [ ] **Step 5: Implement deterministic SCHEMA parsing**

Do not use a general Markdown parser. Implement the exact heading/field grammar from the spec and reject ambiguity.

- [ ] **Step 6: Implement freshness and citation validation**

All accepted page-provenance slugs use `[a-z0-9-]+` and map by an exact ASCII filename-stem directory-listing match to `<content_root>/wiki/pages/<slug>.md`; Windows case folding must not change acceptance. Inline slugs precede returned-array slugs, with first-occurrence deduplication. `CITATION_INVALID` applies strictly to explicit model-array elements; `CITATION_NOT_FOUND` applies to syntactically recognized page slugs. Both are fail-fast and discard the answer.

- [ ] **Step 7: Run tests**

```powershell
cargo test --test wiki_preflight --test citations
```

Expected: PASS.

- [ ] **Step 8: Commit if Git is available**

```bash
git add src/wiki.rs src/citations.rs tests/wiki_preflight.rs tests/citations.rs
git commit -m "feat: validate wiki structure and citations"
```

## Task 7: Implement Full-Content Mutation Snapshots

**Files:**

- Create: `src/snapshot.rs`
- Create: `tests/mutation_snapshot.rs`

- [ ] **Step 1: Write failing snapshot tests**

Build a representative complete `content_root` containing `SCHEMA.md`, `config/`, `bin/` including a hook, `raw/`, `assets/`, `wiki/index.md`, `wiki/overview.md`, `wiki/log.md`, and `wiki/pages/`. Cover deterministic order, additions, removals, directory/file type changes, executable-helper changes, raw-source changes, byte changes, same-size rewrites with restored timestamps, unreadable files, and root escapes. Assert there is no excluded subtree.

- [ ] **Step 2: Define the snapshot shape**

```rust
pub struct SnapshotEntry {
    pub relative_path: String,
    pub kind: EntryKind,
    pub byte_len: Option<u64>,
    pub sha256: Option<[u8; 32]>,
}

pub struct WikiSnapshot {
    pub entries: Vec<SnapshotEntry>,
}
```

- [ ] **Step 3: Run tests and confirm failure**

```powershell
cargo test --test mutation_snapshot
```

Expected: FAIL.

- [ ] **Step 4: Implement streaming hashes**

Recursively enumerate the entire canonical `content_root`. Hash every regular file in bounded chunks; never load an entire page, raw file, or binary asset into memory solely for hashing. Normalize stored relative paths to `/`; reject any symlink/junction/reparse/mount/special entry with `UNSAFE_FILESYSTEM_ENTRY`.

- [ ] **Step 5: Implement comparison**

Return sorted, unique changed paths without bytes, hashes, or absolute roots.

- [ ] **Step 6: Run tests**

```powershell
cargo test --test mutation_snapshot
```

Expected: PASS.

- [ ] **Step 7: Commit if Git is available**

```bash
git add src/snapshot.rs tests/mutation_snapshot.rs
git commit -m "feat: detect wiki mutations by content hash"
```

## Task 8: Implement the Cross-Platform Process Supervisor

**Files:**

- Create: `src/process.rs`
- Create: `tests/process_supervisor.rs`
- Create: `tests/executable_resolution.rs`
- Create: `tests/fixtures/process-helper/Cargo.toml`
- Create: `tests/fixtures/process-helper/src/main.rs`

- [ ] **Step 1: Port the approved spike cases as failing production tests**

Do not copy spike implementation. Recreate tests for stdin/argv separation, dual-pipe pressure, stdout cap, stderr cap, timeout, non-zero exit, process-tree termination, and complete reap.

- [ ] **Step 2: Write failing executable-resolution tests**

Cover bare names, `PATH`/`PATHEXT`, absolute paths, missing files, relative paths, directories, Windows `.exe`, Windows `.cmd`, spaces, and metacharacters.

- [ ] **Step 3: Define bounded request/outcome types**

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
}
```

- [ ] **Step 4: Run tests and confirm failure**

```powershell
cargo test --test process_supervisor --test executable_resolution
```

Expected: FAIL.

- [ ] **Step 5: Implement executable resolution**

Return a canonical classification (`Native` or Windows `BatchShim`). Reject provider values containing raw arguments. Never accept the untrusted question in `args`.

- [ ] **Step 6: Implement concurrent bounded readers**

Start stdout and stderr reader threads before writing stdin. On overflow, signal the supervisor, terminate the tree, join both readers, and return the correct stream/limit/observed details.

- [ ] **Step 7: Implement process containment**

Use the exact Phase 0-approved `process-wrap` composition: Unix process group and Windows Job Object. Treat wrapper/spawn ordering as tested behavior, not an assumption.

- [ ] **Step 8: Implement deadline and cleanup**

Use `Instant`; close stdin, poll/wait, terminate on deadline, reap, and join readers on every return path.

All generated MCP config, Codex schema, and batch helper files use exclusive creation in a fresh system temp directory canonically outside every configured project/content root, request user-only permissions, remain owned until spawn, and are removed only after process reap. Claude's JSON schema stays an inline generated argument. Add tests for an injected unsafe temp root.

- [ ] **Step 9: Run native tests repeatedly**

```powershell
1..20 | ForEach-Object { cargo test --test process_supervisor -- --test-threads=1 }
cargo test --test executable_resolution
```

Expected: 20 clean passes with no surviving helper processes.

- [ ] **Step 10: Commit if Git is available**

```bash
git add src/process.rs tests/process_supervisor.rs tests/executable_resolution.rs tests/fixtures/process-helper
git commit -m "feat: supervise provider process trees"
```

## Task 9: Implement Claude and Codex Adapters

**Files:**

- Create: `src/providers/mod.rs`
- Create: `src/providers/claude.rs`
- Create: `src/providers/codex.rs`
- Create: `tests/claude_adapter.rs`
- Create: `tests/codex_adapter.rs`
- Create: `tests/fixtures/claude/`
- Create: `tests/fixtures/codex/`

- [ ] **Step 1: Write failing Claude argv tests**

Assert the exact spec vector, selected `project_root` cwd, `--add-dir <content_root>`, generated empty MCP config, optional `--plugin-dir`, and no configured entrypoint or question in argv.

The accepted vector must satisfy the Phase 0 minimal-capability fixture. Reject compatibility changes that enable Write/Edit/shell/web/MCP/subagents/session persistence or interactive escalation. Assert every Claude query carries `CLAUDE_READ_SCOPE_BROAD`.

- [ ] **Step 2: Write failing Claude parser tests**

Cover valid structured output, error subtype, `is_error`, malformed JSON, missing structured result, non-zero exit, stderr warning, and contract violation using sanitized fixtures.

- [ ] **Step 3: Write failing Codex argv tests**

Assert top-level approval placement, `exec -C`, read-only sandbox, ephemeral mode, ignored user config, temporary schema, JSONL, stdin `-`, and absence of Codex `--add-dir`.

Assert Codex installed plugins are not loaded/supported under `--ignore-user-config` and every Codex query carries `CODEX_READ_SCOPE_BROAD`.

- [ ] **Step 4: Write failing Codex parser tests**

Cover every non-empty line as JSON, failed/error events, multiple messages, last completed agent message, no final message, malformed JSONL, and schema violation.

- [ ] **Step 5: Write failing version/auth tests**

All probes use the configured resolved executable and bounded process supervisor. Claude uses `auth status --json`; Codex uses `login status`. Cover authenticated, verified logged-out/nonzero behavior mapped to `AUTH_REQUIRED`, malformed status mapped to `INVALID_NATIVE_OUTPUT`, and ordinary process failures. Fixtures contain no account identifiers or real wiki content.

- [ ] **Step 6: Run tests and confirm failure**

```powershell
cargo test --test claude_adapter --test codex_adapter
```

Expected: FAIL.

- [ ] **Step 7: Implement provider request and prompt envelope**

Serialize the `EXTERNAL_QUERY` object with `serde_json`; never format the question into an ad-hoc JSON string.

- [ ] **Step 8: Implement Claude adapter**

Parse one native JSON document and prefer the schema-validated structured result.

- [ ] **Step 9: Implement Codex adapter**

Parse bounded JSONL incrementally and select the last valid completed agent message.

- [ ] **Step 10: Run tests**

```powershell
cargo test --test claude_adapter --test codex_adapter
```

Expected: PASS.

- [ ] **Step 11: Commit if Git is available**

```bash
git add src/providers tests/claude_adapter.rs tests/codex_adapter.rs tests/fixtures/claude tests/fixtures/codex
git commit -m "feat: add claude and codex query adapters"
```

## Task 10: Add the External Read-Only Skill Contract

**Files:**

- Read only: `agents/apm_modules/kfchou/wiki-skills/skills/wiki-query/SKILL.md`
- Create: `agents/overrides/wiki-query/SKILL.md`
- Modify: `agents/.agents/skills/wiki-query/SKILL.md`
- Modify: `agents/.claude/skills/wiki-query/SKILL.md`
- Create: `agents/bin/deploy-wiki-query-overlay.ps1`
- Create: `agents/bin/deploy-wiki-query-overlay.sh`
- Create: `tests/skill_contract.rs`

- [ ] **Step 1: Write failing skill contract tests**

Assert the project-owned overlay and both deployed copies contain one explicit early `external-readonly` branch, require explicit `content_root`, return `wiki-query/v1`, and forbid index generation, saves, logs, offers to save, and writes in that branch. Assert the upstream vendored dependency remains unmodified.

- [ ] **Step 2: Assert deployed copies match the canonical source**

Require exact byte equality between `agents/overrides/wiki-query/SKILL.md` and both deployed copies.

- [ ] **Step 3: Run tests and confirm failure**

```powershell
cargo test --test skill_contract
```

Expected: FAIL.

- [ ] **Step 4: Create the project-owned overlay from the upstream baseline**

Copy the current upstream interactive behavior into the new overlay, preserve it, and add an early external-mode branch that cannot fall through to index generation, logging, or save instructions. Do not patch the third-party file under `apm_modules`.

- [ ] **Step 5: Implement deterministic overlay deployment**

Both scripts resolve their own `agents/` root and replace the Claude/Codex deployed copies byte-for-byte from the overlay. They use no caller cwd assumptions and print that they must be rerun after `apm install` or `apm update`.

- [ ] **Step 6: Deploy and run Rust/existing helper tests**

```powershell
& agents/bin/deploy-wiki-query-overlay.ps1
cargo test --test skill_contract
$env:PYTHONUTF8='1'
python -m unittest discover -s agents\apm_modules\kfchou\wiki-skills\tests -v
```

Expected: Rust test passes and the existing helper suite remains green. Python is used only for the pre-existing wiki helper tests, not by the product.

- [ ] **Step 7: Commit if Git is available**

```bash
git add agents/overrides/wiki-query/SKILL.md agents/.agents/skills/wiki-query/SKILL.md agents/.claude/skills/wiki-query/SKILL.md agents/bin/deploy-wiki-query-overlay.ps1 agents/bin/deploy-wiki-query-overlay.sh tests/skill_contract.rs
git commit -m "feat: add external readonly wiki query mode"
```

## Task 11: Implement the Single-Wiki Query Service

**Files:**

- Create: `src/query.rs`
- Create: `tests/query_service.rs`

- [ ] **Step 1: Write failing happy-path tests with fake providers**

Assert this order:

1. load/resolve one allowlisted wiki and provider;
2. run preflight/freshness;
3. resolve the executable and run bounded version/auth status probes;
4. fingerprint the selected artifact and verify the one current live-probe record;
5. build the fixed prompt;
6. take the before snapshot;
7. invoke the provider;
8. parse/validate contract and citations;
9. take the after snapshot in a guaranteed cleanup path;
10. reject mutation before returning provider content;
11. build one public envelope.

- [ ] **Step 2: Write failing adversarial-input tests**

Use Traditional Chinese, multiline, leading dashes, quotes, backticks, `$()`, `&|<>^%!`, and JSON-looking text. Assert question bytes enter only serialized stdin.

- [ ] **Step 3: Write failing failure-precedence tests**

Cover stale error, provider failure, timeout, oversized stream, malformed native output, invalid contract, invalid/missing citation, and a mutation anywhere under `content_root` during each failure. `READ_ONLY_VIOLATION` wins over every exit `2`–`6` failure and preserves the displaced sanitized code/message; inability to recompute the snapshot becomes `INTERNAL_ERROR`.

- [ ] **Step 4: Run tests and confirm failure**

```powershell
cargo test --test query_service
```

Expected: FAIL.

- [ ] **Step 5: Implement `QueryService`**

Use dependency injection for runner/providers and monotonic time. Do not print, parse CLI arguments, or spawn directly.

- [ ] **Step 6: Implement sanitized diagnostics**

Warnings may include bounded stderr summaries and stale-index notices, but never full prompts, wiki prose, tokens, or absolute protected paths.

- [ ] **Step 7: Run tests**

```powershell
cargo test --test query_service
```

Expected: PASS.

- [ ] **Step 8: Commit if Git is available**

```bash
git add src/query.rs tests/query_service.rs
git commit -m "feat: implement single-wiki query service"
```

## Task 12: Implement Probes, List, and Doctor

**Files:**

- Create: `src/probes.rs`
- Create: `src/doctor.rs`
- Create: `tests/probes.rs`
- Create: `tests/doctor.rs`
- Create: `tests/list.rs`

- [ ] **Step 1: Write failing deterministic fingerprint tests**

Use normalized UTF-8 path, NUL separator, unsigned 64-bit big-endian length, and raw bytes in ordinal path order. Reject escapes and executable plugin components.

- [ ] **Step 2: Write failing probe-store tests**

Cover all three platform cache paths, malformed/duplicate records, the exact logical key `(roots, agent, profile, load, entrypoint)`, and the separate current-verification tuple. A successful live doctor must delete every record for the logical key and write exactly one current record; historical fingerprints cannot accumulate or reactivate after rollback. Cover `compatibility_fingerprint` naming, no TTL, no prompt/answer/wiki content, user-only permissions, and atomic replace. Changing skill/local-plugin/provider version/path/root/contract invalidates; changing only timeout, stream/question limits, freshness mode, comments, or TOML key order does not.

- [ ] **Step 3: Write failing skill/local-plugin fingerprint tests**

Require deterministic project-skill fingerprints. For Claude local plugins, include the manifest and selected skill tree, reject hooks/MCP/settings/lifecycle components, and reject every special filesystem entry. Require a current successful live probe before normal query for every supported project-skill and Claude local-plugin load mode. Assert Codex `$plugin:skill`/installed-plugin configuration is rejected as deferred in version 0.1.0.

- [ ] **Step 4: Write failing list tests**

Zero-wiki config returns an empty successful array. Configured wikis return title, effective default agent, and enabled agents without spawning providers.

- [ ] **Step 5: Write failing static/live doctor tests**

Static doctor checks all configured wiki/provider pairs and does not consume quota. Restrict `checks[].name` and `checks[].code` to the vocabularies in spec Section 15. Emit exact `CLAUDE_READ_SCOPE_BROAD`/`CODEX_READ_SCOPE_BROAD` warnings; for Codex recommend only an OS sandbox/container, never a user permission profile disabled by `--ignore-user-config`. Live doctor requires both selectors, calls `QueryService` in verification mode, publishes a probe only after valid output/citations/zero mutation, and follows error precedence `70,7,6,5,4,3,2,0`.

Assert the executable check reports the canonical resolved provider path and recorded version without exposing unrelated environment values.

- [ ] **Step 6: Run tests and confirm failure**

```powershell
cargo test --test probes --test doctor --test list
```

Expected: FAIL.

- [ ] **Step 7: Implement fingerprints and atomic probe store**

Query reads probes but never writes them. Only successful live doctor publishes a complete record.

- [ ] **Step 8: Implement list and doctor**

The fixed live question is:

```text
Verify external-readonly mode by reporting one fact from the index with a valid
wiki citation. If the wiki has no pages, return no_relevant_material with a gap.
```

- [ ] **Step 9: Run tests**

```powershell
cargo test --test probes --test doctor --test list
```

Expected: PASS.

- [ ] **Step 10: Commit if Git is available**

```bash
git add src/probes.rs src/doctor.rs tests/probes.rs tests/doctor.rs tests/list.rs
git commit -m "feat: add wiki list and doctor verification"
```

## Task 13: Complete the Public CLI

**Files:**

- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `tests/cli_contract.rs`

- [ ] **Step 1: Write failing argument tests**

Cover every public command, global `--config`/`--json`, exactly one wiki, rejection of `all`, nullable derived default agent, positional question after `--`, stdin fallback, both-input rejection, empty input, invalid stdin UTF-8, exact `max_question_bytes` boundary/overflow before provider startup, and stable help name.

- [ ] **Step 2: Write failing output/exit tests**

Assert exactly one JSON document even for parse errors, stderr separation, human ordering, no panic/traceback, every documented exit class, and `wiki: null`/`agent: null` whenever argument failure occurs before those values resolve.

- [ ] **Step 3: Write failing external-cwd tests**

Run the compiled binary from an unrelated temporary directory with an explicit config and assert no cwd discovery.

- [ ] **Step 4: Run tests and confirm failure**

```powershell
cargo test --test cli_contract
```

Expected: FAIL.

- [ ] **Step 5: Implement `clap` parsing and dispatch**

Keep `main.rs` minimal. Convert parse failures into `ARGUMENT_INVALID`; pre-detect literal global `--json` only as needed to preserve the JSON error contract.

- [ ] **Step 6: Run all offline tests**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Expected: PASS.

- [ ] **Step 7: Commit if Git is available**

```bash
git add src/cli.rs src/main.rs tests/cli_contract.rs
git commit -m "feat: expose llm-wikis commands"
```

## Task 14: Implement and Test Release Installers

**Files:**

- Create: `install.sh`
- Create: `install.ps1`
- Create: `tests/installers/verify-install-sh.sh`
- Create: `tests/installers/verify-install-ps1.ps1`

- [ ] **Step 1: Write installer contract tests**

Use a local fake Release server through a test-only override accepted only when an explicit `LLM_WIKIS_INSTALLER_TEST=1` guard is present. Production installer execution ignores/rejects alternate origins and uses the fixed GitHub repository. Do not download public assets in offline tests. Cover latest, pinned `LLM_WIKIS_VERSION`, checksum mismatch, missing checksum tool, smoke failure, unsupported architecture, temp cleanup, and reinstall.

- [ ] **Step 2: Write PATH idempotency tests**

Assert:

- macOS zsh updates `~/.zprofile` once;
- Linux/bash/sh updates `~/.profile` once;
- unsupported shells print instructions without modifying profiles;
- Windows user PATH contains `%LOCALAPPDATA%\llm-wikis\bin` once.

- [ ] **Step 3: Run tests and confirm failure**

```powershell
pwsh -NoProfile -File tests/installers/verify-install-ps1.ps1
```

On Linux/macOS:

```sh
sh tests/installers/verify-install-sh.sh
```

Expected: FAIL because installers do not exist.

- [ ] **Step 4: Implement `install.sh` from the apm-go flow**

Use `set -eu`, `mktemp -d`, a quoted cleanup trap, `curl -fsSL`, fail-closed SHA-256 verification, pre-install `--version`, and atomic replacement within `~/.local/bin` where the filesystem permits.

- [ ] **Step 5: Implement `install.ps1`**

Use strict error handling, `Invoke-WebRequest`, `Get-FileHash`, a unique temp directory, `--version`, `%LOCALAPPDATA%\llm-wikis\bin`, and user-scope PATH update without process-wide string evaluation.

- [ ] **Step 6: Run installer tests twice**

Expected: both runs pass and the second run adds no PATH/profile duplicate.

- [ ] **Step 7: Commit if Git is available**

```bash
git add install.sh install.ps1 tests/installers
git commit -m "feat: install llm-wikis release binaries"
```

## Task 15: Add CI and GitHub Release Automation

**Files:**

- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`
- Create: `tests/release/verify-assets.ps1`
- Create: `tests/release/verify-assets.sh`

This task is blocked until the user authorizes/establishes a Git repository with a GitHub remote, Actions enabled, and Release/attestation permissions. Core implementation may continue while this external prerequisite is pending, but no `v0.1.0` release may be claimed.

- [ ] **Step 1: Add offline CI**

Use `windows-2025`, `ubuntu-24.04`, and ARM64 `macos-15`. Pin reviewed major action versions. Run format, Clippy, all Rust tests, platform process tests, and installer tests. Never invoke Claude/Codex.

- [ ] **Step 2: Add tag/version validation**

For tags matching `v*`, compare the tag without `v` to `cargo metadata --no-deps --format-version 1` package version. Mismatch fails before build.

- [ ] **Step 3: Add native release builds**

Build:

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

- [ ] **Step 4: Smoke-test each renamed asset natively**

Run `--version` and expect `llm-wikis 0.1.0` before uploading artifacts.

- [ ] **Step 5: Generate and independently verify `SHA256SUMS`**

The manifest includes exactly the three binaries, `install.sh`, and `install.ps1`. Verification scripts reject missing, duplicate, or unexpected entries.

- [ ] **Step 6: Add artifact attestations**

Use GitHub's official build-provenance action with `id-token: write` and `contents: read`. Attest the executables and checksum manifest; do not make `gh` a client installer prerequisite.

- [ ] **Step 7: Publish only after the full matrix succeeds**

Create one immutable GitHub Release for the tag and attach:

```text
llm-wikis-windows-amd64.exe
llm-wikis-linux-amd64
llm-wikis-darwin-arm64
install.ps1
install.sh
SHA256SUMS
```

A pending/failed native Windows, Linux, or macOS row blocks the entire version 0.1.0 release; never publish a partial three-asset set. Paid provider live rows are reported separately and do not block binary publication.

- [ ] **Step 8: Validate workflow syntax and scope**

Run available local workflow linting, then:

```powershell
rg -n "claude|codex" .github/workflows
```

Expected: provider names appear only in comments asserting they are not called, or not at all.

- [ ] **Step 9: Commit if Git is available**

```bash
git add .github/workflows tests/release
git commit -m "ci: build and release native llm-wikis binaries"
```

## Task 16: Write Operator Documentation and Run Live Rows

**Files:**

- Create: `docs/llm-wikis.md`
- Modify: `docs/verification/llm-wikis-execution.md`
- Modify: `docs/verification/llm-wikis-preflight.md`
- Add only sanitized fixtures under: `tests/fixtures/claude/`, `tests/fixtures/codex/`

- [ ] **Step 1: Document installation**

Cover latest/pinned commands, asset names, Windows install path, Unix install path, PATH changes, checksum behavior, Apple Silicon-only support, unsigned macOS Gatekeeper flow, and Windows SmartScreen warning.

- [ ] **Step 2: Document configuration**

Cover all three default paths, `--config`, non-overwriting `config init`, multiple wikis, reusable profiles, provider executable overrides, project skills, and Claude local plugins. Explain that Codex installed plugins are deferred because version 0.1.0 uses `--ignore-user-config`. Document overlay ownership, rerunning deployment after every APM sync, fingerprint invalidation, and the required live doctor for every supported entrypoint/load mode.

- [ ] **Step 3: Document query/security contracts**

Cover commands, stdin, JSON envelopes, exit classes, read-only layers, full-content mutation detection, stale indexes, probe paths, live doctor quota, Codex read-allowlist limitation, and troubleshooting.

- [ ] **Step 4: Run complete offline gates**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
$env:PYTHONUTF8='1'
python -m unittest discover -s agents\apm_modules\kfchou\wiki-skills\tests -v
pwsh -NoProfile -File tests/installers/verify-install-ps1.ps1
```

Run POSIX installer tests on Ubuntu and macOS.

- [ ] **Step 5: Run static doctor from an unrelated directory**

Use a temporary config pointing to the current Agents wiki and run both providers. Expected: one JSON document per command and zero live model calls.

- [ ] **Step 6: Run explicitly authorized live doctor/query rows**

For each available platform/provider pair, record version, native format, duration, citations, and protected-tree before/after digest. Do not infer success for unexecuted rows.

- [ ] **Step 7: Verify alternate entrypoints**

Use temporary fixture roots for a differently named project skill and Claude local plugin. Assert Codex installed-plugin configuration fails closed as out of scope.

- [ ] **Step 8: Sanitize fixtures and evidence**

Remove session/account identifiers, absolute user paths, full prompts, timings that identify accounts, tokens, and real wiki prose. Preserve only synthetic event shape.

- [ ] **Step 9: Commit if Git is available**

```bash
git add docs/llm-wikis.md docs/verification tests/fixtures
git commit -m "docs: document and verify llm-wikis operations"
```

## Task 17: Independent Row-by-Row Verification and Final Review

**Files:**

- Read: all implementation files
- Read: `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`
- Modify by independent verifier only: `docs/verification/llm-wikis-v0.1.0-checklist.md`
- Create by independent verifier only: `docs/verification/evidence/llm-wikis-v0.1.0/`
- Modify for confirmed fixes only: affected source/test files

- [ ] **Step 1: Freeze implementation for verification**

Record the Git commit or, without Git, SHA-256 manifest of implementation files in the execution record.

- [ ] **Step 2: Dispatch an independent checklist verifier**

The verifier did not implement the feature and is not the Task 1 checklist author. It executes one checklist row at a time and changes only `status` and `evidence` fields.

- [ ] **Step 3: Record exact evidence per row**

Each `PASS` includes command/inspection, exit status, platform, and a sanitized artifact/output under `docs/verification/evidence/llm-wikis-v0.1.0/`. Unavailable live/platform/release checks remain `PENDING`.

- [ ] **Step 4: Revalidate the frozen checklist requirements**

Recreate the Task 1 canonical UTF-8/LF stream from the six immutable columns, recompute SHA-256 and row count, and compare both values with `docs/verification/llm-wikis-v0.1.0-checklist-baseline.json`. Any mismatch without a recorded user-approved baseline replacement fails the gate; status/evidence changes do not affect this digest.

- [ ] **Step 5: Route each failure through TDD**

For every `FAIL`, dispatch a fresh fix worker using `@systematic-debugging` and `@test-driven-development`. Add a failing regression test, implement the minimum fix, rerun the affected row, then rerun the full offline suite.

- [ ] **Step 6: Use `@requesting-code-review`**

Review correctness, readability, architecture, security, and performance. Treat shell interpolation, unbounded output, incomplete process-tree cleanup, mutation-check gaps, config trust expansion, contract drift, and release-asset mismatch as blocking.

- [ ] **Step 7: Apply findings with `@receiving-code-review`**

Technically verify every recommendation. Do not accept advisory style changes that expand scope.

- [ ] **Step 8: Repeat affected checklist rows**

Only the independent verifier may change a failed row to passed, with new evidence.

- [ ] **Step 9: Run the final complete gate**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
$env:PYTHONUTF8='1'
python -m unittest discover -s agents\apm_modules\kfchou\wiki-skills\tests -v
```

Plus native installer/process/release verification on Windows x64, Ubuntu x64, and macOS ARM64.

- [ ] **Step 10: Use `@verification-before-completion`**

Report separately:

- offline Rust and existing helper tests;
- Windows/Linux/macOS native binary tests;
- installer tests;
- Claude/Codex live rows by platform;
- Claude local-plugin row status and Codex installed-plugin deferral;
- unsigned macOS/Windows warnings;
- GitHub Release status.

Do not claim completion while any required non-live row is failed or pending. Do not claim a live/platform capability whose row remains pending.

- [ ] **Step 11: Commit verified fixes and evidence if Git is available**

```bash
git add src tests agents install.sh install.ps1 .github docs Cargo.toml Cargo.lock rust-toolchain.toml config.example.toml
git commit -m "fix: satisfy independent llm-wikis verification"
```

## Final Handoff Condition

Implementation is ready for release only when:

- Tasks 1 and 2 passed before production code began;
- all required offline, process, installer, and native-binary checklist rows are independently `PASS`;
- every remaining `PENDING` row is an explicitly optional live-provider/plugin claim and is reported as such;
- the plan/spec review loop is approved;
- `development-handoff.md` remains absent;
- MCP and orchestration remain unimplemented.
