# Token Gateway · 本地 AI 網關

> Local-first AI gateway & usage monitor for Windows — aggregate upstream providers behind one OpenAI-compatible endpoint, issue local keys per tool, and see exactly what you burn.
>
> 本地優先的 AI 網關與用量統計工具（Windows）：把多個上游聚合成一個 OpenAI 兼容入口、按工具簽發本地 Key、每一分錢燒在哪裡都看得見。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![Platform: Windows x64](https://img.shields.io/badge/Platform-Windows%20x64-0078D4.svg)](#下載--download)
[![Built with Tauri 2](https://img.shields.io/badge/Built%20with-Tauri%202-ffc131.svg)](https://tauri.app)

---

## 功能 · Features

- **用量統計（7 視圖）**：總覽（含 GitHub 式用量日曆＋Token 趨勢）、模型排行、按月／按日／按時段、統計、渠道拆分，全部本地 SQLite 即時查詢
- **上游來源管理**：OpenAI／Anthropic／Gemini 兼容渠道，模型映射＋四率定價（輸入／輸出／快取讀／快取建），支援時段定價與訂閱費抵扣
- **本地 Key 簽發**：每個工具一把 `sk-local-` Key，配額、限流、模型白名單獨立控制；Key 明文只顯示一次，可輪換、可停用
- **試算比價**：A/B 雙來源同條件試算，自動標出更便宜一方；**網站價格對比**：貼定價頁網址／文字／截圖，AI 抽取價格並與全庫同名定價逐行對比、可按價格排序
- **工具一鍵接管**：Codex／Claude Code 等工具配置自動備份＋寫入網關，回滾一鍵還原；可與 CC Switch 共存（端口衝突自動提示，`~/.codex` 註釋完整保留）
- **歷史導入＋CSV 匯出**：掃描各工具本地會話記錄補登用量；請求明細可匯出 Excel 直開的 CSV
- **macOS 27 風格深色 UI**：Liquid Glass、4 種強調色（偏好存 SQLite，重啟不丟失）、支援 600px 窄窗

- **Usage analytics (7 lenses)**: Overview (contribution calendar + token trend), Models, Monthly, Daily, Hourly, Stats, Channels — all served instantly from local SQLite
- **Upstream management**: OpenAI / Anthropic / Gemini-compatible providers, model mapping + 4-rate pricing (input / output / cache-read / cache-create), time-window pricing and subscription-fee offsets
- **Local key issuance**: one `sk-local-` key per tool, each with its own quota, rate limit and model allowlist; plaintext shown once, rotatable and revocable
- **Cost comparison**: A/B quote under identical conditions with the cheaper side highlighted; **website price check**: paste a pricing URL / text / screenshot, AI extracts the rates and diffs them against every stored same-name pricing, sortable by price
- **One-click tool takeover**: backs up and rewrites Codex / Claude Code configs to point at the gateway, one-click rollback; coexists with CC Switch (port conflicts reported, `~/.codex` comments preserved)
- **History import + CSV export**: backfill usage by scanning tools' local session logs; request details export to Excel-ready CSV
- **macOS 27-style dark UI**: Liquid Glass, 4 accent colors (stored in SQLite, survives restarts), usable down to 600px width

## 下載 · Download

到 [Releases](../../releases) 下載 `v0.1.0`（Windows x64）：

| 文件 | 說明 |
|---|---|
| `token-gateway_0.1.0_x64-setup.exe` | NSIS 安裝包（推薦，3.5MB） |
| `token-gateway_0.1.0_x64_en-US.msi` | MSI 安裝包（5.8MB） |

> 未簽名版本：SmartScreen 會提示「未知的發行者」，選「仍要執行」即可。需 WebView2 Runtime（Win10／11 一般自帶）。

> Grab `v0.1.0` (Windows x64) from [Releases](../../releases). Unsigned build: SmartScreen will warn about an unknown publisher — choose "Run anyway". Requires WebView2 Runtime (bundled with Win10/11).

## 快速開始 · Quick Start

1. 安裝並啟動，頂欄按 **啟動**（預設監聽 `127.0.0.1:15722`，僅本機）
2. **上游來源** → 添加來源 → 填 `base_url`＋Key → 登記模型＋定價
3. **本地 Key** → 簽發 Key → 綁定來源（可選配額／限流／模型白名單）
4. 把工具的 `base_url` 指到 `http://127.0.0.1:15722/v1`、Key 填剛簽發的 `sk-local-…`（或用工具頁一鍵接管）
5. 回到 **用量** 看即時統計

1. Install, launch, hit **啟動** in the top bar (listens on `127.0.0.1:15722`, localhost only)
2. **Providers** → add a provider → fill `base_url` + key → register models + pricing
3. **Keys** → issue a key → bind a provider (optional quota / rate limit / model allowlist)
4. Point your tool at `http://127.0.0.1:15722/v1` with the `sk-local-…` key (or use one-click takeover)
5. Watch live stats under **用量**

## 從源碼構建 · Build from Source

```sh
# 依賴：Node ≥ 22（pnpm）、Rust stable
pnpm install
pnpm tauri dev      # 開發運行
pnpm tauri build    # 打包（需 NSIS / WiX，見下）
```

```sh
# Requires: Node ≥ 22 (pnpm), Rust stable
pnpm install
pnpm tauri dev      # dev run
pnpm tauri build    # packaging (needs NSIS / WiX, see below)
```

- 前端：`pnpm build`（`tsc && vite build`）／後端測試：`cargo test --manifest-path src-tauri/Cargo.toml`
- 打包 Windows 安裝包需 `makensis`（NSIS 3）與 `candle.exe`／`light.exe`（WiX 3）在 `PATH`；無管理員權限可用兩者的便攜 zip 解壓後加入 PATH
- 數據目錄：`%APPDATA%\com.tokencounter.gateway\app.db`（SQLite，WAL）

- Frontend: `pnpm build` (`tsc && vite build`) / backend tests: `cargo test --manifest-path src-tauri/Cargo.toml`
- Windows packaging needs `makensis` (NSIS 3) and `candle.exe` / `light.exe` (WiX 3) on `PATH`; without admin rights, unzip their portable zips and prepend to PATH
- Data dir: `%APPDATA%\com.tokencounter.gateway\app.db` (SQLite, WAL)

## 常見問題 · FAQ

- **啟動失敗／端口被佔用？** 預設 `15722`；若與 CC Switch 等工具衝突，App 會明確提示佔用者，換端口或先停掉對方
- **歷史用量是空的？** 用量頁的導入功能可掃描工具本地會話記錄補登
- **定價對不上？** 先檢查來源定價的時段／訂閱抵扣，再用試算頁的網站價格對比抓出差異
- **Port in use on start?** Default is `15722`; on conflict with tools like CC Switch the app names the occupant — change ports or stop the other tool
- **Empty history?** Use the import function on the Usage page to backfill from tools' local session logs
- **Pricing mismatch?** Check time-window pricing / subscription offsets first, then diff with the website price check on the Calc page

## 致謝 · Acknowledgements

- [CC Switch](https://github.com/farion1231/cc-switch)（MIT）——工具配置共存與互操作設計的參考
- [New-API](https://github.com/QuantumNous/new-api)（AGPLv3）——上游錯誤格式兼容的參考；本項目未使用其代碼
- [TokenBar](https://github.com/Nanako0129/TokenBar)（MIT）——多視角用量呈現的靈感來源
- 本項目全部代碼獨立編寫，與上述項目無代碼級衍生關係

- [CC Switch](https://github.com/farion1231/cc-switch) (MIT) — reference for tool-config coexistence and interop design
- [New-API](https://github.com/QuantumNous/new-api) (AGPLv3) — reference for upstream error-format compatibility; none of its code is used here
- [TokenBar](https://github.com/Nanako0129/TokenBar) (MIT) — inspiration for multi-lens usage views
- All code in this project is written independently; there is no code-level derivation from the above projects

## 授權 · License

[MIT](./LICENSE) © 2026 aaaeeezynx
