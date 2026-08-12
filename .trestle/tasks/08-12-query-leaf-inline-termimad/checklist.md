# Checklist — query 終端渲染改用 leaf --inline

## Hard Gates

| id | check | type |
|---|---|---|
| G1 | `cargo build --all-targets --all-features` | gate |
| G2 | `cargo fmt --all --check` | gate |
| G3 | `cargo clippy --all-targets --all-features -- -D warnings` | gate |
| G4 | `cargo test --all-targets --all-features -- --test-threads=1` | gate |
| G5 | `cargo test --test config_contract -- --test-threads=1` | gate |
| G6 | `cargo test --test cli_contract -- --test-threads=1` | gate |
| G7 | `cargo test --test doctor -- --test-threads=1` | gate |
| G8 | `cargo test --test spec_drift -- --test-threads=1` | gate |
| G9 | `cargo test --test output_contract -- --test-threads=1` | gate |
| G10 | `rg -F "pub mod viewer;" src/lib.rs` | gate |
| G11 | `rg -F "VIEWER_UNAVAILABLE" src/output.rs` | gate |
| G12 | `rg -F "viewer" src/output.rs` | gate |
| G13 | `rg -F "inline" src/viewer.rs` | gate |
| G14 | `rg -F "console" Cargo.toml` | gate |
| G15 | `rg --files-without-match -F "termimad" Cargo.toml` | gate |
| G16 | `rg --files-without-match -F "render_markdown_ansi" src/output.rs` | gate |
| G17 | `rg -F "[viewer]" config.example.toml` | gate |
| G18 | `rg -F "leaf" README.md` | gate |
| G19 | `rg -F "leaf" docs/llm-wikis.md` | gate |
| G20 | `rg -F "VIEWER_UNAVAILABLE" docs/2026-07-28-llm-wikis-external-query-design.md` | gate |
| G21 | `rg -n -- "-inline" docs/llm-wikis.md` | gate |
| G22 | pipe、重新導向、JSON 模式、plain 旗標、NO_COLOR 五條路徑的 stdout 相對 base commit 逐位元不變且不含 ANSI escape —— evaluator 直接比對 tests/cli_contract.rs 中對應斷言相對 base 的 diff，既有期望值不得被放寬或刪除 | gate |
| G23 | viewer 失敗時 stdout 恰好一份完整原始 markdown、無半截 leaf 輸出、無重複內容，警告只在 stderr —— evaluator 核對 design.md 第 2 節失敗分支表的六個分支在 src/cli.rs 都有對應處理，且每一條都在 render 回傳成功之前不寫入 stdout | gate |
| G24 | doctor 的 viewer 探測整個 run 只執行一次（不隨 wiki/agent pair 重複 spawn），且 viewer check 固定排在每個 pair 的 checks 末端，既有九項相對位置不變 —— evaluator 讀 src/doctor.rs 確認探測結果被 clone 而非重算 | gate |
| G25 | 缺 leaf 時 doctor 退出碼維持 0（warn 而非 fail），且 viewer 的失敗未進入 §14 錯誤碼表 —— evaluator 確認 src/error.rs 的 ErrorCode 沒有新增變體 | gate |

