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
released tag instead of installing the latest one.

## Minimal configuration

`llm-wikis config init` writes a platform-native template config (never
overwriting an existing one). A minimal registry looks like:

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

## Usage

```sh
llm-wikis list
llm-wikis doctor --wiki my-wiki --agent claude
llm-wikis query --wiki my-wiki --agent claude -- "What does the ingest pipeline do?"
```

Full command reference, JSON output shape, and error codes:
[`docs/llm-wikis.md`](docs/llm-wikis.md).

## License

MIT — see [`LICENSE`](LICENSE).
