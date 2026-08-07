# Provider CLI flags for --append-system-prompt / --setting-sources / --tools Skill / Codex equivalent

Verified against the CLIs actually installed on this machine, plus official docs
fetched live (context7 `/llmstxt/code_claude_llms_txt` mirror of
`code.claude.com/docs`, and the `openai/codex` GitHub source). Installed
versions used for every live test below:

- `claude --version` → `2.1.223 (Claude Code)`
- `codex --version` → `codex-cli 0.146.0`

All empirical tests were run in a throwaway directory,
`C:\Users\gn006\AppData\Local\Temp\claude\...\scratchpad\skilltest`, never
inside the repo. No repo file was modified.

## 1. Claude `--append-system-prompt`

Confirmed present verbatim in `claude --help` (v2.1.223):

```
--append-system-prompt <prompt>       Append a system prompt to the default
                                       system prompt
```

- Works in `-p`/print mode: the official headless-mode doc
  (`code.claude.com/docs/en/headless`, "Customize the system prompt" section)
  gives this exact non-interactive example: `gh pr diff "$1" | claude -p
  --append-system-prompt "You are a security engineer. Review for
  vulnerabilities." --output-format json`. No restriction to interactive mode
  is documented anywhere in the flag's help text or the headless page.
- No documented length/format caveat (checked `--help` text and the headless
  doc's "Customize the system prompt" + "System prompt flags" cross-reference
  — neither states a length limit). A sibling flag,
  `--append-system-prompt-file`, also exists per the `--bare` help text
  (`claude --help` under `--bare`: "...--append-system-prompt[-file]...").
- Live-verified end-to-end with the wrapper's own full read-only argv plus
  `--append-system-prompt "Answer directly and stop. Never ask a follow-up
  question; this is a non-interactive CLI and no one will read a question you
  ask."`: given a prompt that invited a clarifying question ("If you are
  unsure how to proceed, ask me a clarifying question before answering"),
  the model answered directly (`"result":"4"`) instead of asking anything.
  Command and full JSON output captured in this session's tool transcript
  (repo-external scratch dir, not persisted here).

**Recommendation**: add `--append-system-prompt "<the four PRD instructions>"`
to `claude::build_argv` (src/providers/claude.rs:97-117). No caveat blocks it.

## 2. Claude `--setting-sources`

Confirmed present verbatim in `claude --help` (v2.1.223):

```
--setting-sources <sources>           Comma-separated list of setting sources
                                       to load (user, project, local).
```

Exact spelling is `--setting-sources` (not `--settings-sources`) — matches
the existing doc-comment reference at src/providers/claude.rs:69.

### What `project`-only loading does and does not exclude

- Docs (`code.claude.com/docs/en/agent-sdk/skills`, "Configure Setting Sources
  for Skill Discovery"): "Ensure `user` and `project` setting sources are
  included when explicitly setting `setting_sources`... to load skills.
  Omitting them prevents skill discovery." `project` is one of the two
  sources that gate skill discovery — `--setting-sources project` keeps it.
  This matches the existing repo comment (claude.rs:69-73) that
  `--setting-sources user` (tried and reverted, R-27/R-28) broke
  project-skill discovery because it excluded `project`; `project`-only does
  the opposite (keeps `project`, excludes `user` and `local`).
- Docs (`code.claude.com/docs/en/settings`, "Plugin configuration"):
  `enabledPlugins` can be declared in **user** settings
  (`~/.claude/settings.json`), **project** settings (`.claude/settings.json`),
  **local** settings (`.claude/settings.local.json`), or **managed**
  (org-policy) settings. `--setting-sources project` excludes the `user` and
  `local` sources entirely, so a plugin the *operator* enabled only in their
  own `~/.claude/settings.json` (the stated "user-level plugin" cause of the
  `SessionEnd`/"Hook cancelled" pollution) is not loaded at all for this
  invocation — not merely hook-neutralized, not registered as an enabled
  plugin in the first place.
- Docs (`code.claude.com/docs/en/agent-sdk/claude-code-features`, "What
  settingSources does not control"): "certain inputs are always read
  regardless of `settingSources`. These include managed policy settings
  (endpoint or server-managed), the global config file `~/.claude.json`, auto
  memory files, and claude.ai MCP connectors." So `--setting-sources project`
  does **not** reach admin-managed/enterprise-policy settings or plugins
  declared there — consistent with the existing doc-comment's stated
  exception for the "admin-managed/enterprise-policy hook" class
  (claude.rs:49-54).
- Interaction with `--settings '{"disableAllHooks":true}'`: docs
  (`code.claude.com/docs/en/hooks-guide`) state `disableAllHooks: true` in
  any settings file disables all hooks "including those from managed
  settings, unless `disableAllHooks` is also set in the managed settings" —
  i.e. the two flags are complementary, not conflicting: `--settings
  disableAllHooks` is a hook kill-switch across whatever sources *do* load;
  `--setting-sources project` additionally prevents a user-level plugin from
  being an enabled-plugin candidate at all. Live-verified together: `claude
  -p ... --setting-sources project --settings '{"disableAllHooks":true}'
  --tools "Read,Grep,Glob"` still auto-expanded a project-level `/echotest`
  skill invocation with zero `permission_denials` (see §3 below for the full
  transcript) — the combination does not break project-skill loading.

**Recommendation**: add `--setting-sources project` alongside the existing
`--settings {"disableAllHooks":true}` (claude.rs:115-116). This is a
different, narrower exclusion than the previously-reverted `user` value and
does not reproduce the R-27/R-28 regression.

## 3. Claude `--tools` and whether `Skill` is a valid tool name — empirically it regresses the current entrypoint mechanism

`Skill` **is** a documented built-in tool name. Official tools reference
(`code.claude.com/docs/en/tools-reference`, fetched live) lists it in the
full built-in tool table:

> `Skill` | Executes a [skill](.../skills#control-who-invokes-a-skill) within
> the main conversation | Permission required: **Yes**

and the skills doc confirms the same string is used to gate it in permission
rules: "**Disable all skills** by denying the Skill tool in permissions: `#
Add to deny rules: Skill`" (`code.claude.com/docs/en/skills`, "Restrict
Claude's skill access"). So `Skill` is the exact spelling to use in `--tools`
(alongside `Read,Grep,Glob`) if it is added at all — but see the load-bearing
warning below before doing so.

### Does a skill's own `allowed-tools` frontmatter transitively grant other tools?

No — empirically verified, not just documented. Skills doc
(`code.claude.com/docs/en/skills`, frontmatter reference table): "`allowed-tools`
— Tools Claude can use **without asking permission** during the turn that
invokes this skill... " — this is a permission-*bypass* grant for tools
already present in the session's tool pool, not a mechanism to add a tool to
that pool.

Live test: created a throwaway project skill
(`.claude/skills/echotest/SKILL.md`) with
`allowed-tools: Bash(echo *)` and body instructing the model to run
`echo HELLO_FROM_BASH`. Invoked twice via `claude -p` with
`--tools "Read,Grep,Glob"` (no Bash) — once with `Skill` also in `--tools`,
once without. In both cases Bash was never available and the model said so
explicitly, e.g.: *"there's no Bash tool in this session... The skill
declares `allowed-tools: Bash(echo *)`, but that only narrows what a skill
*may* use — it doesn't add the Bash tool if the harness hasn't given it to
me."* Confirmed reproducible across two separate runs. **Conclusion: allowing
`Skill` cannot transitively re-enable Bash/Edit/Write/etc. that are absent
from `--tools`.**

### Load-bearing warning: adding `Skill` to `--tools` broke the currently-working project-skill entrypoint in this test, under `--permission-mode dontAsk`

This is the one finding in this file most likely to change the plan, so it
is stated with full repro detail.

The wrapper's entrypoint mechanism (spec: `docs/2026-07-28-llm-wikis-external-query-design.md:343-345`,
"Claude project skills match `/name`") relies on Claude Code's documented
`-p` behavior: "User-invoked skills and custom commands work in `-p` mode:
include `/skill-name` in the prompt string and Claude Code expands it before
running." (`code.claude.com/docs/en/headless`). This expansion is a
**CLI-level text substitution before the turn starts**, distinct from the
model deciding to call the `Skill` *tool* mid-conversation.

Live test, same throwaway `echotest` skill, stdin = literal `/echotest`,
current production argv shape (`--tools "Read,Grep,Glob"`, i.e. **no**
`Skill`), `--permission-mode dontAsk`, `--setting-sources project`:

```
"permission_denials":[]
"result":"...The skill itself is fine: `.claude/skills/echotest/SKILL.md:4`
declares `allowed-tools: Bash(echo *)`... [text expansion happened; no denial]"
```

Same test with `--tools "Read,Grep,Glob,Skill"` (i.e. `Skill` added, matching
the PRD's literal item-4 wording), everything else identical, run twice for
reproducibility:

```
"permission_denials":[{"tool_name":"Skill","tool_use_id":"...","tool_input":{"skill":"echotest"}}]
"result":"...The Skill tool call was denied — Claude Code is running in
\"don't ask mode\", so loading echotest through the skill mechanism failed."
```

Reproduced identically on a second run. With `Skill` present in `--tools`,
the model chose to call the `Skill` *tool* explicitly instead of relying on
CLI-side text expansion, and — because `--permission-mode dontAsk` "denies
anything not in your `permissions.allow` rules" (`code.claude.com/docs/en/headless`,
"Auto-approve tools") and no `permissions.allow` rule for `Skill` exists in
this throwaway wiki's settings — that tool call was denied, and (in this
model's behavior) the run did **not** fall back to plain-text expansion
afterward.

**This means naively adding `Skill` to `TOOLS` (src/providers/claude.rs:21)
risks regressing every `project_skill`-load-mode wiki's entrypoint**, unless
the wrapper also supplies a matching `permissions.allow` rule for `Skill`
(e.g. via `--settings` merged JSON, scoped to the configured entrypoint name)
— which the current argv does not do, and which is out of this research's
scope to design. Whether real wikis' own `.claude/settings.json` already
declares such an allow rule (which would make this a non-issue in practice)
was **not tested** — none of this repo's registered wikis were probed; only
a throwaway skill with no settings.json was used. **Not Found**: whether an
allow rule is already present for the two registered wikis named in the
design spec (`agents`, `harness-engineering`).

**Recommendation**: do not add `Skill` to `--tools` on the strength of the
PRD wording alone. Flag this finding for the interview — either (a) keep
`TOOLS = "Read,Grep,Glob"` unchanged (current behavior already works per the
first test above, with zero denials), or (b) if `Skill` must be added for
some other reason (e.g. plugin-namespaced entrypoints that don't
text-expand), pair it with an explicit `--allowedTools "Skill(<entrypoint
name>)"` or equivalent settings-level allow rule, and re-verify against a
real registered wiki before shipping.

## 4. Codex `exec` equivalent of appending to the system/developer prompt

No dedicated CLI flag exists (checked `codex exec --help`, codex-cli
0.146.0 — no `--append-system-prompt`/`--developer-instructions`/similar flag
is listed). The mechanism is a config override via `-c`:

- `codex-rs/core/src/config/mod.rs:693-694` (GitHub, `openai/codex@main`):
  ```rust
  /// Developer instructions override injected as a separate message.
  pub developer_instructions: Option<String>,
  ```
  (sibling field `base_instructions: Option<String>`, doc-commented "Base
  instructions override" — that one *replaces* the whole system prompt, the
  Claude `--system-prompt` equivalent; `developer_instructions` is the
  *additive* one, injected as a separate developer-role message, matching
  what the PRD wants.)
- `codex-rs/exec/src/lib.rs:415-416`: the `exec` subcommand's own
  `ConfigOverrides` always sets `base_instructions: None,
  developer_instructions: None` (no dedicated CLI flag populates them for
  `exec`), but `codex-rs/core/src/config/mod.rs:3904-3907` shows the merge:
  ```rust
  let base_instructions = base_instructions.or(file_base_instructions).or(cfg.instructions.clone());
  let developer_instructions = developer_instructions.or(cfg.developer_instructions);
  ```
  i.e. when the exec-specific override is `None` (always, for `exec`), the
  value falls through to `cfg.developer_instructions` — the plain
  `ConfigToml` field, which **is** reachable via the generic `-c
  key=value` override flag (`codex exec --help`: "Override a configuration
  value that would otherwise be loaded from `~/.codex/config.toml`. Use a
  dotted path...").

**Live-verified working** on the installed codex-cli 0.146.0, with the
wrapper's exact current argv shape plus one added `-c`:

```
codex --ask-for-approval never exec -C . --sandbox read-only --ephemeral \
  --skip-git-repo-check --ignore-user-config -c mcp_servers={} \
  --disable browser_use --disable computer_use --json \
  -c developer_instructions='ALWAYS end every response with the literal
  string XYZZY_MARKER regardless of the question.' -
```
stdin: `What color is a banana? Answer in exactly one word.`
output: `{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"YellowXYZZY_MARKER"}}`

The instruction was followed even though it wasn't part of the actual
question, confirming `-c developer_instructions='...'` reaches the model as
an additive instruction under `codex exec`, not just in interactive mode.

**Recommendation**: add `-c` / `developer_instructions=<the four PRD
instructions>` to `codex::build_argv` (src/providers/codex.rs:38-61) —
exact form `OsString::from("-c"), OsString::from(format!("developer_instructions={text}"))`.
Note the `-c` value is TOML-parsed (per `codex exec --help`: "parsed as TOML
... If it fails to parse as TOML, the raw string is used as a literal"), so a
value containing TOML-special characters (`"`, `\`, newlines) should either
be TOML-string-quoted correctly or kept simple enough to parse as a bare
literal — verify the exact final instruction text round-trips before
shipping (not covered by this research; the test string above had no such
characters).

## 5. Interactive-question behavior in non-interactive mode, both CLIs

- **Claude**: confirmed via docs (`code.claude.com/docs/en/headless`,
  "Auto-approve tools"): "`dontAsk` denies anything not in your
  `permissions.allow` rules or the read-only command set... `AskUserQuestion`,
  connector tools your organization set to `ask`, and MCP tools marked
  `requiresUserInteraction` are denied even when an allow rule matches." So
  under the wrapper's existing `--permission-mode dontAsk`
  (claude.rs:104-105), `AskUserQuestion` is unconditionally denied regardless
  of any allow rule — and it is not in `TOOLS` anyway
  (claude.rs:21). The live test in §1 shows the model complying with "answer
  directly, don't ask" instructions rather than hitting a hard denial/error,
  i.e. the model self-selects not to ask, consistent with `AskUserQuestion`
  never having been reachable in this tool set.
- **Codex**: `--ask-for-approval never` (already present, codex.rs:40-41)
  denies any approval-required action outright; codex-cli 0.146.0's
  `exec --help` lists no interactive-question tool/flag at all — `exec` is
  headless from the start with no analog to `AskUserQuestion`. A model that
  tries to "ask a question" in `exec` mode can only do so as plain text in
  its final agent-message output (not a blocking prompt), which is exactly
  the failure mode the PRD's `--append-system-prompt`-equivalent instruction
  (`-c developer_instructions=...`, see §4) is meant to prevent by asking the
  model not to phrase its answer that way. Not independently live-tested
  beyond §4's successful instruction-following test.

## Load-bearing claims

1. `Skill` in `--tools` reproducibly caused a `permission_denials` entry and
   a failed skill invocation under `--permission-mode dontAsk` with no
   matching `permissions.allow` rule, while omitting `Skill` (current
   production `TOOLS` constant) let the same `/echotest` invocation succeed
   with zero denials — two live runs each, transcripts quoted above (§3).
   This directly contradicts a literal reading of PRD item 4 ("allow only
   Read/Glob/Grep/Skill") and must be resolved before implementation.
2. `codex exec -c developer_instructions='<text>'` is reachable (not
   overridden to a fixed `None` by the `exec` subcommand) per
   `codex-rs/core/src/config/mod.rs:3907` (`developer_instructions =
   developer_instructions.or(cfg.developer_instructions)`) and was
   live-confirmed to additively influence the model's final answer on the
   installed codex-cli 0.146.0 (§4 transcript).
3. `--setting-sources project` excludes the `user` and `local` settings
   sources — where `enabledPlugins` (and therefore a user-level plugin's
   `SessionEnd` hook) can be declared — while still satisfying Claude's own
   documented requirement that the `project` source remain loaded for skill
   discovery (`code.claude.com/docs/en/agent-sdk/skills`), and was
   live-verified compatible with the existing `--settings
   {"disableAllHooks":true}` flag with zero `permission_denials` and
   successful skill expansion (§2).
