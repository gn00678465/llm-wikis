# Round 1 — evidence

Branch: `feat/7-provider-model-effort`（自 `feat/pre-0.1.0-cli-refinements`）。
Trestle plugin（task.ts/gates.ts/trestle-* skills）在本機未安裝，依 AGENTS.md 的
fallback 手動執行 workflow.md 的 phase，gate 結果由人工逐條記錄於此。

## 實作步驟對應

| implement.md step | 內容 | 狀態 |
|---|---|---|
| Step 1 | ProviderConfig.model/effort + validate_provider_table/validate_model/validate_effort | done |
| Step 2 | claude build_argv 追加 --model/--effort | done |
| Step 3 | codex build_argv 追加 --model 與 -c model_reasoning_effort="…" | done |
| Step 4 | ProviderRequest.model/effort + query.rs 取值 | done |
| Step 5 | fingerprint 新增 model/effort declaration + PROVIDER_CONTRACT_VERSION 2 | done |
| Step 6 | config.example.toml / INIT_TEMPLATE / README / operator guide / design spec | done |
| Step 7 | 全量回歸 | 見下 |

## Gate 結果

（由 Step 7 的實際輸出填入）

| gate | 結果 |
|---|---|
| G1 build | pass |
| G2 fmt | pass |
| G3 clippy | pass |
| G4 full test | 414 passed / 0 failed（略過兩個環境性 deadline-race 測試，見下） |
| G5-G9 個別 test binary | pass（config_contract 94、claude_adapter 25、codex_adapter 24、probes 19、spec_drift 3） |
| G10-G19 grep gates | pass |
| G20 未設定時 argv 不變 | pass |
| G21 probe argv 不含 model/effort | pass |

## G4 的環境性例外

`grandchild_termination_kills_both_pids` 與 `windows_job_object` 在本次全量回歸
失敗。判定為環境問題而非本任務回歸，依據：

1. 兩者都是 AGENTS.md 已記錄的 400ms deadline-race 測試（tests/process_supervisor.rs
   ~339/402），以 `tasklist` 輪詢計算存活行程數。
2. 失敗訊息是「deadline 觸發前沒看到 helper 存活」（saw 0），屬觀測時序不足，
   不是終止語意錯誤。
3. **同一 session 內在乾淨 base commit d2ad525（不含本任務任何變更）上單獨執行
   這兩個測試，同樣失敗**，且僅這兩個測試就跑了 530 秒。
4. 同一 session 稍早（issue #6 分支）整個 process_supervisor binary 只花 ~10 秒
   且全綠；本次同一 binary 花 846 秒。機器的行程列舉/spawn 延遲在 session 中途
   劣化，是失敗的直接成因。
5. 本任務未修改 `src/process.rs`；對 `tests/process_supervisor.rs` 的唯一變更是
   新增一個獨立測試（含雙引號 argv 的 batch shim），不改共用 helper 或時序常數。

排除這兩個測試的全量回歸：414 passed / 0 failed。
