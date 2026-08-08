---
name: llm-wikis-usage
description: Use when the user wants to query a registered llm-wikis knowledge base, run llm-wikis list/doctor/config, interpret its --json envelope or exit code, or troubleshoot an llm-wikis error such as ENTRYPOINT_UNVERIFIED, CONFIG_EXISTS, or WIKI_NOT_ALLOWED.
---

# llm-wikis usage

`llm-wikis` is a read-only Rust CLI that answers a question against a
pre-built knowledge base ("wiki") through Claude Code or Codex CLI, from
anywhere on disk. It never writes to a knowledge base. This file is a
compact reference for driving the CLI correctly; see
`references/errors.md` for the full error-code troubleshooting table.

## Commands

```text
llm-wikis --version
llm-wikis [--config <absolute-path>] [--json] config init [--yes] [--force]
llm-wikis [--config <absolute-path>] [--json] config list
llm-wikis [--config <absolute-path>] [--json] config validate
llm-wikis [--config <absolute-path>] [--json] list
llm-wikis [--config <absolute-path>] [--json] doctor [--wiki <id>] [--agent claude|codex] [--live]
llm-wikis [--config <absolute-path>] [--json] query --wiki <id> [--agent claude|codex] [--plain] -- <question>
```

`--config <absolute-path>` overrides the platform-native default config
location; it must be absolute. `--json` makes every subcommand emit exactly
one JSON document on stdout instead of human-readable text — prefer
`--json` whenever you plan to parse the result programmatically.

### `config init`

Creates the parent directory and a starter config only when the
destination does not already exist (never merges).

- **When driving this CLI programmatically** (piped stdin/stdout, or
  `--json`), it always writes the fixed starter template directly — no
  prompts. This is the path you will take.
- On a real interactive terminal (both stdin and stdout are a TTY) without
  `--yes`, it instead runs an interactive setup wizard (asks for
  `default_agent` and each provider's `executable`, Enter accepts the shown
  default). `--yes` skips the wizard and writes the template even on a TTY.
- If the destination already exists: `--force` always overwrites without
  asking; without `--force`, a non-TTY/`--json`/agent-driven invocation
  fails closed with `CONFIG_EXISTS` (exit 2), and a TTY invocation instead
  asks to confirm (default: do not overwrite).
- `config init` never registers a wiki for you — add `[wikis.<id>]` tables
  to the generated TOML by hand (see the commented example it writes).

### `config list` / `config validate`

`config list` prints the fully resolved configuration document (after its
own defaulting). `config validate` performs the identical load-and-validate
step but never prints or writes anything — useful as a pure pass/fail
check. Neither starts a provider.

### `list`

Enumerates every registered wiki id, title, enabled agents, and each wiki's
derived `default_agent` (null when none applies) — never starts a
provider. Run this before `query` to confirm a wiki id exists and whether
`--agent` can be omitted for it.

### `doctor`

Static checks (offline) run by default for every configured wiki/agent
pair, or narrowed with `--wiki`/`--agent`. `doctor --live` additionally
spends real model quota on one minimal live query and **requires both**
`--wiki` and `--agent` explicitly — never run `--live` without narrowing
to exactly one pair. A `query` against an entrypoint that has never passed
a current live-doctor probe fails closed with `ENTRYPOINT_UNVERIFIED`
(exit 3); run `doctor --wiki <id> --agent <agent> --live` first.

### `query`

Requires exactly one `--wiki` (repeating it, or passing `all`, is an
argument error). `--agent` is optional only when the wiki has a usable
`default_agent` (check with `list`). Supply the question as a positional
after a literal `--`, or omit it and pipe the complete question through
stdin — supplying both, or neither, is rejected.

```sh
llm-wikis query --wiki my-wiki --agent claude -- "What does the ingest pipeline do?"
echo "What does the ingest pipeline do?" | llm-wikis query --wiki my-wiki --agent claude
```

Human-mode output on a real terminal renders the answer's markdown with
terminal styling; `--plain` forces raw markdown instead (also forced
automatically whenever stdout is piped/redirected, or `NO_COLOR` is set —
an agent invoking this CLI through a pipe already gets raw markdown without
needing `--plain` at all).

## `--json` envelope and exit codes

`--json` always emits exactly one JSON document on stdout
(`schema_version: "1.0"`), success or failure alike, with a public
`ok`/`operation` pair and either the operation's own success fields or an
`error: {code, message, details?}` object. Every human-mode diagnostic —
including the error line itself — goes to stderr instead, so stdout stays
clean for a success value or is empty on failure.

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

A `doctor` matrix with mixed check failures reports the exit class of the
highest-precedence class present (`70, 7, 6, 5, 4, 3, 2`, else `0`); a
matrix with only passes and warnings is `ok: true` and exit `0`.

## Where to look when something fails

Read `references/errors.md` for the complete error-code table (cause and
what to check next, per code). In short: exit `2` is an input/config
problem (fix the invocation or the TOML); exit `3` is
provider-availability/auth/unverified-entrypoint (check the provider CLI or
rerun `doctor --live`); exits `4`-`6` are provider process/output problems
(usually transient — rerun, or check provider health); exit `7` means the
before/after content snapshot detected a change — stop and investigate
immediately, this should never happen against an unmodified wiki; exit
`70` is an internal/wrapper-side failure.
