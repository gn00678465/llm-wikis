# pre-0.1.0 CLI 強化：skills 目錄、markdown 終端渲染、init 防覆寫與互動設定

三個功能一次納入（使用者選定單一 task）：

1. repo root 加入 `skills/` 目錄，內含教 AI agent 使用本 CLI 的 skill。
2. `query` 人類模式輸出可將 markdown 渲染成終端 ANSI 樣式。
3. `config init` 防覆寫確認 + 互動設定精靈。

## Findings

（證據詳見 `research/skills-directory.md`、`research/markdown-rendering.md`、`research/init-interactive.md`）

- F1 skills 規格：Claude Code 與 Codex CLI 的 SKILL.md 跨工具相容 frontmatter 子集為
  `name, description, license, compatibility, metadata, allowed-tools`；最小必要為
  `name`+`description`。Codex 探索路徑固定 `.agents/skills`，Claude 為 `.claude/skills`。
  plugin marketplace 僅 Claude 可用，結構性排除 Codex（research/skills-directory.md §2, §5）。
- F2 skill 內容素材：`docs/llm-wikis.md` §3.1（指令總覽）、§3.3（--json envelope + exit
  codes）、§3.8（20+ error code 表，宜放 references/）、§2.9（ENTRYPOINT_UNVERIFIED /
  --live probe）。CLI 只有 4 個子命令，研究建議單一總覽 skill + references，不拆多個。
- F3 「leaf」查證：真實存在（RivoLink/leaf），但為互動式全螢幕 previewer、新專案無獨立
  維護訊號，不適合作為渲染依賴（research/markdown-rendering.md §1）。
- F4 渲染方案推薦 `termimad` crate（0.35.1，2026-07 仍發布；依賴 crossterm/minimad）；
  外部工具 pipe（glow/mdcat）要求使用者另裝二進位、mdcat 處於維護權轉移期。掛點在
  `src/cli.rs:669`（`emit_query` 第三分支）；`render_human`（`src/output.rs:172-191`）
  依其 doc comment 維持純格式化、不做串流路由。
- F5 TTY 慣例：repo 已有 `std::io::IsTerminal` 先例（`src/cli.rs:698` stdin、
  `src/cli.rs:820-838` spinner，非 TTY 連物件都不建構）；渲染與互動精靈都應沿用，
  不依賴 crate 自行偵測（gh auth login 的 "could not prompt: EOF" 為反例）。
- F6 init 現況：`config init` 已是 exclusive-create、從不覆寫（`src/config.rs:1090-1112`，
  `OpenOptions::create_new`），已存在回 `CONFIG_EXISTS` exit 2（`src/error.rs:14,78`）。
  缺的是覆寫確認/`--force` 與互動精靈。目前 `ConfigAction::Init` 無任何 flag
  （`src/cli.rs:96-114`）。
- F7 規格衝突：`docs/2026-07-28-llm-wikis-external-query-design.md:139` 明文
  「A future wizard or `config add-wiki` command is outside version 0.1.0.」，且該文件被
  `tests/spec_drift.rs` 機械化核對（error code/check name/warning code 封閉集合，27 碼，
  `docs/...md:909`）。本任務落地必須同步修訂規格文件對應段落（至少 line 72、139）與
  `docs/llm-wikis.md:186-198`。
- F8 prompt crate 推薦 `dialoguer`（console-rs 家族，與既有 indicatif/console 同維護者、
  零新終端後端；`Cargo.lock:176-178,338-343`）；inquire 會帶入全新 crossterm 後端。
- F9 精靈適合欄位：`default_agent`（枚舉 Select）+ `providers.*.executable`（Text，
  預設 "claude"/"codex"）；wiki 註冊涉及路徑語意驗證與條件必填（`src/config.rs:259-347`），
  等同規格排除的 `config add-wiki`，不放入精靈（research/init-interactive.md §1.4）。
- F10 JSON envelope 形狀被規格逐字鎖定（`docs/...md:141`）；互動路徑不得進入 `--json`
  模式，envelope 是否需新欄位（如 overwritten）待決策。

## Decisions

- D1 任務切法：三個功能併入單一 complex task（使用者 2026-08-08 選定）。
- D2 規格文件同步修訂納入本任務範圍（F7 的直接後果，非可選項）：修
  `docs/2026-07-28-llm-wikis-external-query-design.md` 與 `docs/llm-wikis.md` 對應段落，
  使規格與新行為一致。
- D3 渲染（使用者 2026-08-08 選定）：內建 `termimad`，TTY 自動渲染 —
  stdout `is_terminal()` 且 `NO_COLOR` 未設且非 `--json` 時渲染；被 pipe/redirect
  時輸出原始 markdown；新增 `--plain` flag 強制原文。agent 呼叫（pipe/--json）
  行為與現狀 byte-for-byte 一致。
- D4 init 行為（使用者 2026-08-08 選定：TTY 自動進精靈 + `--yes` 略過精靈）：
  - 觸發：stdin+stdout 皆 TTY 且非 `--json` 且未帶 `--yes` → 自動進互動精靈
    （dialoguer；問 `default_agent` Select + `providers.*.executable` Text，
    Enter 採預設；wiki 註冊不入精靈，維持手動編輯 TOML）。
  - `--yes`：跳過精靈直接寫樣板（npm init -y 慣例）；非 TTY／`--json` 下自動
    等同 `--yes`（寫樣板，行為與現狀一致，對 agent 安全）——不新增 `-i` flag。
  - 覆寫：檔案已存在時，TTY 且無 `--force` → dialoguer Confirm 問是否覆寫
    （預設 No；No/Esc → 維持 CONFIG_EXISTS exit 2）；非 TTY 且無 `--force` →
    現狀 CONFIG_EXISTS；帶 `--force` → 不問直接覆寫（TTY 與否皆然，為 agent
    唯一覆寫路徑）。
  - `--json` 路徑永不互動，envelope 形狀維持規格逐字鎖定（F10）。
- D5 skills（使用者 2026-08-08 選定）：純目錄發佈 — repo root 放
  `skills/llm-wikis-usage/`（`SKILL.md` + `references/errors.md`），frontmatter 僅
  `name`+`description`（跨 Claude Code/Codex 相容子集），README 新增安裝說明段
  （手動 copy 到 `~/.claude/skills/`／`.agents/skills/`，並提及 `npx skills add`
  一行指令）。不做 install 腳本自動安裝、不做 plugin marketplace。
- D6 版本 bump 不併入本任務（使用者 2026-08-08 選定）：0.1.0-beta.3 的
  chore(release) bump 於本任務完成後另行處理，比照 beta.2 前例
  （commit 682ddfc）。

## Acceptance Criteria

- [x] AC1: repo root 存在 `skills/llm-wikis-usage/SKILL.md`（frontmatter 僅含
  name 與 description）與 references 錯誤碼文件；內容涵蓋 query/doctor/list/config
  用法、JSON envelope 與 exit code 對照、錯誤排查入口；README 含安裝說明段。
  (evidence: skills/llm-wikis-usage/SKILL.md, skills/llm-wikis-usage/references/errors.md,
  README.md:123-139 安裝說明段, evidence/gates-round-1.json G10-G13/G29 全 pass)
- [x] AC2: `query` 人類模式在 stdout 為 TTY、NO_COLOR 未設、未帶 --plain 時以
  termimad 渲染 markdown；被 pipe/redirect、帶 --plain、或 NO_COLOR 有值時輸出
  原始 markdown 與現狀 byte-for-byte 一致；--json 路徑完全不變。
  (evidence: tests/cli_contract.rs, test `live_doctor_then_query_succeed_end_to_end_with_schema_absent_warning`
  --plain byte-identity 斷言 tests/cli_contract.rs:2164-2168,
  tests/output_contract.rs, test `render_markdown_ansi_produces_escape_bytes_for_a_heading_and_bold_text`
  tests/output_contract.rs:170, `render_markdown_ansi_round_trips_plain_text_without_markdown_syntax`
  tests/output_contract.rs:181, evidence/manual-tty-verification.md G27 四情境)
- [x] AC3: `config init` 在 TTY 且未帶 --yes 時自動進入互動精靈（default_agent
  Select、provider executable Text、Enter 採預設）；--yes 或非 TTY 或 --json 時
  寫樣板（與現狀一致）；產出的 config 通過 `config validate`。
  (evidence: tests/cli_contract.rs, test `config_init_yes_produces_byte_identical_output_to_the_default_template`
  tests/cli_contract.rs:441, `config_init_generated_file_passes_config_validate_regardless_of_which_path_produced_it`
  tests/cli_contract.rs:469, evidence/manual-tty-verification.md G28 情境 1)
- [x] AC4: `config init` 遇既存檔案：TTY 且無 --force 時 Confirm 確認（預設 No，
  拒絕即維持 CONFIG_EXISTS exit 2）；非 TTY 且無 --force 維持 CONFIG_EXISTS；
  帶 --force 直接覆寫。--json envelope 形狀不變。
  (evidence: tests/cli_contract.rs, test `config_init_force_overwrites_an_existing_file_without_a_prompt`
  tests/cli_contract.rs:389, `config_init_without_force_still_refuses_to_overwrite_an_existing_file`
  tests/cli_contract.rs:421, tests/config_init.rs, test `success_envelope_matches_the_exact_contract`
  tests/config_init.rs:100, `failure_envelope_keeps_created_false_and_carries_the_public_error`
  tests/config_init.rs:124, evidence/manual-tty-verification.md G28 情境 2-4)
- [x] AC5: 規格文件（design 文件與 operator 指南）同步修訂 wizard/覆寫/渲染相關
  段落，`cargo test` 全綠（含 spec_drift）。
  (evidence: docs/2026-07-28-llm-wikis-external-query-design.md:72,129,134,139,151,
  docs/llm-wikis.md:191-212,450-499, evidence/gates-round-1.json G7/G19-G24 全 pass,
  evidence/round-1.md G4 段落記錄 process_supervisor.rs 兩個已知 flaky 測試以外全綠)

## Verification Plan

```
cargo build --all-targets --all-features
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
cargo test --test config_init --test cli_contract --test spec_drift --test config_contract -- --test-threads=1
rg -n "^name:" skills/llm-wikis-usage/SKILL.md
rg -n "^description:" skills/llm-wikis-usage/SKILL.md
rg --files-without-match "^(license|compatibility|metadata|allowed-tools|when_to_use|argument-hint|disable-model-invocation|user-invocable|context):" skills/llm-wikis-usage/SKILL.md
rg -n "termimad" Cargo.toml
rg -n "dialoguer" Cargo.toml
rg -n -- "--force" docs/2026-07-28-llm-wikis-external-query-design.md
rg -n -- "--yes" docs/2026-07-28-llm-wikis-external-query-design.md
rg -n -- "--force" docs/llm-wikis.md
rg -n -- "--plain" docs/llm-wikis.md
rg -n "^version = \"0.1.0-beta.2\"" Cargo.toml
```

## Expected Files

- `skills/llm-wikis-usage/**`
- `src/cli.rs`
- `src/config.rs`
- `src/output.rs`
- `docs/llm-wikis.md`
- `docs/2026-07-28-llm-wikis-external-query-design.md`
- `tests/config_init.rs`
- `tests/cli_contract.rs`
- `tests/output_contract.rs`
- `tests/config_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
