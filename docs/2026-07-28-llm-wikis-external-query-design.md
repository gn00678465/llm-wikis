# LLM Wikis External Query Design

Date: 2026-07-28 (amended 2026-07-30, corrected 2026-07-31, corrected 2026-08-04 seven times, corrected 2026-08-05 three times, corrected 2026-08-06 twice)  
Version: 0.2.16  
Status: user-approved design; revised after three independent review passes; amended 2026-07-30 against two real knowledge bases with measured evidence; corrected 2026-07-31 after an independent three-pass re-review (0 blockers — see §23, 0.2.1); Codex MCP-exclusion and per-provider constraint corrections from instrumented Phase 0 live evidence, user-approved (§23, 0.2.2 and 0.2.3); Claude wiki-side hook-neutralization correction from a Task 15 PR review finding, user-approved (§23, 0.2.4), itself corrected the same day after a live experiment showed it broke project-skill discovery, user-approved (§23, 0.2.5), then extended the same day to a static allowlist deny check after a live experiment showed the hook-only mitigation left other executable settings keys open, user-approved (§23, 0.2.6), then narrowed and hardened the same day (allowlist shrunk to enabledPlugins only, symlinked settings paths rejected, the query-time check moved immediately before provider spawn with its TOCTOU residual stated honestly, ENTRYPOINT_INVALID's taxonomy broadened) after a fourth review round, user-approved (§23, 0.2.7), then hardened again the same day (the `.claude` directory itself checked for special entries, not only the two settings filenames; the query-time check relocated a second time, into the Claude adapter immediately before spawn; a full prose-accuracy sweep) after a fifth review round, user-approved (§23, 0.2.8), then the same day reaffirmed `enabledPlugins`'s admission as an explicit, evidence-backed risk acceptance (not a safety guarantee) and added a new operator-facing warning for it, after documentation research and an empirical test, user-approved (§23, 0.2.9), then the same day corrected a self-contradictory hook-neutralization sentence and made the `CLAUDE_ENABLED_PLUGINS_DECLARED` warning driven by the same authoritative pre-spawn check that enforces the allowlist rather than a separate, potentially-stale early read, after a sixth review round, user-approved (§23, 0.2.10), then corrected two mutation-detection/provider-call-ordering overclaims plus two more of the identical pattern found by a systematic sweep, after a seventh review round, user-approved (§23, 0.2.11), then fixed a real code bug where a spawn-failure path silently reverted the enabledPlugins-declared signal, corrected an over-strong "mechanically enforced" claim about model-behavior items, and fixed a broken cross-reference, after an eighth review round, user-approved (§23, 0.2.12), then, after a ninth review round found three more prose defects in the same overclaim family, replaced scattered restatement with one canonical safety-claims reference (operator guide §3.10) that every other safety sentence in both documents now points at instead of re-deriving, user-approved (§23, 0.2.13), then corrected two release-plumbing defects found after the v0.1.0-beta release was published and deleted — the bundled installers never pinned to their own release, and installer self-checksumming gave no real integrity — user-approved (§23, 0.2.14), then a published-tag-name recovery correction after GitHub refused to re-cut the same deleted tag, user-approved (§23, 0.2.15), then four pre-0.1.0-stable CLI refinements (`config list`/`config validate`, an interactive-only query spinner, human-mode error lines moved to stderr, and additive non-interactive provider directives plus a narrower `--setting-sources project` re-addition), user-approved (§23, 0.2.16)  
Project root: `<root>`

> **Section numbering is stable.** §6.5 and §9 were removed in 0.2.0 but their headings are retained
> as tombstones, so every existing reference from the acceptance checklist and the implementation plan
> continues to resolve. See §23 Revision History for every change and its evidence.

## 1. Purpose

Provide a distributable, read-only `llm-wikis` command-line interface for querying one registered knowledge base from any development project without opening an interactive Claude or Codex session and manually copying the result.

The CLI is implemented as one Rust binary and installed directly from GitHub Releases without a Python runtime. The MVP externalizes only `query`; wiki initialization, ingestion, update, merge, lint, audit, answer filing, and cross-wiki orchestration remain outside this implementation.

## 2. Current State and Problem

Two knowledge bases are registered. They live inside one Git repository at `D:\Wikis` and are produced by **different toolchains**, so their internal layouts differ:

```text
D:\Wikis\agents                       produced by the wiki-skills toolchain
├── SCHEMA.md                         schema at the content root
├── config/  bin/  raw/  assets/
├── .claude/skills/wiki-query/        entrypoint /wiki-query
├── .agents/skills/wiki-query/        entrypoint $wiki-query
└── wiki/
    ├── index.md
    └── pages/                        17 pages, flat

D:\Wikis\harness-engineering          produced by the llm-wiki skill
├── .claude/skills/llm-wiki/          entrypoint /llm-wiki
├── .agents/skills/llm-wiki/          entrypoint $llm-wiki
└── wiki/                             ← the content root
    ├── SCHEMA.md                     schema at the content root
    ├── index.md
    ├── concepts/  entities/  sources/  synthesis/   359 pages, typed directories
    └── graph/
```

Interactive Claude and Codex sessions can use each wiki's installed skill. External projects cannot query either knowledge base through a stable machine interface, so users must switch sessions and copy answers manually.

The wrapper cannot hard-code any entrypoint. Configuration records each wiki's real entrypoint, because a knowledge base may expose the same query contract through:

- a differently named project skill, as `agents` and `harness-engineering` already do;
- a Claude plugin skill such as `/knowledge-tools:ask-wiki`;
- a Codex skill mention such as `$ask-wiki`;
- a provider-specific installation mechanism.

**The wrapper also cannot hard-code a wiki layout.** Layout is a property of the toolchain that produced the wiki, and two toolchains already disagree. A third would disagree again. Where a wiki keeps its schema, index, and pages is therefore the skill's knowledge, not the wrapper's: **the skill navigates, the wrapper verifies.**

Provider behavior also differs:

- Claude uses `-p` for non-interactive execution, can load skills from an additional directory, and supports one schema-validated JSON result.
- Codex uses `exec`, discovers repository skills relative to its working directory, and emits JSONL events even when its final result follows an output schema.
- Claude `--add-dir` grants additional file access; Codex `--add-dir` grants an additional writable root. They are not interchangeable.

## 3. Scope

### 3.1 In Scope

- an `llm-wikis query` command that accepts exactly one registered wiki;
- a trusted TOML registry of wikis, each declaring its own entrypoints and query prompt;
- Claude Code and Codex CLI provider adapters;
- project skills with configurable names, used exactly as the wiki ships them;
- a constrained Claude local-plugin entrypoint;
- a fixed `wiki-query/v1` result contract, carried by the prompt envelope and enforced by provider output schemas;
- structured JSON success and failure envelopes;
- citation extraction, validation, and wiki namespacing;
- static and optional live `llm-wikis doctor` checks;
- an `llm-wikis config init` that writes non-interactively (piped/scripted, `--json`, or `--yes`) or through a short interactive wizard for `default_agent` and the two provider executables (a real terminal, no `--yes`), and never merges into an existing file — an existing destination without `--force` is `CONFIG_EXISTS`; `--force` overwrites it;
- one native Rust executable with no Python runtime dependency;
- GitHub Release binaries and checksum-verifying install scripts;
- Windows x64, Linux/WSL x64, and macOS Apple Silicon portability;
- an internal Query Service reusable by a future MCP adapter.

### 3.2 Explicitly Out of Scope

- multi-wiki fan-out, question decomposition, or synthesis;
- an `llm-wikis orchestration` implementation;
- MCP server implementation;
- query answer saving or operation logging;
- index regeneration or index freshness checking during query;
- **modifying, overlaying, replacing, or installing any skill file in any knowledge base**;
- externalizing any other wiki skill;
- direct retrieval APIs such as `search_wiki` or `read_wiki_page`;
- arbitrary wiki paths supplied by query callers;
- arbitrary prompt templates, system prompts, shell commands, or raw provider CLI arguments in configuration;
- automatic discovery of all installed skills or plugins;
- automatic discovery or inference of a wiki's internal layout by the wrapper;
- HTTP service, remote execution, or multi-user tenancy;
- changing the current wiki page or citation format;
- Intel macOS and Linux ARM binaries;
- package-manager distribution through Cargo, Homebrew, WinGet, Scoop, apt, or similar channels;
- Codex installed-plugin loading, because it conflicts with the version 0.1.0 `--ignore-user-config` hardening posture until an explicit safe plugin-load mechanism is verified;
- an interactive wizard for registering wikis (`config add-wiki`) — `config init`'s own short wizard covers only `default_agent` and provider executables, never wiki registration;
- a built-in self-update command;
- Apple Developer ID signing, Apple notarization, or Windows Authenticode signing in version 0.1.0.

A future `llm-wikis orchestration` command may call the same Query Service once per wiki and synthesize normalized results. It receives a separate design and implementation plan.

## 4. Design Decisions

1. `query` handles exactly one wiki ID per invocation.
2. The public command is provider-neutral; provider-specific behavior stays behind adapters.
3. Configuration records explicit provider entrypoints instead of deriving names.
4. Every configured entrypoint must be able to satisfy `wiki-query/v1`; the result shape is enforced by the provider's output schema and validated by the wrapper, not asserted by the skill.
5. Read-only enforcement belongs to the harness: tool restriction, sandboxing, no session persistence, trusted roots, and mutation checks.
6. `project_root` and `content_root` are separate required fields. They may be equal.
7. The wrapper owns citation validation and adds the wiki namespace. The model never supplies an authoritative public namespace.
8. External query never regenerates or validates the freshness of an index. Interactive mutation workflows own index maintenance.
9. **The wrapper asserts no wiki layout.** Preflight verifies only that a content root exists and contains Markdown; the skill locates the schema, index, and pages.
10. CLI and a future MCP adapter call the same Query Service. MCP must not reimplement retrieval and must not shell out through the public CLI.
11. Questions go to child processes through stdin. They are never interpolated into a shell command.
12. Configured project roots, content, skills, plugins, and the per-wiki `query_prompt` are trusted operator inputs. User questions are untrusted.
13. The product is one synchronous Rust CLI. Concurrent pipe readers and a platform-specific process supervisor provide bounded output, timeout, and process-tree termination without an async application runtime.
14. Provider executable locations may be overridden only by a trusted global configuration value containing one command name or one absolute path; raw arguments remain implementation-owned.
15. Version 0.1.0 release assets are raw platform executables plus `SHA256SUMS`, not ZIP or tar archives.
16. macOS version 0.1.0 supports Apple Silicon only and is explicitly published as an unsigned preview.
17. **`llm-wikis` never writes to a knowledge base.** It has no command that does.

## 5. Public CLI

### 5.1 Commands

```text
llm-wikis --version
llm-wikis [--config <absolute-path>] [--json] config init [--yes] [--force]
llm-wikis [--config <absolute-path>] [--json] config list
llm-wikis [--config <absolute-path>] [--json] config validate
llm-wikis [--config <absolute-path>] [--json] list
llm-wikis [--config <absolute-path>] [--json] doctor [--wiki <id>] [--agent claude|codex] [--live]
llm-wikis [--config <absolute-path>] [--json] query --wiki <id> [--agent claude|codex] [--plain] -- <question>
```

`llm-wikis --version` prints `llm-wikis 0.1.0`.

`llm-wikis config init` creates the parent directory and a valid starter configuration. On a real terminal (both stdin and stdout are a TTY), without `--json` and without `--yes`, it runs a short interactive wizard collecting `default_agent` and the two provider executables (Enter accepts the shown default) before writing the customized template; every other invocation shape — piped/scripted, `--json`, or `--yes` on a TTY — writes the fixed non-interactive template directly, unchanged from the wizard's own all-defaults output. It never merges an existing file: an existing destination without `--force` is `CONFIG_EXISTS` and exit `2` (a TTY invocation without `--force` asks to confirm the overwrite first, default no); `--force` always overwrites without asking. The generated file contains runtime and provider defaults, an empty wiki registry, and a commented wiki example. `config add-wiki` (registering a wiki interactively) remains outside version 0.1.0.

In JSON mode, config initialization emits exactly `{ "schema_version": "1.0", "ok": true, "operation": "config_init", "path": "<absolute config path>", "created": true }` on success. Failure uses the same operation with `created: false` and the public `error` object.

`llm-wikis config list` loads the configuration exactly the way the top-level `list`/`doctor`/`query` already do (`--config` override or the platform-native default) and prints the resolved document — the parsed configuration after serde's own field-level defaulting, never additionally filesystem-canonicalized per wiki (that remains `doctor`'s job). It is distinct from the top-level `llm-wikis list` above, which enumerates the registered wikis rather than the whole configuration document; clap's `config` prefix keeps the two commands unambiguous even though both resolve to a bare `list` token at their own subcommand depth. In JSON mode `config list` emits `{ "schema_version": "1.0", "ok": true, "operation": "config_list", "path": "<config path>", "config": {...} }` on success, or the same operation with no `config` field and the public `error` object on failure. `llm-wikis config validate` performs the identical load-and-validate step but never echoes the parsed document back and never writes anything: `{ "schema_version": "1.0", "ok": true, "operation": "config_validate", "path": "<config path>" }` on success, the same operation with the public `error` object on failure. Neither subcommand starts a provider or introduces a new error code — both reuse `Config::load`'s existing mapping.

`llm-wikis query` accepts one and only one `--wiki`. Repeating it or supplying `all` is an argument error.

If the positional question is omitted, the command reads the complete question from stdin. When both are supplied, the command fails rather than merging ambiguous inputs.

`--agent` is optional only when `default_agent` exists and the selected wiki enables it.

Normal human output prints the answer followed by gaps and warnings. On a real terminal (stdout is a TTY), without `--plain` and without `NO_COLOR` set, the answer's markdown is rendered with terminal styling; a piped/redirected stdout, `--plain`, or `NO_COLOR` (any value) prints the raw markdown instead. `--json` emits exactly one JSON document on stdout. Diagnostics — including every human-mode error line, for every subcommand — go to stderr; `--json` mode is unaffected, since its single JSON document (success or failure) always goes to stdout.

An interactive `query` invocation (stderr is a terminal) shows a spinner on stderr while the provider call is in flight, cleared before the answer or error line prints; a piped/redirected/non-interactive invocation shows nothing extra at all, on either stream.

`llm-wikis list` loads the registry and lists every configured wiki without starting a provider. `llm-wikis doctor` defaults to static checks for every configured wiki and every provider enabled by each wiki. `--wiki` and `--agent` narrow that matrix. Because live checks consume model quota, `llm-wikis doctor --live` requires both selectors.

`list` derives each wiki's `default_agent` from the global `default_agent`. The field is the configured value only when that provider is enabled for the wiki; otherwise it is JSON `null`. Omitting `--agent` from `query` is valid only when this derived value is non-null.

### 5.2 Configuration Selection

The installed CLI has one platform-native default configuration path:

```text
Windows:   %APPDATA%\llm-wikis\config.toml
Linux/WSL: ${XDG_CONFIG_HOME:-~/.config}/llm-wikis/config.toml
macOS:     ~/Library/Application Support/llm-wikis/config.toml
```

An optional `--config <absolute-path>` override is an operator/testing feature and establishes a new trust boundary. Relative override paths are rejected. The override is not accepted through MCP or other untrusted callers.

Configuration discovery never walks the caller's current project.

### 5.3 List and Doctor Machine Output

`llm-wikis --json list` returns:

```json
{
  "schema_version": "1.0",
  "ok": true,
  "operation": "list",
  "wikis": [
    {
      "id": "agents",
      "title": "Agents Knowledge Base",
      "default_agent": "claude",
      "agents": ["claude", "codex"]
    }
  ]
}
```

`llm-wikis --json doctor` returns one result per selected wiki/provider pair:

```json
{
  "schema_version": "1.0",
  "ok": true,
  "operation": "doctor",
  "live": false,
  "results": [
    {
      "wiki": "agents",
      "agent": "claude",
      "ok": true,
      "checks": [
        {
          "name": "entrypoint",
          "status": "pass",
          "code": null,
          "message": "Configured project skill is statically addressable."
        }
      ]
    }
  ]
}
```

Check `status` is `pass`, `warn`, or `fail`. Overall `ok` is false when any check fails; a matrix containing only passes and warnings is `ok: true` and exit `0`. A command-level argument or configuration failure adds the same top-level `error` object used by query and emits an empty `wikis` or `results` array. Pair-specific doctor failures remain in `results[].checks`.

List uses exit `0` after a valid config and exit `2` for config failure. Doctor performs all selected static checks before any live check and uses the global error-to-exit mapping in Section 14. For example, invalid config is `2`, missing provider CLI is `3`, live child timeout is `5`, invalid native output is `6`, and a detected mutation is `7`. When a matrix has failures from more than one class, the process selects the first present class from this precedence order: `70`, `7`, `6`, `5`, `4`, `3`, `2`, then `0`.

## 6. Configuration Contract

The file is TOML so it supports comments and a strict Rust deserializer can reject unknown or mistyped keys. The registry written by `config init` is valid with zero configured wikis; `list` returns an empty array, while `query` reports `WIKI_NOT_ALLOWED` until the operator adds a wiki.

The configured registry for the two real knowledge bases:

```toml
config_version = 1
default_agent = "claude"

[providers.claude]
executable = "claude"

[providers.codex]
executable = "codex"

[runtime]
timeout_seconds    = 180
max_question_bytes = 65536
max_stdout_bytes   = 1048576
max_stderr_bytes   = 65536

[wikis.agents]
title        = "Agents Knowledge Base"
project_root = "D:/Wikis/agents"
content_root = "D:/Wikis/agents"
agents       = ["claude", "codex"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.agents.claude]
load       = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"

[wikis.agents.codex]
load       = "project_skill"
entrypoint = "$wiki-query"
skill_path = ".agents/skills/wiki-query/SKILL.md"

[wikis.harness-engineering]
title        = "Harness Engineering"
project_root = "D:/Wikis/harness-engineering"
content_root = "D:/Wikis/harness-engineering/wiki"
agents       = ["claude", "codex"]
query_prompt = "Use the llm-wiki skill's query workflow to answer from this wiki."

[wikis.harness-engineering.claude]
load       = "project_skill"
entrypoint = "/llm-wiki"
skill_path = ".claude/skills/llm-wiki/SKILL.md"

[wikis.harness-engineering.codex]
load       = "project_skill"
entrypoint = "$llm-wiki"
skill_path = ".agents/skills/llm-wiki/SKILL.md"
```

The paths above are one configured Windows machine. The generated commented example says to replace them with operator-owned absolute paths. Relative wiki paths remain supported and resolve from the configuration file, but an example installed under `%APPDATA%`, XDG config, or `~/Library/Application Support` must not imply that `../agents` refers to the CLI repository.

`max_question_bytes`, `max_stdout_bytes`, and `max_stderr_bytes` are positive byte counts. The complete UTF-8 question is limited before provider startup; overflow is `QUESTION_TOO_LARGE`, and invalid stdin UTF-8 is `QUESTION_INVALID_UTF8`. Provider stream caps are enforced independently while the child is running. Crossing either stream cap terminates the process tree and returns `OUTPUT_TOO_LARGE` with the offending stream named in error details.

Required/default rules:

- `config_version` is required and must equal `1`;
- `default_agent` is optional;
- `[runtime]` is optional; its fields independently default to the values in the example;
- `[providers]` and `[wikis]` default to empty tables;
- each provider subtable is optional until a wiki enables that provider;
- `providers.<agent>.executable` defaults to the matching command name when its provider table exists;
- a wiki requires `title`, `project_root`, `content_root`, `query_prompt`, a non-empty unique `agents` array, and one `[wikis.<id>.<agent>]` table per enabled agent;
- an enabled agent missing its global provider table is `PROVIDER_CONFIG_MISSING`;
- an enabled agent missing its per-wiki table is `CONFIG_INVALID`;
- each per-wiki provider table requires `load` and `entrypoint`, plus the fields its load mode requires as shown in Section 6.2.

### 6.1 Path Rules

- `project_root` and `content_root` resolve relative to the configuration file.
- `content_root` must be a real directory contained by `project_root`. **Containment is inclusive: `content_root` may equal `project_root`.**
- `content_root` has no required internal structure. See §8.1 step 6.
- `skill_path` for `project_skill` resolves relative to `project_root`.
- `plugin_dir` for `local_plugin` resolves relative to the configuration file.
- A local plugin's `skill_path` resolves relative to `plugin_dir`.
- Every path is resolved canonically.
- While resolving a configured root/path, every component on that path is checked for symlink, junction, reparse, mount, or other special status. Full recursive special-entry scans apply to the complete `content_root` and the selected project-skill/Claude-local-plugin artifact tree, **excluding any `.claude/` and `.agents/` directories immediately beneath `content_root`** — such directories exist there only when `content_root` equals `project_root` (the `agents` case); for a nested content root such as `harness-engineering`'s `wiki/`, they are siblings of `content_root` and are outside the scan by geometry alone. Unrelated portions of `project_root` are not recursively scanned. Any encountered special entry is `UNSAFE_FILESYSTEM_ENTRY`, even when its target would remain contained. Canonical regular paths outside their declared root use `PATH_OUTSIDE_ALLOWED_ROOT`.
- Wiki IDs match `[a-z0-9]+(?:-[a-z0-9]+)*` and are unique.
- Query callers select an allowlisted ID; they cannot supply a path.
- A provider `executable` is either one command name resolved through the current process `PATH`/`PATHEXT`, or one absolute path.
- A provider declaration may additionally carry an optional `model` and an optional `effort`. Both are passed as separate argument values on `query` and live `doctor` only — never on the version probe, the authentication probe, static `doctor`, or `list`. `model` is one alias or fully qualified model ID: non-empty, at most 128 bytes, free of control characters and whitespace, and not beginning with `-`. `effort` is one token of letters, digits, `_`, and `-`, at most 32 bytes, beginning with a letter or digit. Neither is checked against a provider-specific list of accepted values: which levels a model supports is the provider's own question, and an unsupported one is an ordinary provider failure. Unset fields add nothing to the invocation.
- Provider executable values containing arguments, shell syntax, control characters, or relative path components are rejected.
- `doctor` records and reports the canonical executable selected after resolution.

The `.claude/` and `.agents/` exclusion exists because those trees are provider skill directories, and because both registered knowledge bases contain apm-managed symlinks there (`skills/markitdown` → a shared target outside the wiki) that no containment relaxation could admit. **This is not blanket coverage by another mechanism** (an earlier version of this paragraph overclaimed that `skill_fingerprint`, §15.1, covers these trees — corrected, R-31): `skill_fingerprint` hashes only the one configured skill/plugin directory, not the whole `.claude`/`.agents` tree. For a Claude wiki specifically, `project_root/.claude` itself and its two settings files are covered by a narrower, targeted special-entry check as part of the settings-surface deny gate (§10.2/§12/§15 R-29 through R-31); every other path under `.claude`/`.agents` remains genuinely outside both the recursive special-entry scan and `skill_fingerprint`.

### 6.2 Supported Provider Load Modes

Claude:

```toml
[wikis.example.claude]
load = "project_skill"
entrypoint = "/ask-wiki"
skill_path = ".claude/skills/ask-wiki/SKILL.md"
```

or:

```toml
[wikis.example.claude]
load = "local_plugin"
entrypoint = "/knowledge-tools:ask-wiki"
plugin_dir = "../plugins/knowledge-tools"
skill_path = "skills/ask-wiki/SKILL.md"
```

Codex:

```toml
[wikis.example.codex]
load = "project_skill"
entrypoint = "$ask-wiki"
skill_path = ".agents/skills/ask-wiki/SKILL.md"
```

Every load mode is disabled for normal query until `doctor --live` succeeds for the exact current machine, provider executable/version, resolved roots, entrypoint, `query_prompt`, configuration identity, and project-skill/local-plugin fingerprint. This one-time-per-fingerprint gate prevents an APM refresh, Claude local-plugin update, or local skill edit from silently changing the behavior behind a configured entrypoint. Live doctor is never implicit.

### 6.3 Entrypoint Validation

- Claude project skills match `/name`.
- Claude plugin skills match `/plugin-name:skill-name`.
- Codex project skills match one unnamespaced `$name` in version 0.1.0; `$plugin-name:skill-name` is reserved for the deferred installed-plugin design.
- Names use ASCII letters, digits, dots, underscores, and hyphens.
- A Claude plugin entrypoint contains exactly one colon separating two valid names.
- Entrypoints are single tokens. Whitespace, newlines, control characters, quotes, and shell metacharacters are rejected.
- The wrapper does not append provider namespaces, rename the entrypoint, or infer it from `skill_path`.

### 6.4 Forbidden Configuration

The schema rejects unknown keys by default, including:

```text
claude_args
codex_args
shell_command
system_prompt
prompt_template
allowed_tools
sandbox
mcp_config
```

Provider safety flags, the tool set, and the structure of the query prompt envelope are implementation-owned.

**`query_prompt` is the single exception**, and it is narrowly constrained:

- required, one per wiki;
- a single line: no newline, no carriage return, no other control character;
- at most 500 UTF-8 bytes;
- placed verbatim in the prompt **after the entrypoint token and before the `EXTERNAL_QUERY` envelope**, and nowhere else;
- it cannot alter, override, or shadow any envelope field, provider flag, or tool restriction.

It exists so an operator can state how a particular knowledge base is queried — which skill drives it — without the wrapper needing to know, and without any file in the knowledge base being modified. It is trusted operator input under Design Decision 12, not caller input.

### 6.5 Skill Ownership — Removed in 0.2.0

*(Heading retained as a tombstone so existing references resolve. See §23, R-09.)*

Version 0.1.0 does not own, overlay, replace, patch, generate, or install any skill file in any knowledge base. Each wiki's skill is used exactly as that wiki ships it. The external-readonly contract is carried by the prompt envelope (§7.1) and enforced by provider output schemas and the harness (§10, §12), not by modified skill text.

## 7. `wiki-query/v1` Contract

### 7.1 Invocation Envelope

The wrapper builds the complete prompt. The configured entrypoint is the first token, followed by the configured `query_prompt`, followed by a fixed envelope:

```text
<entrypoint>

<query_prompt>

EXTERNAL_QUERY:
{
  "contract": "wiki-query/v1",
  "mode": "external-readonly",
  "wiki_id": "agents",
  "content_root": "<canonical absolute path>",
  "question": "<untrusted question>",
  "required_result": {
    "contract": "wiki-query/v1",
    "knowledge_status": "grounded | no_relevant_material",
    "answer": "string",
    "citations": ["bare page slug"],
    "gaps": ["string"],
    "warnings": ["string"]
  },
  "constraints": [ <provider-specific constraint list — see below> ]
}
```

The `constraints` array is **provider-specific**, because the second constraint must state each provider's actual tool surface truthfully (R-26: telling Codex "no shell tool is available" left it unable to read any file, since command execution *is* Codex's only read mechanism — Phase 0 recorded the model's own warning to that effect).

Claude (tool set `Read,Grep,Glob`, no command execution):

```text
"Read only. Do not write, save, commit, log, cache, or regenerate anything.",
"Do not run scripts or shell commands; no such tool is available.",
"Do not offer to save the answer.",
"Use the content_root above; do not infer a different wiki location.",
"Answer only from this wiki; do not fill gaps from general knowledge.",
"Return the required_result object as your final output."
```

Codex (reads through command execution confined by the read-only sandbox):

```text
"Read only. Do not write, save, commit, log, cache, or regenerate anything.",
"Reading wiki files with read-only commands is permitted; the sandbox enforces read-only. Do not attempt writes, index regeneration, installs, or network access.",
"Do not offer to save the answer.",
"Use the content_root above; do not infer a different wiki location.",
"Answer only from this wiki; do not fill gaps from general knowledge.",
"Return the required_result object as your final output."
```

Only the second entry differs; the other five are byte-identical across providers.

The JSON is serialized by the wrapper. The question is data, not a prompt template fragment.

### 7.2 Required Behavior

In `external-readonly` mode the invoked skill must:

1. use the explicit `content_root`;
2. locate and read that wiki's own schema, index, and relevant pages **using its own knowledge of the wiki's layout**, plus one relevant level of cross-references;
3. treat wiki content as the source of truth and not fill gaps from general knowledge;
4. return grounded answers, inline native wiki citations, disagreements, gaps, and follow-up questions;
5. omit answer-saving offers;
6. never regenerate the index;
7. never write pages, logs, reports, commits, caches, or other files;
8. never attempt mutation through any tool. Under Claude no command execution exists at all; under Codex, command execution is confined by the read-only sandbox and must be used only to read wiki files (R-26);
9. return the normalized result object required below.

Interactive invocation retains the current behavior; nothing in a wiki's skill is modified.

**How these are guaranteed.** Items 1, 2, 3, 4, and 9 are carried by the envelope's `constraints` and `required_result`, and item 9 is additionally enforced mechanically by the provider's output schema (§10.2, §10.3) and validated by the wrapper (§7.3, §11). Items 5 through 8 describe *model behavior* the prompt instructs, not something the harness can compel outright — what the harness actually contributes toward each of them differs per item (full mechanical write-prevention for some, detection-only backstop for others, no mechanical backstop at all for item 5 specifically) and is stated precisely, once, canonically, in the operator guide's **§3.10 "The complete safety picture" (`docs/llm-wikis.md`)**. This specification deliberately does not restate that item-by-item breakdown here, after an earlier version of this paragraph twice drifted out of sync with what the code actually does (§23, R-34, R-35) — one canonical location is safer than two that must be kept identical by hand.

This is a deliberate change from version 0.1.0, which required editing each wiki's skill to add an early mode branch rather than relying on the prompt. The residual risk is understood and accepted: a skill whose interactive workflow instructs it to save or regenerate may attempt to, fail against the harness, and return a degraded answer. That is an answer-quality risk, not a safety risk, and it is preferable to modifying files in knowledge bases this project does not own. See §23, R-08.

### 7.3 Model Result Object

Both providers must produce:

```json
{
  "contract": "wiki-query/v1",
  "knowledge_status": "grounded",
  "answer": "Grounded answer with [[wiki-slug]] citations.",
  "citations": ["wiki-slug"],
  "gaps": ["Optional gap"],
  "warnings": []
}
```

Rules:

- `contract` must equal `wiki-query/v1`;
- `knowledge_status` is exactly `grounded` or `no_relevant_material`;
- `answer` is a non-empty string;
- `citations`, `gaps`, and `warnings` are arrays of strings;
- `grounded` requires at least one citation that the wrapper can resolve;
- `no_relevant_material` requires an empty citation array and at least one non-empty `gaps` item;
- provider-native metadata does not replace this object.

## 8. Query Service

The internal Query Service has five bounded components:

1. **Config Loader** — loads and validates the trusted TOML.
2. **Wiki Resolver** — resolves one wiki ID, roots, provider table, and provider.
3. **Preflight** — checks content-root minimum structure, entrypoint availability, and executable/auth readiness.
4. **Provider Adapter** — runs Claude or Codex and parses its native output.
5. **Result Normalizer** — validates the contract, citations, public envelope, and errors.

The CLI and future MCP server call this service directly.

### 8.1 Query Flow

1. Parse CLI input without starting an agent.
2. Load the fixed or explicitly trusted config.
3. Resolve exactly one wiki ID and enabled provider.
4. Validate the complete UTF-8 question and `max_question_bytes`.
5. Canonicalize all configured paths and verify containment.
6. Require:
   - a canonical `content_root` that exists and is a directory;
   - at least one regular `.md` file anywhere beneath `content_root`;
   - the configured project skill or local plugin artifacts when statically addressable.

   The wrapper asserts no internal wiki layout. Where a wiki keeps its schema, index, and pages is a property of the skill that produced it.
7. Resolve the provider executable; run its bounded version and non-billable authentication-status probes.
8. Fingerprint the selected skill/local-plugin artifact and require the one current live-probe record for the logical key/current-verification tuple.
9. Build the fixed prompt envelope from the entrypoint, `query_prompt`, and `EXTERNAL_QUERY` object.
10. Snapshot the complete canonical `content_root` with content hashes.
11. Invoke the provider with stdin/stdout/stderr separated.
12. Enforce timeout, independent stdout/stderr size limits, and process-tree termination.
13. Parse provider-native output.
14. Validate `wiki-query/v1`.
15. Extract and validate page-provenance citations against actual pages.
16. Add the wiki namespace.
17. Recompute and compare the complete content-root snapshot.
18. Emit exactly one public result envelope.

## 9. Index Freshness — Removed in 0.2.0

*(Heading retained as a tombstone so existing references resolve. See §23, R-04.)*

The wrapper performs **no index discovery, parsing, comparison, or freshness checking**. It never reads an index and never regenerates one. Design Decision 8 stands: external query does not mutate the index.

A wiki whose index is a generated artifact should be regenerated before querying, because a stale or missing index degrades the **skill's** navigation. That belongs in operator documentation as guidance, not in the wrapper as a gate. A wiki with a malformed, duplicated, or absent index still works.

## 10. Process Supervision and Provider Adapters

### 10.1 Common Process Supervisor

The Query Service passes a structured argument vector to a synchronous process supervisor. It never renders that vector into a shell command string. The supervisor:

- launches the provider in its own process group or platform-equivalent containment;
- sends the complete prompt through stdin and then closes stdin;
- drains stdout and stderr concurrently so neither pipe can deadlock the child;
- enforces the two byte limits while streaming, not after buffering an unbounded result;
- enforces the monotonic deadline across spawn, write, read, wait, and termination;
- kills the entire process group on timeout, overflow, cancellation, or parser-aborting failure;
- waits for termination and joins both pipe readers before returning;
- preserves stdout, stderr, exit status, elapsed time, and termination reason as separate bounded fields.

On Unix, the supervisor uses a dedicated process group. On Windows, it uses a Job Object or an equivalently tested process-tree primitive. Direct `.exe` providers are started without a shell. When PATH resolution selects a `.cmd` or `.bat` provider shim, only the trusted, fixed provider argv passes through a dedicated Windows batch adapter; the untrusted question remains on stdin. Phase 0 must prove paths with spaces and representative Windows metacharacters do not change the command shape.

Provider flag spellings may change after Phase 0, but the safety invariant may not: the corrected vector exposes only the minimum skill-invocation capability plus non-mutating read access, disables or excludes Write, Edit, web access, MCP, subagents, and session persistence, and preserves non-interactive no-escalation behavior. Shell/command execution is excluded entirely under Claude; under Codex it cannot be excluded — it is the provider's only read mechanism — and is instead confined by the read-only sandbox and restricted by the envelope to reading wiki files (R-26). MCP exclusion must be **explicit per provider** and is scoped to **external and user-configured MCP servers**: ignoring user configuration is not sufficient on its own, because a provider may ship built-in runtime MCP servers that live outside user configuration entirely (observed in Phase 0: Codex's bundled `node_repl` server for its browser/computer-use features, disabled by explicit flags). A provider's own internal introspection tooling — observed as Codex's built-in `codex` server answering `list_mcp_resources` with no external reach — cannot be disabled, is benign, and is recorded in bounded diagnostics rather than treated as a violation (R-26). A substitute permission mode that re-enables a forbidden capability is not an acceptable compatibility fix. The capability spike records the minimal tool set that both invokes the configured entrypoint and satisfies this invariant.

Generated empty MCP configuration, the Codex JSON-schema file, and any batch-adapter helper live in a fresh system temporary directory that is canonically outside every configured `project_root` and `content_root`. Files use exclusive creation and user-only permissions where supported, remain owned/open by the wrapper until child startup, and are removed after the child is reaped. Claude's `--json-schema` value is an implementation-generated inline JSON string, not a path. An unsafe temporary location fails before provider startup.

Authentication readiness uses provider status commands, not a paid model request: Claude runs `claude auth status --json`; Codex runs `codex login status`. Phase 0 records sanitized success/logged-out fixtures and exact exit behavior. A recognized logged-out state is `AUTH_REQUIRED`; malformed status output is `INVALID_NATIVE_OUTPUT`; status-process timeout/nonzero follows the normal process error mapping unless the verified provider uses nonzero specifically for logged-out state.

### 10.2 Claude Adapter

The child process working directory is the selected `project_root`, not the caller's development project and not an unverified empty directory.

Target invocation:

```text
claude
  --add-dir <content_root>
  -p
  --input-format text
  --no-session-persistence
  --permission-mode dontAsk
  --tools Read,Grep,Glob
  --strict-mcp-config
  --mcp-config <generated-empty-config>
  --output-format json
  --json-schema <inline-result-schema>
  --settings {"disableAllHooks":true}
  --append-system-prompt <non-interactive-directives>
  --setting-sources project
```

For `local_plugin`, add:

```text
--plugin-dir <plugin_dir>
```

The configured entrypoint and `query_prompt` appear only in the stdin prompt.

Requirements:

- do not use `--dangerously-skip-permissions`;
- do not expose Bash, Edit, Write, Web, MCP, or subagent tools;
- do not persist sessions;
- parse stdout as one JSON document;
- reject `is_error: true` or non-success subtype;
- prefer `structured_output`;
- cap stderr in diagnostics.

`--json-schema` carries the `wiki-query/v1` result shape, which is how §7.2 item 9 is enforced mechanically rather than by prose.

**`--append-system-prompt`/`-c developer_instructions=` (D5, PRD 08-06-pre-0-1-0-cli-refinements)**: both providers additionally receive a short, fixed, non-interactive directive text — answer directly and stop; never ask whether to save the answer; never update the wiki, its index, frontmatter, or any log; never ask a follow-up question a non-interactive CLI has no one present to answer — appended via each provider's own system-prompt-append mechanism (Claude's `--append-system-prompt`, Codex's `-c developer_instructions=`, §10.3). This is a separate channel from the closed, byte-identical-per-item `constraints` array carried in the stdin prompt (§7.1); it reinforces model *behavior* the harness cannot mechanically compel (spec §7.2's items 5-8 distinction) rather than replacing any existing enforcement layer. Live-verified (research/provider-cli-flags.md §1, §4): a model instructed this way answered directly instead of asking a clarifying question it was otherwise invited to ask.

Claude's cwd is `project_root`, so read reach can exceed `content_root`. **When `content_root` is a strict subdirectory of `project_root`**, static doctor and every Claude query emit `CLAUDE_READ_SCOPE_BROAD`: `Claude read tools can inspect the configured project root, not only the selected content root; use an OS sandbox or container for stricter confidentiality.` When the two roots are equal the warning is not emitted, because read reach is exactly `content_root` and the message would be false.

**`--settings` threat and mitigation, corrected mechanism (R-27, corrected by R-28)**: because the child's cwd is `project_root`, an untrusted operator-controlled directory this wrapper does not own, Claude Code's `-p` mode auto-loads that directory's `.claude/settings.json`/`settings.local.json` and **executes any hooks they declare** (`SessionStart`, `PreToolUse`, ...) as arbitrary shell — entirely outside the `--tools Read,Grep,Glob` gate, which restricts only built-in tools, not hook commands. `.claude/`/`.agents/` immediately under `content_root` are excluded from the mutation snapshot (§12), so such a hook's writes there would be undetectable by that layer.

R-27 originally also added `--setting-sources user`, intending to exclude the wiki's project/local settings from being loaded at all. A live four-arm controlled experiment (R-28, §23) found this broke the tool's primary load mode: Claude's **project-skill discovery is itself gated on the `project` setting source** — excluding it made every `project_skill`-load-mode wiki's slash entrypoint resolve to `"Unknown command: /<name>"` instead of invoking the skill. `--setting-sources` was **not** used in the canonical argv from R-28 until PRD 08-06-pre-0-1-0-cli-refinements (D6).

**`--setting-sources project` (D6, PRD 08-06-pre-0-1-0-cli-refinements)**: the canonical argv above now includes `--setting-sources project` — the *opposite* exclusion from the reverted `user` value: it drops the `user` and `local` setting sources (where a user-level `enabledPlugins`/hook can live — the source of a `SessionEnd` "Hook cancelled" pollution symptom this change fixes) while keeping `project` (which project-skill discovery itself depends on, per the R-28 finding immediately above). This does not reproduce the R-27/R-28 regression: live-verified together with `--settings {"disableAllHooks":true}`, a project-level `/name` skill invocation still resolved and answered correctly, with zero `permission_denials` (research/provider-cli-flags.md §2, §3). Like every other layer in this section, it does not reach an admin-managed/enterprise-policy setting source (`code.claude.com/docs/en/agent-sdk/claude-code-features`, "What settingSources does not control": managed policy settings are always read regardless of `settingSources`) — consistent with the residual gap already stated below.

The wiki's own project/local settings **are** loaded (required for skill discovery), and the argv flags above are **not** the load-bearing trust boundary by themselves (corrected by R-29, narrowed by R-30, further hardened by R-31, §23): a live test found that `apiKeyHelper` — a key that is neither `hooks` nor tool-shaped — executed a command even with `--settings {"disableAllHooks":true}` present. R-29 through R-31 therefore add a **preflight deny check** (§12, §15) that statically inspects `project_root/.claude` and, if it exists as an ordinary directory, `settings.json`/`settings.local.json` beneath it, and refuses the wiki outright unless every top-level key is in a small allowlist — as of R-30, **`enabledPlugins` alone**, with its value validated to be an object of plain booleans. R-29 originally also admitted `permissions.{allow,deny,defaultMode}` on the reasoning that `--tools Read,Grep,Glob` bounds tool *names*; R-30 removed `permissions` entirely after finding that a permission rule's *value* can pre-authorize a specific filesystem path for the exposed `Read` tool (e.g. `{"permissions":{"allow":["Read(/some/secret/**)"]}}`), which is not a tool-name concern at all. Everything else — `apiKeyHelper`, `awsCredentialExport`, `awsAuthRefresh`, `gcpAuthRefresh`, `otelHeadersHelper`, `statusLine` (all run a configured command), `permissions` in its entirety, `env` (can redirect API traffic), `hooks`, and any key not yet enumerated — fails the wiki closed. Both `project_root/.claude` itself and each settings path beneath it are checked with `symlink_metadata`, and any symlink, junction, reparse point, directory-where-a-file-was-expected, or other non-regular entry is rejected as `UNSAFE_FILESYSTEM_ENTRY` rather than treated as absent (R-30, extended to the `.claude` directory itself by R-31): checking only the two settings filenames is not sufficient, since a `.claude` *directory* that is itself a symlink pointing at an initially empty or not-yet-existing location would pass a plain existence check on both filenames exactly like a dangling file-level symlink would, letting the real settings file be created at the true target afterward — `.claude/` is excluded from the recursive special-entry scan (§6.1), so nothing else would catch either case.

This check runs at both `doctor` and `query` time; at `query` time it is called from inside the Claude provider adapter itself (R-31), immediately before the one call that actually spawns the child process — after the version/auth probes, the fingerprint gate, the before-snapshot, and this request's own temp-file/argv construction, none of which need to precede it. **This narrows the check-to-spawn window to the remaining syscall gap; it does not close it.** A write to the wiki's `.claude/` directory landing between this check returning and the operating system actually starting the child process still wins that race — eliminating the window fully would require OS-level isolation (e.g. a held filesystem snapshot or a mandatory-access-control policy) outside this wrapper's scope. This is stated honestly rather than claimed as closure. The check itself is one function (`check_claude_wiki_settings_surface`), called identically from both `doctor` and this pre-spawn site — not duplicated logic.

On top of that gate, the three argv-level layers remain as defense in depth for the allowlisted surface and anything the deny check's authors did not anticipate: (a) `--settings {"disableAllHooks":true}` disables every hook declared by user, project, local, or plugin-dir settings (CLI-supplied settings outrank all of those, so this also covers a configured `--plugin-dir`'s own hooks) — it does **not** reach an admin-managed/enterprise-policy hook, stated fully in the residual gap below; (b) `--strict-mcp-config` plus the empty MCP config locks out any MCP server the settings might declare; (c) `--tools Read,Grep,Glob` bounds the built-in tool surface regardless of any tool-related setting. **Honest residual gap**: no CLI flag, and no static check, disables an admin-managed/enterprise-policy hook — that class of hook is out of this project's scope, and Phase 0's implementation machine was confirmed to have no managed settings configured.

**Codex investigated, not extended (R-29, unchanged by R-30)**: Codex loads project-level configuration (including a project `.codex/config.toml`, project-local hooks, and project-local execpolicy `.rules` files) only for a project it considers **trusted**; an untrusted project's `.codex/` layer, including all three, is not loaded at all. This wrapper's Codex invocation (§10.3) never establishes trust for the wiki's `project_root`, so this class of surface fails closed by Codex's own design — no `llm-wikis`-side deny check was added for Codex. Verified by design/documentation research, not by a live exploit attempt: neither registered wiki's `.codex/` or `.agents/` directory presently contains any hooks, config, or execpolicy file to test against, and adding one to prove the negative was judged unnecessary given Codex's documented trust-gating architecture.

A pre-implementation live spike must verify the exact current layout of each registered wiki, skill discovery, explicit `content_root`, schema output, zero wiki mutations, and every proposed flag name/value against the implementation machine's recorded Claude Code version. The argument vector in this section is proposed rather than timeless: an unsupported help surface blocks implementation until this specification and plan are corrected and re-reviewed. The successful result becomes a versioned capability fixture; no historical provider version is assumed.

### 10.3 Codex Adapter

Target invocation:

```text
codex
  --ask-for-approval never
  exec
  -C <project_root>
  --sandbox read-only
  --ephemeral
  --skip-git-repo-check
  --ignore-user-config
  -c mcp_servers={}
  -c developer_instructions=<non-interactive-directives>
  --disable browser_use
  --disable computer_use
  --output-schema <temporary-schema-file>
  --json
  -
```

Do not use Codex `--add-dir`; its documented purpose is granting an additional writable root.

`--ask-for-approval` is a top-level Codex option in the currently verified CLI and therefore precedes `exec`. The pre-implementation spike treats the structured argument vector—not a shell command string—as a versioned capability fixture and rejects a provider version whose help surface no longer accepts it.

Requirements:

- send the prompt through stdin;
- parse every non-empty stdout line as JSON;
- reject error and failed-turn events;
- select the last completed agent message;
- validate the final message against `wiki-query/v1`;
- cap event diagnostics and stderr;
- treat the traditional read-only sandbox as write prevention, not a guarantee that unrelated filesystem paths are unreadable.

`--output-schema` carries the `wiki-query/v1` result shape, which is how §7.2 item 9 is enforced mechanically rather than by prose.

`-c mcp_servers={}` and the two `--disable` flags exist because `--ignore-user-config` does not exclude MCP by itself: Codex bundles a built-in runtime MCP server (`node_repl`, backing its `browser_use`/`computer_use` features) that lives in the Codex installation, not in user configuration. All three flags are `exec`-scoped options accepted by the verified codex-cli 0.144.6 (live-verified in the Phase 0 R-25 rerun: exit 0, empty stderr — recorded in the preflight report, Row 11).

**`-c developer_instructions=<...>` (D5, PRD 08-06-pre-0-1-0-cli-refinements)**: `codex exec` has no dedicated system-prompt-append flag; `developer_instructions` is a plain `ConfigToml` field reachable through the generic `-c key=value` override and is Codex's additive equivalent of Claude's `--append-system-prompt` above — live-verified on codex-cli 0.146.0 to additively influence the model's final answer (research/provider-cli-flags.md §4). Grouped adjacent to the pre-existing `-c mcp_servers={}` override purely for readability; `-c` overrides are order-independent among themselves. The `-c` value is TOML-parsed before falling back to a raw-string literal, so the directive text is kept free of `"`, `\`, and newline characters.

Instrumented Phase 0 capture then identified the one `mcp_tool_call` item that persists under this vector: server `"codex"`, tool `"list_mcp_resources"`, no error — the provider's own internal introspection surface, with no external reach and no user-configured server involved (R-26). The offline test suite parses actual event items precisely (top-level item `type` only, no recursive string scanning) and asserts that any `mcp_tool_call` item names server `"codex"` and an introspection tool only; an item naming any other server is a forbidden-capability signal recorded in bounded diagnostics.

Because the adapter uses `--ignore-user-config`, an operator-managed Codex user permission profile is not a supported hardening path in version 0.1.0. Sensitive knowledge bases that require a strict read allowlist need an OS sandbox/container exposing only the selected wiki. Static doctor and every Codex query emit warning code `CODEX_READ_SCOPE_BROAD` with the message: `Codex read-only sandbox prevents writes but does not limit reads to the selected wiki; use an OS sandbox or container for strict confidentiality.`

## 11. Citation Normalization

`wiki-query/v1` citations are query-provenance pointers to wiki pages the provider read. They are distinct from the formal source footnotes a wiki's own schema may define. Existing footnotes may legitimately target `raw/<file>`, `assets/<file>`, or an HTTP(S) URL; those drive-by targets are permitted in answer prose but are not entries in the model's `citations` array and do not replace page provenance.

The wrapper reads no schema file and selects no parser. One fixed grammar is always applied.

### 11.1 Grammar

Three inline forms are recognized as page provenance:

```text
[[slug]]              bare
[[slug|label]]        labelled — the slug is the text before the first `|`
[[slug](any/path)]    markdown
```

In every form the slug must match `[a-z0-9-]+`. All other inline text is outside the page-provenance grammar and is ignored — including `raw/<file>`, `assets/<file>`, HTTP(S) URLs, `[[raw/<file>]]`, `[[assets/<file>]]`, ordinary Markdown links, and malformed or dangling wiki-link prose.

The labelled form is recognized because it accounts for a third of one registered wiki's citations; excluding it would discard that provenance. The markdown form is recognized because it costs nothing to accept.

### 11.2 Resolution

A page-provenance slug resolves to **exactly one** regular file named `<slug>.md` anywhere beneath the canonical `content_root`, matched by exact ASCII filename stem:

- zero matches is `CITATION_NOT_FOUND`;
- two or more matches is `CITATION_AMBIGUOUS`.

Resolution reuses the file list the before-snapshot already built, so it costs no additional directory walk.

Resolution is independent of filesystem case-folding: enumerate regular files, accept only `.md`, strip that exact suffix, and require one exact ASCII filename-stem match. A case-insensitive Windows lookup does not authorize a case-mismatched citation.

`CITATION_AMBIGUOUS` also serves as a mis-pointed-`content_root` signal. Both registered wikis have zero ambiguous slugs at their configured content roots; ambiguity appears only when a content root is set to a parent directory containing vendored or raw duplicates.

### 11.3 Strictness

Strictness depends on **who asserted the citation**:

| Source | Unsafe target | Unresolvable slug |
|---|---|---|
| The model's explicit `citations` array | `CITATION_INVALID`, fail-fast | `CITATION_NOT_FOUND` or `CITATION_AMBIGUOUS`, fail-fast |
| Slugs extracted inline from the answer | ignored | **dropped silently** |

`CITATION_INVALID` applies when an element of the model `citations` array contains path separators, traversal, anchors, a URL, or unsafe characters, or does not match `[a-z0-9-]+`.

Inline extraction is best-effort because wiki content legitimately contains example wikilinks that point at nothing — including inside schema files a skill is instructed to read first. Discarding a correct, well-cited answer because the model echoed one syntax example from its own schema is a bad trade. The explicit array is the model's deliberate assertion about its own sources and is held strictly.

`knowledge_status = "grounded"` requires at least one citation that resolves, from either source, after deduplication. When every candidate from both sources fails to resolve, the result is `CONTRACT_VIOLATION` — not a citation error. `knowledge_status = "no_relevant_material"` with any resolved citation, or without a non-empty gap, is also `CONTRACT_VIOLATION`. The wrapper never infers status from prose.

### 11.4 Ordering and Namespacing

The wrapper preserves all inline-extracted resolved slugs in answer order, appends resolved array slugs in array order, and deduplicates the combined sequence by first occurrence. It then creates public objects containing the selected wiki ID:

```json
{
  "wiki": "agents",
  "slug": "harness-engineering"
}
```

The wrapper never trusts a model-provided wiki namespace.

## 12. Read-Only Enforcement and Trust Model

Configured roots, skill files, plugin files, wiki content, and `query_prompt` are trusted operator-controlled inputs. The question is untrusted.

Enforcement layers:

- fixed wiki allowlist;
- canonical path containment;
- no shell command construction;
- stdin question transport;
- fixed provider arguments;
- Claude read/search-only tools;
- Claude wiki-side settings surface denied by allowlist at both `doctor` and `query` time (§10.2/§15 R-29 through R-31 — `project_root/.claude`, if it exists, and `settings.json`/`settings.local.json` beneath it, are loaded/read regardless of trust, and several keys beyond `hooks` execute a command or widen reach on their own, e.g. `apiKeyHelper`, live-confirmed to execute even with hooks disabled; only `enabledPlugins` is admitted, with its value validated; `permissions` is denied entirely — a permission rule's *value*, not just its key, can widen reach; everything else fails the wiki closed as `ENTRYPOINT_INVALID`; `.claude` itself and each settings path are checked with `symlink_metadata`, and any symlink/junction/reparse/directory-where-a-file-was-expected is rejected as `UNSAFE_FILESYSTEM_ENTRY` rather than treated as absent — checking only the two filenames is not sufficient, since a symlinked `.claude` directory bypasses that the same way a symlinked file would), plus `--settings {"disableAllHooks":true}` (R-27/R-28) as defense in depth for the admitted surface — project/local settings are still loaded, since skill discovery depends on them, so reach is bounded by these layers rather than by non-loading; the `query`-time check is called from inside the Claude adapter itself, immediately before the one call that spawns the child (R-31 — the genuinely last point, not merely an earlier step), narrowing the check-to-spawn window to the remaining syscall gap, not closing it (honestly stated, not claimed closed); the further residual gap is that no flag or check disables an admin-managed/enterprise-policy hook; Codex's equivalent project-level surface is trust-gated by its own design and was not extended with a matching check (investigated, §10.2);
- Codex read-only sandbox;
- no session persistence;
- empty Claude MCP configuration;
- disabled Codex user configuration **plus an explicitly emptied Codex MCP server table and disabled browser/computer-use features (§10.3, R-25/R-26 — disabling user configuration alone does not exclude Codex's built-in MCP surface)**;
- no query-time index generator;
- provider output schemas constraining the result shape;
- timeout and output limits;
- a mutation-sensitive content snapshot before and after the child process;
- **no `llm-wikis` command that writes to a knowledge base.**

The content snapshot covers every directory and regular file beneath the canonical `content_root`, including its schema file, all configuration, executable helpers/hooks, raw/assets, index/log/overview files, every page, and any compiled graph or cache artifact — **except any `.claude/` and `.agents/` directories immediately beneath `content_root`** (present there only when `content_root` equals `project_root`; a nested content root has them as siblings, already outside the snapshot). For each accepted entry it records the normalized relative path and whether it is a directory or regular file; regular files additionally record byte length and a streaming SHA-256 digest. The before/after comparison detects additions, removals, directory/regular-file type changes, and same-size content rewrites even when timestamps are preserved. Any symlink, junction, reparse point, mount point, or other special entry outside the excluded directories aborts preflight/snapshot with `UNSAFE_FILESYSTEM_ENTRY`.

**What actually covers the excluded `.claude`/`.agents` directories, stated precisely (R-30 — a prior version of this paragraph overclaimed blanket `skill_fingerprint` coverage of "provider skill trees," corrected after Codex review iteration 4 finding 5)**: `skill_fingerprint` (§15.1) hashes only the *configured skill's own directory* — for `project_skill`, the skill file's parent directory; for `local_plugin`, the whole configured `plugin_dir`, which is typically outside `content_root` entirely. For Claude wikis specifically, `project_root/.claude/settings.json` and `settings.local.json` are additionally covered by the settings-surface allowlist deny check (§10.2/§12 R-29/R-30) — a gate, not a hash-based change detector. Neither mechanism covers every other file that could exist under `.claude/`/`.agents/` (other skills, slash commands, cached plugin data); those remain genuinely unmonitored by both the snapshot and the fingerprint.

Any difference is `READ_ONLY_VIOLATION`; the model result is rejected and sorted changed relative paths are reported without content. Snapshot comparison runs in a `finally`-equivalent path after success, provider failure, timeout, or output overflow. When a mutation accompanies another non-internal failure, `READ_ONLY_VIOLATION` and exit `7` dominate; the sanitized original failure appears as `error.details.secondary_error`. `INTERNAL_ERROR` dominates only when the wrapper cannot complete the integrity comparison itself. This is a detection layer, not a replacement for OS sandboxing.

Local plugins that declare hooks, MCP servers, settings, or other executable lifecycle components are rejected by static doctor for MVP. Only the configured query skill is allowed.

## 13. Public Result Envelope

Success:

```json
{
  "schema_version": "1.0",
  "ok": true,
  "operation": "query",
  "wiki": {
    "id": "harness-engineering",
    "title": "Harness Engineering"
  },
  "agent": "claude",
  "contract": "wiki-query/v1",
  "knowledge_status": "grounded",
  "answer": "Grounded answer with [[harness-engineering]].",
  "citations": [
    {
      "wiki": "harness-engineering",
      "slug": "harness-engineering"
    }
  ],
  "gaps": [],
  "warnings": [
    {
      "source": "wrapper",
      "code": "CLAUDE_READ_SCOPE_BROAD",
      "message": "Claude read tools can inspect the configured project root, not only the selected content root; use an OS sandbox or container for stricter confidentiality."
    }
  ],
  "duration_ms": 63352,
  "child_exit_code": 0,
  "raw_format": "claude-json"
}
```

Failure:

```json
{
  "schema_version": "1.0",
  "ok": false,
  "operation": "query",
  "wiki": {
    "id": "agents",
    "title": "Agents Knowledge Base"
  },
  "agent": "codex",
  "contract": "wiki-query/v1",
  "knowledge_status": null,
  "answer": null,
  "citations": [],
  "gaps": [],
  "warnings": [],
  "duration_ms": 812,
  "child_exit_code": null,
  "raw_format": null,
  "error": {
    "code": "ENTRYPOINT_UNVERIFIED",
    "message": "the selected entrypoint fingerprint has not passed a current live doctor probe -- run `llm-wikis doctor --wiki agents --agent claude --live` first"
  }
}
```

Argument failures before wiki or agent resolution use `null` for the unresolved fields.

Public `warnings` is an array of closed objects with `source`, `code`, and `message`. Wrapper warning codes are `WIKI_SCHEMA_ABSENT`, `CLAUDE_READ_SCOPE_BROAD`, `CODEX_READ_SCOPE_BROAD`, and `CLAUDE_ENABLED_PLUGINS_DECLARED`. Each model-supplied warning string is normalized to `{ "source": "provider", "code": "PROVIDER_WARNING", "message": <string> }`. Wrapper warnings appear first in deterministic generation order, followed by provider warnings in model order.

`raw_format` is exactly `claude-json` after a parsed Claude native envelope, `codex-jsonl` after parsed Codex events, or `null` when no native format was successfully established.

Every failure `error` has required string fields `code` and `message` plus an optional object `details`. Error-specific detail schemas are closed to unknown keys.

For `OUTPUT_TOO_LARGE`:

```json
{
  "stream": "stdout",
  "limit_bytes": 1048576,
  "observed_bytes": 1048577
}
```

`stream` is exactly `stdout` or `stderr`; both byte counts are non-negative integers and `observed_bytes` is greater than `limit_bytes`.

For `CITATION_AMBIGUOUS`:

```json
{
  "slug": "loop-engineering",
  "match_count": 2
}
```

`match_count` is an integer greater than or equal to 2. Matching paths are never included; they would leak structural information about the wiki.

For `READ_ONLY_VIOLATION`:

```json
{
  "changed_paths": [
    "wiki/pages/harness-engineering.md"
  ],
  "secondary_error": {
    "code": "TIMEOUT",
    "message": "The provider exceeded the configured deadline."
  }
}
```

`changed_paths` is a sorted, unique, non-empty array of slash-separated paths relative to `content_root`. It never contains file content, an absolute path, or traversal. `secondary_error` is optional and contains only the displaced public `code` and sanitized `message`; it never nests another `details` object.

## 14. Error and Exit Contract

Complete error mapping:

| Code | Exit | Meaning |
|---|---:|---|
| `ARGUMENT_INVALID` | 2 | Invalid or ambiguous CLI input |
| `QUESTION_INVALID_UTF8` | 2 | Stdin question is not valid UTF-8 |
| `QUESTION_TOO_LARGE` | 2 | Complete UTF-8 question exceeds `max_question_bytes` |
| `CONFIG_INVALID` | 2 | Missing, malformed, unsupported, or unknown configuration |
| `CONFIG_EXISTS` | 2 | `config init` refuses to overwrite an existing destination |
| `WIKI_NOT_ALLOWED` | 2 | Wiki ID is absent from the registry |
| `PATH_OUTSIDE_ALLOWED_ROOT` | 2 | A configured path escapes its declared root |
| `UNSAFE_FILESYSTEM_ENTRY` | 2 | A monitored tree contains a symlink, junction, reparse point, mount point, or special entry |
| `WIKI_INVALID` | 2 | Content root is missing, is not a directory, or contains no Markdown |
| `PROVIDER_CONFIG_MISSING` | 2 | A wiki enables a provider without a corresponding global provider table |
| `AGENT_UNSUPPORTED` | 2 | Provider is not enabled for the wiki |
| `ENTRYPOINT_INVALID` | 2 | Entrypoint syntax or statically addressable artifacts are invalid, or (Claude) the wiki's own project settings surface is not statically safe to invoke against (R-29/R-30) |
| `CITATION_INVALID` | 2 | Explicit-array citation syntax is unsafe |
| `CITATION_NOT_FOUND` | 2 | Explicit-array citation does not map to an existing page |
| `CITATION_AMBIGUOUS` | 2 | Explicit-array citation matches two or more pages |
| `CLI_NOT_FOUND` | 3 | Provider executable is unavailable |
| `AUTH_REQUIRED` | 3 | Provider authentication is unavailable |
| `ENTRYPOINT_UNVERIFIED` | 3 | Current entrypoint/provider/config fingerprint has not passed live doctor |
| `NONZERO_EXIT` | 4 | Child process returned non-zero |
| `TIMEOUT` | 5 | Child process exceeded the deadline |
| `OUTPUT_TOO_LARGE` | 5 | Native output exceeded the configured cap |
| `TERMINATION_FAILED` | 5 | The process tree could not be confirmed terminated/reaped |
| `INVALID_NATIVE_OUTPUT` | 6 | Claude JSON or a Codex JSONL event is malformed |
| `NO_FINAL_MESSAGE` | 6 | Codex produced no completed agent message |
| `CONTRACT_VIOLATION` | 6 | Final result does not satisfy `wiki-query/v1` |
| `READ_ONLY_VIOLATION` | 7 | A protected wiki path, type, or content digest changed |
| `INTERNAL_ERROR` | 70 | Unexpected error or an incomplete integrity comparison at the wrapper boundary |

Twenty-seven codes. Exit `0` means success. These mappings apply identically to query and command-level doctor failures. A query-time `READ_ONLY_VIOLATION` dominates every exit `2`–`6` error and records the displaced failure as `secondary_error`; `INTERNAL_ERROR` dominates only when integrity verification itself cannot complete.

The wrapper process exit code equals its documented class. A failed invocation still emits exactly one JSON envelope in `--json` mode.

## 15. Doctor

Static checks:

- supported config version and no unknown keys;
- unique valid wiki IDs;
- canonical contained roots, including the inclusive-equality case;
- content-root minimum structure;
- `query_prompt` presence and constraints;
- entrypoint syntax;
- project skill or local plugin skill path;
- local plugin manifest identity and absence of hooks/MCP/settings;
- Claude only: `project_root/.claude` itself is a regular directory (not a symlink/junction/reparse point), and, when present, `settings.json`/`settings.local.json` beneath it are regular files declaring no key outside the closed allowlist (`enabledPlugins` alone, value-validated) — §10.2/§12 R-29 through R-31, under check name `entrypoint`; re-run identically (same function, one source of truth) at query time, inside the Claude adapter immediately before the child process spawns, not only at doctor time;
- provider executable and version;
- provider authentication status, read through each provider's non-billable status surface (§10.1), under check name `auth`;
- platform sandbox warning.

Doctor `checks[].name` is one of `config`, `roots`, `wiki_structure`, `entrypoint`, `executable`, `auth`, `read_scope`, `live_contract`, or `mutation`. `checks[].code` is `null` for pass, a stable warning code for warn, or one error code from Section 14 for fail.

`CODEX_READ_SCOPE_BROAD` and the conditional `CLAUDE_READ_SCOPE_BROAD` use check name `read_scope`.

`WIKI_SCHEMA_ABSENT` uses check name `wiki_structure` and warns when no `SCHEMA.md` exists at the content root: `No SCHEMA.md at the content root; most wiki toolchains place one there. Confirm content_root points at the wiki root rather than a parent or child directory.` This is a warning, never a failure — the wrapper requires no schema file. It exists because minimum structure alone cannot distinguish a correct content root from its parent, and a parent directory can be an order of magnitude larger to snapshot.

Live checks, only with `--live`:

1. invoke the selected configured entrypoint with a minimal harmless query;
2. require a result satisfying `wiki-query/v1`;
3. validate native and normalized outputs;
4. confirm identical protected-tree content snapshots;
5. record the complete successful-probe identity locally outside the wiki;
6. invalidate the probe when any identity or fingerprint value changes.

Live doctor consumes model quota and is never run implicitly by `query`. Every configured entrypoint requires a current live probe for its complete fingerprint before normal query.

### 15.1 Probe Store

Live doctor writes a machine-local cache outside every configured wiki:

```text
Windows:   %LOCALAPPDATA%\llm-wikis\probes-v1.json
Linux/WSL: ${XDG_CACHE_HOME:-~/.cache}/llm-wikis/probes-v1.json
macOS:     ~/Library/Caches/llm-wikis/probes-v1.json
```

The document has this shape:

```json
{
  "schema_version": 1,
  "records": [
    {
      "wiki_id": "agents",
      "canonical_project_root": "D:\\Wikis\\agents",
      "canonical_content_root": "D:\\Wikis\\agents",
      "agent": "codex",
      "agent_executable": "C:\\path\\to\\codex.exe",
      "agent_version": "0.145.0",
      "load": "project_skill",
      "entrypoint": "$wiki-query",
      "skill_fingerprint": "sha256:...",
      "compatibility_fingerprint": "sha256:...",
      "verified_at": "2026-07-30T12:00:00Z"
    }
  ]
}
```

Doctor computes `skill_fingerprint` over every directory and regular file under the configured project-skill directory — **the wiki's own skill directory**, since nothing is installed by this tool. For a Claude local plugin it additionally includes `.claude-plugin/plugin.json` and rejects hooks, MCP, settings, or other executable lifecycle components. Any special filesystem entry is `UNSAFE_FILESYSTEM_ENTRY`.

**Derived artifacts are excluded from the fingerprint**: `__pycache__/` directories and `*.pyc` / `*.pyo` files. They are byte-compilation output, not skill semantics, and a registered wiki's skill directory contains scripts whose compilation output changes on unrelated interactive runs. Excluding them prevents a probe from being invalidated by a fact that carries no meaning.

Artifact fingerprints use SHA-256 over this deterministic byte stream for files sorted by ordinal normalized relative path: UTF-8 path with `/` separators, one NUL byte, unsigned 64-bit big-endian file length, raw file bytes. The stored form is lowercase `sha256:` plus 64 hexadecimal digits.

The logical record key is exactly canonical content/project roots, selected provider, load mode, and exact entrypoint. Exactly zero or one record may exist for that logical key.

The record's current-verification tuple additionally contains canonical provider executable path/version, skill fingerprint, and normalized `compatibility_fingerprint`. The compatibility fingerprint is SHA-256 over canonical JSON for the selected wiki, its provider table, its `query_prompt`, the provider declaration — its executable, and its optional `model` and `effort` — and the implementation-owned provider safety/contract version after validation and path resolution. **Changing `model` or `effort` invalidates a successful probe**, for the same reason changing `query_prompt` does: a verification run against one model does not vouch for another. Execution-policy and presentation fields such as timeout, byte limits, comments, and TOML key order do not invalidate a successful probe. **Changing `query_prompt` does invalidate it**, because it changes what the model was told.

A successful live doctor removes every existing record for the logical key and atomically writes exactly one record with the current-verification tuple. It never retains historical fingerprints for the same logical key. Query resolves the logical key, requires exactly one record, and compares every current-verification field; an APM/skill rollback therefore fails unless that exact current state passes a new live doctor. A missing, malformed, duplicate, or mismatched record is unverified; there is no independent time-to-live in version 1. Doctor writes through an atomic temporary-file replacement and requests user-only permissions where the platform supports them. Query only reads the cache. Probe prompts, answers, and wiki content are never stored.

## 16. Testing Strategy

Implementation follows test-driven development with Rust unit, integration, CLI, and platform-specific process tests. Offline CI never invokes a paid provider.

### 16.1 Offline Unit Tests

Configuration:

- valid two-wiki registry matching Section 6;
- malformed TOML, wrong version, unknown keys, duplicate IDs;
- valid differently named skills for each wiki;
- valid Claude plugin namespace;
- valid Codex `$name` and rejected/deferred `$plugin-name:skill-name` entrypoints;
- invalid entrypoint whitespace, control characters, and metacharacters;
- `query_prompt` missing, multi-line, control characters, over 500 bytes;
- rejection of every forbidden key in Section 6.4;
- path traversal and symlink/junction/reparse/mount rejection, plus the `.claude/` and `.agents/` exclusion;
- `content_root` equal to `project_root` accepted;
- `content_root` outside `project_root` rejected.

Preflight:

- content root missing, not a directory, and containing no Markdown;
- content root containing exactly one Markdown file accepted;
- `WIKI_SCHEMA_ABSENT` emitted when no `SCHEMA.md` at the content root, and query still succeeds;
- no index is read, required, or parsed under any configuration;
- plugin manifest rejection when executable components exist;
- all-entrypoint live-probe requirement;
- missing, malformed, duplicate, stale, and fingerprint-invalidated probe records;
- probe invalidated by a changed `query_prompt`;
- probe **not** invalidated by a regenerated `__pycache__` or `.pyc`.

Prompt envelope:

- entrypoint, `query_prompt`, and `EXTERNAL_QUERY` appear in that exact order;
- `query_prompt` cannot alter any envelope field;
- the question is serialized by `serde_json`, never formatted into a string.

Claude parser:

- success JSON with structured output;
- error subtype and `is_error`;
- malformed JSON;
- stderr warning;
- non-zero exit, timeout, and oversized output.

Codex parser:

- valid JSONL and final agent message;
- malformed line;
- failed turn or error event;
- missing final message;
- final schema violation;
- stderr warning, non-zero exit, timeout, and oversized output.

Process supervision:

- concurrent stdout/stderr draining without deadlock;
- independent byte caps with the offending stream reported;
- monotonic timeout across the complete lifecycle;
- process-tree termination and reap on every failure path, including `TERMINATION_FAILED`;
- stdin closure and Traditional Chinese/multiline transport;
- Windows `.exe` and `.cmd` provider resolution and the batch adapter boundary;
- paths with spaces and representative shell metacharacters;
- no untrusted question data in provider argv.

Normalization:

- all three inline forms recognized; the labelled form yields the slug before `|`;
- non-provenance text ignored, not rejected;
- content-root-wide unique resolution; zero matches; two or more matches;
- Windows case folding does not authorize a case-mismatched citation;
- explicit-array strictness versus best-effort inline dropping;
- an answer whose inline slugs are all unresolvable but whose array resolves is accepted;
- an answer with no resolvable citation from either source is `CONTRACT_VIOLATION`;
- ordering and first-occurrence deduplication;
- wrapper-added wiki namespace;
- structured `no_relevant_material` with no citations and a non-empty gap;
- full-content snapshot additions, removals, type changes, and same-size rewrites with preserved timestamps.

Contract drift:

- the error table in Section 14 matches the implemented error enum exactly, in both directions;
- each code's exit class matches the implementation;
- the doctor check-name vocabulary in Section 15 matches the implemented set;
- the wrapper warning-code vocabulary in Section 13 matches the implemented set.

CLI:

- `--version` reports the Cargo package version;
- `config init` creates platform-native parents and a valid empty registry;
- `config init` refuses to overwrite an existing file;
- question argument and stdin forms;
- rejection when both are present;
- exactly one wiki requirement;
- human and JSON output;
- stdout/stderr separation;
- per-stream size details and doctor failure-class precedence;
- warn-only doctor matrix yields `ok: true` and exit `0`;
- exit-code mapping;
- Unicode paths and Traditional Chinese questions;
- leading dashes, quotes, shell metacharacters, and multiline input without shell interpretation.

### 16.2 Live Matrix

| ID | Platform | Provider | Wiki | Required assertions |
|---|---|---|---|---|
| LIVE-01 | Windows | Claude | `agents` | Correct skill, explicit content root, valid JSON, resolvable citations, no mutation, no `CLAUDE_READ_SCOPE_BROAD` (roots equal) |
| LIVE-02 | Windows | Codex | `agents` | Correct skill, valid JSONL parsing, resolvable citations, no mutation |
| LIVE-03 | Windows | Claude | `harness-engineering` | Typed-directory layout navigated by the skill, resolvable citations, no mutation, `CLAUDE_READ_SCOPE_BROAD` present (strict containment) |
| LIVE-04 | Windows | Codex | `harness-engineering` | Same, plus `wiki/graph/` and any cache path unchanged |
| LIVE-05 | Linux/WSL | Claude | either | Same assertions and portable path handling |
| LIVE-06 | Linux/WSL | Codex | either | Same assertions and sandbox behavior recorded |
| LIVE-07 | macOS ARM64 | Claude | either | Native ARM binary, zsh environment, resolvable citations, no mutation |
| LIVE-08 | macOS ARM64 | Codex | either | Native ARM binary, JSONL parsing, sandbox behavior recorded, no mutation |
| LIVE-09 | Windows or Linux/WSL | Claude | fixture | Differently named local plugin skill resolves; plugin contains no rejected components |

Each live test records provider version, duration, raw format, warnings, citations, and before/after protected-tree digests. Live tests are separate from the fast offline suite because they consume time and model quota. Binary distribution support and live provider verification are reported separately: a native build may be released after its offline and installer gates pass, but documentation must not claim live Claude/Codex verification for a platform until its corresponding rows pass.

## 17. Pre-Implementation Gates

No production implementation starts until both gates below pass.

### 17.1 Independent Acceptance Checklist

An independent review session receives only this approved specification and the implementation-plan path. Before any production code is written, it creates `docs/verification/llm-wikis-v0.1.0-checklist.md`.

The main/orchestrating session and implementation workers must not author, weaken, remove, or self-mark checklist requirements. The checklist:

- maps every normative acceptance requirement to one or more observable checks;
- identifies the platform and evidence required for each check;
- separates offline, native-binary, installer, live-provider, and release assertions;
- uses one row per independently verifiable behavior;
- starts with every row unresolved.

The checklist author also records a canonical digest and row count over all immutable columns—ID, requirement, platform, phase, command/inspection, and expected result—excluding only status and evidence. Final verification recomputes this digest and fails closed on any unapproved requirement change or deleted row.

After implementation, a second independent verification session—distinct from both the checklist author and all implementation workers—checks rows one at a time and records the command, exit status, relevant sanitized output, and artifact path under `docs/verification/evidence/llm-wikis-v0.1.0/`. A failed or unavailable row remains failed or pending; it is never converted to pass through prose. Material checklist defects require user-approved spec/checklist correction rather than implementation-session edits.

### 17.2 Disposable Capability Spikes

Small, disposable Rust spikes must validate the risky assumptions before the production crate is implemented:

1. resolve and invoke the installed Claude executable and Codex executable/shim;
2. transport Traditional Chinese, multiline text, quotes, leading dashes, and shell metacharacters through stdin without question bytes appearing in argv;
3. confirm the exact current Claude JSON and Codex JSONL command surfaces and sanitized output shapes, including the output-schema flags that enforce the result contract;
4. invoke each registered wiki's real entrypoint with the fixed envelope and its configured `query_prompt`, proving the existing skill returns a `wiki-query/v1` result without mutating the wiki;
5. enforce independent stdout/stderr caps and timeout without deadlock;
6. terminate and reap a deliberately spawned child process tree;
7. prove full-content mutation detection catches a same-size rewrite with a restored timestamp;
8. prove platform config/cache directories and executable resolution, including a Windows `.cmd` shim and paths containing spaces;
9. prove temp-artifact safety, including rejection of an unsafe temporary root before spawn.

Spike source is isolated from production modules and deleted or retained only under a clearly marked `spikes/` directory; production code must be developed again through failing tests. Sanitized commands, versions, results, and unresolved platform rows are recorded in `docs/verification/llm-wikis-preflight.md`.

**Phase 0 passes per row, not per task.** "Row" here means a row of the Phase 0 record `docs/verification/llm-wikis-preflight.md` — one platform × capability combination drawn from the spike list above. It is a distinct row set from both the §16.2 Live Matrix (`LIVE-01`–`LIVE-09`, which gates live-verification claims) and the §17.1 acceptance checklist (which gates final release verification); the three sets must never be conflated even where their counts coincide. Task 2 has passed when every row reachable on an available platform is `PASS`. A row that cannot be executed because its platform or its paid provider is unavailable is recorded `PENDING` with the reason; a `PENDING` row blocks the release claim it supports and does not block production implementation. Any demonstrated `FAIL` in a provider flag, output shape, process primitive, target toolchain, or platform assumption stops the affected implementation/release path until the specification and plan are corrected and re-reviewed.

The local implementation platform's provider flags/output and process primitive must pass before production code. **The three-target native smoke build and the Linux/macOS process-primitive rows are owned by the release automation task, not by the spike task**, because they require CI that the release task defines; the spike task must not be asked to run machinery a later phase builds. Any pending native binary/process/installer row blocks the entire three-asset `v0.1.0` release; a pending paid live-provider row blocks only that provider/platform support claim.

## 18. Release and Installation

### 18.1 Release Assets

Release automation requires a user-approved Git repository with a configured GitHub remote, Actions enabled, and permission to create Releases/attestations. Core implementation may proceed without that external state, but version 0.1.0 cannot be published until the prerequisite and every three-platform native row pass.

A `v0.1.0` tag triggers the GitHub Actions release workflow. The tag must equal the Cargo package version. Quality gates run before any GitHub Release is published:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

The workflow builds and natively smoke-tests:

| Asset | Rust target | Support |
|---|---|---|
| `llm-wikis-windows-amd64.exe` | `x86_64-pc-windows-msvc` | Windows x64 |
| `llm-wikis-linux-amd64` | `x86_64-unknown-linux-musl` | Linux/WSL x64 |
| `llm-wikis-darwin-arm64` | `aarch64-apple-darwin` | macOS Apple Silicon |

It also publishes:

- `SHA256SUMS`;
- GitHub artifact attestations for the executables and checksum manifest.

`install.ps1` and `install.sh` are deliberately **not** release assets. Both scripts default to `.../releases/latest/download/...` when `LLM_WIKIS_VERSION` is unset (§18.2), so a copy attached to one specific tagged release would silently install `latest` instead of that release while looking self-contained — a release-provided `SHA256SUMS` entry for the installer gives no real integrity guarantee either, since the same release controls both the script and its own checksum. The documented install path fetches both scripts from `raw.githubusercontent.com/.../refs/heads/main/` instead (§18.2), which is their single source of truth.

The tag is marked as a GitHub pre-release (`gh release create --prerelease`) exactly when it carries a pre-release identifier — a `-` after the version core, e.g. `v0.1.0-beta` — and not otherwise. This matters because `.../releases/latest/download/...`, which both installers use by default, deliberately skips pre-releases; only a plain `vX.Y.Z` tag ever becomes "latest". During a pre-release-only period (no stable tag published yet), an unpinned installer run therefore 404s; both installers detect this and fail with an explicit, actionable message instead of a bare download error (§18.2) rather than the release workflow silently marking a pre-release as the normal Latest release.

Release assets are immutable for a version. A failed or pending Windows, Linux, or macOS native matrix row, checksum, smoke test, or installer test prevents the entire `v0.1.0` publication; version 0.1.0 is not released with a partial asset set. The workflow never invokes Claude or Codex. Pending paid live-provider rows block only their explicit support claims, not the binary release.

**A published tag name is a one-time resource.** This repository has GitHub's immutable releases in effect, and the reservation survives deletion: after the first `v0.1.0-beta` release was published and then deleted — release and tag both removed, and confirmed absent from `git ls-remote --tags` — re-creating that same tag was still refused with `GH013 ... Cannot create ref due to creations being restricted`. The restriction is name-specific, not blanket: an unrelated probe tag pushed and deleted cleanly in the same session, and no ruleset appears under the repository, rule-suite, or account rulesets APIs, so the behaviour is only observable by attempting the push. The operational consequence is that there is no "delete the release and re-cut the same version" recovery path — a defective release must be superseded by a new pre-release identifier (`v0.1.0-beta` → `v0.1.0-beta.1`), with `Cargo.toml` bumped to match, because the workflow's `version-check` job compares the tag to the package version. Verify the asset set before tagging, not after.

### 18.2 Installers

`install.ps1`:

- downloads latest unless `LLM_WIKIS_VERSION` selects an explicit version;
- when the unpinned `latest` download 404s (no stable release published — see §18.1's pre-release marking rule), fails with an explicit, actionable message telling the operator to set `LLM_WIKIS_VERSION` instead of a bare download failure; never queries the GitHub API to distinguish this from another download failure;
- recognizes Windows x64 only;
- downloads and verifies the matching raw `.exe` against `SHA256SUMS`;
- runs the downloaded binary with `--version` before installation;
- installs to `%LOCALAPPDATA%\llm-wikis\bin\llm-wikis.exe`;
- adds that directory to the user PATH only when absent, without duplicating entries.

`install.sh` follows the proven `apm-go` release-download flow:

- downloads latest unless `LLM_WIKIS_VERSION` selects an explicit version;
- when the unpinned `latest` download 404s (no stable release published — see §18.1's pre-release marking rule), fails with the same explicit, actionable message as `install.ps1`, never a bare download failure; never queries the GitHub API to distinguish this from another download failure;
- accepts Linux x86-64 and macOS ARM64 only, with explicit errors for Intel Mac and unsupported Linux architectures;
- downloads the raw binary and `SHA256SUMS` into a temporary directory;
- fails closed when neither `sha256sum` nor `shasum` is available;
- runs the downloaded binary with `--version` before installation;
- installs to `~/.local/bin/llm-wikis`;
- when needed, idempotently adds that directory to `~/.zprofile` for macOS zsh or `~/.profile` for Linux/bash/sh;
- prints manual PATH instructions for an unsupported shell instead of editing an unknown profile.

Both installers clean temporary files on success and failure. Re-running an installer upgrades, downgrades, or repairs the binary. Version 0.1.0 has no separate uninstaller or self-update command; operator documentation describes manual removal.

The macOS binary is an unsigned preview. Documentation explains Gatekeeper's normal user approval flow without advising users to disable platform security. Windows documentation similarly warns that the unsigned binary may trigger SmartScreen. SHA-256 and GitHub attestations provide integrity/provenance evidence but do not replace platform code signing.

## 19. Rollout

### Phase 0 — Independent Gates

- independent acceptance checklist;
- disposable Rust capability spikes, judged per row;
- corrected and re-reviewed spec/plan if any assumption fails.

### Phase 1 — Query Core

- Cargo project and strict configuration;
- wiki resolver and minimum-structure preflight;
- process supervisor and Claude/Codex adapters;
- result normalization, citation validation, mutation hashing, and errors;
- `--version`, `config init`, `list`, static `doctor`, and `query`;
- offline tests.

### Phase 2 — Hardening and Distribution

- live doctor and probe invalidation;
- plugin entrypoint validation;
- platform-native offline and process tests;
- install scripts and GitHub Release workflow;
- operator and security documentation;
- explicitly authorized live-provider rows.

### Phase 3 — Independent Verification

- freeze implementation changes except verified fixes;
- execute the independent checklist one row at a time;
- send failures back through failing regression tests;
- rerun affected rows and the complete offline suite;
- publish only the support claims backed by recorded evidence.

MCP and orchestration require separate approved designs after the query MVP is stable.

## 20. Acceptance Criteria

- `llm-wikis` is a Rust 0.1.0 binary with no Python runtime dependency.
- A command launched outside a wiki project can query either registered wiki without copy/paste.
- The caller selects one allowlisted wiki ID, never a path.
- Each wiki's real entrypoint is configuration, not a product constant.
- The wrapper asserts no wiki layout; both registered layouts work without wrapper changes.
- Provider executables use safe defaults and trusted command-name/absolute-path overrides.
- A differently named compatible project skill works by changing configuration only.
- Every configured entrypoint works only after a current fingerprinted live probe.
- **No `llm-wikis` command has a code path that writes to a knowledge base** (§4 Design Decision 17). What additionally bounds a write attempted through the invoked provider itself — what is mechanically prevented, what is only detected, and what is not covered at all — is stated once, canonically, in the operator guide's §3.10 (`docs/llm-wikis.md`); this bullet does not restate it.
- Both providers receive the same fixed external query contract through stdin.
- The `wiki-query/v1` result shape is enforced by provider output schemas and validated by the wrapper.
- Claude and Codex use independently tested argument vectors and native-output parsers.
- Every successful answer has a mechanically validated `knowledge_status`: `grounded` with at least one resolvable citation, or `no_relevant_material` with a non-empty gap.
- Every accepted citation resolves to exactly one existing page and carries the wrapper-selected wiki ID.
- An answer whose inline example wikilinks do not resolve is not discarded when its explicit citations do.
- `query` exposes no tool through which the provider could regenerate an index, update a log, or modify wiki content — Claude has no Write/Edit/Bash/web tool at all, and Codex's read-only sandbox blocks any write its own tools might attempt. A model *offering* to save an answer (spec §7.2 item 5) is a different, narrower claim with a different, weaker guarantee — no mechanical prevention or detection of the offer itself, only of an actual write succeeding — spelled out precisely in the operator guide's §3.10, not restated here.
- Full-content snapshots reject additions, removals, type changes, and same-size rewrites.
- The UTF-8 question is rejected before provider startup when invalid or larger than `max_question_bytes`.
- JSON mode always emits one normalized document.
- Timeout, auth, native-output, contract, citation, and mutation failures are stable and machine-readable.
- The Section 14 error table and the implemented error enum are mechanically verified to agree.
- `config init` creates a valid platform-native template and never overwrites.
- All three native executables pass offline and installer smoke gates before release.
- macOS support is Apple Silicon only and clearly marked as unsigned preview.
- Live provider claims are limited to platform/provider rows actually executed successfully.
- An independent checklist exists before production implementation and is verified row-by-row afterward.
- No orchestration or MCP code is included in this implementation.

## 21. References

- Registered knowledge bases: `D:\Wikis\agents`, `D:\Wikis\harness-engineering`
- Claude CLI reference: <https://docs.anthropic.com/en/docs/claude-code/cli-usage>
- Claude skills and additional-directory behavior: <https://code.claude.com/docs/en/skills>
- Claude plugin namespace behavior: <https://code.claude.com/docs/en/plugins>
- Codex CLI reference: <https://learn.chatgpt.com/docs/developer-commands?surface=cli>
- Codex skill discovery and invocation: <https://learn.chatgpt.com/docs/build-skills>
- Codex permission profiles: <https://learn.chatgpt.com/docs/permissions>
- Rust process API: <https://doc.rust-lang.org/stable/std/process/index.html>
- Cargo build targets and release profiles: <https://doc.rust-lang.org/cargo/commands/cargo-build.html>
- GitHub artifact attestations: <https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations>
- `apm-go` installer reference: <https://raw.githubusercontent.com/gn00678465/apm-go/refs/heads/main/install.sh>
- CLI-Anything principles: <https://raw.githubusercontent.com/HKUDS/CLI-Anything/refs/heads/main/README.md>

## 22. Repository Note

This workspace is intentionally not a Git repository. Documentation is written and reviewed here, and implementation records task checkpoints in `docs/verification/llm-wikis-execution.md` instead of commits. That decision was taken on 2026-07-29 and is recorded there. Release automation (§18) remains blocked until a Git repository with a GitHub remote is separately approved. The intentionally removed `development-handoff.md` must not be recreated.

The two registered knowledge bases live in a **different**, already-existing Git repository at `D:\Wikis`. This project never writes to it.

## 23. Revision History

### 0.2.16 — 2026-08-06

Four pre-0.1.0-stable CLI refinements, user-approved via interview (PRD `08-06-pre-0-1-0-cli-refinements`): `config list`/`config validate` subcommands, an interactive-only query spinner, human-mode error lines moved to stderr for every subcommand, and additive non-interactive provider directives plus a narrower `--setting-sources project` re-addition.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-39 | §5.1 | `config list` and `config validate` subcommands added: `list` loads and prints the resolved configuration (`Config::load`'s output as-is, never additionally filesystem-canonicalized per wiki); `validate` performs the identical load-and-validate step without echoing the document back. Both honor `--config`/`--json`, reuse `Config::load`'s existing error mapping, start no provider, and introduce no new `ErrorCode`. `doctor` is unchanged — it already satisfied the environment-verification half of the original ask. Named `config list` rather than the originally implemented `config show` (D7, round-1 rename): unambiguous alongside the pre-existing top-level `list` (wiki registry) because clap resolves the two at different subcommand depths. | `src/cli.rs::run_config_list`/`run_config_validate`, `src/config.rs::config_list_envelope`/`config_validate_envelope`; `tests/cli_contract.rs` (`config_list_json_success_and_failure_shapes` and peers), `tests/config_contract.rs` (`config_list_envelope_success_matches_the_exact_contract` and peers). |
| R-40 | §5.1 | An interactive `query` invocation (stderr is a terminal) shows a spinner on stderr for the duration of the blocking provider call, cleared before either the answer or the error line prints; a non-interactive invocation (the entire automated test suite, since `assert_cmd` always captures stderr through a pipe, never a pty) shows nothing extra on either stream. | `src/cli.rs::start_query_spinner`; `tests/cli_contract.rs`'s `live_doctor_then_query_succeed_end_to_end_with_schema_absent_warning` human-mode sub-test asserts stderr is empty on a piped run. |
| R-41 | §5.1 | Human-mode error lines move to stderr for every subcommand (`config init`/`list`/`validate`, `list`, `doctor`, `query`) — previously all five printed `"error: CODE (message)"` to stdout via independent `println!` sites (or, for `query`, via `render_human`'s own error branch). `--json` mode is unaffected: its single JSON document, success or failure, still goes to stdout exclusively. `render_human` no longer renders an error line at all — that became `cli.rs`'s sole responsibility, via one shared `eprint_error_line` helper. | `src/cli.rs::eprint_error_line` and every `run_*`/`print_*_human` call site; `src/output.rs::render_human`'s narrowed doc comment; `tests/cli_contract.rs` (`unknown_subcommand_is_argument_invalid_not_a_panic` flipped, plus four new `*_human_mode_failure_prints_the_error_line_to_stderr_only` tests). |
| R-42/D5 | §10.2, §10.3 | Both providers additionally receive a short, fixed, non-interactive directive text via their own system-prompt-append mechanism — Claude's `--append-system-prompt`, Codex's `-c developer_instructions=` (no dedicated flag exists for `exec`; reaches the same config field via the generic `-c` override) — instructing the model to answer directly and stop, never ask about saving, never update the wiki/index/frontmatter/log, and never ask an unanswerable follow-up question. A separate channel from the closed, per-item-identical `constraints` array in the stdin prompt (§7.1), untouched by this change. | research/provider-cli-flags.md §1 (Claude, live-verified: a model invited to ask a clarifying question answered directly instead) and §4 (Codex, live-verified on codex-cli 0.146.0: an out-of-band instruction additively influenced the final answer); `src/providers/mod.rs::NON_INTERACTIVE_SYSTEM_DIRECTIVES`; `tests/claude_adapter.rs::exact_argv`, `tests/codex_adapter.rs::exact_argv`. |
| R-43/D6 | §10.2 | `--setting-sources project` is added back to the Claude argv — the *opposite* exclusion from the `user` value reverted by R-28 (drops `user`/`local`, keeps `project`, which project-skill discovery itself depends on per the R-28 finding). Fixes a `SessionEnd` "Hook cancelled" pollution symptom traced to a user-level plugin/hook, without reproducing the R-27/R-28 regression. `--tools` stays `Read,Grep,Glob` — `Skill` was investigated and deliberately **not** added: a live repro showed it reproducibly breaks the `/name` project-skill entrypoint under `--permission-mode dontAsk` (the model calls the `Skill` tool instead of relying on CLI-side text expansion, and that call is denied with no fallback), while the current tool set works with zero denials. | research/provider-cli-flags.md §2 (docs + live-verified compatible with `--settings {"disableAllHooks":true}` and successful skill expansion, zero `permission_denials`) and §3 (the `Skill`-regression repro, two runs, load-bearing warning against adding it); `src/providers/claude.rs::SETTING_SOURCES_PROJECT`; `tests/claude_adapter.rs::hook_neutralization_settings_flag_present_without_excluding_setting_sources` (rewritten for D6, doc comment records the R-27→R-28→D6 history). |

### 0.2.15 — 2026-08-06

Re-cutting the corrected release under the original `v0.1.0-beta` tag proved impossible: GitHub refused the tag creation even after the release and tag had been deleted. User-approved move to a new pre-release identifier.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-38 | §18.1 | Package version `0.1.0-beta` → `0.1.0-beta.1`, and §18.1 now records that **a published tag name is a one-time resource** under GitHub's immutable releases: the name reservation survives deleting both the release and the tag, so there is no "delete and re-cut the same version" recovery path. A defective release must be superseded by a new pre-release identifier, with `Cargo.toml` bumped to match because the workflow's `version-check` job compares tag to package version. Practical rule recorded with it: verify the asset set before tagging, not after. | Observed directly: after `gh release delete v0.1.0-beta --cleanup-tag` succeeded and `git ls-remote --tags origin` returned empty, `git push origin v0.1.0-beta` was refused with `remote: error: GH013: Repository rule violations found for refs/tags/v0.1.0-beta` / `Cannot create ref due to creations being restricted`. The restriction is name-specific, not a blanket tag rule: a probe tag (`probe-tag-creation`) pushed and deleted cleanly in the same session. `GET /repos/{owner}/{repo}/rulesets`, `.../rulesets/rule-suites`, and `/user/rulesets` all returned empty or 404, so no ruleset explains it and the behaviour is only observable by attempting the push. `tests/version_cli.rs` expectation and the operator guide's pin examples updated to `0.1.0-beta.1`. |

### 0.2.14 — 2026-08-06

The `v0.1.0-beta` release was published, then deleted together with its tag by the user, after two release-plumbing defects were found post-publish. User-approved fix before re-cutting the tag.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-37 | §18.1, §18.2 | **Two defects fixed.** (1) The release workflow attached `install.ps1`/`install.sh` to the asset set. This was actively harmful, not merely redundant: the published `install.sh` resolves an unpinned run's download base as `.../releases/latest/download` regardless of which release it was downloaded from, so a copy of the script fetched *from* a specific tagged release still installed `latest` when run — the release looked self-contained and was not. Attaching `SHA256SUMS` entries for the installers added no real integrity either, since the same release controls both the script and its own checksum entry (self-checksumming). The documented install path (README.md, `docs/llm-wikis.md`) already fetches both scripts from `raw.githubusercontent.com/.../refs/heads/main/`, never from a release, so the release copies were also a second, drift-prone source of truth. Fixed by removing both scripts from the asset set entirely (copy step, `sha256sum` argument list, `upload-artifact` paths, `gh release create` arguments); a release now attaches exactly the three native binaries and `SHA256SUMS`. (2) `v0.1.0-beta` published as a normal (non-prerelease) GitHub Release, so it became the repository's "latest" despite being a beta. Simply adding `--prerelease` unconditionally would break the default (unpinned) install path once a real beta is correctly marked, since GitHub's `.../releases/latest/download/...` deliberately skips pre-releases and no stable release existed yet. Fixed in two halves: the workflow now passes `--prerelease` iff `GITHUB_REF_NAME` carries a pre-release identifier (a `-` after the version core), and both installers now detect an unpinned download failure and print an explicit, actionable message ("no stable release published yet -- set LLM_WIKIS_VERSION to a specific tag...") instead of a bare download error — deliberately without querying the GitHub API to distinguish the cause, which would add unauthenticated rate limits and fragile JSON parsing to a POSIX-sh script. | User-supplied evidence: the deleted `v0.1.0-beta` release's `install.sh` asset, inspected directly, showed the `latest`-resolution bug (`DOWNLOAD_BASE="${BASE_URL}/latest/download"` used whenever `LLM_WIKIS_VERSION` is unset, independent of which release served the script) and the self-checksumming gap (`SHA256SUMS` generated from and verified against assets in the same release). `tests/release/verify-assets.sh`/`.ps1` updated to assert exactly four assets (three binaries + `SHA256SUMS`) and to fail if either installer script is present, in the manifest or the directory. `tests/installers/verify-install-ps1.ps1`/`verify-install-sh.sh` extended with a new case: an unpinned install against a fixture server/tree with no `latest` release present exits non-zero with the new message and places no binary. `cargo test --all-targets --all-features -- --test-threads=1` and `tests/installers/verify-install-ps1.ps1` re-run after the change (see `docs/verification/llm-wikis-execution.md`). |

### 0.2.13 — 2026-08-05

Ninth review round: a Codex-authored code review of PR #1 (iteration 9) verified R-35's finding-1 fix arm-by-arm (spawn failure, termination/timeout, nonzero exit, success, and parse-failure paths all preserve `claude_enabled_plugins_declared`; the three remaining `no_child_outcome` sites all precede the check; the new test is non-vacuous) and found the whole-PR regression clean. Every remaining finding was prose — five of them, three IMPORTANT, all in the same overclaim family as R-30 finding 5, R-31, R-34, and R-35 finding 2, across seven consecutive review rounds. Rather than a ninth sentence-by-sentence patch, this revision restructures instead of patching. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-36 | §4, §7.2, §20; operator guide §2.8, §3.5, §3.6, new §3.10 | **Structural change**: created one canonical, complete safety-claims reference — operator guide §3.10 "The complete safety picture" (`docs/llm-wikis.md`), organized as three explicit lists: mechanically prevented (mechanism named per entry), instructed-and-detected-after-the-fact (detection mechanism and coverage limit named), and not-covered (every currently-accepted gap this project knows about, stated directly). The operator guide is canonical; the specification's §7.2 and §20 no longer restate the item-by-item breakdown — they state their own narrow, standalone-true claim and point at §3.10 for the rest. **Five findings folded into this restructuring**: (1) IMPORTANT — §20's item-5 correction (R-35) still conflated "offering to save" (the actual scope of §7.2 item 5) with "saving an answer or claiming mutation" (a different behavior with a different, undocumented failure mode); §20's bullet now states only what is narrowly true (no tool exists to regenerate/write/modify) and points at §3.10 for the offer-vs-write distinction. (2) IMPORTANT — operator guide §3.6 said "neither [provider] can write anywhere," contradicting the managed-hook and TOCTOU residuals stated elsewhere; corrected to state write safety as a bounded, not absolute, claim, with §3.10 as the complete statement. (3) IMPORTANT — §3.6 also said equal `project_root`/`content_root` roots make Claude's read reach "genuinely... exactly `content_root`" — true only for this project's own configuration surface; the operator's own *user-scope* Claude settings (never inspected by this project) could still widen it via a `permissions.allow` rule. Corrected to state this precisely as an accuracy point, not a vulnerability, since user-scope settings are the operator's own trusted configuration. (4) MINOR — `tests/config_contract.rs`'s header comment still listed `permissions.{allow,deny,defaultMode}` as admitted; the implementation (R-30) denies `permissions` entirely. Corrected, with the R-29→R-30 history kept for context. (5) MINOR — `InvokeOutcome`'s doc comment (`src/providers/mod.rs`) said `claude_enabled_plugins_declared` is `false` "whenever no child ran," which stopped being true the moment R-35's own fix shipped (a spawn failure is "no child ran" but the field is `true` there when the check found `enabledPlugins` declared). Corrected to describe the actual rule (false only before the check runs or when it errors; carried through every outcome once it has run) and to warn a future refactor against reintroducing the shortcut. | Codex's iteration-9 review, relayed by the coordinator with all five findings and exact locations. No live row: findings 1-4 and the restructuring are documentation-only; finding 5 is a doc-comment-only correction to an already-shipped, already-tested field — none change behavior. `cargo test --all-targets --all-features -- --test-threads=1` re-run in full after every edit: 354 passed (unchanged — no test logic touched), 0 failed. Per instruction, this revision does not claim the underlying overclaim pattern is now exhausted; it records what was restructured and lets the next review judge. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 9". |

### 0.2.12 — 2026-08-05

Eighth review round: a Codex-authored code review of PR #1 (iteration 8) confirmed both iteration-7 findings resolved and a whole-PR regression sweep clean, then found three new issues — one a real code bug reintroducing the exact class of gap the iteration-6 fix existed to close, on the error path this time; two prose (one normative-claim precision, one a broken cross-reference this loop itself introduced in 0.2.11). User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-35 | §4, §7.2, §20 | (a) **Finding 1 (real code bug)**: `ClaudeAdapter::invoke`'s spawn-attempt failure path routed through `no_child_outcome`, which unconditionally sets `claude_enabled_plugins_declared: false` — correct for the three call sites that run *before* the authoritative settings check, wrong for this one, which runs *after* that check has already determined the value. A query whose settings genuinely declare `enabledPlugins` and whose spawn then failed would silently lose the `CLAUDE_ENABLED_PLUGINS_DECLARED` warning — precisely the class of gap R-33 (iteration 6) existed to close, reintroduced on the error path. Fixed: that one call site now constructs `InvokeOutcome` directly, carrying the already-computed boolean through instead of delegating to `no_child_outcome`. (b) **Finding 2 (normative-claim precision)**: §7.2's "How these are guaranteed" paragraph said items 5-8 are "enforced mechanically by the harness, not by the prompt" without qualification — true in effect for items 6-8 (write-prevention: no Write/Edit/Bash/web tool exists, Codex's sandbox blocks writes regardless of what the model tries) but not for item 5 (omitting an answer-saving offer), which is text output the harness has no mechanism to prevent or detect at all — the harness backs the *outcome*, not the *attempt or the offer*. Rewritten to distinguish "cannot succeed, and is detected if it lands" (items 6-8) from "no mechanical backstop at all, prompt-instructed only" (item 5). The identical correction applied to §20's parallel Acceptance Criteria bullet. (c) **Finding 3 (broken cross-reference)**: the 0.2.11 correction to §20's "no file is created/modified/deleted" bullet cited "§7.2 item 17" — §7.2 has only nine items; the real source is §4's Design Decision 17. Fixed. A full audit of every `§`-section and `R-`-revision cross-reference introduced across R-27 through R-34 found no other broken reference: every `§N`/`§N.M` token resolves to a real heading in the spec or (when explicitly labelled "operator guide") in `docs/llm-wikis.md`, every `item N` token is within its section's actual item range, and every `R-N` token referenced anywhere in the document has a matching row in this table — this one was the only break. | Finding 1: TDD'd with a genuine red run — a new `claude_adapter` test (`spawn_failure_after_authoritative_check_still_reports_enabled_plugins_declared`) declares `enabledPlugins` in a fixture's settings, queues a simulated spawn failure on the `FakeProcessRunner`, and asserts `InvokeOutcome::claude_enabled_plugins_declared` is still `true`; run against the pre-fix code (`src/providers/claude.rs` temporarily reverted via `git stash`, restored after, diffed byte-identical) it failed exactly as predicted, then passed once restored. Findings 2/3: reasoned correction plus the cross-reference audit described above; `cargo test --test spec_drift` re-run and passing (unaffected — none of the three normative tables `spec_drift.rs` parses were touched). Full gate re-run: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features -D warnings` clean, `cargo test --all-targets --all-features -- --test-threads=1` → 354 passed (353 + this one new test), 0 failed. No live row re-run: finding 1 is an error-path fix with no argv/enforcement/allowlist change (the same reasoning R-33 already established for the analogous success-path fix); findings 2/3 are documentation-only — reasoned in the checkpoint rather than silently skipped. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 8". |

### 0.2.11 — 2026-08-05

Seventh review round of the Claude wiki-side settings surface work: a Codex-authored code review of PR #1 (iteration 7) confirmed findings A and B from iteration 6 both resolved, all `InvokeOutcome` construction sites threaded correctly, no behavioral regression anywhere in the PR — zero code findings — plus two new prose-accuracy findings. Rather than another one-sentence patch, a systematic sweep for the same defect class (absolute/totality wording making a safety, detection, or scope claim) was run across `docs/llm-wikis.md`, this specification, and every doc comment in `src/`, per instruction. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-34 | §7.2, §20; operator guide §2.8, §3.4 | (a) **Finding A (IMPORTANT)**: §7.2's "How these are guaranteed" paragraph claimed the §12 content snapshot "detects any mutation that occurred regardless" — false, since §12 itself excludes the two top-level `.claude`/`.agents` directories. Rewritten to state precisely what is detected (a mutation anywhere in the `content_root` tree the snapshot actually covers) and what is not (a mutation confined entirely to one of the two excluded trees, e.g. from an admin-managed hook or the residual check-to-spawn race). (b) **Finding B (MINOR)**: the operator guide's §3.4 layer 7 said an unsafe-settings rejection happens "before any provider call" — false for `query`, whose bounded version and `auth status` probes (spec §8.1 steps 7-8) already invoke the provider executable by the time this check runs (R-31 relocated it to immediately before the real answer-generating spawn, inside `ClaudeAdapter::invoke`). Rewritten to state the rejection happens before the query is ever answered, explicitly noting the two probes run first. (c) **Sweep-discovered, same pattern as (a)**: the operator guide's §2.8 ("`llm-wikis` never modifies a knowledge base") independently repeated finding A's exact overclaim — "the before/after content snapshot ... still catches any mutation that does land, regardless of how it got there" — corrected identically. (d) **Sweep-discovered, same pattern as (a)**: this specification's own §20 Acceptance Criteria stated, bolded, "No file in any knowledge base is created, modified, or deleted by any `llm-wikis` command" — a stronger, unqualified claim that reads as covering the same residuals items (a)/(c) admit exist. Narrowed to what is actually guaranteed unconditionally — no `llm-wikis` command has a code path that writes — with a pointer to the honestly-stated residuals in §7.2/§10.2/§12 for the rest, rather than implying those residuals do not exist. | The systematic sweep grepped `docs/llm-wikis.md`, this specification, and every `///`/`//!` doc comment in `src/` for "any", "all", "every", "always", "never", "no ", "none", "regardless", "cannot", "impossible", "before any", "at all", then checked each occurrence making a safety/detection/scope claim against the actual code path. Full results, including every location checked (not only those changed), recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 7". No other instance of either defect pattern was found; `src/snapshot.rs`'s own module doc comment (the actual implementation of the layer both overclaims mischaracterized) was already accurate throughout and needed no change. Documentation-only correction: no code path changed, so no live row was re-run — reasoned in the checkpoint rather than silently skipped. Full gate re-run: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features -D warnings` clean, `cargo test --all-targets --all-features -- --test-threads=1` → 353 passed (unchanged from iteration 6, since no test logic changed), 0 failed. |

### 0.2.10 — 2026-08-04

Sixth same-day review round of the Claude wiki-side settings surface: a Codex-authored code review of PR #1 (iteration 6) returned NO blocking findings — all eight iteration-5 findings confirmed resolved, no regressions — plus one IMPORTANT prose finding and one MINOR behavioral finding. The orchestrator accepted both and specified the required fixes. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-33 | §10.2, §12, §15 | (a) **Prose (finding A)**: the sentence "`disableAllHooks` disables every hook regardless of source" was self-contradictory within its own paragraph — the same paragraph's "Honest residual gap" sentence admits an admin-managed/enterprise-policy hook is *not* disabled by any flag. Rewritten, here and at every other instance of the same unqualified "every/all hook ... regardless" phrasing found in a live (non-historical) file, to state the scope correctly up front: "disables every hook declared by user, project, local, or plugin-dir settings" — explicitly *not* an admin-managed/enterprise-policy hook — with the exception stated inline rather than only several sentences later. (b) **Behavior (finding B)**: `check_claude_wiki_settings_surface`'s `Ok(bool)` at the authoritative pre-spawn call site (`ClaudeAdapter::invoke`, R-31) used to be discarded (`if let Err(e) = ...`) — fine for enforcement, since the `Err` path was already handled, but it silently dropped the one signal that should drive `CLAUDE_ENABLED_PLUGINS_DECLARED`. `QueryService::query` separately re-read the same check *earlier* (before the version/auth probes, R-32) purely to surface the warning as early as possible in generation order. A wiki whose settings started declaring `enabledPlugins` only between that earlier read and `invoke`'s authoritative one would still be enforced correctly (admitted, since `enabledPlugins` is allowlisted) but would spawn with the warning silently missing — the operator told nothing about an admitted risk-acceptance key that *was* in effect for that call. Fixed by removing the early read entirely and threading the authoritative boolean through a new `InvokeOutcome::claude_enabled_plugins_declared` field (`false` for every Codex path and every Claude path where no check ran or it rejected the wiki); `QueryService::query` now pushes the warning once, after `invoke_provider` returns, driven by that field — there is exactly one read of the settings file for this purpose now, and it is the same one that enforces the allowlist. | Prose: grepped the full touched-file set (`docs/2026-07-28-llm-wikis-external-query-design.md`, `docs/llm-wikis.md`, `src/providers/claude.rs`, `tests/claude_adapter.rs`) for the exact contradictory pattern; every live instance fixed identically. One historical instance (`docs/verification/llm-wikis-execution.md`'s "review loop iteration 2" checkpoint prose, dated 2026-08-04 early in this task) carries the same pattern but was deliberately left unedited, consistent with this document's own established practice of not rewriting dated checkpoint/revision-history text after the fact — recorded explicitly, not silently skipped, in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 6". Behavior: TDD'd with a genuine red run — a new `query_service` test (`enabled_plugins_warning_is_never_missed_even_if_settings_appear_after_the_probes_start`) starts a fixture with no settings file at all and injects one from inside a custom `ProcessRunner` the first time `run` is called (i.e. no earlier than the version probe, strictly after where the old early read used to execute and strictly before `invoke`'s own check); run against the pre-fix code (temporarily reverted via `git stash`, restored after, diffed byte-identical) it failed exactly as predicted (`expected the enabledPlugins advisory warning ... got []`), then passed after restoring the fix. Full gate re-run: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features -D warnings` clean, `cargo test --all-targets --all-features -- --test-threads=1` → 353 passed (352 + this one new test), 0 failed. No live rows re-run: neither fix touches provider invocation semantics (finding A is prose-only; finding B changes which of two identical filesystem reads drives a warning, not the argv, the enforcement decision, or anything else observable by the provider) — reasoned here rather than silently skipped, per instruction. Recorded in full in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 6". |

### 0.2.9 — 2026-08-04

Resolution of the finding 1 hold from the 0.2.8/iteration-5 review: whether `enabledPlugins` should remain admitted in `check_claude_wiki_settings_surface`'s allowlist at all, given the theoretical risk that project-scope plugin activation could reach an undocumented-restriction component (LSP servers). Backed by documentation research and an empirical test, both cited below. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-32 | §12, §13, §15 | `enabledPlugins` is **kept** in the allowlist (denying it would reject the real `harness-engineering` wiki, whose settings declare exactly and only that key). A new wrapper warning code, `CLAUDE_ENABLED_PLUGINS_DECLARED` (§13), is emitted by both `doctor` and `query` whenever a wiki's Claude settings declare `enabledPlugins` — a short, non-alarmist message stating the key is admitted because headless-mode activation was observed to have no effect on the tested CLI version, not a documented guarantee, and that plugin hooks remain disabled and MCP remains locked regardless. `check_claude_wiki_settings_surface`'s doc comment records the full risk-acceptance reasoning: official plugin-component-type documentation lists Skills, Agents, Hooks, MCP servers, LSP servers, Monitors, Themes, Workflows, Output styles, Channels, and a `bin/` directory; Monitors are documented interactive-only; LSP servers are documented as auto-starting subprocesses with **no** documented interactive-only restriction (the theoretical reach path); `enabledPlugins` itself is documented as honored from project/local scope (unlike `permissions.allow`/`additionalDirectories`, documented to fail closed under `-p`); a plugin's own settings can only carry `agent`/`subagentStatusLine`, so it cannot contribute `additionalDirectories` or a new marketplace source itself. | Empirical: on Claude Code 2.1.221, this project's exact argv, cwd = a disposable fixture wiki, a `.claude/settings.local.json` declaring `enabledPlugins` for a plugin that **is installed but not user-scope-enabled** produced a `system/init` event whose loaded-plugin count matched only the pre-existing user-scope-enabled set, with the declared plugin absent and `plugin_errors: null` — project-scope activation had no observed effect in headless mode on this version. This is recorded as an observation on one CLI version, not a contract; a future release honoring project-scope activation would require revisiting this admission. Re-verified against both real registered wikis, including `harness-engineering`'s `enabledPlugins`-only case now correctly surfacing the new warning while still passing; LIVE-01/03/09 re-run and passing. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 5". |

### 0.2.8 — 2026-08-04

Fifth same-day correction of the Claude wiki-side settings surface: a Codex-authored code review of PR #1 (iteration 5) returned two BLOCKING findings, three prose-accuracy findings (the fourth consecutive round with this defect class), and two test-quality findings; the orchestrator held finding 1 (`enabledPlugins`'s admission) pending separate research and specified fixes for the rest. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-31 | §6.1, §10.2, §12, §15 | (a) `symlink_metadata` on the two settings *filenames* only examines the final path component; `check_claude_wiki_settings_surface` now also checks `project_root/.claude` itself the same way, before looking at either filename — a `.claude` directory that is itself a symlink/junction pointing at an initially empty location previously passed both files' "not found" branch, letting the real settings file be created at the true target afterward. (b) The `query`-time call site moves a second time: out of `QueryService::query` entirely and into `ClaudeAdapter::invoke`, immediately before the one call that spawns the real process — after plugin-dir canonicalization, temp-dir/temp-file creation, and argv/schema construction, none of which needed to precede it. One function remains the single source of truth, called identically from `doctor` and this new call site. The residual check-to-spawn window narrows from "several query steps" to the syscall gap between the check returning and `Command::spawn` executing — still not claimed closed. §14's error-code prose is unaffected (still the broadened `ENTRYPOINT_INVALID`/`UNSAFE_FILESYSTEM_ENTRY` pair from R-30). A pre-existing overclaim in §6.1 (the `.claude`/`.agents` special-entry-scan exclusion rationale, predating this task, claiming blanket `skill_fingerprint` coverage) was found during the same sweep and corrected to match the already-corrected §12 text. | Live: the orchestrator's own analysis and the worker's reproduction confirmed (1) a `.claude`-directory symlink fixture would have passed the R-30 check unmodified; (2) `check_claude_wiki_settings_surface`'s R-30 call site in `query.rs` still left plugin-dir resolution, `ProviderRequest` construction, temp-dir/temp-file creation, and argv/schema generation between the check and the real spawn. TDD'd (failing tests captured, including a strengthened `query_service` test that pins the two probe requests were actually captured — proving the check runs *after* them, not merely "before some spawn," per the same review's test-quality finding). Re-verified against both real registered wikis, including `harness-engineering`'s `enabledPlugins`-only case; LIVE-01/03/09 re-run and passing. A complete grep-based sweep of every settings-allowlist/TOCTOU/fingerprint-coverage/read-reach claim in the affected source and doc files was performed and is recorded in full in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 5", including every location checked, not only those changed. |

### 0.2.7 — 2026-08-04

Fourth same-day correction of the Claude wiki-side settings surface: a Codex-authored code review of PR #1 (iteration 4) returned two new BLOCKING findings against the R-29 fix plus two minor findings (error-code taxonomy, an operator-doc overclaim about fingerprint coverage); the orchestrator accepted all four and specified the required fixes. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-30 | §10.2, §12, §14, §15 | (a) Shrinks the R-29 allowlist to `enabledPlugins` alone — `permissions.{allow,deny,defaultMode}` is removed entirely, key and values, since a permission rule's *value* (not just its key) can pre-authorize a specific path for the exposed `Read` tool; `enabledPlugins`'s own value is now validated to be an object of plain booleans. (b) The settings-path check now uses `symlink_metadata` and rejects any non-regular-file entry (symlink, junction, reparse point, directory, mount point) as `UNSAFE_FILESYSTEM_ENTRY`, rather than a plain existence check that would report a dangling symlink as absent. (c) The `query`-time check moves to immediately before the provider is spawned (after version/auth probes, the fingerprint gate, and the before-snapshot), narrowing the check-to-spawn TOCTOU window; §10.2/§12 state honestly that this narrows, not closes, the window — full closure would need OS-level isolation, out of scope. §14's `ENTRYPOINT_INVALID` definition is broadened (not replaced with a new code) to explicitly cover this class of failure, judged coherent because the exit class (2, argument/config-class, preceding any provider call) and doctor check name (`entrypoint`) are already identical to the pre-existing local-plugin-lifecycle use of the same code, and a new code would fragment an already-coherent bucket without changing exit/handling behavior. §12's content-snapshot paragraph and the operator guide's mutation-detection section are corrected to state precisely what `skill_fingerprint` actually covers (the one configured skill/plugin directory, not the whole `.claude`/`.agents` tree), removing a pre-existing overclaim found during the same review pass (finding 5). `docs/verification/llm-wikis-v0.1.0-checklist.md` row `PROC-22`'s staleness (recording the pre-R-27 argv) remains explicitly deferred to Task 16, per the plan's own rule that the checklist is the independent verifier's artifact. | Live: the orchestrator's own `apiKeyHelper` reproduction (same fixture as R-29's evidence) plus manual analysis found (1) a hand-constructed `{"permissions":{"allow":["Read(C:/Users/alice/.ssh/**)"]}}` fixture would have passed the R-29 check unmodified — `--tools Read,Grep,Glob` bounds tool names, not path-qualified rule values; (2) a dangling `.claude/settings.json` symlink would pass a plain existence check and let its target be created before invocation, since `.claude/` is excluded from the recursive special-entry scan. TDD'd with a genuine red run (enforcement temporarily reverted to the pre-R-30 shape via a backed-up/restored/diffed file, not merely reasoned about): the new/updated tests for both sub-issues failed against the reverted code with the exact expected diffs, then passed again after restoring the fix. Re-verified against both real registered wikis: `harness-engineering`'s actual `settings.local.json` (`enabledPlugins` only) still passes; LIVE-01/03/09 re-run and passing. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 4". |

### 0.2.6 — 2026-08-04

Third same-day correction of the Claude wiki-side settings surface, after a Codex-authored code review of PR #1 (iteration 3) found the R-27/R-28 hook-only mitigation left other executable settings keys open, independently verified live by the orchestrator before authorization. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-29 | §10.2, §12, §15 | Adds a static allowlist deny check (§15, under check name `entrypoint`): a Claude wiki's `project_root/.claude/settings.json`/`settings.local.json` may declare only `enabledPlugins` and `permissions.{allow,deny,defaultMode}`; any other top-level key (or any other key under `permissions`) fails the wiki closed with `ENTRYPOINT_INVALID`, at both `doctor` and `query` time (closing the TOCTOU gap between the two). `--settings {"disableAllHooks":true}` (R-27/R-28) is kept as defense in depth for the admitted surface. §10.2 and §12 restate the trust boundary honestly: settings load, bounded by the deny check plus the hook/MCP/tool argv layers, not by non-loading. Investigated and deliberately not extended to Codex: Codex loads project-level configuration (`.codex/config.toml`, project hooks, project execpolicy `.rules` files) only for a **trusted** project, and this wrapper's invocation never establishes trust, so that surface fails closed by Codex's own design. | Live: the orchestrator's own scratch-fixture reproduction of the R-27/R-28-corrected argv (`--settings {"disableAllHooks":true}`, no `--setting-sources`) found that a wiki declaring `apiKeyHelper` as a command still executed it — `disableAllHooks` bounds only the `hooks` key, not `apiKeyHelper`/`awsCredentialExport`/`awsAuthRefresh`/`gcpAuthRefresh`/`otelHeadersHelper`/`statusLine` (all of which run a configured command), `permissions.additionalDirectories` (widens read reach), or `env` (can redirect API traffic) — falsifying the R-28 trust-boundary text as shipped. The corrected check was TDD'd (failing tests first) and re-verified against both real registered wikis: `harness-engineering`'s actual `settings.local.json` (`{"enabledPlugins":{"llm-wiki@llm-wiki":true}}`) is exactly the allowlisted case and continues to pass. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 3". |

### 0.2.5 — 2026-08-04

Same-day correction of 0.2.4/R-27, after re-verifying LIVE-01/03/09 under the R-27 argv found the fix itself broke project-skill discovery. Diagnosed by the orchestrator via a live four-arm controlled experiment on a disposable scratch fixture (never a registered wiki), independently authorized. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-28 | §10.2, §12 | Removes `--setting-sources user` from the Claude argv (R-27's other addition, `--settings {"disableAllHooks":true}`, is kept). §10.2 gains a corrected-mechanism paragraph and the trust-boundary is restated honestly: project/local settings are loaded (required for skill discovery), bounded by the hook/MCP/tool layers rather than by non-loading. §12's bullet is updated to match. | Re-verifying LIVE-01 under the R-27 argv failed: `INVALID_NATIVE_OUTPUT`, `"expected value at line 1 column 1"`, ~2.9s (far faster than any real model turn). A four-arm experiment isolated the cause: (A) old argv + plain prompt succeeds — ruling out the concurrent Claude Code 2.1.220→2.1.221 auto-update as the cause; (B) R-27 argv + plain prompt succeeds — ruling out the new flags alone; (C) R-27 argv + a project-skill slash entrypoint fails with `"result":"Unknown command: /probe"`, `duration_ms:13` — exactly the LIVE-01 failure signature; (D) old argv + the same entrypoint succeeds. Root cause: `--setting-sources user` excludes the `project` setting source, and Claude's project-skill discovery is gated on that source — this affects **both** registered wikis (`/wiki-query`, `/llm-wiki`), both `project_skill` load mode. Two further arms validated the fix on the same fixture (now with both a project skill and a project `SessionStart` hook): (E) `--settings {"disableAllHooks":true}` alone — skill resolved and answered correctly, and no hook marker file was created; (F) neither flag (control) — the hook marker file *was* created, confirming the hook is genuinely live in that fixture and (E)'s absence is real suppression. LIVE-01/03/09 re-verified passing under the corrected argv on Claude Code 2.1.221. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 2". |

### 0.2.4 — 2026-08-04

Correction from a Codex-authored code review of PR #1 (Task 15's docs/live-verification pull request), independently verified by the orchestrator before authorization. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-27 | §10.2, §12 | The Claude argv gains `--setting-sources user` and `--settings {"disableAllHooks":true}`, appended immediately after `--json-schema` and before any `local_plugin` `--plugin-dir`. §12's enforcement-layer list gains the corresponding bullet, naming the honest residual gap (admin-managed/enterprise-policy hooks cannot be disabled by any flag). | Claude Code 2.1.220's `-p` mode auto-loads and executes hooks (e.g. `SessionStart`) declared by whatever directory it is launched in — confirmed by official documentation (settings precedence: CLI flags outrank user/project/local; `--setting-sources` accepts `user`/`project`/`local`; `disableAllHooks` is a documented `settings.json` boolean key) and empirically, live, on a disposable scratch fixture (never a registered wiki): the pre-correction argv let a `SessionStart` hook write an arbitrary file containing real session metadata; the identical invocation with the two new flags did not, and the query still completed normally. Recorded in `docs/verification/llm-wikis-execution.md`, Task 15 "review loop iteration 1". |

### 0.2.3 — 2026-07-31

Second correction from the same Task 2 Step 13 loop, after an instrumented rerun identified the persisting `mcp_tool_call` item and the cause of Codex's empty results. User-approved.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-26 | §7.1, §7.2, §10.1, §10.3, §12 | (Includes the §10.1 safety-invariant sentence: shell execution "disabled or excluded" now reads excluded under Claude, confined-and-read-only under Codex — caught by the 0.2.3 independent review as a residual contradiction.) The envelope `constraints` array becomes provider-specific: the "no shell tool" constraint is true for Claude but disabled Codex's only read mechanism (sandboxed command execution), so Codex's list permits read-only commands while the sandbox enforces read-only. §7.2 item 8 is qualified per provider. The MCP invariant is scoped to external/user-configured servers; Codex's internal introspection surface (server `"codex"`, e.g. `list_mcp_resources`) is benign, undisableable, and recorded in diagnostics. The offline Codex item-type assertion accepts `mcp_tool_call` items naming server `"codex"` with introspection tools only. §12's Codex layer bullet states the full exclusion mechanism. | Instrumented Phase 0 rerun (authorized): the sole `mcp_tool_call` item is server `"codex"`, tool `"list_mcp_resources"`, `error: null`; the model's own recorded warning — "The supplied constraints prohibited shell commands, and no permitted local-file reading tool was available" — explains every `no_relevant_material` on `agents`×codex. Recorded in `docs/verification/llm-wikis-preflight.md` Row 11 addendum 2. |

### 0.2.2 — 2026-07-31

Correction from Phase 0 live evidence, applied through the plan's Task 2 Step 13 correction loop with explicit user approval.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-25 | §10.1, §10.3 | The Codex argv adds `-c mcp_servers={}`, `--disable browser_use`, and `--disable computer_use`; §10.1 states that MCP exclusion must be explicit per provider because ignoring user configuration cannot reach a provider's built-in runtime MCP servers. | Phase 0 Step 11: 2/2 live Codex event streams contained the `mcp_tool_call` type string despite `--ignore-user-config`. Root cause: Codex bundles a `node_repl` MCP server inside its own installation (`AppData\Local\OpenAI\Codex\runtimes\cua_node\`), outside user config. Both content-root snapshots stayed byte-identical, so the residual layers held. The disable flags were live-verified as accepted by codex-cli 0.144.6 in the R-25 rerun (exit 0, empty stderr), recorded in `docs/verification/llm-wikis-preflight.md` Row 11 addendum; the `node_repl` identification came from orchestrator-session `codex mcp list` inspection during the correction loop, recorded here. Superseded in part by R-26, which identified the persisting item as Codex's internal introspection surface. |

### 0.2.1 — 2026-07-31

Corrections from the independent three-pass re-review (spec internal, plan internal, spec↔plan↔reality cross-check; report at `.scratch/spec-plan-review/independent-review-2026-07-30.md`). The review found no blockers and no design changes; every entry below corrects prose, not behavior.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-21 | §6.1, §12, §23 R-11 | The `.claude/`/`.agents/` exclusion is restated with its actual geometry: such directories sit beneath `content_root` only when `content_root == project_root` (the `agents` case); for a nested content root they are siblings, outside the scan, and the exclusion is a no-op. R-11's claim that the exclusion protects both wikis is corrected — for `harness-engineering` it protects nothing and nothing needs protecting. | Review finding M2 against §2's own layout diagram. |
| R-22 | §15 | The static-checks list gains the previously omitted authentication-status bullet; `auth` was already one of the nine `checks[].name` values and §10.1 already described the non-billable probe. | Review finding N1 (found independently by two passes). |
| R-23 | §17.2 | "Row" is defined: a Phase 0 row is a `docs/verification/llm-wikis-preflight.md` row, distinct from §16.2 `LIVE-0N` rows and §17.1 checklist rows. | Review finding N2 — three same-sized row sets invited conflation. |
| R-24 | §23 R-14, R-02 | R-14's "thirteen times" compared the wrong-root snapshot against the *other* wiki's root; against its own correct root (`wiki/`, 7.1 MB) the factor is ≈10×. R-02's "91 candidate directories" figure appears in no recorded evidence file and is dropped. | Review findings N6 and NIT-1; `spike-results.json` targets B and C. |

### 0.2.0 — 2026-07-30

Amended after inspecting the two real knowledge bases and measuring the assumptions against them. Every change below has recorded evidence in `.scratch/spec-plan-correction/`: `codex-verification-2026-07-29.md` (independent Codex verification of spec-versus-reality, with spec line numbers and on-disk evidence), `spike-results.json` (a disposable Rust measurement spike over both wikis), and one ticket per decision.

| ID | Sections | Change | Evidence |
|---|---|---|---|
| R-01 | §2, §6, §16.2, §20, §21 | The assumed single `agents/` project is replaced by the two real knowledge bases and their genuinely different layouts. | Direct inspection. |
| R-02 | §6.1, §8.1, §15 | Preflight no longer requires `SCHEMA.md`, `wiki/index.md`, or `wiki/pages/`. It requires only an existing directory containing at least one Markdown file. | Codex scanned every candidate directory under `harness-engineering` and found none satisfying the three-clause requirement; the combination was unsatisfiable, though `SCHEMA.md` alone sits at the content root of both wikis. |
| R-03 | §11, §14, §15 | The `link_style` mechanism is deleted: no `SCHEMA.md` parsing, no `link_style`, no `link_style_rules`, no `LINK_STYLE_UNSUPPORTED`, no corresponding doctor check. One fixed superset grammar is always applied. | It selected between two nested parsers and in practice selected nothing — `agents` declares `obsidian`, `harness-engineering` has no `## Cross-References` section and would default to it. The spike found the markdown form in no real wiki content: all seven occurrences are in upstream documentation of the syntax. |
| R-04 | §9 (removed), §6, §8.1, §13, §14, §15, §16.1 | Index freshness checking is removed entirely, with `index_freshness`, `INDEX_MAY_BE_STALE`, `INDEX_STALE`, and the `audit-*` exclusion. | The spike showed `harness-engineering` already reports stale for one reason: `log.md`, an operation log, is newer than `index.md`. The only fix is an exclusion list naming which files are pages — the layout knowledge R-02 removed. §9 already described itself as "not proof". |
| R-05 | §11.2, §13, §14 | Citations resolve to exactly one `<slug>.md` anywhere beneath `content_root`; two or more is the new `CITATION_AMBIGUOUS` at exit 2 with closed details `{slug, match_count}`. | The spike measured zero ambiguous slug-shaped stems at both configured content roots (31 and 361 stems). Thirteen collisions appeared only at a deliberately wrong root, making ambiguity a useful mis-configuration signal. |
| R-06 | §11.1 | The labelled form `[[slug\|label]]` is recognized as page provenance. | It is 2,811 of 8,545 citations in `harness-engineering` — 33 %. Excluding it, as 0.1.0 did, would discard a third of that wiki's provenance. |
| R-07 | §11.3 | Inline citation extraction becomes best-effort; strictness applies only to the model's explicit `citations` array. | The spike found `agents` resolves only 17 of 41 distinct extracted slugs; the remainder are syntax examples, three of them inside `SCHEMA.md`, the first file a skill is told to read. Fail-fast would discard correct answers for echoing a schema example. §11 already drew this same line for `CITATION_INVALID`. |
| R-08 | §7.1, §7.2, §3.2, §4, §12, §20 | The external-readonly mode is carried by the prompt envelope, not by a mode branch added to each wiki's skill. 0.1.0's explicit prohibition on relying on a later prompt is overridden. | Codex confirmed neither existing skill implements the contract, so 0.1.0 required modifying skills in two repositories this project does not own. The prohibition's concern is real but its severity is bounded: writes are prevented mechanically by the tool set, the Codex sandbox, and the content snapshot, so the residual risk is answer quality, not safety. |
| R-09 | §6.5 (removed), §3.2, §4, §12, §20 | All skill ownership, overlay, deployment, and installation is removed. `llm-wikis` has no command that writes to a knowledge base. | Follows from R-08. An intermediate design in which the binary embedded and installed its own skill was adopted and then withdrawn: writing out the resulting configuration showed the two query profiles became byte-identical except for their names, revealing the design had drifted back to a shared skill that discards each wiki's own layout knowledge. |
| R-10 | §6, §6.4, §15.1 | `query_profiles` is removed; each wiki declares its provider tables inline and a constrained one-line `query_prompt`. `QUERY_PROFILE_NOT_FOUND`, `PROVIDER_PROFILE_MISSING`, and `CONTRACT_UNSUPPORTED` are deleted. | With two wikis on two toolchains the profile table indirected nothing. `query_prompt` is a narrow, validated exception to §6.4, justified by Design Decision 12's classification of configuration as trusted. `CONTRACT_UNSUPPORTED` lost its subject once no skill declares implementing a contract. |
| R-11 | §6.1, §12 | Any `.claude/` and `.agents/` directories immediately beneath `content_root` are excluded from the special-entry scan and the content snapshot. Such directories exist there only when `content_root` equals `project_root`. | In `agents` (`content_root == project_root`), the apm-managed symlinks at `.claude/skills/markitdown` and `.agents/skills/markitdown` target outside the wiki, and no containment relaxation could admit them — without the exclusion every scan aborts. In `harness-engineering` the same trees are siblings of the nested `wiki/` content root and were never inside the scan; the exclusion is a no-op there. The spike confirmed both wikis clear to zero special entries, and those trees are already covered by `skill_fingerprint`. |
| R-12 | §6.1, §4 | Containment is explicitly inclusive: `content_root` may equal `project_root`. | `D:\Wikis\agents` is the only directory under that wiki able to serve as a content root, and it is also the project root. |
| R-13 | §10.2, §13, §15 | `CLAUDE_READ_SCOPE_BROAD` is emitted only when `content_root` is a strict subdirectory of `project_root`. | When the roots are equal, read reach is exactly `content_root` and the warning's own text is false. |
| R-14 | §15 | New warning `WIKI_SCHEMA_ABSENT` under check name `wiki_structure`, never a failure. | The spike showed a content root set one level too high passes minimum preflight on 449 Markdown files, then snapshots 69.2 MB across 642 files at 6.9 s — nearly ten times its correct root (`wiki/`, 7.1 MB). A missing `SCHEMA.md` at the content root is the only cheap signal distinguishing the two. |
| R-15 | §15.1 | `__pycache__/`, `*.pyc`, and `*.pyo` are excluded from `skill_fingerprint`. | `harness-engineering`'s skill directory contains Python scripts whose byte-compilation output changes on unrelated interactive runs; without the exclusion, ordinary use invalidates the probe. |
| R-16 | §15.1 | `query_prompt` participates in the compatibility fingerprint. | It changes what the model was told, so a probe taken under a different prompt does not verify the current one. |
| R-17 | §17.2, §19 | Phase 0 passes per row; unreachable rows are `PENDING` and block release claims, not implementation. The three-target native smoke build and the Linux/macOS process rows move to the release automation task. | 0.1.0's Task 2 was told to run native CI that only the release task defines, and its Final Handoff Condition simultaneously required Task 2 to have "passed" — jointly unsatisfiable. §17.2 already stated the per-row rule; the plan contradicted it. |
| R-18 | §16.1 | New offline test class: the Section 14 error table, the Section 15 check-name vocabulary, and the Section 13 warning-code vocabulary are mechanically compared against the implementation. | Adopted from `microsoft/shell-use`, which generates its agent-facing surface from its command model "so it can never drift". Generation was rejected here as over-built for a five-command CLI with no SDK; a test buys the same protection while leaving this specification authoritative. |
| R-19 | §14 | The error table is 27 codes, down from 31. | Removed `LINK_STYLE_UNSUPPORTED` (R-03), `INDEX_STALE` (R-04), `QUERY_PROFILE_NOT_FOUND`, `PROVIDER_PROFILE_MISSING`, `CONTRACT_UNSUPPORTED` (R-10); added `CITATION_AMBIGUOUS` (R-05). |
| R-20 | §22 | The no-Git decision is recorded as taken rather than pending. | Decided 2026-07-29. |

Sections §6.5 and §9 retain their headings as tombstones so references from the acceptance checklist and implementation plan continue to resolve.

Unchanged by this revision: the single-wiki-per-invocation model, the provider adapter split, stdin question transport, the process supervision contract, the full-content mutation snapshot, the live-probe gate, the public envelope shape, the exit-class taxonomy, and the release and installer design.
