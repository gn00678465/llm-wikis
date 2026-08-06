# llm-wikis — Operator Guide

`llm-wikis` is a Rust CLI that runs a read-only question against a pre-built
knowledge base ("wiki") through Claude Code or Codex CLI, from anywhere on
disk — no `cd` into the wiki's own project, no copy/paste of files, no
hand-built prompt. It never modifies a knowledge base. This guide covers
installation, configuration, and the query/security contract. For the formal
specification, see
[`docs/2026-07-28-llm-wikis-external-query-design.md`](2026-07-28-llm-wikis-external-query-design.md).

## 1. Installation

### 1.1 Latest and pinned installs

**Linux / macOS (Apple Silicon only):**

```sh
curl -fsSL https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.ps1 | iex
```

Both scripts install the **latest** published release by default. Set
`LLM_WIKIS_VERSION` (an exact tag, e.g. `v0.1.0`) before running either
script to install a specific version instead — this pins, upgrades, or
downgrades in place:

```sh
curl -fsSL https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.sh | LLM_WIKIS_VERSION=v0.1.0 sh
```

(The variable must be scoped to `sh` — the process that actually reads it — not to `curl`. `LLM_WIKIS_VERSION=v0.1.0 curl ... | sh` sets the variable only for `curl`'s environment and never reaches the piped `sh`.)

```powershell
$env:LLM_WIKIS_VERSION = "v0.1.0"
irm https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.ps1 | iex
```

### 1.2 Release assets

| Asset | Rust target | Platform |
|---|---|---|
| `llm-wikis-windows-amd64.exe` | `x86_64-pc-windows-msvc` | Windows x64 |
| `llm-wikis-linux-amd64` | `x86_64-unknown-linux-musl` | Linux/WSL x86-64 |
| `llm-wikis-darwin-arm64` | `aarch64-apple-darwin` | macOS, **Apple Silicon only** |

There is **no Intel/x86-64 macOS asset**. `install.sh` detects an
unsupported OS/architecture (Intel Mac, non-x86-64 Linux) and fails with an
explicit error rather than installing the wrong binary. A release also
publishes `install.ps1`, `install.sh`, a `SHA256SUMS` checksum manifest, and
GitHub artifact attestations for every executable and for `SHA256SUMS`
itself.

### 1.3 Install paths and PATH changes

| Platform | Install path | PATH change |
|---|---|---|
| Windows | `%LOCALAPPDATA%\llm-wikis\bin\llm-wikis.exe` | That directory is added to the **user** PATH registry value, only if not already present (no duplicate entries across re-runs). |
| Linux | `~/.local/bin/llm-wikis` | `~/.local/bin` is idempotently added to `~/.profile` if a supported shell is detected and the entry is missing. |
| macOS | `~/.local/bin/llm-wikis` | `~/.local/bin` is idempotently added to `~/.zprofile` (zsh) if missing. |

For an unrecognized shell, `install.sh` prints manual PATH instructions
instead of guessing at an unknown profile file. Restart your shell (or
re-source the profile) after a first install so the new PATH entry takes
effect.

### 1.4 Checksum behavior

Both installers download the raw binary and the published `SHA256SUMS`
manifest, then verify the binary's digest against the matching `SHA256SUMS`
entry **before** installing it. A mismatch fails closed — the download is
discarded and nothing is installed. `install.sh` also fails closed (with an
explicit error, before any download) when neither `sha256sum` nor `shasum`
is available on the host. After the checksum passes, both installers run the
downloaded binary once with `--version` as a smoke test before moving it
into place; a binary that fails to execute (wrong architecture, truncated
download) is also rejected before installation.

### 1.5 Platform security prompts

Neither release binary is code-signed in 0.1.0. SHA-256 digests and GitHub
attestations give you integrity/provenance evidence, but they are **not** a
substitute for platform code signing, and the OS will still warn you:

- **macOS (Gatekeeper).** The `darwin-arm64` binary is an unsigned preview.
  On first run, macOS Gatekeeper will refuse to open it from Finder/normal
  execution. This is normal user-approval friction, not a defect — do not
  disable Gatekeeper or `spctl` system-wide to work around it. Approve this
  one binary via **System Settings → Privacy & Security → "Open Anyway"**
  after the first blocked launch attempt (or `xattr -d
  com.apple.quarantine <path-to-binary>` if you understand what that
  removes and accept the reduced protection for that one file only).
- **Windows (SmartScreen).** The Windows binary is unsigned too, so
  Microsoft Defender SmartScreen may show an "unrecognized app" warning on
  first run. Use **More info → Run anyway** if you trust the source (verify
  the `SHA256SUMS` digest first). Do not disable SmartScreen system-wide.

### 1.6 Manual removal

Version 0.1.0 ships **no uninstaller and no self-update command**. To remove
`llm-wikis` completely:

1. Delete the installed binary:
   - Windows: `%LOCALAPPDATA%\llm-wikis\bin\llm-wikis.exe` (and the now-empty
     `bin`/`llm-wikis` directories, if you want them gone too).
   - Linux/macOS: `~/.local/bin/llm-wikis`.
2. Remove the PATH entry the installer added:
   - Windows: edit the user PATH environment variable (System Properties →
     Environment Variables, or `[Environment]::SetEnvironmentVariable('PATH',
     ..., 'User')`) and remove the `%LOCALAPPDATA%\llm-wikis\bin` segment.
   - Linux: remove the added line from `~/.profile`.
   - macOS: remove the added line from `~/.zprofile`.
3. Optionally delete the config and cache directories (§2.1) if you also
   want the registered-wiki configuration and the live-probe cache gone.

Re-running an installer instead of removing it upgrades, downgrades, or
repairs an existing install in place — there's no separate "repair" command
either, because re-running an installer already does that.

## 2. Configuration

### 2.1 The three config paths and three cache paths

`llm-wikis` has one platform-native default config path, derived purely from
environment variables — **never** from the caller's current working
directory:

| Platform | Config path |
|---|---|
| Windows | `%APPDATA%\llm-wikis\config.toml` |
| Linux/WSL | `${XDG_CONFIG_HOME:-~/.config}/llm-wikis/config.toml` |
| macOS | `~/Library/Application Support/llm-wikis/config.toml` |

The live-doctor probe cache (§2.6) has its own three platform-native paths,
also derived only from environment variables:

| Platform | Cache path |
|---|---|
| Windows | `%LOCALAPPDATA%\llm-wikis\probes-v1.json` |
| Linux/WSL | `${XDG_CACHE_HOME:-~/.cache}/llm-wikis/probes-v1.json` |
| macOS | `~/Library/Caches/llm-wikis/probes-v1.json` |

Neither path is configurable independently of the config path (there is no
`--cache` flag); the cache always sits at its platform-native location.

### 2.2 `--config` is a trust boundary, not a convenience flag

`--config <path>` overrides the default config path. It is an **operator and
testing feature**, not a caller-facing one:

- The path **must be absolute**. A relative path is rejected as
  `ARGUMENT_INVALID` before anything else runs — this is deliberate, so a
  relative override can never resolve differently depending on which
  directory the process happened to be launched from.
- It is **never discovered from the caller's project** — nothing in
  `llm-wikis` walks up from the current directory looking for a config file,
  and there is no per-project config file convention to spoof.
- It is **not accepted through MCP or any other untrusted caller path**. If
  you build automation on top of `llm-wikis`, do not let an untrusted
  request choose the config path; that would let it point the process at an
  arbitrary registry (and therefore an arbitrary set of allowed wikis and
  provider executables).

Use `--config` for local testing, CI, or deliberately running against a
second, non-default registry — not as a request-time parameter.

### 2.3 `config init` never overwrites

```powershell
llm-wikis config init
llm-wikis --config C:\path\to\alt-config.toml config init
```

Creates the parent directory (if needed) and a valid, non-interactive
starter config — runtime/provider defaults, an **empty** wiki registry, and
one commented example wiki block — **only when the destination does not
already exist**. An existing destination fails closed with `CONFIG_EXISTS`
(exit `2`); it is never merged or overwritten. There is no wizard and no
`config add-wiki` command in 0.1.0 — add wikis by hand-editing the TOML.

### 2.4 Registering wikis

A minimal registry:

```toml
config_version = 1
default_agent  = "claude"

[providers.claude]
executable = "claude"

[providers.codex]
executable = "codex"

[runtime]
timeout_seconds    = 180
max_question_bytes = 65536
max_stdout_bytes   = 1048576
max_stderr_bytes   = 65536

[wikis.my-wiki]
title        = "My Wiki"
project_root = "/path/to/my-wiki"
content_root = "/path/to/my-wiki"
agents       = ["claude", "codex"]
query_prompt = "Use the wiki-query skill to answer from this wiki."

[wikis.my-wiki.claude]
load       = "project_skill"
entrypoint = "/wiki-query"
skill_path = ".claude/skills/wiki-query/SKILL.md"

[wikis.my-wiki.codex]
load       = "project_skill"
entrypoint = "$wiki-query"
skill_path = ".agents/skills/wiki-query/SKILL.md"
```

You can register **any number of wikis**; each is looked up by `id`
(`[wikis.<id>]`, matching `[a-z0-9]+(?:-[a-z0-9]+)*`, unique). `query`
selects exactly one by that ID — callers pick an allowlisted ID, never a
path. See [`config.example.toml`](../config.example.toml) for a full
two-wiki worked example (both wikis this project itself queries: one where
`content_root` equals `project_root`, one where it is nested).

**Per-wiki provider tables.** Every enabled agent needs both a global
`[providers.<agent>]` table (provider-wide: which executable to run) and a
per-wiki `[wikis.<id>.<agent>]` table (wiki-specific: which skill/entrypoint
answers this wiki). An enabled agent missing its global table is
`PROVIDER_CONFIG_MISSING`; missing its per-wiki table is `CONFIG_INVALID`.

`project_root` and `content_root` resolve relative to the config file's own
directory, never relative to the CLI's own install location or the caller's
cwd. `content_root` must be a real directory contained by `project_root`
(equality is allowed — see the `agents` example above).

### 2.5 `query_prompt`

Required, one per wiki. It exists so an operator can state *how* a
particular wiki should be queried (which skill drives it) without
`llm-wikis` needing to know or without modifying any file in the knowledge
base. Constraints:

- required, exactly one line — no newline, carriage return, or other
  control character;
- at most 500 UTF-8 bytes;
- placed verbatim in the prompt after the entrypoint token and before the
  fixed `EXTERNAL_QUERY` envelope, and nowhere else — it cannot alter,
  override, or shadow any envelope field, provider flag, or tool
  restriction.

**Changing `query_prompt` invalidates the wiki's live probe.** The live
doctor probe (§2.9) is keyed in part on the exact `query_prompt` text,
because it changes what the model is told. Edit it, and `query` will refuse
with `ENTRYPOINT_UNVERIFIED` until you rerun `doctor --live` for that
wiki/agent pair.

### 2.6 Provider executable overrides

```toml
[providers.claude]
executable = "claude"

[providers.codex]
executable = "/opt/tools/codex-nightly"
```

`executable` is either one bare command name (resolved through the current
process `PATH`/`PATHEXT`) or one absolute path — never a string containing
arguments, shell syntax, control characters, or relative path components.
Use this to point at a specific install (a pinned version, a wrapper script
location, a non-default install directory) instead of whatever `claude`/
`codex` resolves to on `PATH`.

### 2.7 Project skills and Claude local plugins

Two load modes exist per provider:

**`project_skill`** (both providers) — the entrypoint is a skill that
already lives inside the wiki's own project, addressed by a path relative to
`project_root`:

```toml
[wikis.example.claude]
load       = "project_skill"
entrypoint = "/ask-wiki"
skill_path = ".claude/skills/ask-wiki/SKILL.md"

[wikis.example.codex]
load       = "project_skill"
entrypoint = "$ask-wiki"
skill_path = ".agents/skills/ask-wiki/SKILL.md"
```

**`local_plugin`** (Claude only in 0.1.0) — the entrypoint lives in a
separate local plugin directory, addressed relative to the config file:

```toml
[wikis.example.claude]
load       = "local_plugin"
entrypoint = "/knowledge-tools:ask-wiki"
plugin_dir = "../plugins/knowledge-tools"
skill_path = "skills/ask-wiki/SKILL.md"
```

The entrypoint is **configuration, not a product constant** — nothing about
`/wiki-query`, `/llm-wiki`, or any other name is hard-coded; every registered
wiki names its own skill and its own load mode. A local plugin that declares
hooks, MCP servers, settings, or any other executable lifecycle component is
rejected by static `doctor` — only the configured query skill itself is
allowed.

**Codex installed plugins are out of scope for 0.1.0.** Codex's `$name`
entrypoint form only supports one unnamespaced project-skill name;
`$plugin-name:skill-name` (the installed-plugin form) is parsed, recognized,
and explicitly **rejected** as reserved/deferred — it is not silently
ignored or partially supported. The Codex adapter additionally runs with
`--ignore-user-config`, so even if a Codex installed plugin were configured
at the user level outside `llm-wikis`, it would never be consulted: Codex
user configuration (and therefore any user-level plugin) is deliberately
excluded from every query and doctor invocation.

### 2.8 `llm-wikis` never modifies a knowledge base

State this plainly to anyone operating this tool: **`llm-wikis` itself has
no code path that writes to a registered wiki.** Each wiki's own skill is
used **exactly as that wiki ships it** — no overlay, no patch, no generated
file, no installed skill. If a wiki's interactive skill would normally
offer to save an answer or regenerate an index, the external-readonly
prompt envelope tells it not to, and several independent, mostly-mechanical
layers (§3.4) work against a mutation actually reaching disk — not one
single guarantee, and not, as of this writing, a fully closed one. **§3.10
is the complete, canonical statement of what is prevented, what is only
detected, and what is not covered at all** — read it before treating this
paragraph's summary as the full picture.

### 2.9 Live probes and `ENTRYPOINT_UNVERIFIED`

Every configured entrypoint requires **one current live-doctor probe** for
its exact fingerprint before `query` will use it normally. The fingerprint
covers the current machine, provider executable/version, resolved roots,
entrypoint, `query_prompt`, configuration identity, and a content hash of
the wiki's own skill (or local plugin) directory.

If **any** of those changes — you edit `query_prompt`, the wiki's skill
directory changes (an APM refresh, a Claude local-plugin update, you hand-
edit the skill), the provider is upgraded, or the roots move — the stored
probe no longer matches, and `query` fails closed with
`ENTRYPOINT_UNVERIFIED` (exit `3`) until you rerun:

```powershell
llm-wikis doctor --wiki my-wiki --agent claude --live
```

This is a one-time-per-fingerprint gate, not a cache you can disable: it
exists specifically so a silent upstream change to "what a configured
entrypoint actually does" can never be used by `query` without a fresh,
explicit, quota-consuming confirmation that the contract still holds.

## 3. Query and Security Contract

### 3.1 Commands

```text
llm-wikis --version
llm-wikis [--config <absolute-path>] [--json] config init
llm-wikis [--config <absolute-path>] [--json] list
llm-wikis [--config <absolute-path>] [--json] doctor [--wiki <id>] [--agent claude|codex] [--live]
llm-wikis [--config <absolute-path>] [--json] query --wiki <id> [--agent claude|codex] -- <question>
```

`--version` prints `llm-wikis 0.1.0`. `list` loads the registry and lists
every configured wiki **without starting a provider** — no model call, no
live check. `doctor` without `--wiki`/`--agent` runs static checks (§3.7)
for every configured wiki/provider pair; either selector narrows the
matrix. `doctor --live` requires **both** selectors explicitly, because a
live check consumes model quota and an unqualified `--live` could otherwise
fan out across an entire registry.

`query` accepts exactly one `--wiki`; repeating it, or passing `all`, is
`ARGUMENT_INVALID`. `--agent` is optional only when `default_agent` is set
in the config **and** that agent is enabled for the selected wiki — `list`
reports the per-wiki derived default (`null` when it doesn't apply)
so you can check before omitting `--agent`.

### 3.2 Stdin transport and question input

```powershell
llm-wikis query --wiki my-wiki --agent claude -- "What does the ingest pipeline do?"
echo "What does the ingest pipeline do?" | llm-wikis query --wiki my-wiki --agent claude
```

Supply the question as a positional **after a literal `--`**, or omit it and
pipe the complete question through stdin. Supplying both is rejected rather
than merged (`ARGUMENT_INVALID`); supplying neither is also rejected. The
complete UTF-8 question is size-checked against `max_question_bytes` before
any provider starts (`QUESTION_TOO_LARGE`); invalid UTF-8 on stdin is
`QUESTION_INVALID_UTF8`. The question is passed to the provider as literal
stdin data — never interpolated into a shell command string — so shell
metacharacters, quotes, leading dashes, and multi-line/Unicode text all
transport unchanged.

### 3.3 JSON envelopes and exit classes

`--json` emits **exactly one** JSON document on stdout per invocation
(`schema_version: "1.0"`), success or failure alike; human mode prints the
answer, then gaps, then warnings, to stdout, with diagnostics on stderr.
Every failure uses the same public `error: {code, message, details?}`
object. The process exit code always equals the failure's documented class:

| Exit | Meaning |
|---:|---|
| `0` | Success |
| `2` | Argument/config/citation/path input error |
| `3` | Provider unavailable, unauthenticated, or entrypoint unverified |
| `4` | Child process returned non-zero |
| `5` | Timeout, output-size overflow, or termination failure |
| `6` | Malformed or contract-violating native output |
| `7` | A protected wiki path, type, or content digest changed |
| `70` | Internal error, or an incomplete integrity comparison |

A `doctor` matrix with mixed failures reports the exit class of the
**highest-precedence** class present, in order `70, 7, 6, 5, 4, 3, 2`, else
`0` — a matrix with only passes and warnings is `ok: true` and exit `0`.
`READ_ONLY_VIOLATION` always dominates any other exit-2-through-6 failure
from the same invocation (the displaced failure survives as
`error.details.secondary_error`); `INTERNAL_ERROR` dominates only when the
integrity comparison itself could not complete. Twenty-seven error codes in
total — see specification §14 for the complete table.

### 3.4 Read-only enforcement layers

No single mechanism carries the read-only guarantee; several independent
layers stack:

1. Fixed wiki allowlist — callers select a configured ID, never a path.
2. Canonical path containment and symlink/junction/reparse/mount rejection.
3. No shell command construction anywhere in the process supervisor.
4. Question transport is stdin data, never provider argv.
5. Fixed, non-configurable provider arguments (no `claude_args`/
   `codex_args`/`shell_command`/similar keys exist at all — rejected as
   unknown config keys).
6. Claude runs with `Read,Grep,Glob` only — no Bash, Edit, Write, or web
   tool is exposed.
7. **A wiki whose `.claude/settings.json`/`settings.local.json` declares
   any key other than `enabledPlugins` is refused outright** — `doctor` and
   `query` both fail closed with `ENTRYPOINT_INVALID` before the actual
   query is ever answered. This is **not** "before any provider call" in
   the literal sense: `query`'s bounded, non-billable version and `auth
   status` probes (spec §8.1 steps 7-8) do invoke the provider executable
   first — this check runs afterward, authoritatively, immediately before
   the one call that would actually spawn the provider to answer the
   question (§10.2 R-31). `doctor`'s own static check runs alongside its
   own version/auth probes in the same `doctor` call; it does not prevent
   them. This is the load-bearing layer, not the argv flags below: Claude
   Code's `-p` mode auto-loads these files from the wiki's own
   `project_root` regardless of trust, and several documented keys beyond
   `hooks` execute a command or widen reach on their own — `apiKeyHelper`,
   `awsCredentialExport`, `awsAuthRefresh`, `gcpAuthRefresh`,
   `otelHeadersHelper`, `statusLine` (all run a configured command),
   `permissions.additionalDirectories` (widens read reach beyond
   `project_root`), and `env` (can redirect API traffic). Confirmed live: a
   wiki declaring `apiKeyHelper` executed it even with `--settings
   {"disableAllHooks":true}` (below) present. The check is an **allowlist**,
   not a denylist: **only `enabledPlugins` is admitted**, and its value is
   validated to be an object of plain booleans (nothing else). An earlier
   version of this check also admitted `permissions.{allow,deny,defaultMode}`
   on the reasoning that `--tools Read,Grep,Glob` bounds tool *names* — true,
   but a permission rule's *value* can still pre-authorize a specific path
   for the exposed `Read` tool (e.g. `{"permissions":{"allow":["Read(/some/secret/**)"]}}`),
   which is not a tool-name concern at all; `permissions` is now denied
   entirely rather than partially validated. `enabledPlugins` is admitted
   because it only toggles a plugin the operator already trusted at the
   *user* level (`~/.claude`) — a wiki's project settings cannot use it to
   introduce a new plugin source of its own. The real `harness-engineering`
   wiki has exactly `enabledPlugins` and keeps working. Anything else fails
   closed, including a setting a future Claude Code release adds that this
   project has never heard of. A settings path — **or the `.claude`
   directory itself** — that is a symlink, junction, reparse point,
   directory-where-a-file-was-expected, or other non-regular entry is
   rejected as `UNSAFE_FILESYSTEM_ENTRY` rather than silently treated as
   absent: checking only the two settings filenames is not enough, since a
   `.claude` *directory* that is itself a symlink pointing at an empty (or
   not-yet-existing) external location would pass a plain existence check on
   both filenames the same way a dangling file-level symlink would, letting
   the real settings file be created at the true target afterward — this
   wrapper checks `.claude` itself first, then each settings file.
8. This check runs at both `doctor` time and, at `query` time, as the
   genuinely last thing before the child process is spawned — inside the
   Claude provider adapter itself, immediately before the one call that
   starts the real `claude` process, after every other query step
   (executable/version/auth probes, the live-probe fingerprint gate, prompt
   construction, the before-snapshot, and this request's own temp-file/argv
   setup) has already run — so a wiki's settings changing between a `doctor`
   pass and a later `query` cannot slip past a stale result. **This narrows
   the window between "checked safe" and "the provider reads it" to the
   remaining syscall gap; it does not close it**: a write to the wiki's
   `.claude/` directory landing in the instant between this check returning
   and the operating system actually starting the child process still wins
   that race. Eliminating the window entirely would need OS-level isolation
   this wrapper does not provide.
9. On top of layers 7-8, `--settings {"disableAllHooks":true}` neutralizes
   every hook declared by user, project, local, or plugin-dir settings for
   the session (CLI-supplied settings outrank all of those) — **not** an
   admin-managed/enterprise-policy hook, spelled out in the residual gap
   below — `--strict-mcp-config`'s empty MCP configuration (layer 12 below)
   locks out any MCP server the settings might declare, and the `--tools
   Read,Grep,Glob` restriction (layer 6) bounds the built-in tool surface
   regardless of any tool-related setting. These exist because the wiki's
   own settings are still loaded (required for skill discovery — excluding
   them via `--setting-sources user` was tried and found to break every
   `project_skill`-mode wiki's entrypoint). **Honest residual gap**: no CLI
   flag can disable an admin-managed/enterprise-policy hook regardless of
   any of the above — that is out of this project's scope, and the
   implementation machine has no managed settings.
10. Codex runs under `--sandbox read-only`; the sandbox is a write-prevention
    guarantee only, not a read-scope limiter (§3.6).
11. No session persistence on either provider.
12. An explicitly empty MCP configuration (Claude `--mcp-config`, Codex
    `-c mcp_servers={}` plus `--disable browser_use --disable
    computer_use`, since `--ignore-user-config` alone does not exclude
    Codex's bundled `node_repl`-backed MCP surface).
13. No query-time index generation, regeneration, or repair of any kind.
14. A provider output schema (`--json-schema`/`--output-schema`)
    mechanically constrains the result shape.
15. Timeout and independent stdout/stderr byte caps.
16. A full-content, before/after SHA-256 snapshot of the protected tree
    (§3.5) — a detection layer, not a substitute for the layers above.
17. No `llm-wikis` command writes to a knowledge base at all. This covers
    every write path `llm-wikis` itself has — it does not, and cannot,
    cover a provider's own lifecycle-hook mechanism running arbitrary code
    the wiki declares; layers 7-9 are what bound (not eliminate — see layer
    8's stated TOCTOU residual) that for Claude, with the managed-policy
    exception noted in layer 9. Codex's project-level configuration
    (including hooks and exec policies under a project `.codex/` directory)
    is loaded only for a **trusted** project, and this wrapper's invocation
    never establishes trust — Codex fails closed on this by its own design,
    so no equivalent `llm-wikis`-side gate exists for it (verified by
    design/documentation, not by a live exploit attempt: neither registered
    wiki's `.codex/`/`.agents/` directory presently contains any hooks,
    config, or execpolicy files to test against).

### 3.5 Mutation detection

Before invoking the provider, `llm-wikis` walks the complete canonical
`content_root` (every directory and regular file — schema, config,
executable helpers/hooks, raw/assets, index/log/overview files, every page,
any compiled cache/graph artifact — **except** any `.claude/`/`.agents/`
directory immediately beneath `content_root`) and records each entry's
type, and for regular files, byte length and a streaming SHA-256 digest. It
repeats the walk after the provider exits and compares. Any addition,
removal, type change, or same-size content rewrite (even with a preserved
timestamp) is `READ_ONLY_VIOLATION` (exit `7`); the sorted list of changed
relative paths is reported — never file content. This runs on every outcome
(success, provider failure, timeout, output overflow), so a mutation is
caught regardless of what else happened.

**What actually covers the excluded `.claude`/`.agents` directories, stated
precisely rather than as a blanket claim**: the live-probe skill fingerprint
(§2.9) hashes only the *configured skill's own directory* — for
`project_skill`, the skill file's parent directory (e.g.
`.claude/skills/wiki-query/`), not the whole `.claude/` tree; for
`local_plugin`, the whole configured `plugin_dir`, which is typically a
separate directory outside `content_root` entirely, referenced by the
`plugin_dir` config value, not `.claude/` itself. Separately, for Claude
wikis specifically, `project_root/.claude/settings.json` and
`settings.local.json` are covered by the settings-surface deny check (§3.4
layer 7) — an allowlist gate, not a hash-based change-detector. **Neither
mechanism covers every other file that could exist under `.claude/`/`.agents/`**
(other skills, slash commands, cached plugin data, anything not the one
configured skill or the two named settings files) — those remain outside
both the mutation snapshot and the fingerprint, a genuine gap, not one this
project currently closes. §3.10 places this alongside every other
prevented/detected/not-covered claim in one canonical list.

### 3.6 Read-scope warnings and the OS-sandbox recommendation

Both providers can technically *read* further than `content_root`. Write
safety is a separate, narrower claim than "neither can write anywhere" —
§3.10 states precisely what write-prevention does and does not cover;
in short, the mechanical-prevention and detection layers there bound
writes through the mechanisms this project controls, with named,
currently-accepted gaps (an admin-managed hook, a check-to-spawn race),
not an unconditional guarantee.

- **`CLAUDE_READ_SCOPE_BROAD`** — emitted whenever `content_root` is a
  strict subdirectory of `project_root` (Claude's working directory is
  `project_root`, so its read tools can reach beyond the selected content
  root). Not emitted when the two roots are equal: as far as this
  project's own configuration surface goes, read reach is exactly
  `content_root` in that case. This warning's absence certifies that
  narrower claim, not an absolute one — the operator's own *user-scope*
  Claude settings (never inspected by this project; §3.10) could still
  pre-authorize the `Read` tool for a path outside `content_root` even
  when the roots are equal. That is the operator's own trusted
  configuration, not a wiki- or caller-supplied vector.
- **`CODEX_READ_SCOPE_BROAD`** — emitted on every Codex check/query,
  unconditionally, because Codex's read-only sandbox prevents writes but
  does not itself limit *reads* to the selected wiki.

Both warning messages end with the same recommendation: **use an OS
sandbox or container exposing only the selected wiki** if you need a
strict read allowlist for a sensitive knowledge base. Because the Codex
adapter runs with `--ignore-user-config`, an operator-managed Codex
permission profile is not a supported hardening path in 0.1.0 — OS-level
sandboxing is the only strict option today.

### 3.7 Doctor and the live-probe cost

Static `doctor` checks (`checks[].name` is one of `config`, `roots`,
`wiki_structure`, `entrypoint`, `executable`, `auth`, `read_scope`,
`live_contract`, `mutation`) run entirely offline: config validity, unique
IDs, canonical contained roots, content-root minimum structure,
`query_prompt` constraints, entrypoint syntax, statically-addressable
skill/plugin artifacts, local-plugin lifecycle-component rejection,
provider executable resolution, and provider **authentication status**
(read through each provider's own non-billable status command — `claude
auth status --json` / `codex login status` — never a paid model request).

**`doctor --live` does consume model quota.** It is never run implicitly by
`query`; you must ask for it explicitly, with both `--wiki` and `--agent`
set. Each live check sends one real, minimal question to the real
provider, validates the result against `wiki-query/v1`, confirms an
identical before/after content-root snapshot, and — only on success —
publishes exactly one probe record for that wiki/agent's current
fingerprint to the local, machine-only probe cache (§2.1). Budget for one
paid call per wiki/agent pair you verify this way.

### 3.8 Troubleshooting by error class

| You see | Likely cause | What to do |
|---|---|---|
| `CONFIG_INVALID` / `CONFIG_EXISTS` | Registry TOML is malformed, has an unknown key, or `config init`'s destination already exists | Fix the TOML; use a different `--config` path or hand-edit the existing file — `config init` never overwrites |
| `WIKI_NOT_ALLOWED` | The `--wiki` ID isn't in the registry | Check `llm-wikis list`; add the wiki to the config |
| `PATH_OUTSIDE_ALLOWED_ROOT` / `UNSAFE_FILESYSTEM_ENTRY` | A configured path resolves outside its declared root, or a symlink/junction/reparse/mount was found in a scanned tree — including, for a Claude wiki, at `.claude/settings.json`/`settings.local.json` itself (§3.4 layer 7) | Fix the offending path in config; do not symlink inside a monitored tree, the configured skill directory, or (Claude) the wiki's settings files |
| `WIKI_INVALID` | `content_root` is missing, not a directory, or has zero `.md` files anywhere beneath it | Point `content_root` at a real directory that actually contains the wiki's Markdown pages |
| `WIKI_SCHEMA_ABSENT` (warning, not a failure) | No `SCHEMA.md` at `content_root` | **Usually means `content_root` points one level too high** (or, less often, one level too low) — most wiki toolchains put `SCHEMA.md` at the wiki root, so this is an expensive-not-fatal hint to re-check the exact directory, not something you need to fix to proceed |
| `CLAUDE_ENABLED_PLUGINS_DECLARED` (warning, not a failure) | Claude only — the wiki's settings declare `enabledPlugins` (§3.4 layer 7) | Informational: `enabledPlugins` is admitted, but the message states this is an evidence-backed risk acceptance, not a documented CLI guarantee — nothing to fix unless you want the wiki to stop declaring it |
| `PROVIDER_CONFIG_MISSING` / `AGENT_UNSUPPORTED` | Wiki enables an agent with no matching `[providers.<agent>]` table, or a wiki you queried never enabled that agent at all | Add the global provider table, or use an agent the wiki actually enables |
| `ENTRYPOINT_INVALID` | Entrypoint syntax is wrong, the statically-addressable skill/plugin file doesn't exist where configured, or — Claude only — the wiki's own `.claude/settings.json`/`settings.local.json` declares a key outside the small admitted allowlist (§3.4 layer 7) | Check `/name`, `/plugin:skill`, or `$name` syntax; check `skill_path`/`plugin_dir` resolve to a real file; for the settings case, the message names the rejected key — remove it from the wiki's settings (only `enabledPlugins` is admitted; `permissions` and everything else is rejected outright, regardless of spelling) |
| `CLI_NOT_FOUND` | The configured provider executable isn't resolvable | Check `providers.<agent>.executable`, install/PATH the provider CLI |
| `AUTH_REQUIRED` | The provider's own status command reports logged out | Log in to that provider CLI directly (`claude`/`codex` login flow) — `llm-wikis` never handles credentials itself |
| `ENTRYPOINT_UNVERIFIED` | No current live-doctor probe for this exact fingerprint | Run `doctor --wiki <id> --agent <agent> --live` (§2.9) |
| `NONZERO_EXIT` / `TIMEOUT` / `OUTPUT_TOO_LARGE` / `TERMINATION_FAILED` | The provider process failed, ran too long, streamed more than the configured byte cap, or couldn't be confirmed terminated | Check `runtime.timeout_seconds`/byte-cap settings; check provider health directly |
| `INVALID_NATIVE_OUTPUT` / `NO_FINAL_MESSAGE` / `CONTRACT_VIOLATION` | The provider's raw output didn't parse, produced no final answer, or didn't satisfy `wiki-query/v1` | Usually transient/provider-side; rerun. A wiki whose skill can't ground an answer under the read-only constraints will also surface here |
| `CITATION_NOT_FOUND` / `CITATION_AMBIGUOUS` / `CITATION_INVALID` | A citation the model explicitly asserted doesn't resolve to exactly one page | `CITATION_AMBIGUOUS` in particular is a strong signal `content_root` is set to a parent directory containing duplicate/vendored pages — check the root, not the wiki's own content |
| `READ_ONLY_VIOLATION` | The before/after content snapshot detected a change | Stop and investigate immediately — this should never happen against an unmodified wiki; the changed relative paths (not content) are listed in `error.details.changed_paths` |
| `INTERNAL_ERROR` | Unexpected wrapper-side failure, or the integrity comparison itself couldn't complete | File an issue with the sanitized error message; never assume read-only held if this fires mid-integrity-check |

### 3.9 Three things operators get wrong

1. **The wrapper never reads or validates a wiki's index.** `llm-wikis`
   performs no index discovery, parsing, or freshness checking of any
   kind. A wiki with a malformed, duplicated, or entirely absent index
   still works — do not "fix" a wiki's index file to satisfy a rule that
   does not exist in this tool.
2. **A generated index should still be regenerated before querying — but
   that is advice, not a gate.** A stale or missing index can degrade the
   *skill's own* navigation of the wiki (it may miss recently added
   pages), so keeping it fresh is good practice for answer quality. Nothing
   in `llm-wikis` enforces, checks, or blocks on this.
3. **`WIKI_SCHEMA_ABSENT` usually means `content_root` is one level too
   high** (pointing at the wiki's parent project directory rather than the
   wiki root itself where `SCHEMA.md` actually lives) — occasionally one
   level too low. It's a warning because minimum structure alone can't
   distinguish a correct root from its parent, and it's worth checking
   because scanning and snapshotting a parent directory is needlessly
   expensive, not because anything is actually broken.

### 3.10 The complete safety picture (canonical reference)

Seven consecutive PR reviews found a safety, detection, or scope claim
stated slightly more strongly than the code actually delivers — always a
different sentence, scattered across this guide, the specification, and a
few source doc comments. Restating each corrected sentence one at a time
kept reproducing the same failure mode elsewhere. This section exists to
stop that: it is the **one, canonical, complete statement** of what is
prevented, what is only detected, and what is not covered at all. Every
other safety sentence in this guide and in the specification either stands
on its own without needing this section's nuance, or is shorthand for it —
if a shorter claim elsewhere and this section ever seem to disagree, this
section is correct and the shorter one is under-qualified.

**(a) Mechanically prevented** — true regardless of what the model
attempts, because the mechanism does not depend on the model's cooperation:

| Prevented | Mechanism |
|---|---|
| Claude writing, editing, or executing anything via a tool call | `--tools Read,Grep,Glob` (layer 6, §3.4) — no Write, Edit, Bash, or web tool exists in the argv at all; there is no tool such a call could name |
| Codex writing anything, through any tool it does have (including its shell/exec tools) | `--sandbox read-only` (layer 10, §3.4) — an OS-level write-prevention guarantee, independent of what Codex's own tools attempt |
| A wiki's own `.claude/settings.json`/`settings.local.json` (or a symlinked `.claude` directory) smuggling in an executable or reach-widening key | `check_claude_wiki_settings_surface`'s allowlist (layer 7, §3.4) — only `enabledPlugins` is admitted; everything else fails the wiki closed as `ENTRYPOINT_INVALID`/`UNSAFE_FILESYSTEM_ENTRY` before the provider ever spawns |
| A hook declared by user, project, local, or plugin-dir settings running during the session | `--settings {"disableAllHooks":true}` (layer 9, §3.4) — CLI-supplied settings outrank every other source |
| An MCP server a wiki's settings might declare being reachable | Empty MCP configuration plus the two `--disable` flags (layer 12, §3.4) |
| The operator's own user-level Codex configuration, including any user-level plugin or permission profile, being consulted at all | `--ignore-user-config` (§3.4, Codex adapter) |
| Codex loading a wiki's project-level trust surface (`.codex/config.toml`, project hooks, execpolicy) | Codex's own trust-gating design (this wrapper's invocation never establishes trust) — verified by documentation, not a check this project authored (layer 17, §3.4) |
| A configured path, the skill/plugin artifact tree, or (Claude) the `.claude` directory/settings files resolving through a symlink, junction, reparse point, or mount | Canonical containment plus special-entry rejection, `UNSAFE_FILESYSTEM_ENTRY` (layers 2 and 7, §3.4) |
| The result shape being anything other than `wiki-query/v1` | The provider's own output schema, mechanically constrained (layer 14, §3.4) |
| A shell metacharacter, quote, or the question itself being interpreted as a command | Stdin-only question transport; no shell command construction anywhere (layers 3-4, §3.4) |

**(b) Instructed, and backstopped by detection if the instruction fails** —
the harness cannot stop a model from *attempting* these; it can stop the
attempt from *succeeding* (via the prevention layers in (a), which is why
these two lists overlap) and can *detect* one that lands anyway:

- The external-readonly prompt's instruction to never regenerate the
  index, never write pages/logs/reports/caches, and never attempt mutation
  through any tool (spec §7.2 items 6-8) is primarily backed by (a)'s
  prevention layers — under normal operation there is no tool to attempt
  them with. If a gap in (a) not listed there (see (c) below) ever let a
  write land anyway, the before/after content snapshot (§3.5) detects it —
  **with a stated coverage limit**: it covers every directory and regular
  file beneath `content_root` **except** the `.claude`/`.agents`
  directories when they are immediate children of `content_root` itself. A
  mutation confined entirely to one of those two excluded trees is not
  caught by this layer.

**(c) Not covered** — real, currently-accepted gaps, stated directly
rather than left to be inferred from what other sections don't mention:

- **Admin-managed/enterprise-policy Claude hooks.** No CLI flag or static
  check in this project can disable or detect one. Verified absent on this
  project's own implementation machine; that is not a guarantee it is
  absent on every machine this tool runs on.
- **The check-to-spawn TOCTOU window.** The settings-surface check
  (layer 7/8, §3.4) runs at the latest practical point — immediately
  before the one call that spawns the provider — but a write to the
  wiki's `.claude/` directory landing in the syscall gap between that
  check returning and the operating system actually starting the process
  still wins the race. Narrowed to that gap, not closed; closing it fully
  would need OS-level isolation (a held filesystem snapshot, or a
  mandatory-access-control policy) outside this project's scope.
- **The operator's own user-scope Claude settings.**
  `check_claude_wiki_settings_surface` inspects only the *wiki's own*
  `project_root/.claude/settings.json`/`settings.local.json` — never the
  operator's own `~/.claude/settings.json`, and it could not without
  reading outside the wiki entirely. A `permissions.allow` rule the
  operator has configured there for themselves could pre-authorize
  Claude's `Read` tool for a path outside `content_root`/`project_root`,
  even when `content_root` equals `project_root`. This is the operator's
  own trusted configuration (Design Decision 12), not something a wiki or
  a caller controls — an accuracy point about what the *absence* of
  `CLAUDE_READ_SCOPE_BROAD` actually certifies (§3.6), not a
  vulnerability.
- **`enabledPlugins`, kept admitted as an evidence-backed risk
  acceptance, not a proven-safe mechanism** (spec §23 R-32). Project-scope
  plugin activation was empirically observed to have no effect in headless
  mode on the tested Claude Code version — an observation on one version,
  not a documented contract. This is surfaced to the operator via the
  `CLAUDE_ENABLED_PLUGINS_DECLARED` warning; it is not prevented.
- **Read reach beyond `content_root`** (§3.6) — a confidentiality gap,
  not a write-safety one. Claude's read tools can reach the whole
  `project_root` when `content_root` is a strict subdirectory of it
  (`CLAUDE_READ_SCOPE_BROAD`); Codex's read-only sandbox never limits
  reads to the selected wiki at all (`CODEX_READ_SCOPE_BROAD`,
  unconditional, every check/query). The recommended mitigation for a
  sensitive knowledge base is an OS-level sandbox or container exposing
  only the selected wiki, not anything this tool itself provides.
- **A model's own false claims in its answer text.** Nothing here fact-
  checks whether the model's prose accurately describes what it did —
  only citations are structurally validated (§11 of the specification). A
  model claiming, in its answer, that it saved or updated something is not
  caught by any mechanism above. Spec §7.2 item 5 ("omit answer-saving
  offers") is the prompt's only defense against a model even *offering* to
  save, and — unlike items 6-8 — it has **no mechanical backstop at all**:
  offering to save is text in the answer, not a tool call, so there is
  nothing to prevent or detect.
