# 研究：`llm-wikis query` 的終端機 Markdown 渲染

## 1. 「leaf」真的存在嗎

存在。`leaf` 是 [RivoLink/leaf](https://github.com/RivoLink/leaf)（[README](https://github.com/RivoLink/leaf/blob/main/README.md)），一個「terminal Markdown previewer — GUI-like experience」。

- 支援平台：macOS / Linux / Android(Termux) / Windows，安裝管道包含 shell script、PowerShell script、npm、Homebrew、Cargo、Scoop、AUR（來源：README fetch 摘要，2026-08-08）。
- 支援 pipe 讀取：README 範例含 `cat TESTING.md | leaf` 與 `claude "explain Rust lifetimes" | leaf` —— 與本任務情境（AI agent 輸出接 leaf）幾乎一致。
- 功能較重：TOC 側欄、mermaid 圖表、LaTeX 公式、watch mode、fuzzy picker，是互動式全螢幕 previewer，不是單純「印出渲染後文字就結束」的工具。
- 成熟度訊號薄弱：README 未列版本歷史 / release date；目前唯二可查證的第三方報導是 2026 年的 [Hacker News「Show HN」貼文](https://news.ycombinator.com/item?id=47888423) 與另一篇 [HN 討論](https://news.ycombinator.com/item?id=48400742)、[dev.to 介紹文](https://dev.to/rivolink/leaf-a-terminal-markdown-previewer-1ncj)——即作者本人的發表串，尚未看到獨立第三方的長期維護或社群採用證據。star 數、發布頻率、CI 狀態未查證（Not Found，WebFetch 對 GitHub repo 首頁的抓取未回傳這些欄位）。

結論：使用者說的「leaf」不是記錯名字，是真實工具，但屬於新興、單一作者維護、互動式全螢幕工具，跟本任務要的「pipe 過去印一段渲染文字就結束」的批次用途不完全對齊，且沒有作為函式庫被其他 Rust 專案內嵌的先例（它是一個獨立可執行檔）。

## 2. 外部工具管線方案（pipe 給外部 renderer）

比較三個成熟候選（皆可作為外部二進位被偵測 + pipe）：

| 工具 | 語言/生態 | 跨平台 | 是否維護中 | 備註 |
|---|---|---|---|---|
| [glow](https://github.com/charmbracelet/glow)（charmbracelet） | Go | macOS/Linux/Windows/FreeBSD/OpenBSD，choco/scoop/winget/brew 皆可裝 | 是（Charm 生態核心專案，社群大） | 支援 `cmd | glow -` pipe（[README](https://github.com/charmbracelet/glow/blob/master/README.md)），`-p` 用 `$PAGER`（預設 `less -r`）分頁，`-s dark|light|<json>` 選樣式，`-w` 控制寬度；README 未提 `NO_COLOR`，但會自動偵測終端背景色決定 dark/light |
| [mdcat](https://github.com/swsnr/mdcat) | Rust | 有 GitHub Actions 產出的跨平台 release binaries，`cargo install mdcat` 可裝 | **原始 repo 已於 2026-06-19 封存（archived），維護者已改指向 fork [BIRSAx2/mdcat](https://github.com/BIRSAx2/mdcat) 繼續發布** | crates.io 上 `mdcat` crate 最新版 2.15.0，`updated_at` 2026-08-03（比 archive 日期新，代表新維護者仍在發布），可連結為 `mdless` 走分頁模式 |
| [leaf](https://github.com/RivoLink/leaf) | 疑似 Rust（README 有 `cargo install`，未在頁面明確標示語言） | macOS/Linux/Android/Windows | 見上節：不確定（新專案） | 互動式，功能最重 |
| bat | Rust | 三平台 | 是（成熟） | `bat` 本質是語法高亮 `cat`，對 Markdown 只做 code-fence 內語法上色，不解析 heading/bold/table 等 Markdown 語意結構——不是完整的 Markdown renderer，只能算「半調子」方案 |

外部工具管線的通用作法：CLI 在啟動時依序 `which`/`where` 偵測 PATH 上是否有候選二進位（例如順序 `glow` → `mdcat` → `bat`），找到就把 `render_human` 產生的 markdown 字串經 stdin pipe 給它、把它的 stdout 轉印出來；找不到任何一個就 fallback 印原始 markdown 純文字。

**這條路徑對本專案的結構性缺點**：
- llm-wikis 本身的定位是「單一可執行檔、免安裝額外執行環境」（`src/cli.rs` 目前所有子命令都是純 Rust 內部邏輯，doctor/init 等指令沒有 shell-out 到第三方 CLI 的先例——`Grep` 全庫搜尋 `Command::new` 主要出現在 `src/process.rs`／agent 執行路徑，屬於呼叫 Claude/Codex agent 這個「本質上就要外部程序」的場景，不是為了格式化輸出而 shell-out）。若渲染 markdown 要求使用者「另外裝 glow」，等於把一個純展示需求變成一個新的外部相依安裝步驟，且三個候選都不保證預裝在乾淨 Windows/macOS/Linux 機器上。
- Windows 上二進位偵測要處理 `.exe` 副檔名與 PATHEXT，且 mdcat 目前正處於維護權轉移期（archived → fork），穩定性訊號較弱。

## 3. 內建渲染方案（Rust crate 直接渲染）

### 候選 A：`termimad`（推薦）

- crates.io API（`GET /api/v1/crates/termimad`，2026-08-08 查證）：最新版 `0.35.1`，`updated_at = 2026-07-11`——近一個月內有發布，維護活躍。
- 定位精準：官方描述就是「A library to display rich (Markdown) snippets and texts in a rust terminal application」（[Canop/termimad](https://github.com/Canop/termimad)），正是「把 markdown 字串轉成終端 ANSI 輸出」這個單一需求，不是完整互動式 pager。
- 依賴：`crossterm`（跨平台終端操作，官方文件稱支援到 Windows 7）、`minimad`（輕量 markdown parser）、`lazy_static`——三者皆為成熟、廣泛使用的 crate（來源：WebSearch 摘要引用 Canop/termimad README/docs.rs）。
- 對現有依賴樹的影響：查證 `Cargo.lock`，目前 repo 完全沒有 `crossterm`／`minimad`／`termimad`（`Grep` 搜尋 `Cargo.lock` 中 `^name = "(console|crossterm|termimad|...)"` 只命中 `console`），代表加入 termimad 會新增一條獨立的終端後端（crossterm），跟現有 `indicatif`（走 `console` crate，見 `Cargo.lock:338-348` `indicatif` 的 `dependencies = ["console", "number_prefix", "portable-atomic", "unicode-width", "web-time"]`）並存、互不衝突，但代表終端能力上會有兩套抽象（console + crossterm）同時存在於依賴樹——這是唯一的技術代價，不影響功能正確性。

### 候選 B：`mdcat` / `mdcat-ng` as library

- `mdcat` crate 的核心其實已抽成獨立庫（曾發佈為 `pulldown-cmark-mdcat`），主要 API 是 `push_tty` / `process_file`（來源：WebSearch 摘要，[docs.rs/mdcat-ng](https://docs.rs/mdcat-ng/latest/mdcat/)）。
- 缺點：功能設計以「渲染一整個檔案到 tty」為核心心智模型（含 iTerm2/Kitty/Sixel inline image protocol 偵測），對本任務「渲染一段已經在記憶體裡的 answer 字串」用途偏重；且原始 repo 剛封存、由社群 fork 接手（見第 2 節），版本延續性與 crate 命名權（`mdcat` vs `mdcat-ng`）在 2026-08 這個時間點仍在變動中，屬於維護狀態不穩定的訊號。

### 候選 C：`bat` as library

- `bat` 主要以二進位發佈為主，官方不鼓勵把它當函式庫嵌入其他工具（Not Found：本次未查到官方文件明確表態，但其 crates.io 描述與生態使用方式皆以 CLI 為主）；且如第 2 節所述，bat 對 Markdown 語意結構（heading/table/blockquote）不做渲染，只做 code fence 語法上色，不滿足使用者要的「標題、粗體」渲染需求。予以排除。

### 候選 D：`comrak` + 手工 ANSI

- `comrak` 目前不在 `Cargo.toml`（已讀取全文確認），採用此路徑等於要自己寫一個 CommonMark AST → ANSI 序列的渲染器，是「重新發明 termimad 已經做好的事」，違反用最少程式碼解決問題的原則，排除。

**推薦結論：內建方案，用 `termimad` crate。** 理由：功能與需求精準對齊（headings/bold/code block 上色）、維護活躍（1 個月內有發布）、依賴輕（3 個成熟 crate）、不需要使用者額外安裝任何外部二進位（跟 llm-wikis 現有「單一可執行檔」的產品定位一致）、Windows 相容（crossterm 官方支援到 Win7，也是本 repo 主要驗證平台）。外部工具方案（glow/leaf/mdcat）在功能上更豐富，但都要求終端使用者另外安裝，且 mdcat 正處於維護權轉移期、leaf 尚無獨立維護訊號，都不適合當作「找不到就退化」以外的預設路徑。

## 4. 行為設計：何時渲染、旗標、NO_COLOR

先例調查：
- **glow**：無旗標時自動偵測終端背景色套用 dark/light style；透過 `-p` 走 `$PAGER`；透過 `-s` 指定/停用樣式；README 未明確提 `NO_COLOR`（Not Found：glow 原始碼層級的 NO_COLOR 支援未逐一查證，僅查證到 README 沒提及）。
- **bat**：業界標準做法是 `--color=auto|always|never`，`auto` 依 `isatty(stdout)` 判斷（廣泛認知的慣例，非本次逐行查證 bat 原始碼所得，列為業界共識而非本 repo 專屬證據）。
- **NO_COLOR**：[no-color.org](https://no-color.org/) 是被 glow/bat/多數現代 CLI 遵循的社群慣例——只要環境變數 `NO_COLOR` 有任何非空值，程式就應停用色彩/樣式輸出，改印純文字。

對應到 llm-wikis 既有程式碼慣例（**已找到可直接沿用的 TTY 偵測 pattern，且是 stdlib、零新依賴**）：
- `src/cli.rs:698` `resolve_question_bytes` 用 `std::io::stdin().is_terminal()` 判斷 stdin 是否為互動終端。
- `src/cli.rs:828` `start_query_spinner` 用 `std::io::stderr().is_terminal()` 判斷是否顯示 spinner，且明確註解「a pipe, a redirect, or CI never satisfies this」，確保自動化測試不會被互動邏輯污染 stdout/stderr 的 byte-for-byte 比對（`src/cli.rs:820-826` 的 doc comment）。
- 這兩處用的都是 Rust 1.70+ 內建的 `std::io::IsTerminal` trait，**不需要任何新 crate**——`std::io::stdout().is_terminal()` 一樣可直接呼叫，用同一套 pattern 判斷 stdout 是否為互動終端。

草擬行為規格（僅供 planner 參考，未落地成程式碼）：
1. 渲染只發生在人類模式（`--json` 完全不受影響——`emit_query` 的 `if json { ... }` 分支，`src/cli.rs:660-661`，優先於任何渲染判斷，維持現狀）。
2. 預設值：`stdout.is_terminal() == true` 時才嘗試渲染；stdout 被 pipe/redirect 時預設印原始 markdown（不渲染），呼應現有 `stderr().is_terminal()` 的既有慣例與 `resolve_question_bytes` 對「互動 vs 非互動」的一致區分。
3. `NO_COLOR` 環境變數有值時，即使是 TTY 也不渲染（純文字），依循 no-color.org 慣例。
4. 提供顯式旗標覆寫自動偵測，供腳本/使用者強制指定，例如 `--no-render`（強制純文字，即使是 TTY，方便複製貼上原文）與 `--render`（強制渲染，即使被 pipe，例如接 `less -R`）；確切旗標命名與是否納入 v1 屬於 planner 決策範圍，本研究只確認「需要一個顯式覆寫旗標」這個需求存在（先例：glow 的 `-p`/`-s`、bat 的 `--color=always|never|auto` 都提供顯式覆寫，不是純靠自動偵測）。

## 5. 現有程式碼：渲染要掛在哪裡

- `src/cli.rs:654-672` `emit_query(json: bool, envelope: QueryEnvelope) -> i32`：目前三分支——`json` 印 JSON envelope（661）；`error` 存在時印錯誤行到 stderr（667，呼叫 `eprint_error_line`）；否則印 `render_human(&envelope)`（669）。**渲染要接在第三分支**：`else { print!("{}", render_human(&envelope)); }` 這行是目前唯一印出 markdown 原文到 stdout 的位置，改動範圍應僅限這裡，把 `render_human` 的輸出結果視情況（TTY + 非 NO_COLOR + 未帶 `--no-render`）餵給 termimad 轉成 ANSI 字串後再印出，否則原樣印出。
- `src/output.rs:172-191` `render_human`：純字串組裝函式（answer → gaps → warnings），**沒有任何 I/O**，其 doc comment（`src/output.rs:160-171`）已明確界定「stream-routing 決策不是這個函式的責任，是 cli.rs 的責任」——這代表 markdown→ANSI 的渲染邏輯也不應該塞進 `render_human`，應該和 `eprint_error_line`／JSON 分支一樣，留在 `cli.rs::emit_query` 這一層做選擇，維持 `output.rs` 對「格式化 vs 串流路由」既有的職責切分慣例。

## Load-bearing claims

1. `leaf`（RivoLink/leaf）確實存在、跨平台、支援 pipe，但是互動式 previewer、單一作者、無獨立維護訊號可查——證據：[github.com/RivoLink/leaf README](https://github.com/RivoLink/leaf/blob/main/README.md) fetch 摘要 + 僅查到作者自己的 [HN Show HN 貼文](https://news.ycombinator.com/item?id=47888423)。
2. `termimad` 是本任務推薦的內建方案：功能對齊、近期仍在發布（0.35.1 / 2026-07-11）、且此依賴樹目前完全不含 crossterm/minimad——證據：`https://crates.io/api/v1/crates/termimad` API 回應 + `D:\Projects\llm-wikis\Cargo.lock` 對 `console`/`indicatif` 的 grep 結果（`Cargo.lock:338-348`），對照 `Cargo.toml:14-23` 現有相依清單裡沒有 crossterm 系列。
3. 渲染邏輯該掛在 `src/cli.rs:654-672` 的 `emit_query` 第三分支（`src/cli.rs:669`），且應沿用該檔案已有的 `std::io::IsTerminal` 慣例（`src/cli.rs:698`、`src/cli.rs:828`）做 TTY 偵測，不需要新增偵測用的 crate；`render_human`（`src/output.rs:172-191`）維持純格式化函式、不擔任串流路由角色，證據見其 doc comment `src/output.rs:160-171`。
