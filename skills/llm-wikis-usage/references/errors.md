# llm-wikis error reference

Every failure carries a public `error: {code, message, details?}` object
(`--json` mode) or an `error: CODE (message)` line on stderr (human mode).
The process exit code always equals the code's documented class (see
`SKILL.md`'s exit-code table). This file maps each code to its likely cause
and what to check next, for troubleshooting a single failing invocation.

| Code | Exit | Likely cause | What to check next |
|---|---:|---|---|
| `ARGUMENT_INVALID` | 2 | Invalid or ambiguous CLI input — missing `--wiki`, repeated `--wiki`, `--wiki all`, missing `--` before the question, both a positional question and stdin supplied, or `doctor --live` missing `--wiki`/`--agent` | Re-check the invocation shape against `SKILL.md`'s command grammar; the error message usually names the exact rule violated |
| `QUESTION_INVALID_UTF8` | 2 | The stdin question is not valid UTF-8 | Ensure the piped input is UTF-8 text |
| `QUESTION_TOO_LARGE` | 2 | The complete UTF-8 question exceeds `runtime.max_question_bytes` | Shorten the question, or raise `max_question_bytes` in the config |
| `CONFIG_INVALID` | 2 | Registry TOML is malformed, has an unknown key, or fails semantic validation | Fix the TOML; `config validate` gives a pass/fail check without side effects |
| `CONFIG_EXISTS` | 2 | `config init`'s destination already exists | Use `--force` to overwrite, or edit the existing file by hand — `config init` never merges |
| `WIKI_NOT_ALLOWED` | 2 | The `--wiki` id isn't in the registry | Run `llm-wikis list` to see registered ids; add the wiki to the config if missing |
| `PATH_OUTSIDE_ALLOWED_ROOT` | 2 | A configured path resolves outside its declared root | Fix `project_root`/`content_root` in the config |
| `UNSAFE_FILESYSTEM_ENTRY` | 2 | A symlink/junction/reparse/mount was found in a scanned tree — including, for a Claude wiki, at `.claude/settings.json`/`settings.local.json`/the `.claude` directory itself | Do not symlink inside a monitored tree, the configured skill directory, or (Claude) the wiki's settings files/directory |
| `WIKI_INVALID` | 2 | `content_root` is missing, not a directory, or has zero `.md` files anywhere beneath it | Point `content_root` at a real directory that actually contains the wiki's Markdown pages |
| `PROVIDER_CONFIG_MISSING` | 2 | A wiki enables an agent with no matching global `[providers.<agent>]` table | Add the missing `[providers.<agent>]` table to the config |
| `AGENT_UNSUPPORTED` | 2 | The wiki queried never enabled the requested agent | Use an agent the wiki actually enables (check with `list`) |
| `ENTRYPOINT_INVALID` | 2 | Entrypoint syntax is wrong, the statically-addressable skill/plugin file doesn't exist where configured, or (Claude only) the wiki's own settings declare a key outside the small admitted allowlist | Check `/name`, `/plugin:skill`, or `$name` syntax; check `skill_path`/`plugin_dir` resolve to a real file; for the settings case the message names the rejected key — only `enabledPlugins` is admitted |
| `CITATION_INVALID` | 2 | A citation the model explicitly asserted has unsafe syntax | Provider-side issue; rerun the query |
| `CITATION_NOT_FOUND` | 2 | An explicit-array citation doesn't map to an existing page | Usually provider-side; if persistent, check `content_root` actually contains the cited page |
| `CITATION_AMBIGUOUS` | 2 | An explicit-array citation matches two or more pages | Strong signal `content_root` is set to a parent directory containing duplicate/vendored pages — check the root, not the wiki's own content |
| `CLI_NOT_FOUND` | 3 | The configured provider executable isn't resolvable | Check `providers.<agent>.executable`; install or add the provider CLI to PATH |
| `AUTH_REQUIRED` | 3 | The provider's own status command reports logged out | Log in to that provider CLI directly (`claude`/`codex` login flow) — `llm-wikis` never handles credentials itself |
| `ENTRYPOINT_UNVERIFIED` | 3 | No current live-doctor probe exists for this exact fingerprint (roots, entrypoint, `query_prompt`, provider version, skill content all included) | Run `llm-wikis doctor --wiki <id> --agent <agent> --live` |
| `NONZERO_EXIT` | 4 | The provider process returned a non-zero exit code | Check provider health directly; rerun |
| `TIMEOUT` | 5 | The provider process exceeded `runtime.timeout_seconds` | Raise the timeout if the query is legitimately slow, or investigate why the provider hung |
| `OUTPUT_TOO_LARGE` | 5 | The provider streamed more than `runtime.max_stdout_bytes`/`max_stderr_bytes` | Raise the byte cap, or narrow the question |
| `TERMINATION_FAILED` | 5 | The process tree could not be confirmed terminated/reaped | Environment issue; investigate the host's process management |
| `INVALID_NATIVE_OUTPUT` | 6 | The provider's raw output didn't parse as expected (Claude JSON / Codex JSONL) | Usually transient/provider-side; rerun |
| `NO_FINAL_MESSAGE` | 6 | Codex produced no completed agent message | Usually transient/provider-side; rerun |
| `CONTRACT_VIOLATION` | 6 | The final result didn't satisfy the `wiki-query/v1` contract (e.g. no resolvable citation, or a `no_relevant_material` status with a citation) | A wiki whose skill can't ground an answer under the read-only constraints will surface here; rerun, or investigate the skill/question |
| `READ_ONLY_VIOLATION` | 7 | The before/after content snapshot detected a change to the wiki | Stop and investigate immediately — this should never happen against an unmodified wiki; changed relative paths (never content) are in `error.details.changed_paths` |
| `INTERNAL_ERROR` | 70 | Unexpected wrapper-side failure, or the integrity comparison itself couldn't complete | Never assume read-only held if this fires mid-integrity-check; report the sanitized error message |

Two warning codes are not failures (they appear in the success envelope's
`warnings` array, `ok: true`, exit `0`):

| Code | Meaning |
|---|---|
| `WIKI_SCHEMA_ABSENT` | No `SCHEMA.md` at `content_root` — usually means `content_root` points one level too high (or, less often, too low); most wiki toolchains put `SCHEMA.md` at the wiki root |
| `CLAUDE_READ_SCOPE_BROAD` / `CODEX_READ_SCOPE_BROAD` | The provider's read tools can inspect more than just the selected `content_root` (Claude: when `content_root` is a strict subdirectory of `project_root`; Codex: always, its sandbox prevents writes but not reads outside the wiki) |
| `CLAUDE_ENABLED_PLUGINS_DECLARED` | Claude only — the wiki's settings declare `enabledPlugins`; informational, not something to fix |
