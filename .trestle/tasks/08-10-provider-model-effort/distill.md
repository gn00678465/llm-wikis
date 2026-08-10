# Distill — provider model 與 reasoning effort

## 值得沉澱到 AGENTS.md 的

- **Codex adapter 的 `--ignore-user-config` 是設計上的單一通道約束**：任何要影響
  Codex 行為的設定（model、reasoning effort、未來的其他 config key）都必須由
  llm-wikis registry 明確轉成 invocation argv，operator 自己的 Codex 設定檔一律
  讀不到。新增這類設定時不要假設「使用者可以自己在 Codex 設定」。
- **argv value 內含雙引號的 Windows batch shim 路徑已實測安全**：
  `-c model_reasoning_effort="high"` 這種形狀經 `.cmd` shim spawn 後逐字送達
  （`tests/process_supervisor.rs::batch_shim_preserves_quoted_config_override_argument`）。
  既有的 metacharacter 測試只涵蓋 `& | ^ %VAR% !DELAYED!`，不含引號 —— 引號是
  Windows argv 編碼自己的分隔符，值得單獨釘住。
- **`tests/process_supervisor.rs` 兩個 deadline-race 測試的判定流程**：AGENTS.md
  已記錄「只有 src/process.rs 或該檔案有改才算回歸」。本任務補上實務作法：該檔
  有改（新增獨立測試）時，用 `git stash` 回到 base commit 單獨跑那兩個測試比對，
  比推論可靠。本次比對顯示同一 session 內同一 binary 可以從 10 秒退化到 846 秒。

## 已在本任務文件內處理、不需外擴的

- model/effort 驗證規則、套用範圍、fingerprint 語意：已寫入 `docs/llm-wikis.md`
  與 design 規格 §6.1/§15.1，屬產品文件而非 agent 工作慣例。
- `PROVIDER_CONTRACT_VERSION` 的升版時機：常數自身的 doc comment 已規定。

## ARCHITECTURE.md / PRODUCT.md

無需更新：本任務沒有新增元件、沒有改變模組邊界，也沒有改變產品定位——
只是既有 provider 宣告多了兩個 optional 欄位。
