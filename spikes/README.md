# llm-wikis-spikes

Disposable pre-implementation capability spikes for `llm-wikis` (see
`docs/2026-07-28-llm-wikis-query-cli.md` Task 2 and
`docs/2026-07-28-llm-wikis-external-query-design.md` §17.2).

**This code is evidence only. It is never copied into production.** The
production crate is developed again from scratch through failing tests
(Task 3 onward). Nothing under `spikes/` is a dependency of, or a source
for, `src/`.

Results are recorded in `docs/verification/llm-wikis-preflight.md`.

## Subcommands

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH
cargo run --manifest-path spikes/Cargo.toml -- <subcommand>
```

- `stdin-boundary`
- `windows-resolution`
- `platform-dirs`
- `bounded-pipes`
- `process-tree`
- `mutation-hash`
- `temp-artifacts`
- `provider-contract --wiki <agents|harness> --agent <claude|codex>` — **only
  run under explicit user authorization**; consumes model quota and reads a
  real, real knowledge base under `D:\Wikis` strictly read-only (never
  writes anything there). Builds the spec §7.1 envelope, invokes the real
  `claude`/`codex` CLI with the exact §10.2/§10.3 argv, snapshots
  `content_root` before/after (§12), and validates the result against
  `wiki-query/v1` (§7.3) with citation resolution (§11.2).

`__fixture <mode>` is a hidden mode used internally by several spikes,
which re-invoke this same binary as the "child under test". It is not a
user-facing subcommand.

`process-tree-child` is a separate helper binary used only by the
`process-tree` spike.
