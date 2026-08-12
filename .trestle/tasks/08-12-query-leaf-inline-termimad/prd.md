# query 終端渲染改用 leaf --inline 取代內建 termimad

來源：GitHub issue #8。使用者決定 leaf **取代**（非並存）08-08 任務上線的
內建 termimad 渲染。

## Findings

（leaf 行為的一手實測見 `research/leaf-inline.md`，以下引用其 F 編號）

- F1 現況渲染掛點：`src/cli.rs:843` 的 `emit_query`——stdout 是 TTY 且未帶
  `--plain` 且 `NO_COLOR` 未設時走 `render_markdown_ansi`（`src/output.rs:194-210`，
  termimad），否則印原始 markdown。issue #8 敘述的「目前直接輸出原始 markdown」
  前提對現況已不成立。
- F2 `leaf --inline` 存在且行為符合需求（research F1/F8）：可由 stdin 直接讀取、
  不需檔案參數、不進 alternate screen、不等待按鍵、成功時 stderr 為空、
  單次 spawn 約 70-90ms。
- F3 失敗一律 exit code 1（research F4），未區分使用者錯誤與 IO 錯誤 ——
  fallback 判斷可用 `status.success()` 單一條件。
- F4 leaf **完全不讀 `NO_COLOR`**（research F3/F7，原始碼 `resolve_format` 層級
  確認）。no-color 語意必須留在 llm-wikis 呼叫端。
- F5 leaf 省略 SPEC 時依自己的 `is_stdout_terminal` 決定 ansi/plain，且非 tty 時
  寬度固定退回 80 欄（research F6，原始碼 `render_width` 確認）。但顯式傳
  `ansi:<width>` 可完全繞開這兩個自動判斷。
- F6 SPEC 語法嚴格：只接受 `ansi`/`plain`/`ansi:<N>`/`plain:<N>`；打錯的字串會被
  當成**檔名**去開而非回報格式錯誤（research F2 的 `--inline foo` 案例）。
- F7 `--inline` 是 leaf 1.21.0（2026-05-09）才加入的旗標（research F11/F12）；
  比它舊的既有安裝都沒有這個旗標。doctor 只測「leaf 可執行」不足以保證可用。
- F8 leaf 一律載入使用者的 config/theme，**沒有等效於 Codex `--ignore-user-config`
  的旗標**（research F10）；`LEAF_THEME` 環境變數可覆寫主題，但 config 檔其他
  欄位仍生效。
- F9 leaf 維護度較 08-08 研究時明確改善（research F12）：1813 stars、
  2026-08-11 仍有 push、約 5-14 天一版；但專案僅 4 個月歷史、`--inline` 僅 3 個月、
  watcher 數 5，上游 SPEC 語法仍在調整中。
- F10 寬度來源：移除 termimad 會一併移除 crossterm。`console` crate 已因
  `indicatif`（`Cargo.toml:17`）在依賴樹中，其 `Term::stdout().size()` 可提供
  終端寬度，不需要新增終端後端。
- F11 `render_markdown_ansi` 目前有兩個測試在 `tests/output_contract.rs`
  （08-08 任務 AC2 的證據），移除 termimad 會一併移除它們。

## Decisions

- D1 leaf 取代 termimad（使用者 2026-08-12 選定），不並存。termimad 依賴、
  `render_markdown_ansi`（`src/output.rs:194-210`）與其兩個測試一併移除。
- D2 `backend` 預設 `"leaf"`，所有 viewer 失敗情境一律「stdout 乾淨退回原始
  markdown + stderr 一行簡短警告」（使用者 2026-08-12 選定，覆蓋規劃階段
  建議的「未安裝時靜默」版本）：
  - 找不到可執行檔、啟動失敗、非零退出碼三者都警告，與 issue #8 錯誤處理
    章節字面一致。
  - 警告只寫 stderr，stdout 永遠是完整且唯一的一份內容——不得出現 leaf 已印
    一半又補一份 fallback 的重複輸出（issue #8 明列的禁止情境）。
  - 代價已知並接受：移除 termimad 後，沒裝 leaf 的使用者每次 query 都會看到
    一行 stderr 警告；pipe／`--json`／重新導向的 stdout 不受影響。
- D3 不強制 leaf 主題（使用者 2026-08-12 選定）：spawn 時不設 `LEAF_THEME`，
  使用者自己的 leaf config/theme 生效。理由：operator 既然選了 backend=leaf
  就自己管 leaf 設定，覆寫是越權；且 research F10 未能證實 `LEAF_THEME` 可中和
  config 檔其他欄位，強制也換不到真正的確定性。機器可讀路徑（`--json`、pipe）
  結構上不經過 leaf，輸出契約不受主題影響。`[viewer]` 不新增 theme 欄位
  （issue #8 初版明列不做自訂 Leaf theme）。
- D4 doctor 完整回報 viewer 狀態（使用者 2026-08-12 選定），為此修訂兩個
  規格鎖定的封閉詞彙表，兩者都由 `tests/spec_drift.rs` 雙向核對：
  - `checks[].name` 由 9 個增為 10 個，新增 `viewer`
    （`src/output.rs:14` + design 規格 line 931）。
  - wrapper warning code 由 4 個增為 5 個，新增 `VIEWER_UNAVAILABLE`
    （`src/output.rs:64-77` + design 規格 line 831）——因為規格 §15 規定 warn
    狀態的 `checks[].code` 必須是一個 stable warning code。
- D5 viewer check 的狀態語意：缺 leaf 是 **warn 而非 fail**。對照既有的
  provider `executable` check：缺 provider CLI 時 query 完全無法進行，所以是
  fail；缺 leaf 時 query 照常成功、答案完整輸出，只是未經渲染，因此不得讓
  `doctor` 因為一個純顯示層元件而以非零碼結束（`dominant_exit`，
  `src/error.rs:137`），會打斷以 doctor 當閘門的 CI/腳本。狀態表：
  - `backend = "plain"` → pass（未設定外部 viewer）
  - `backend = "leaf"`、解析成功且 `--inline` 探測成功 → pass（記錄 canonical
    路徑與版本）
  - `backend = "leaf"`、PATH 找不到 → warn `VIEWER_UNAVAILABLE`
  - `backend = "leaf"`、找得到但 `--inline` 探測失敗（1.21.0 以前的舊版） →
    warn `VIEWER_UNAVAILABLE`，同碼不同訊息
  - `backend` 值非法 → 在 config load 階段即 `CONFIG_INVALID`，到不了這裡
- D6 doctor 的 viewer 探測必須實際跑 `--inline`，不能只測 `--version`
  （Findings F7：1.21.0 以前的 leaf 跑得起來但沒有這個旗標）。

- D7 渲染機制：**capture leaf 的 stdout，並顯式傳 `ansi:<實際終端寬度>` SPEC**。
  這是需求推導出來的，不是偏好：issue #8 明文禁止「leaf 已輸出部分內容後又補一份
  fallback」，唯一能保證的方式是先取得完整輸出、確認成功才寫到 stdout；而一旦
  capture，leaf 就看不到真 tty（Findings F5），必須由 llm-wikis 顯式指定格式與
  寬度。research 把這兩件事列為互斥的結構性張力，顯式 SPEC 正是解法。
- D8 寬度來源用 `console::Term::stdout().size()`（Findings F10）。`console` 已因
  `indicatif` 在依賴樹中，改為直接依賴不新增終端後端；移除 termimad 會一併移除
  crossterm。
- D9 觸發條件沿用現有判斷，只把「呼叫 termimad」換成「spawn leaf」：
  `backend == leaf` 且 stdout 是 TTY 且未帶 `--plain` 且 `NO_COLOR` 未設。
  其餘情況（pipe、重新導向、`--json`、`--plain`、`NO_COLOR`）完全不 spawn
  leaf，直接輸出原始 markdown，與現況逐位元相同。NO_COLOR 語意留在 llm-wikis
  這端（Findings F4：leaf 不認得它）。
- D10 SPEC 字串只能由封閉列舉組出（`ansi`/`plain`/`ansi:<u32>`/`plain:<u32>`），
  任何動態內容都不得流入 SPEC 位置——Findings F6：打錯的 SPEC 會被 leaf 當成
  檔名去開，錯誤訊息會誤導成「檔案不存在」。
- D11 spawn 走既有的 `src/process.rs` 的 `run`，不另外寫裸 `Command`：它已提供
  stdin 寫入、bounded 輸出、timeout 與整棵 process tree 終止，且
  `resolve_executable` 剛在 issue #6 修好 Windows PATHEXT 語意，裸命令 `leaf`
  會正確解析到 `leaf.exe`。
- D12 `[viewer]` 只有 `backend` 與 optional `executable` 兩個鍵，**不做 `fallback`
  鍵**：issue #8 建議的 `fallback = "plain"` 只有一個合法值，等於沒有選擇的設定
  面積。fallback 行為本身照做（永遠退回原始 markdown）並寫進文件，issue 的驗收
  條件「Leaf 啟動失敗時可回退至原始 Markdown」由行為滿足，不需要這個鍵。
  這是對 issue 建議設定格式的刻意精簡。

## Acceptance Criteria

- [x] AC1: `[viewer]` 區段支援 `backend`（`plain`/`leaf`，預設 `leaf`）與 optional
  `executable`；非法值在 config load 階段即 `CONFIG_INVALID`。
  (evidence: src/config.rs, tests/config_contract.rs)
- [x] AC2: `backend = "leaf"` 且 stdout 為 TTY、未帶 --plain、NO_COLOR 未設時，
  完整 markdown 經 stdin 交給 leaf，argv 為 --inline 加一個封閉列舉組出的 SPEC。
  (evidence: src/cli.rs, tests/cli_contract.rs)
- [x] AC3: pipe、重新導向、--json、--plain、NO_COLOR 五種情況都不 spawn leaf，
  stdout 與本任務前逐位元相同且不含 ANSI escape。
  (evidence: tests/cli_contract.rs, tests/output_contract.rs)
- [x] AC4: viewer 失敗（找不到執行檔、啟動失敗、非零退出碼）時 stdout 輸出完整
  且唯一一份原始 markdown，警告只寫 stderr，且不重新執行 provider query。
  (evidence: src/cli.rs, tests/cli_contract.rs)
- [x] AC5: doctor 新增 viewer check，狀態依 D5 的表；缺 leaf 為 warn 且 doctor
  退出碼維持 0；探測實際使用 --inline 而非 --version。
  (evidence: src/doctor.rs, tests/doctor.rs)
- [x] AC6: `checks[].name` 與 wrapper warning code 兩個封閉詞彙表同步擴充，
  規格文件與常數一致，spec_drift 雙向核對通過。
  (evidence: src/output.rs, docs/2026-07-28-llm-wikis-external-query-design.md, tests/spec_drift.rs)
- [x] AC7: termimad 依賴、`render_markdown_ansi` 與其測試全部移除，repo 內不再
  有 termimad 或 crossterm 的直接依賴。
  (evidence: Cargo.toml, src/output.rs)
- [x] AC8: README、operator guide、config.example.toml、config init 樣板與規格
  文件同步說明 leaf 安裝需求、設定方式與故障排除。
  (evidence: README.md, docs/llm-wikis.md, config.example.toml, src/config.rs)

## Verification Plan

```
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features -- --test-threads=1
```

## Expected Files

- `src/cli.rs`
- `src/config.rs`
- `src/doctor.rs`
- `src/output.rs`
- `src/viewer.rs`
- `src/lib.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `config.example.toml`
- `docs/llm-wikis.md`
- `docs/2026-07-28-llm-wikis-external-query-design.md`
- `tests/cli_contract.rs`
- `tests/config_contract.rs`
- `tests/doctor.rs`
- `tests/list.rs`
- `tests/query_service.rs`
- `tests/viewer.rs`
- `AGENTS.md`
- `.trestle/workspace/ARCHITECTURE.md`
- `tests/output_contract.rs`
- `tests/spec_drift.rs`
