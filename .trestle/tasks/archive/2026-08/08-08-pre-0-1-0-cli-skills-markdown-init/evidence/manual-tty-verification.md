# Manual real-TTY verification — G27 / G28

- Verifier: Madao (task/repo owner, `gitStatus` git user)
- Date: 2026-08-08
- Binary: `target\debug\llm-wikis.exe` (local `cargo build`, debug profile;
  package version `0.1.0-beta.2` per `Cargo.toml`, plus this task's
  uncommitted working-tree changes — the Round 1 diff under review)
- Terminal: a real interactive Windows terminal (not `assert_cmd`, which
  never provides a controlling terminal — see `tests/cli_contract.rs`'s own
  comment at the `--plain` regression test, `tests/cli_contract.rs:2141-2146`)
- Corresponding gates: `checklist.md` G27 (rendering) and G28
  (init wizard/overwrite), matrices in `design.md` §1.2 and §2.2

These are exactly the scenarios `evidence/round-1.md`'s "Action items for
the next round" item 4 asked for; Round 1 correctly withheld pass on G27/G28
because no transcript existed at the time.

## G27 — markdown rendering (design.md §1.2 matrix)

All four scenarios ran against a registered wiki's `query` command.

1. **Real TTY, no flags** — stdout is a terminal, `NO_COLOR` unset, no
   `--plain`: markdown rendered with ANSI styling via `termimad`. Confirmed
   by the verifier. Note recorded by the verifier: unordered-list markers
   (`-`/`*`) render as a bullet glyph rather than the literal character —
   expected `termimad` skin behavior, not a defect.
2. **Piped / redirected stdout** — same invocation piped to a file/`cat`:
   output is raw markdown text, no ANSI escape bytes. Confirmed.
3. **Real TTY with `--plain`** — forces raw markdown despite the TTY.
   Confirmed.
4. **Real TTY with `NO_COLOR=1`** — forces raw markdown despite the TTY.
   Confirmed.

Result: all four match `design.md` §1.2's decision matrix exactly. G27 PASS.

## G28 — `config init` wizard / overwrite guard (design.md §2.2 matrix)

All four scenarios ran against `--config C:\Temp\lw-test\config.toml`.

1. **Fresh path, real TTY, no flags** — file did not exist yet: the
   interactive wizard ran (`default_agent` Select, then
   `providers.claude.executable` / `providers.codex.executable` Text
   prompts), every prompt accepted via Enter (its shown default), and the
   file was written successfully. Confirmed.

2. **Same path, re-run, real TTY, no `--force`** — file now exists: the
   overwrite `Confirm` appeared and was answered `no` (declining the
   default). Verified verbatim terminal transcript:

   ```
   \target\debug\llm-wikis.exe --config C:\Temp\lw-test\config.toml config init; $LASTEXITCODE
   configuration already exists at C:\Temp\lw-test\config.toml. Overwrite? no
   error: CONFIG_EXISTS (configuration file already exists)
   2
   ```

   File left untouched, `error.code = CONFIG_EXISTS`, exit code `2`.
   Matches `design.md` §2.2's "No/Esc → `CONFIG_EXISTS`, exit 2, no write"
   row exactly.

3. **Same path, re-run, real TTY, no `--force`, `Confirm` answered `yes`** —
   wizard ran again (same prompts as scenario 1), then the file was
   overwritten with the newly-collected answers. Confirmed.

4. **Same path, `--force`, stdout piped** — no `Overwrite?` prompt and no
   wizard prompts appeared at all; the file was overwritten directly and
   silently. Confirmed — matches `design.md` §2.2's `--force` rows
   (force is unconditional overwrite consent, independent of TTY/pipe
   state).

Result: all four match `design.md` §2.2's decision matrix exactly. G28 PASS.

## Conclusion

Every `dialoguer`/TTY-render code path exercised in this session behaved
exactly as `design.md` §1.2/§2.2 specify. G27 and G28 are satisfied.
