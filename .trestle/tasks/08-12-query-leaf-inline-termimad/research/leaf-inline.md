# 研究：`leaf --inline` 實測（取代 termimad 內建渲染）

測試環境：Windows 11, `leaf.exe` v1.21.0, 位於
`C:\Users\gn006\AppData\Local\Programs\leaf\leaf.exe`。所有指令皆透過本機
Bash (Git Bash) 執行；測試檔案位於 scratchpad
(`C:\Users\gn006\AppData\Local\Temp\claude\D--Projects-llm-wikis\69ffc927-0d21-44b4-afbc-3ed03e700b91\scratchpad`)。
測試文件 `test.md` 含 heading/bold/italic/bullet list/fenced code block/table。

來源分兩類標註：**[MEASURED]**＝本機實際執行指令得到的結果；**[SOURCE]**＝從
`github.com/RivoLink/leaf` 原始碼（`raw.githubusercontent.com` 抓取）逐行確認的
邏輯，用來解釋/佐證 MEASURED 結果背後的機制；**[DOC-ONLY]**／**[NOT VERIFIED]**
＝僅查到文件敘述、未能在本機或原始碼層級查證。

## F1 — `leaf --inline` 可透過 stdin 讀 markdown，不需要檔案參數 [MEASURED]

指令：
```
cat test.md | leaf.exe --inline
```
結果：`exit=0`，stdout 印出渲染後文字（heading 用 unicode box-drawing 字元框住、
bullet 用 `•`、code fence 有語法標記字元、table 有框線），符合 issue #8「透過
stdin 傳給 leaf --inline」的前提。不需要 `-` 或任何額外旗標指定 stdin 來源。

## F2 — SPEC 語法：只接受 `--inline`／`--inline ansi`／`--inline plain`／`--inline ansi:<N>`／`--inline plain:<N>`；`--inline:<N>`（無 ansi/plain、直接接冒號）會被判定為未知旗標 [MEASURED]

| 指令 | exit | 結果 |
|---|---|---|
| `--inline` (無參數) | 0 | 與 `--inline plain` 逐 byte 相同（見 F3） |
| `--inline ansi` | 0 | 887→2809 bytes，含 29 個 ESC(0x1b) |
| `--inline plain` | 0 | 887 bytes，0 個 ESC |
| `--inline ansi:100` | 0 | 與 `--inline ansi` 內容相同（測試文件無長行，寬度未觸發折行差異，見 F6 另用長行驗證寬度確實生效） |
| `--inline:80`（無 ansi/plain，直接冒號緊接） | **1** | `Error: Unknown flag: --inline:80` |
| `--inline foo`（非 ansi/plain 且無冒號的字串） | **1** | `Error: Cannot read: foo` → `os error 2`（把 `foo` 當成**檔案路徑**去開，不是回報「未知 SPEC」）——含意：若上層程式碼組出的 SPEC 字串打錯（例如手滑成 `ansii`），leaf 不會清楚報「格式錯誤」，而是嘗試把打錯的字串當檔名讀取，錯誤訊息會誤導成「檔案不存在」 |

## F3 — 未帶 SPEC 的預設值 = 依 `is_stdout_terminal` 自動選擇 ansi/plain，**且 NO_COLOR 對此判斷完全沒有影響**（因為原始碼裡沒有讀取 NO_COLOR）[MEASURED + SOURCE]

MEASURED：`diff` 確認 `--inline`（本機非 tty 環境下執行，stdout 被 Bash 工具
的管線捕捉）與 `--inline plain` 輸出逐 byte 相同（887 bytes）。

SOURCE（`src/inline.rs`，經 `raw.githubusercontent.com/RivoLink/leaf/main/src/inline.rs`
抓取確認）：
```rust
pub(crate) fn resolve_format(spec: &InlineSpec, is_stdout_terminal: bool) -> ResolvedFormat {
    match spec.format {
        InlineFormat::Ansi => ResolvedFormat::Ansi,
        InlineFormat::Plain => ResolvedFormat::Plain,
        InlineFormat::Auto if is_stdout_terminal => ResolvedFormat::Ansi,
        InlineFormat::Auto => ResolvedFormat::Plain,
    }
}
```
`src/main.rs`（同來源抓取確認）呼叫處：
```rust
let is_tty = io::stdout().is_terminal();
let width = inline::render_width(spec, is_tty);
let format = inline::resolve_format(spec, is_tty);
```
`is_tty` 用的是真正的 `std::io::IsTerminal::is_terminal()`（跟 llm-wikis 現有
`src/cli.rs:698`／`src/cli.rs:828` 用的機制同一族）。**整個 resolve_format 函式
沒有任何一行讀取 `NO_COLOR` 環境變數**——這代表：

- 若上層省略 SPEC，leaf 自己會依 stdout 是否為真 tty 決定 ansi/plain，行為與
  llm-wikis 現有 `render_markdown_ansi` 呼叫端的 TTY 判斷邏輯「精神一致」。
- 但若上層明確傳 `ansi`（例如為了自己控制寬度 `ansi:100` 又想要顏色），leaf
  **不會**因為 `NO_COLOR=1` 就退回 plain（見 F7 實測）。換句話說 leaf 不是
  no-color.org 慣例的實作者，NO_COLOR 判斷責任必須留在 llm-wikis 呼叫端自己做
  （這正是 termimad 版本 `src/cli.rs` 現有 TTY/NO_COLOR 判斷模式該保留、只是把
  「呼叫 termimad」換成「決定要不要傳 `--inline plain` / 省略 SPEC 給 leaf 子行程」）。

## F4 — Exit code／錯誤情境總表 [MEASURED]

| 情境 | 指令重點 | exit | stderr（節錄） |
|---|---|---|---|
| 成功（有效 markdown, plain/ansi） | 見 F2 | 0 | 空字串 |
| 空 stdin、無檔案參數 | `printf "" \| leaf --inline plain` | 1 | `Error: --inline requires a file path or stdin input` |
| 缺檔案參數（給不存在路徑） | `leaf does_not_exist.md --inline plain` | 1 | `Error: Cannot read: <path>` … `os error 2` |
| stdin 非合法 UTF-8（灌 200 bytes /dev/urandom） | 見上 | 1 | `Error: stdin is not valid UTF-8` … `invalid utf-8 sequence of 1 bytes from index 6` |
| 未知 SPEC 字串（無冒號、非 ansi/plain） | `--inline foo` | 1 | 被當檔名讀取失敗（見 F2） |
| `--inline:80`（語法本身不合法） | 見 F2 | 1 | `Error: Unknown flag: --inline:80` |
| 未知 theme | `--theme nonexistent-theme-xyz` | 1 | `Error: Unknown theme "nonexistent-theme-xyz"` |

結論：leaf 對失敗情境一律 exit code 1（未觀察到區分「使用者輸入錯誤」與
「系統/IO 錯誤」的不同 exit code），錯誤訊息都寫到 stderr、stdout 為空——這對
「fallback 到原始 markdown」的實作友善（可用 `status.success()` 單一條件判斷，
不用細分 exit code）。

## F5 — 8 MB stdin 上限，超過會報錯而非截斷 [SOURCE]

`src/main.rs`：`const MAX_STDIN_BYTES: usize = 8 * 1024 * 1024;`，
`read_stdin_limited` 讀超過上限時 `bail!("stdin exceeds the maximum supported
size of {} bytes", max_bytes)`（明確回傳錯誤，不是靜默截斷）。未在本機重現
8MB 輸入（未實測，因為建置 8MB 合法 UTF-8 markdown成本高且對本任務結論影響
小——llm-wikis query 的單次回覆遠小於 8MB），標記為 **[SOURCE ONLY，未 MEASURED]**。
對實作而言：理論上有一個需要處理的失敗分支，但實務發生機率極低。

## F6 — 寬度 SPEC (`:N`) 確實改變折行寬度；非 tty 情境下的預設寬度是固定 80 [MEASURED + SOURCE]

MEASURED：對一段長段落分別用 `--inline plain:40` 與 `--inline plain:120`，
每行最長長度分別鎖在 40 與 120（`awk '{print length}' | sort -rn` 驗證），
證實 `:<width>` 語法確實生效、非僅解析不生效。

SOURCE（`src/inline.rs`）：
```rust
pub(crate) fn render_width(spec: &InlineSpec, is_stdout_terminal: bool) -> usize {
    if let Some(w) = spec.width { return w.max(MIN_WIDTH); }
    if is_stdout_terminal {
        crossterm::terminal::size().map(|(cols, _)| (cols as usize).max(MIN_WIDTH)).unwrap_or(DEFAULT_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}
const DEFAULT_WIDTH: usize = 80;
```
非 tty（例如 llm-wikis 把 leaf 的 stdout 自己接管再轉印、或使用者重新導向）
時固定退回 80 欄，不會偵測到「實際終端」寬度（因為 leaf 進程本身此時看不到
真終端）。這對「llm-wikis spawn leaf、leaf 的 stdout 直接繼承成 llm-wikis 的
stdout（inherit，而非再次截取轉印）」的實作方式很重要：只有讓 leaf 的 stdout
真正繼承 llm-wikis 的 stdout handle（而不是 llm-wikis 自己 capture 再印），
`is_stdout_terminal` 判斷跟終端寬度偵測才可能拿到正確值；若 llm-wikis 用
`Command::output()`／capture stdout 再自己 println，leaf 進程看到的 stdout
永遠是非 tty，會固定退化成 80 欄 + plain。**這是本研究最重要的實作限制**。

## F7 — `NO_COLOR=1` 對明確指定 `ansi` 的輸出沒有效果（不會退化成 plain）[MEASURED]

```
cat test.md | NO_COLOR=1 leaf.exe --inline ansi   # exit=0, 仍有 29 個 ESC bytes
cat test.md | NO_COLOR=1 leaf.exe --inline        # exit=0, 0 個 ESC bytes（跟沒設 NO_COLOR 時的預設行為相同，因為本機是非 tty，本就會退化成 plain，不能證明是 NO_COLOR 生效）
```
配合 F3 的原始碼（`resolve_format` 沒有讀 NO_COLOR），**結論是 leaf 不認得
NO_COLOR**；No-Color 的責任必須留在 llm-wikis 呼叫端（用現有 `NO_COLOR`／
`--plain` 判斷邏輯決定要不要傳 `--inline ansi`，或乾脆一律不傳 SPEC、讓 leaf
自己用 is_stdout_terminal 判斷，但那樣就繞不開 F6 提到的「必須讓 leaf 直接繼承
真 stdout handle」的限制）。

## F8 — 成功執行時 stderr 為空，且不會佔用終端（no alternate screen／no raw mode／不等待按鍵）[MEASURED]

- 所有成功案例（F2/F3 表格）`stderr` 皆為空字串（`cat err_*.txt` 輸出空白）。
- 計時：5 次 `--inline plain`（70–88 ms）、3 次 `--inline ansi`（70–71 ms），
  每次都是進程立刻結束，非阻塞、非等待輸入（見下）。
- 用 `timeout 5 bash -c "... | leaf --inline < /dev/null"` 驗證：exit code 是
  `1`（等同 F4 的「空 stdin」錯誤），**不是 124**（timeout 的訊號），代表就算
  故意餵空輸入，leaf 仍立即回報錯誤並結束，不會卡住等待鍵盤輸入或進入
  alternate screen。這與 `--inline` 選項名稱本身承諾的「Render to stdout (no
  TUI)」一致。

## F9 — 啟動延遲：單次 subprocess spawn 約 70–90 ms [MEASURED]

見 F8 計時數據。對「每次 query 都額外 spawn 一次 leaf」的 UX 成本評估：
70–90 ms 相對於一次 LLM query（通常數百 ms 到數秒）是可接受的額外延遲，但
若未來要對同一批輸出多次呼叫（例如 streaming 分段渲染）需重新評估。

## F10 — leaf 會讀取使用者的 config/theme 檔案，且沒有「忽略使用者設定」的旗標；此檔案在本機目前不存在 [SOURCE + MEASURED（不存在的驗證）]

SOURCE（`src/config.rs`，抓取確認）：
- 設定檔路徑：Windows `%APPDATA%\leaf\config.toml`；macOS/Linux
  `$XDG_CONFIG_HOME/leaf/config.toml`，若未設定 `XDG_CONFIG_HOME` 則
  `$HOME/.config/leaf/config.toml`。
- 環境變數覆寫：`LEAF_THEME`（主題）、`LEAF_WIDTH`（寬度，需 ≥20）、
  `LEAF_TAB_TITLE_LENGTH`。
- `load_config()` 找不到檔案或解析失敗時回傳預設值＋可選警告，**但沒有任何
  「完全跳過讀取設定檔」的旗標或機制**（不像 Codex adapter 用的
  `--ignore-user-config`，`AGENTS.md` 中記載的那個模式）。

SOURCE（`src/main.rs`，抓取確認）：`main()` 一開始就呼叫
`config::load_config(&overrides)`，**在處理 `--inline` 之前**，代表
`--inline` 模式一樣會套用使用者的 config/theme 設定（至少 ansi 模式的配色會被
使用者自訂主題影響；未查證 plain 模式或版面結構是否也受影響——**[NOT
VERIFIED]**，原始碼片段沒有進一步展開 theme 套用到 inline renderer 的細節）。

MEASURED：本機以下路徑目前都不存在任何 leaf 相關設定檔（`find` 掃描
`~/.config`、`%APPDATA%`、`%LOCALAPPDATA%\Programs\leaf` 均只有執行檔本身，
無 `config.toml`）——代表本次所有測試都是在「無使用者自訂設定」的乾淨狀態下
跑的，尚未實測「有自訂 theme 時 `--inline ansi` 輸出會變」這件事本身
（因為建立/驗證需要先跑一次 `--config` 開編輯器，會阻塞等待互動，未在此次
研究中執行以避免掛住 session）。

**含意（供 planner 評估）**：若要保證 `llm-wikis query` 的渲染輸出對所有使用者
（不論其 leaf 個人設定為何）是一致、可預期的（例如色彩主題不會因人而異、不
會因某人裝了奇怪 theme 而整段輸出變成不可讀色），目前沒有 leaf 內建的
`--ignore-user-config` 或等效旗標可用。可行的緩解：用 `LEAF_THEME=<known-good>`
環境變數強制指定一個已知主題（覆寫使用者 config 的 theme 欄位，但 config 檔
其他欄位仍會生效）；或接受「使用者環境會影響顏色」是這次整合的已知取捨（純
plain SPEC 應該不受 theme 顏色影響，只有 ansi 受影響——**此點未逐行查證 theme
是否也影響 plain 模式的文字/符號選擇，標記 NOT VERIFIED**）。

## F11 — 跨平台安裝與 `--inline` 涵蓋範圍：僅能驗證 Windows，其餘平台為文件層級 [DOC-ONLY, 除版本號外皆未在本機驗證]

- 安裝管道（來源：README fetch，`github.com/RivoLink/leaf`）：
  - macOS/Linux/Android(Termux)：shell script (`install.sh`)、Homebrew
    (`leaf-markdown-viewer`)、Cargo (`leaf-markdown-viewer`)、npm
    (`@rivolink/leaf`)。
  - Windows：PowerShell installer、Scoop (`leaf-markdown-viewer`)、Cargo
    (`leaf-markdown-viewer`)、npm (`@rivolink/leaf`)。
  - ArchLinux：AUR (`leaf-markdown-viewer-bin`)。
- 執行檔命名：Unix 系統為 `leaf`，Windows 為 `leaf.exe`（README troubleshooting
  段落 + 本機實測 `leaf.exe` 檔名一致）。
- **`--inline` 涵蓋版本**：本機安裝版本剛好就是 1.21.0——查 CHANGELOG
  （見 F12）確認 `--inline` 是在 1.21.0（2026-05-09）才新增的功能。**所有比
  1.21.0 舊的既有安裝（例如使用者半年前用某個安裝管道裝的 leaf）都不會有
  `--inline` 旗標**——這是一個真實的相容性風險：`llm-wikis doctor` 檢查
  「leaf 是否可啟動」若只測 `leaf --version` 成功就判定可用，不足以保證
  `--inline` 存在，應該額外檢查版本號 ≥ 1.21.0 或直接嘗試呼叫
  `--inline plain` 探測。
- 未能驗證：macOS Apple Silicon／Linux 上以上任一安裝管道裝出來的實際二進位
  是否 100% 具備 `--inline` 且行為與本機 Windows 版一致（本次研究環境只有
  Windows，無法交叉驗證）。**[NOT VERIFIED — 平台限制]**。

## F12 — 版本/維護度訊號：比前次研究（08-08）顯著更新，`--inline` 是近 3 個月內才加入的新功能 [SOURCE, GitHub API + CHANGELOG.md 皆為 2026-08-12 查證]

- GitHub API (`api.github.com/repos/RivoLink/leaf`)：
  `stargazers_count: 1813`、`forks_count: 80`、`open_issues_count: 7`、
  `archived: false`、`language: Rust`、`created_at: 2026-04-06`、
  `pushed_at: 2026-08-11T19:40:12Z`（前一天，非常活躍）。這比前次研究
  （08-08 archive，`research/markdown-rendering.md:10`）記錄的「star 數、
  發布頻率、CI 狀態未查證（Not Found）」有明確進展——星數與活躍度指標現在都
  查得到，且顯示是個小而活躍、非停滯的專案。
- CHANGELOG.md（`raw.githubusercontent.com/RivoLink/leaf/main/CHANGELOG.md`）：
  專案從 2026-04-07 的 1.0.0 到目前最新 1.27.0（2026-08-05）平均約每
  5–14 天發一版，屬於高頻率發版；`--inline` 是在 1.21.0（2026-05-09）
  才加入的功能，距今（2026-08-12）約 3 個月，**本機安裝的版本剛好停在
  --inline 剛推出的那一版**，比目前最新版落後 6 個 minor release
  （1.21.0 → 1.27.0）。
- 仍然成立的疑慮（沿用前次研究結論）：單一組織 (RivoLink) 主導、
  `subscribers_count: 5`（watcher 數低，維護者社群窄）、專案本身歷史僅約
  4 個月（`created_at: 2026-04-06`），`--inline` 功能本身資歷更短（3 個月）。
  這代表功能「存在且實測可用」，但作為「取代已上線的 termimad 內建渲染」的
  唯一終端渲染路徑，其上游穩定性歷史仍短，未來 API/SPEC 語法變動風險不可視為
  零（例如目前 `--inline` 的 `[SPEC]` 語法在 changelog 中曾有多次
  1.2x.x patch，代表這塊仍在被積極調整）。

## Open risks for implementation

1. **stdout 必須是繼承的 real handle，不能被 llm-wikis 自己 capture 再轉印**——
   否則 leaf 子行程永遠看到非 tty，寬度固定退化成 80、且省略 SPEC 時的
   auto-ansi 判斷也會失效（F6）。這對「先取得 leaf 輸出、決定要不要 fallback
   才印出」的錯誤處理設計是結構性張力：若要在 leaf exit code 非 0 時安全
   fallback（issue 要求「避免 leaf 已輸出部分內容後又完整 fallback，造成內容
   重複」），通常需要先 capture 到 buffer 再一次性判斷成功/失敗才印出——但
   capture 又會讓 leaf 看到非 tty。需要在 planner 階段明確決定：要嘛接受
   「leaf 直接繼承 stdout（拿不到 buffer，無法保證不重複）」，要嘛接受
   「capture＋事後決定，但寬度/ansi 永遠走非 tty 分支，此時 llm-wikis 端必須
   自己顯式傳 `ansi:<實際終端寬度>` SPEC，不能依賴 leaf 的 auto 偵測」。
2. **NO_COLOR／`--plain` 覆寫邏輯必須留在 llm-wikis 呼叫端**，因為 leaf 完全
   不讀 NO_COLOR（F3、F7）——沿用現有 `src/cli.rs` 的 TTY/NO_COLOR 判斷模式，
   決定要不要傳 `ansi` 給 leaf，不能假設「反正 leaf 會自己處理」。
3. **使用者自訂 leaf theme/config 沒有可關閉的旗標**（F10），輸出色彩在不同
   使用者機器上可能不一致；若需要確定性輸出，只能用 `LEAF_THEME` 環境變數
   強制指定已知主題，且尚未查證這是否能完全覆蓋所有 config 欄位的影響。
4. **`--inline` 只在 1.21.0（2026-05-09）以後才存在**（F11），`llm-wikis
   doctor` 的可用性檢查若只測「leaf 可執行」不夠，需要額外測 `--inline`
   本身可用（例如探測呼叫），否則舊版 leaf 使用者會在真正 query 時才發現
   fallback。
5. SPEC 打錯字串會被誤判成檔名並嘗試讀檔（F2 的 `--inline foo` 案例）——
   組裝 SPEC 字串的程式碼必須嚴格限制在 `ansi`/`plain`/`ansi:<u32>`/
   `plain:<u32>`/空字串五種形式，不可讓任何動態內容流入 SPEC 位置。
6. 8 MB stdin 上限（F5）理論存在但風險低，僅記錄、不建議特別處理。
7. 本研究只在單一 Windows 機器、單一 leaf 版本（1.21.0）上實測；macOS/Linux
   上的 `--inline` 行為（尤其真 tty 下的 auto-ansi 分支、寬度偵測）完全依賴
   對原始碼的靜態推論，未做交叉平台驗證（F11 已標註 NOT VERIFIED）。

## Load-bearing claims

1. `leaf --inline` 能透過 stdin 讀取 markdown 並正常渲染、退出碼 0、stderr 為
   空，且省略 SPEC 時會依「stdout 是否為真 tty」自動選 ansi/plain（非 tty
   時固定退化為 plain、寬度固定 80）——證據：本機實測 F1/F3（`diff` 逐 byte
   比對 `--inline` 與 `--inline plain` 輸出相同）＋原始碼
   `src/inline.rs::resolve_format`／`render_width` 與 `src/main.rs` 呼叫處
   （皆經 `raw.githubusercontent.com/RivoLink/leaf/main/src/{inline,main}.rs`
   抓取確認）。
2. leaf **不讀取 `NO_COLOR`**——明確傳 `ansi` 時 `NO_COLOR=1` 不會讓輸出退化
   成 plain（本機實測仍有 29 個 ESC byte），且原始碼 `resolve_format` 函式內
   沒有任何 NO_COLOR 相關程式碼；因此 no-color.org 合規性必須由 llm-wikis
   呼叫端自行決定要不要傳 `ansi` SPEC，不能委託給 leaf。
3. leaf 會在啟動時載入使用者 config/theme 檔（`src/main.rs` 呼叫
   `config::load_config` 早於 `--inline` 處理），且**沒有任何旗標可以跳過**
   （比對 `--help` 全部選項列表，沒有 `--ignore-user-config` 或同義旗標）；
   本機目前雖無此設定檔（`find` 掃描確認），但這代表輸出在不同使用者機器上
   的顏色/主題不保證一致，是這次「用 leaf 取代 termimad」相對 termimad（純
   函式庫呼叫、無外部狀態）新增的不確定性來源。
