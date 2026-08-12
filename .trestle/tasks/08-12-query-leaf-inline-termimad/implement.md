# Implementation plan — query 終端渲染改用 leaf --inline

順序：設定層 → viewer 模組 → 渲染掛點 → doctor 與詞彙表 → 移除 termimad →
文件 → 全量回歸。termimad 的移除刻意排在 viewer 上線**之後**，讓任何一步失敗
都還有可用的渲染路徑，build 不會長時間處於半殘狀態。每步先寫失敗測試再實作，
每步結束跑一次 Verification Plan。

## Step 0 — baseline

跑一次 Verification Plan 確認分支起點是綠的。失敗即為既有狀況，停下回報，
不歸因本任務。已知例外見 AGENTS.md 的兩個 deadline-race 測試。

**Rollback point**：尚未修改任何檔案。

## Step 1 — `[viewer]` 設定層

Design 參照：§1、prd D12。

1. `tests/config_contract.rs`：`[viewer]` 缺席時 backend 預設為 leaf；
   `backend = "plain"`/`"leaf"` 皆接受；非法 backend 值、非法 executable
   一律 `CONFIG_INVALID`；`fallback` 這個鍵不存在（`deny_unknown_fields` 擋下）。
2. `src/config.rs`：新增 `ViewerConfig { backend: ViewerBackend, executable:
   Option<String> }` 與 `enum ViewerBackend { Plain, Leaf }`（serde
   `rename_all = "snake_case"`，`Default` 為 `Leaf`）；`Config` 加
   `#[serde(default)] pub viewer: ViewerConfig`；驗證沿用 `validate_executable`。

**Rollback point**：`git checkout -- src/config.rs tests/config_contract.rs`。

## Step 2 — `src/viewer.rs`

Design 參照：§1。

1. 新增模組與 `resolve` / `probe` / `render` 三個函式，SPEC 由封閉列舉組出。
2. 全部經 `process::run`，timeout 10s、8 MiB / 64 KiB 上限。
3. 單元測試：SPEC 字串組裝（`plain`、`ansi:120`、寬度下限夾到 20）；
   `resolve` 在找不到執行檔時回 `CLI_NOT_FOUND`。
4. `src/lib.rs` 掛上 `pub mod viewer;`。

**Rollback point**：`git checkout -- src/lib.rs && rm src/viewer.rs`。

## Step 3 — 渲染掛點

Design 參照：§2。此步結束時 leaf 已可用，termimad 仍在（兩者暫時並存）。

1. `tests/cli_contract.rs`：pipe / 重新導向 / `--json` / `--plain` /
   `NO_COLOR` 五條路徑的 stdout 與本任務前逐位元相同且無 ANSI；viewer 失敗時
   stdout 恰好一份完整原始 markdown、警告只在 stderr。
2. `src/cli.rs::emit_query`：改為 §2 的 capture-then-commit 控制流，
   窮舉 §2 的六個失敗分支。

**Rollback point**：`git checkout -- src/cli.rs tests/cli_contract.rs`。

## Step 4 — doctor viewer check 與兩個詞彙表

Design 參照：§3、§4。

1. `src/output.rs`：`DOCTOR_CHECK_NAMES` 末端加 `viewer`；`WrapperWarningCode`
   加 `VIEWER_UNAVAILABLE`（含 `ALL` 陣列與 `as_str`）。
2. `docs/2026-07-28-llm-wikis-external-query-design.md`：line 831 與 line 931
   兩句規範性詞彙表同步。
3. `src/doctor.rs`：整個 run 只探測一次，結果 clone 進每個 pair 的 checks
   末端；狀態依 prd D5。
4. `tests/doctor.rs`：缺 leaf 時該 check 為 warn 且 doctor 退出碼為 0；
   `backend = "plain"` 時為 pass；多 pair 時 leaf 只被探測一次。
5. `cargo test --test spec_drift` 必須通過（漏改任一邊都會被擋）。

**Rollback point**：`git checkout -- src/output.rs src/doctor.rs docs/ tests/`。

## Step 5 — 移除 termimad

Design 參照：§1、prd D1/AC7。

1. `Cargo.toml`：移除 `termimad`；`console` 由傳遞依賴改列為直接依賴。
2. `src/output.rs`：移除 `render_markdown_ansi`。
3. `tests/output_contract.rs`：移除該函式的兩個測試（08-08 AC2 的證據，
   隨其實作一併退場）。
4. `cargo tree` 確認 termimad 與 crossterm 都不再是直接依賴。

**Rollback point**：`git checkout -- Cargo.toml Cargo.lock src/output.rs tests/output_contract.rs`。

## Step 6 — 文件

Design 參照：§5。逐檔更新後跑 `cargo test --test config_init --test cli_contract
--test config_contract`（INIT_TEMPLATE 有逐字比對測試）。

**Rollback point**：`git checkout -- README.md config.example.toml docs/ src/config.rs`。

## Step 7 — 全量回歸

跑完整 Verification Plan，證據寫進 `evidence/round-1.md`；gate 結果由
`gates.ts run 08-12-query-leaf-inline-termimad --round 1` 產出。
