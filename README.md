# llm-wikis

A small Rust CLI that lets an AI coding agent (Claude Code or Codex CLI) answer
a question from a pre-built, curated knowledge base — a "wiki" — without the
caller needing to `cd` into that wiki's own project, copy files, or hand-craft
a prompt. You register a wiki once in a config file; after that, `llm-wikis
query` can be run from anywhere.

The wrapper never modifies the knowledge base it queries: every invocation is
read-only, and a full-content snapshot check rejects any run that would have
added, removed, resized, or otherwise mutated a wiki page. See
[`docs/llm-wikis.md`](docs/llm-wikis.md) for the full operator guide
(configuration reference, error codes, troubleshooting, and platform notes).

## Supported platforms

| Asset | Platform |
|---|---|
| `llm-wikis-windows-amd64.exe` | Windows x64 |
| `llm-wikis-linux-amd64` | Linux/WSL x86-64 |
| `llm-wikis-darwin-arm64` | macOS (Apple Silicon only) |

The macOS binary is an unsigned preview — see `docs/llm-wikis.md` for the
Gatekeeper approval flow. The Windows binary is unsigned too, so SmartScreen
may warn on first run.

**Verification status:** all three platforms above have verified native
binary + installer coverage (CI builds, smoke-tests, and installs on each).
Live end-to-end provider queries (`llm-wikis query` actually talking to
Claude Code or Codex CLI and returning a grounded answer) have been verified
on Windows only, for both providers. The equivalent Linux/WSL and macOS live
query paths are unverified, not known-broken — they simply haven't been run
on those platforms yet.

## Install

**Linux / macOS:**

```sh
curl -fsSL https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/gn00678465/llm-wikis/refs/heads/main/install.ps1 | iex
```

Both scripts download the matching release asset, verify it against the
published `SHA256SUMS`, smoke-test it with `--version`, and install it onto
your `PATH` (`~/.local/bin` on Linux/macOS, `%LOCALAPPDATA%\llm-wikis\bin` on
Windows). Re-running either installer upgrades, downgrades, or repairs the
binary in place. Set `LLM_WIKIS_VERSION` before running to pin an exact
released tag instead of installing the latest one — required while only a
pre-release exists, since an unpinned run only ever resolves to the newest
non-prerelease tag and fails with an explicit message if there isn't one yet.

## Minimal configuration

`llm-wikis config init` writes a platform-native template config, never
merging into an existing one. Run interactively (a real terminal, no
`--yes`), it runs a short setup wizard for `default_agent` and the two
provider executables; piped/scripted (or `--json`, or `--yes`) it writes
the fixed template directly. An existing destination needs `--force` to
overwrite (a TTY without `--force` asks first; a script without `--force`
fails closed with `CONFIG_EXISTS`). A minimal registry looks like:

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

See [`config.example.toml`](config.example.toml) for a fuller worked example
and [`docs/llm-wikis.md`](docs/llm-wikis.md) for every field's meaning.

Each `[providers.*]` table also takes an optional `model` and `effort`:

```toml
[providers.claude]
executable = "claude"
model      = "opus"
effort     = "high"

[providers.codex]
executable = "codex"
model      = "gpt-5.6-sol"
effort     = "high"
```

Both are optional and, when unset, nothing extra reaches the provider — it
keeps choosing for itself. They apply to `query` and `doctor --live` only,
never to the version or authentication probes. Whether a given model accepts
a given effort is the provider's own question; an unsupported combination
comes back as an ordinary provider failure.

## Usage

```sh
llm-wikis list
llm-wikis doctor --wiki my-wiki --agent claude
llm-wikis query --wiki my-wiki --agent claude -- "What does the ingest pipeline do?"
llm-wikis query --wiki my-wiki --agent claude --plain -- "What does the ingest pipeline do?"
```

On a real terminal, `query`'s human-mode answer renders markdown with
terminal styling through [leaf](https://github.com/RivoLink/leaf), an
optional external viewer. `--plain` forces raw markdown instead (also forced
automatically when stdout is piped/redirected or `NO_COLOR` is set).

leaf is not bundled and is never installed for you. Without it, everything
still works: the full answer prints as raw markdown and a one-line warning
goes to stderr. Install it from its own instructions (leaf 1.21.0 or newer,
which is when `--inline` arrived), or turn the viewer off entirely:

```toml
[viewer]
backend = "plain"
```

`llm-wikis doctor` reports viewer readiness as its `viewer` check. A missing
viewer is a warning, never a failure — doctor's exit code stays 0, because a
missing renderer costs you formatting, not answers.

Full command reference, JSON output shape, and error codes:
[`docs/llm-wikis.md`](docs/llm-wikis.md).

## AI agent skill

`skills/llm-wikis-usage/` teaches an AI coding agent how to drive this CLI
(commands, `--json` envelope shape, exit codes, and error troubleshooting).
Install it by copying the directory into the agent's own skills location:

```sh
# Claude Code (personal scope)
cp -r skills/llm-wikis-usage ~/.claude/skills/

# Codex CLI
cp -r skills/llm-wikis-usage .agents/skills/
```

Or install directly from this repository with
[`skills`](https://www.npmjs.com/package/skills):

```sh
npx skills add https://github.com/gn00678465/llm-wikis --skill llm-wikis-usage
```

## Development

### Build from source

Requires [rustup](https://rustup.rs/); `rust-toolchain.toml` pins the
toolchain (MSRV 1.97, edition 2024) and the first `cargo` invocation
installs it automatically.

```sh
git clone https://github.com/gn00678465/llm-wikis.git
cd llm-wikis
cargo build --release
# binary at target/release/llm-wikis(.exe)
```

### Automated checks (same three gates as CI)

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

Notes:

- Tests must run with `--test-threads=1` (CI does too).
- The `tests/process_supervisor.rs` binary takes ~12-16 minutes and
  contains two 400ms deadline-race tests
  (`grandchild_termination_kills_both_pids`, `windows_job_object`) that
  fail intermittently on high-latency/sandboxed machines. That is a timing
  race, not a regression — treat a failure there as real only if
  `src/process.rs` or that test file changed.

### Live verification (real providers)

Requires an installed, logged-in Claude Code and/or Codex CLI.

```sh
llm-wikis config init                                  # write the template, then register your wiki
llm-wikis config validate                              # check the registry
llm-wikis config list                                  # inspect the resolved config
llm-wikis doctor --wiki <id> --agent claude            # static checks
llm-wikis doctor --wiki <id> --agent claude --live     # real provider probe; records the probe query needs
llm-wikis query  --wiki <id> --agent claude -- "your question"
llm-wikis --json query --wiki <id> --agent claude -- "your question"
```

A successful run keeps the answer (or the single `--json` document) on
stdout only; progress and human-readable errors go to stderr, and a JSON
answer is `ok:true` with `knowledge_status:"grounded"` and citations that
resolve to wiki pages.

Three setup pitfalls worth knowing up front:

1. A wiki must not live under the system temp directory — the temp-root
   disjointness guard rejects the run (exit 70) by design.
2. A Claude wiki skill's `SKILL.md` must declare
   `allowed-tools: Read, Grep, Glob`, or every read is denied during the
   skill turn and answers degrade to "unable to access wiki pages" — see
   `docs/llm-wikis.md` §2.7a.
3. When scripting `query` with a positional question, redirect stdin
   (`< /dev/null`) if the caller's stdin is a pipe that never closes —
   the dual-input check reads stdin to EOF before any provider work.

## License

MIT — see [`LICENSE`](LICENSE).
