## 2026-08-07T05:19:54.600Z — pre-0.1.0 CLI refinements

四項 CLI 優化:config list/validate 子命令、query spinner(stderr、僅互動終端)、全子命令人類可讀錯誤改 stderr、Claude --append-system-prompt + --setting-sources project 與 Codex -c developer_instructions=(不加 Skill,實測會回歸)。真實 claude/codex 端對端驗證通過;operator guide 新增 §2.7a(SKILL.md allowed-tools 要求)與 README Development 區段。PR #5(draft)。後續:使用者實測回報 TOML 反斜線模板誤導與 query 缺 -- 的錯誤訊息問題,轉入新 task 修正。

Commits: ddc7999 78638bf 1c8c9f8 b09198d 4026ea8

