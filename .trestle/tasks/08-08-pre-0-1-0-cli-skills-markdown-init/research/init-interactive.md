# `config init` 防覆寫 + 互動精靈：現況、crate 選型、CLI 先例、建議行為矩陣

範圍：僅涵蓋 PRD 標題中 "init 防覆寫與互動設定" 這一塊；skills 目錄與 markdown 終端渲染由其他 research 檔案處理。

## 1. 現況（repo 內證據）

### 1.1 `config init` 已經不會覆寫

`src/config.rs:1090-1112` 的 `init()`：

```
pub fn init(path: &Path) -> Result<ConfigInitOutcome, AppError> {
    ...
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => { ... Ok(ConfigInitOutcome { created: true, .. }) }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(AppError::new(
            ErrorCode::ConfigExists,
            "configuration file already exists",
        )),
        Err(e) => Err(config_invalid(...)),
    }
}
```

- 用 `OpenOptions::create_new(true)`（原子的 exclusive-create），已存在時回傳 `ErrorCode::ConfigExists`（`src/error.rs:14,78`），exit code 2（`src/error.rs:105-121`），**從不覆寫、從不合併**。
- `src/cli.rs:376-417` 的 `run_config_init` 只是把這個結果包成 `ConfigInitEnvelope` 印出（`--json` 走 `render_json_generic`，人類模式走 `eprint_error_line`），完全沒有覆寫路徑，也沒有任何 confirm/prompt 邏輯。
- **這是規格明文行為，且有機讀測試守護**：`docs/2026-07-28-llm-wikis-external-query-design.md:139` — 「`llm-wikis config init` creates the parent directory and a valid, non-interactive starter configuration only when the destination does not exist. It never overwrites or merges an existing file; an existing destination is `CONFIG_EXISTS` and exit `2`.」同段最後一句：「**A future wizard or `config add-wiki` command is outside version 0.1.0.**」（`docs/2026-07-28-llm-wikis-external-query-design.md:139`）。同文件 line 72 也把「a non-interactive, non-overwriting `llm-wikis config init`」列為既有設計目標之一。
- 這份文件是被機械化驗證的規格，不是普通文件：`tests/spec_drift.rs:1-21` 解析 `docs/2026-07-28-llm-wikis-external-query-design.md` 的 Section 14 錯誤表、Section 15 check 名稱、Section 13 warning code，並與 `src/error.rs`／`src/output.rs` 的實作互相比對；註解明白寫著「The specification stays the authority」。Section 14 的 27 個 error code 是封閉集合，`docs/...md:909` 原文「Twenty-seven codes.」。

  **結論**：PRD 想要的「互動精靈」目前是規格明文排除在 0.1.0 之外的功能。要落地就必須同步修改這份規格文件（至少 §2.3/Section 5 對應段落與 line 139 那句），否則會被 `tests/spec_drift.rs` 之外、人工核對規格與行為的流程視為 spec drift。這不是 blocker，但規劃時要把「更新 docs/2026-07-28-...md」列進同一個變更範圍。

### 1.2 目前沒有 `--force`／confirm 相關 flag

`src/cli.rs:96-114` 的 `ConfigAction` 只有 `Init`、`List`、`Validate` 三個 variant，`Init` 沒有帶任何欄位（不像其他 subcommand 有 `#[arg(...)]`）。`grep force` 在 `src/` 下沒有任何命中（除了不相關的 `enforce*` 字樣）。要加 `--force`/`-i`/`--interactive` 都是全新 clap 參數。

### 1.3 `--json` / TTY 偵測慣例

`src/cli.rs:20` 已經 `use std::io::{IsTerminal, Read}`，並且已經在兩處用 `is_terminal()` 閘門互動行為：

- `src/cli.rs:698`：`resolve_question_bytes` 用 `std::io::stdin().is_terminal()` 判斷是否該讀 stdin（query 的 positional-vs-stdin 邏輯）。
- `src/cli.rs:827-838`：`start_query_spinner` 只在 `std::io::stderr().is_terminal()` 為 true 時才建立 `indicatif::ProgressBar`，並且註解明講設計理由：「an `assert_cmd`-driven test, a pipe, a redirect, or CI never satisfies this, so the spinner branch is unreachable from any automated test and never contaminates stdout/stderr byte-for-byte comparisons. `None` is returned, not a disabled/no-op spinner, so a non-interactive run never even constructs an `indicatif` handle.」（`src/cli.rs:820-826`）

  這是本 repo 對「互動 UI 只在真終端機出現，且非終端機時連物件都不建構」的既有先例，新的互動精靈應該完全沿用同一模式：`stdin`/`stdout` 皆為 terminal 才進互動路徑；`--json` 一律不進互動路徑（`--json` 是 machine-readable 契約，互動 prompt 會污染或阻塞它）。

- `--json` 是 global flag（`src/cli.rs:54-56`），`dispatch`（`src/cli.rs:281-306`）取出 `cli.json` 後往下傳給每個 `run_config_*`，所以任何新邏輯都能簡單地用同一個 `json: bool` 參數關掉互動。

### 1.4 Config 欄位（哪些適合精靈問）

`src/config.rs:373-383`（`Config`）＋子表：

| 欄位 | 型別/位置 | 精靈適合度 |
|---|---|---|
| `default_agent` | `Option<Agent>`，`config.rs:376` | 適合：二選一（claude/codex/none），有限枚舉，`Select`/`Confirm` 剛好 |
| `providers.claude.executable` / `providers.codex.executable` | `Option<String>`，`config.rs:216-219,233-236` | 適合但低優先：預設就是 `"claude"`/`"codex"`（INIT_TEMPLATE 已寫死，`config.rs:1045-1049`），只有 PATH 外執行檔才需要問 |
| `runtime.timeout_seconds`/`max_question_bytes`/`max_stdout_bytes`/`max_stderr_bytes` | `u64`，`config.rs:158-169` | 低優先：都有 sane default（180s / 64KiB / 1MiB / 64KiB，`config.rs:141-152`），一般使用者不需要在 init 當下就調 |
| `wikis.<id>.*`（`title`/`project_root`/`content_root`/`agents`/`query_prompt`/`claude`/`codex` 子表） | `WikiConfig`，`config.rs:287-297` | **不適合放進 init 精靈**：`project_root`/`content_root` 需要通過 `PathOutsideAllowedRoot`/`WikiInvalid`/`UnsafeFilesystemEntry` 等語意驗證（`config.rs:307-347`），`ProviderWikiConfig`（`load`/`entrypoint`/`skill_path`/`plugin_dir`）依 `load` 模式有條件式必填欄位（`config.rs:259-281`），且規格已經明講「add wikis by hand-editing the TOML」「A future ... `config add-wiki` command is outside version 0.1.0」（`docs/...md:139`，`docs/llm-wikis.md:197`）。把 wiki 註冊塞進 init 精靈等於在做規格明確排除的 `config add-wiki`，範圍蔓延風險高。

  **建議**：若要做「互動設定部分設定值」，精靈範圍應限縮在 `default_agent` + 兩個 `providers.*.executable`（如果非 PATH 預設值），wiki 註冊維持「精靈產生的樣板 + 使用者手動編輯」不變。這也是最小可行、最貼近現有 `INIT_TEMPLATE` 形狀的做法。

### 1.5 JSON envelope 是規格鎖定的精確形狀

`docs/2026-07-28-llm-wikis-external-query-design.md:141`：「In JSON mode, config initialization emits exactly `{ "schema_version": "1.0", "ok": true, "operation": "config_init", "path": "<absolute config path>", "created": true }` on success.」——`ConfigInitEnvelope`（`src/config.rs:1116-1124`）目前欄位剛好是 `schema_version`/`ok`/`operation`/`path`/`created`/`error`，一個不多一個不少。任何新增欄位（例如 `overwritten: bool`、`interactive: bool`）都要同步改這份規格的那句話，否則又是一處 spec drift；且互動精靈本身就不該出現在 `--json` 路徑（見 1.3），所以 envelope 形狀理論上不需要因為互動功能而改動——只有「加 `--force` 導致的覆寫成功」是否要在 envelope 裡標示，才需要決定是否碰這句規格。

## 2. Rust 互動 prompt crate 選型：`dialoguer` vs `inquire`

### 2.1 資料來源

- crates.io API（`https://crates.io/api/v1/crates/<name>`，需要 `User-Agent` header 才會回資料，直接 `curl` 網頁版本是 SPA 沒有內容）。
- GitHub API（`https://api.github.com/repos/<org>/<repo>`）取 `pushed_at`/`open_issues_count`/`stargazers_count`/`archived`。
- ctx7（`npx ctx7@latest library/docs`）：`dialoguer` 在 ctx7 索引中查不到對應 Rust crate（`library dialoguer` 回傳的都是不相關套件，如 Dialogue Ts、Godot Dialogue Manager），`inquire` 有命中 `/mikaelmello/inquire`，並取得其 `Confirm`/`Text`/`Select` 用法片段（見下）。

### 2.2 客觀數據（2026-08-08 查得）

| | dialoguer | inquire |
|---|---|---|
| 最新版本 | 0.12.0 | 0.9.4 |
| crates.io 最後更新 | 2025-08-23 | 2026-02-24 |
| GitHub 最後 push | 2026-07-08 | 2026-03-02 |
| GitHub open issues | 97 | 88 |
| GitHub stars | 1608 | 2617 |
| archived？ | 否 | 否 |
| 下載量（總計） | 74,415,492 | 18,490,828 |
| 必要相依（`kind: normal`, `optional: false`） | `console ^0.16`, `shell-words ^1.1` | `bitflags ^2`, `dyn-clone ^1`, `unicode-segmentation ^1`, `unicode-width ^0.2` |
| Default feature 額外拉入 | `editor`→`tempfile`（**repo 已有此依賴**，`Cargo.toml:21`），`password`→`zeroize` | `crossterm` 0.29（default features 之一，`macros`/`crossterm`/`one-liners`/`fuzzy`）——**repo 目前完全沒有 crossterm/termion，是全新的終端後端** |
| 與 repo 既有依賴的重疊 | **高**：`indicatif 0.17` 已經在 `Cargo.lock` 帶入 `console 0.15.11`（`Cargo.lock:176-178,338-343`）；`console-rs` 這個 org 下 `console`/`indicatif`/`dialoguer` 是同一個維護者家族，官方 README 明講三者互相搭配（透過 WebFetch 讀取 `github.com/console-rs/dialoguer` 得到：「dialoguer pairs with other libraries in the family, specifically listing console and indicatif」） | **低（走 default features 時）**：`console` 只是 `inquire` 的一個*非 default* optional feature；要拿掉 crossterm、改用 console backend 需要 `default-features = false` 再手動點回 `macros`/`one-liners`/`fuzzy` 等，屬於非常規用法 |
| Confirm/Select/Text API | `Confirm::new().interact()` / `interact_opt()`（`docs.rs/dialoguer` 讀到：`interact()` 在 stderr 渲染，`interact_opt()` 支援 Esc/q 取消，回 `Option`） | `Confirm::new(...).with_default(true).prompt()`；`Select::new(...).prompt()`；`Text::new(...).with_default(...).with_validator(...).prompt()`（ctx7 `/mikaelmello/inquire` docs 命中，見下方引用） |
| 非 TTY 行為文件化程度 | docs.rs 頁面**沒有**明文說明 non-terminal/piped stdin 時的行為（WebFetch 讀取確認：「The documentation provided does not include any explicit information about behavior when stdin is not a terminal」） | 同樣沒有查到明文非 TTY 保證 |

ctx7 對 inquire 的 Confirm 範例（`/mikaelmello/inquire`，來源 `https://github.com/mikaelmello/inquire/blob/main/README.md`）：

```rust
use inquire::Confirm;
let proceed = Confirm::new("Do you want to continue?")
    .with_default(true)
    .with_help_message("This action cannot be undone")
    .prompt()?;
```

### 2.3 對抗性檢查（維護/風險面）

- 兩者都非 archived、近 6 個月內都有 push，都不是「已死」的 crate。
- dialoguer 的 open issue 數（97）略高於 inquire（88），但下載量是 inquire 的 4 倍，屬正常規模差異，不足以當作維護信號。
- 兩者官方文件都沒有明講「stdin 非終端機時會怎樣」——不管選哪個，**都必須由呼叫端自己用 `IsTerminal` 閘門**，不能信任 crate 自己偵測非互動環境。這點在 `gh auth login` 的真實事故上得到印證：非互動情境下呼叫互動 prompt 函式會產生「could not prompt: EOF」這種原始錯誤直接洩漏給使用者（`WebSearch` 命中 GitHub issue 討論，`gh auth login` 在非 TTY session 下會卡住/報錯，官方建議改用 `GH_TOKEN` 環境變數或 `--with-token` < stdin，而不是依賴函式庫自己判斷）——這正是本專案已經用 `is_terminal()` 主動避開的模式（見 1.3）。

### 2.4 推薦

**`dialoguer`**。理由（依重要度）：

1. **零額外終端後端**：repo 已經因為 `indicatif` 帶入 `console`（`Cargo.lock:176-178`），`dialoguer` 用同一個 `console` 當渲染層，新增依賴實質上只有 `shell-words`（小）＋ default features 帶的 `tempfile`（已是既有依賴，`Cargo.toml:21`）與 `zeroize`（小，`password` feature 用得到，本專案其實用不到密碼輸入，可以 `default-features = false, features = ["editor"]` 或乾脆都關掉只留 `Confirm`/`Select`/`Input` 這幾個不需要任何 feature 的基本型別，進一步縮小依賴面）。這完全符合 ladder 的「已安裝依賴優先」精神——不是新裝一個依賴，是延用同家族已經在依賴樹裡的東西。
2. **同維護者家族**：`console-rs/dialoguer` 與 `console-rs/indicatif` 是同一個 GitHub org，API 風格、`console::Term`、顏色/樣式慣例一致，不用學兩套終端抽象。
3. `interact_opt()` 支援 Esc 取消（回 `None`），對「使用者中途反悔」這種 UX 細節比較直接。

**備選**：若未來精靈需求變複雜（多步驟表單、fuzzy filter 選單、日期輸入），`inquire` 的 API 明顯更豐富（`with_validator`/`with_autocomplete`/`with_formatter` 等鏈式 builder），屆時可重新評估——但那需要接受多引入一個終端後端（crossterm）的代價。以本 PRD 描述的範圍（幾個 Confirm + 最多一兩個 Select/Text），`dialoguer` 已經足夠，不需要 inquire 的進階能力。

## 3. 主流 CLI 的 init 慣例

| CLI | 已存在時行為 | 非 TTY fallback | 來源 |
|---|---|---|---|
| `git init` | **靜默重新初始化，不報錯不覆寫既有內容**——官方文件原文：「Running `git init` in an existing repository is safe. It will not overwrite things that are already there.」 | 不適用（`git init` 本身不互動） | `git-scm.com/docs/git-init`（WebFetch） |
| `cargo init` | **報錯拒絕**：已有 `Cargo.toml` 時直接失敗，需手動刪除或換目錄 | 不適用（`cargo init` 不互動） | WebSearch 結果彙整自 rust-lang/cargo issue 討論（非官方文件逐字引用，屬社群共識轉述） |
| `npm init`（無 initializer，legacy 互動模式） | **不是「拒絕」也不是「靜默覆寫」，而是「增量合併」**：讀取既有 `package.json`，用其既有值當互動問題的預設值，「strictly additive」地保留原有欄位——官方文件語意：初始器缺省時退回 legacy 互動流程，依現有欄位猜測預設值 | `npm init -y`/`--yes` 跳過問答直接寫檔（非嚴格等於「非 TTY 自動判斷」，是顯式 flag） | `docs.npmjs.com/cli/init/`（WebSearch 摘要，未逐字引用） |
| `gh auth login` | 不是「檔案已存在」情境，但同樣是「互動優先」設計：預設走多步驟互動精靈 | **明確要求非互動環境改用不同介面**：`--with-token`（讀 stdin）或 `GH_TOKEN` 環境變數；若在非 TTY 下仍嘗試互動問答，會產生 `could not prompt: EOF` 這種未被優雅處理的錯誤（官方在 Actions 場景建議直接設 `GH_TOKEN`，不要呼叫 `gh auth login`） | `cli.github.com/manual/gh_auth_login` + GitHub issue 討論（WebSearch） |

**歸納**：

- 「已存在時默默覆寫」在這幾個先例裡幾乎不存在（`npm init` 看似例外，但它是「合併保留舊值」不是「覆寫成新樣板」，跟本專案 template-init 的語意不同——本專案沒有「解析既有 config 再合併」的需求／能力）。
- 「已存在時拒絕＋要求顯式動作」（`cargo init`、以及本專案現有的 `CONFIG_EXISTS`）和「已存在時安全地不動它」（`git init`）是兩個站得住腳的模式；本專案現況已經是前者的變形（拒絕但沒有 `--force` 出口）。
- 非 TTY 下沒有一個先例是「讓互動 prompt 函式庫自己去讀 stdin 然後炸掉」——`gh` 的教訓正是反面案例。本專案自己的 `start_query_spinner`（1.3）已經示範了正確模式：**先判斷 `is_terminal()`，非終端機直接跳過整個互動分支**，而不是呼叫互動函式庫再指望它處理非 TTY。

## 4. 建議 CLI 介面與行為矩陣（草案，僅供規劃參考，未修改任何程式碼）

延續現有 `ErrorCode::ConfigExists`（不新增 error code，維持 Section 14 封閉的 27 碼——`docs/...md:909`），新增兩個 clap flag：

- `--force`：跳過「已存在」的確認，直接覆寫（equivalent to today's create-new 但允許 truncate-and-write）。
- `--interactive` / `-i`：明確要求進入互動精靈（範圍見 1.4：`default_agent` + 兩個 `providers.*.executable`）。

行為矩陣（TTY 判定用 `stdin.is_terminal() && stdout.is_terminal()`，比照 `resolve_question_bytes`/`start_query_spinner` 的既有 `IsTerminal` 用法）：

| stdin+stdout 皆為 TTY？ | 檔案已存在？ | `--json` | `--force` | `--interactive`/`-i` | 建議行為 |
|---|---|---|---|---|---|
| — | 否 | 是 | — | — | 現況不變：直接寫 template，回傳 `ok:true` envelope |
| 是 | 否 | 否 | — | 未指定 | **維持樣板模式**（不強制精靈，避免行為突變讓既有腳本/使用者意外進入問答）；如需精靈必須顯式加 `-i` |
| 是 | 否 | 否 | — | 有指定 | 進互動精靈：問 `default_agent`（Select: claude/codex/none）、兩個 provider executable（Text，預設值＝`"claude"`/`"codex"`，Enter 直接採預設）；完成後寫檔 |
| 否（非 TTY，例如被 agent 呼叫、CI、pipe） | 否 | 否 或 是 | — | 有指定 | `-i` 在非 TTY 下**必須直接報錯**（`ArgumentInvalid`，訊息類似「--interactive requires an interactive terminal」），不得嘗試呼叫 `dialoguer`——避免重演 `gh auth login` 的「could not prompt: EOF」 |
| — | 是 | 是 | 否 | — | 現況不變：`CONFIG_EXISTS`，exit 2，`--json` 照樣輸出單一失敗 envelope |
| 是 | 是 | 否 | 否 | — | **新增**：`dialoguer::Confirm`（stderr 渲染，比照 `Confirm` 預設 `false`）詢問「configuration already exists at <path>. Overwrite? (y/N)」；Yes→覆寫，No 或 Esc→維持 `CONFIG_EXISTS` 失敗退出 |
| 否 | 是 | 任一 | 否 | — | **不進 confirm**，直接維持現況 `CONFIG_EXISTS`——這是本 PRD「非 TTY 對 agent 必須安全」的核心：agent 呼叫時絕不能卡在等待 stdin 的 confirm prompt 上 |
| — | 是 | 任一 | 是 | — | 略過確認，直接覆寫（`--force` 是「我已經確認過了」的顯式訊號，TTY 與否都適用，這也是唯一在非 TTY 下能讓 agent 主動覆寫的路徑） |
| 是 | 是 | 否 | 是 | 有指定 | `--force` + `-i`：覆寫後直接進互動精靈（等同「重新設定」） |

**設計原則摘要**：

1. **非 TTY 預設路徑必須與今天完全一致**（樣板 init、`CONFIG_EXISTS` 拒絕），因為此 CLI 的主要呼叫者是 AI agent／腳本，不是人類——這是 PRD 原文自己點出的限制，也呼應 `start_query_spinner` 的既有設計理由（`src/cli.rs:820-826`）與規格對「non-interactive」的多處強調（`docs/...md:72,139,153`）。
2. **互動精靈是 opt-in（`-i`），不是 TTY 就自動觸發**——理由：`config init` 目前在 TTY 下也是純樣板行為，貿然讓「偵測到 TTY 就問問題」變成預設，會是一個對現有人類使用者的隱性行為改變（且 `assert_cmd` 之類的整合測試如果不小心繼承了終端屬性也可能誤觸發，如 `src/cli.rs:822` 註解所警告的）。
3. **覆寫確認只在「TTY 且非強制」時才問**；`--force` 是唯一能在非 TTY 下允許覆寫的手段，且無論 TTY 與否都跳過 prompt——這給 agent 一個明確、可預期、不會卡住的「我要覆寫」訊號路徑。
4. `-i` 在非 TTY 下直接報 `ArgumentInvalid` 而不是靜默退化成樣板模式——因為使用者/agent 顯式要求了互動，靜默改變行為（忽略 flag）比報錯更容易讓呼叫者誤以為精靈跑過了。這點是本研究的建議取捨，非規格明文規定，規劃時可討論是否改成「非 TTY 下 `-i` 靜默退化為樣板」（如 npm `-y` 反向情境），兩者都有 CLI 先例可循，需要 PRD 決策而非研究單方面拍板。
5. 這一整組新行為（`--force`、`-i`、覆寫確認、精靈）都需要同步更新 `docs/2026-07-28-llm-wikis-external-query-design.md`（至少 line 72、139、909 附近的 27-code/no-wizard 表述）與 `docs/llm-wikis.md:186-198`，否則會製造規格與實作不一致——即使 `tests/spec_drift.rs` 目前只機械比對 error code/check name/warning code 三張表，不比對這段散文，人工審查仍會抓到落差。

## Load-bearing claims

1. `config init` 今天已經是 exclusive-create、從不覆寫、已存在即 `ErrorCode::ConfigExists`（exit 2）——`src/config.rs:1090-1112`，`src/error.rs:14,78,105-121`；PRD 的「不能覆寫」訴求在程式碼層面已經滿足，缺的是「要覆寫時的確認機制」與互動精靈本身。
2. 規格文件明文把「互動精靈」列為 0.1.0 範圍外：「A future wizard or `config add-wiki` command is outside version 0.1.0.」（`docs/2026-07-28-llm-wikis-external-query-design.md:139`），且該文件是被 `tests/spec_drift.rs`（`tests/spec_drift.rs:1-21`）機械化核對錯誤碼/檢查名稱/警告碼的權威規格來源——任何落地方案都必須同步修規格文件，否則是已知的 spec drift，不是可以忽略的邊角案例。
3. 本專案已有「用 `IsTerminal` 閘門互動行為、非終端機時連物件都不建構」的明確先例（`src/cli.rs:820-838` 的 `start_query_spinner`，及 `src/cli.rs:698` 的 `resolve_question_bytes`），互動精靈與覆寫確認的閘門邏輯應直接沿用同一模式，而不是依賴 `dialoguer`/`inquire` 自身對非 TTY 的處理（兩者官方文件都未明文保證非 TTY 行為，且 `gh auth login` 的「could not prompt: EOF」是這種依賴選錯層級會出事的真實案例，來源見 WebSearch 對 `cli/cli` issue 的彙整）。
