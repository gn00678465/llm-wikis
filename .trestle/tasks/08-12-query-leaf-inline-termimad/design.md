# Design — query 終端渲染改用 leaf --inline

Branch：自 `feat/pre-0.1.0-cli-refinements` 開新分支。

## 0. 必須存活的跨切面不變式

- **非渲染路徑逐位元不變**：pipe、重新導向、`--json`、`--plain`、`NO_COLOR`
  五種情況的 stdout 與本任務前完全相同，且不含任何 ANSI escape。這五條路徑
  結構上不 spawn leaf。
- **stdout 永遠恰好一份完整內容**：viewer 失敗時不得出現半截 leaf 輸出加一份
  fallback 的重複內容（issue #8 明列的禁止情境）。由 §2 的 capture-then-commit
  控制流保證。
- **viewer 失敗不得重跑 provider query**：viewer 發生在 query 成功之後，
  `emit_query` 拿到的是已經完成的 `QueryEnvelope`，結構上不存在重跑路徑。
- **不新增 `ErrorCode`**：viewer 的所有失敗都是 warn，不進 §14 錯誤碼表。
  新增的是一個 wrapper warning code（§13）與一個 check name（§15）。

## 1. 模組邊界：`src/viewer.rs`

cli.rs（渲染）與 doctor.rs（檢查）**共用同一份實作**，不各寫一份探測邏輯。

```rust
// src/viewer.rs
pub struct Viewer { exe: ResolvedExecutable }

/// 解析 [viewer].executable（或平台預設裸命令 leaf/leaf.exe）。
/// 直接重用 process::resolve_executable —— issue #6 剛修好的 Windows PATHEXT
/// 語意讓裸命令 `leaf` 正確解析到 leaf.exe。
pub fn resolve(cfg: &ViewerConfig, env: &dyn EnvLookup) -> Result<Viewer, AppError>;

/// doctor 用：以固定的極小 markdown 實際跑一次 `--inline plain`，確認這個
/// 二進位真的支援 --inline（Findings F7：1.21.0 以前跑得起來但沒有這個旗標，
/// 只測 --version 會漏判）。
pub fn probe(&self) -> Result<String, AppError>;   // Ok(version 字串)

/// query 用：capture 模式渲染。永不繼承 stdout。
pub fn render(&self, markdown: &str, width: u16) -> Result<String, AppError>;
```

`render`/`probe` 都經 `process::run`（`ProcessRequest`），取得 stdin 寫入、
bounded 輸出、timeout 與 process tree 終止；不寫裸 `Command`。
timeout 取 10s、max_stdout 8 MiB（對齊 leaf 自己的 stdin 上限）、max_stderr 64 KiB。

### SPEC 組裝

只由封閉列舉組出，任何動態內容都不得流入 SPEC 位置（Findings F6：打錯的字串
會被 leaf 當檔名開，錯誤訊息誤導成「檔案不存在」）：

```rust
enum InlineSpec { Plain, Ansi(u16) }   // → "plain" / "ansi:<n>"
```

寬度由 `console::Term::stdout().size()` 取得（`.1` 是 cols），下限夾到 20
（leaf 的 `LEAF_WIDTH` 最小值）。`console` 目前是 indicatif 的傳遞依賴，改列為
直接依賴；termimad 與其帶入的 crossterm 一併移除。

## 2. 渲染控制流（`src/cli.rs::emit_query`）

判斷順序與現況相同，只把「呼叫 termimad」換成「spawn leaf」：

```
if backend != Leaf || !stdout.is_terminal() || plain || NO_COLOR 有值:
        print!("{human}")                     ← 與本任務前逐位元相同
else:
        match viewer::resolve(...).and_then(|v| v.render(&human, width)):
            Ok(rendered) if !rendered.is_empty() => print!("{rendered}")
            Ok(_) | Err(e) => { eprintln!(warning); print!("{human}") }
```

**capture-then-commit**：在 `render` 回傳 `Ok` 之前，一個 byte 都不會寫到
stdout。這就是「安全 fallback」與「leaf 自動偵測」互斥（research 風險 #1）的解法
——放棄 leaf 的自動偵測，改由 llm-wikis 顯式傳 `ansi:<width>`，兩者同時成立。

### 失敗分支窮舉（對應 research F4）

| 分支 | 偵測方式 | 結果 |
|---|---|---|
| 找不到執行檔 | `resolve_executable` → `CLI_NOT_FOUND` | warn + 原始 |
| 啟動失敗 | `process::run` → `Err` | warn + 原始 |
| 非零退出碼（空 stdin、非法 UTF-8、未知 theme、被當檔名的錯 SPEC） | `exit_code != Some(0)` | warn + 原始 |
| timeout / 輸出超限 | `termination != Completed` | warn + 原始 |
| stdout 非合法 UTF-8 | `String::from_utf8` 失敗 | warn + 原始 |
| stdout 為空 | `rendered.is_empty()` | warn + 原始 |

警告一律寫 stderr、單行、不含 leaf 的原始 stderr 全文（只帶簡短原因）。

## 3. doctor 的 viewer check

viewer 是**全域**性質，但 `checks[]` 掛在每個 wiki/agent pair 底下。作法：

- **整個 doctor run 只探測一次**（`probe` 只 spawn 一次），結果 clone 進每個
  pair 的 checks 向量。不可每個 pair 各 spawn 一次。
- 位置固定在**每個 pair 的 checks 最後一個**，既有九項的相對位置完全不動，
  既有 `--json` 消費者的索引假設不受影響。
- `DOCTOR_CHECK_NAMES` 把 `viewer` 加在陣列末端（第 10 個），與上述位置一致。

狀態表見 prd.md D5。warn 的 `code` 是新的 `VIEWER_UNAVAILABLE`。

## 4. 兩個封閉詞彙表的同步修訂

| 詞彙表 | 常數 | 規格句 |
|---|---|---|
| doctor check name 9→10 | `src/output.rs:14` | design 規格 line 931 |
| wrapper warning code 4→5 | `src/output.rs:64-77` | design 規格 line 831 |

`tests/spec_drift.rs` 對兩者都做雙向核對，改一邊漏另一邊會被擋下——這是保護，
不是障礙。

## 5. 文件

| 檔案 | 變更 |
|---|---|
| `config.example.toml` | 新增 `[viewer]` 區段與說明 |
| `src/config.rs` INIT_TEMPLATE | 同上，註解形式 |
| `README.md` | 渲染段落改寫：leaf 安裝需求、未安裝時的行為 |
| `docs/llm-wikis.md` | `[viewer]` 設定、doctor viewer check、故障排除 |
| design 規格 §13/§15 | 兩句規範性詞彙表 |
