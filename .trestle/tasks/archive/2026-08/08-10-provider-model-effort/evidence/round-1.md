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

`gates.ts run 08-10-provider-model-effort --round 1`（plugin 重新載入後執行）：
**verdict: pass，30 個 gate 全數通過，0 失敗**，結果在 `evidence/gates-round-1.json`。
含 G4 `cargo test --all-targets --all-features -- --test-threads=1` 完整跑完、
未略過任何測試。

歷程（保留，因為 AGENTS.md 的新規則就是從這裡蒸餾出來的）：

- 本檔最初以手寫方式記錄一次 `PASS_WITH_ENVIRONMENT_EXCEPTION`，因為當時
  `grandchild_termination_kills_both_pids` 與 `windows_job_object` 失敗，
  該次 process_supervisor binary 花了 846 秒（同一 session 稍早只要約 10 秒）。
  該手寫檔被 gates.ts 輪替保存為 `evidence/gates-round-1.1.json`。
- 判定為環境問題的依據是實測而非推論：`git stash` 回到乾淨 base commit
  `d2ad525`（不含本任務任何變更）單獨執行那兩個測試，同樣失敗，且僅這兩個
  測試就花了 530 秒。
- 第一次 gates.ts 正式執行（`evidence/gates-round-1.2.json`）verdict 為 block，
  唯一失敗項是 `verify-declared-empty` —— prd.md 的 `## Verification Plan`
  用了編號清單而非 fenced code block，gates.ts 因此讀不到任何指令。屬本任務
  規劃文件的格式缺陷，與實作無關；修正格式後重跑即通過。
- 最後這次執行時主機延遲已恢復，G4 全量測試（含上述兩個 deadline-race 測試）
  自然通過，因此本輪不需要任何 waive 或 attest。

手動 gate G20（未設定時 argv 逐位元不變）與 G21（probe argv 不含 model/effort）
由 gates.ts 列為 manual，人工核實方式與結論記錄於此：

- G20：`git diff d2ad525 -- tests/claude_adapter.rs tests/codex_adapter.rs`
  未刪除任何 expected 向量行，兩個原有 exact_argv 測試只有呼叫端補上
  `None, None`。
- G21：兩個 adapter 的 `probe_request` 呼叫端未變動，仍各自傳入字面 args
  向量，未改走 `build_argv`。

另有一次獨立驗證（Sonnet subagent，2026-08-10）覆核 issue #7 全部九項驗收條件
皆 CONFIRMED、無 findings，並以 `--strict-config` 對實際安裝的
`codex-cli 0.147.0` 正面驗證 `model_reasoning_effort` 是被該版本承認的設定鍵
（對照組亂打的 key 會被拒絕）。
