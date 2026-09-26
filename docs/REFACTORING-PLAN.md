# Token Gateway 重構計畫書

> 版本：v1（2026-09-26）
> 基準線 commit：`38f060a`（`chore: 建立版本控制基準線（重構前快照）`）
> 離線快照：`_backup/token-counter-snapshot-20260926-195436.zip`（9,909 項，56.7 MB）
> 撰寫原則：**本文件只做診斷與規劃，尚未修改任何一行程式碼。**

---

## 0. 閱讀指引

| 你的問題 | 看哪一節 |
|---|---|
| 專案整體架構是什麼 | §1 |
| 有沒有過肥／冗餘的檔案 | §2 |
| 有沒有明顯的 bug | §3 |
| 該不該補 Git 版控 | §4（**已完成**） |
| 400 錯誤與 Codex 會話續用問題怎麼處理 | §5（**含我對「不確定怎麼做」的具體建議**） |
| 接下來怎麼動工 | §6 |

**一句話結論**：這是一個**測試紮實、註解詳盡、但核心模組已嚴重膨脹**的專案。它的 bug 幾乎全部集中在同一個地方——`proxy.rs` + `translate.rs` 組成的**翻譯／相容層**，而且成因是同一個架構決策錯誤（用「事後剝離重試」猜上游能力，見 §5.2）。因此**不建議大重寫**，建議沿著現有 146 個測試的保護網做**外科式重構**。

---

## 1. 專案現況

### 1.1 工作區組成：三種東西混在一起

`D:\token counter` 不是單一專案，而是三類資產混放：

| 路徑 | 性質 | 處置 |
|---|---|---|
| `token-gateway/` | **Tauri 2 桌面應用（本體）** | ✅ 納入版控 |
| `codex/` | `git clone` 的 **OpenAI Codex 上游原始碼**（`origin = github.com/openai/codex`，HEAD `6b0a1a8`，自帶 `.git`，8,674 檔） | ❌ 已 `.gitignore` |
| `icon-lab/`、`src-tauri/icons/concepts/` | App 圖示設計資產（10 版 SVG + 38 張 1024px PNG，數 GB） | ❌ 已 `.gitignore` |
| `.workbuddy/memory/` | 前次 session 的設計決策記錄 | ✅ 納入版控 |
| `.workbuddy/tmp/` | Agent 暫存 | ❌ 已 `.gitignore` |

> ⚠️ **`codex/` 是重要的參考物，不是垃圾。** 我在本次分析中用它核對了 Codex 的 SQLite 檔名與 schema（§3 B3），這是本計畫書最關鍵的一項證據來源。**建議保留在磁碟上，只是不納入 git。**

### 1.2 技術棧

| 層 | 技術 |
|---|---|
| 桌面框架 | Tauri 2（`tray-icon`） |
| 後端 | Rust 2021、`axum` 0.8、`reqwest` 0.12、`rusqlite` 0.32（bundled SQLite）、`tokio` 1、`toml_edit` 0.22 |
| 前端 | React 19、TypeScript 6、Vite 8、Tailwind CSS 4、TanStack Query 5 |
| 資料 | SQLite（WAL）於 `%APPDATA%\com.tokencounter.gateway\app.db` |
| 打包 | NSIS / WiX（Windows x64） |

### 1.3 後端模組職責

| 檔案 | 實作行 | 測試行 | 職責 |
|---|---:|---:|---|
| `proxy.rs` | **1,660** | 1,438 | 本地網關：鑑權 → 路由 → 翻譯 → 轉發 → SSE 用量解析 → 落庫 |
| `translate.rs` | 1,616 | 572 | 三格式互轉：Anthropic ↔ OpenAI Chat ↔ Responses |
| `tools.rs` | **2,500** | 919 | 七種工具接管（claude／codex／opencode／hermes／dsh／cursor／antigravity） |
| `usage.rs` | 1,338 | 414 | 四率定價、時段定價、訂閱抵扣、CSV 匯出 |
| `commands.rs` | 801 | **0** | 61 個 Tauri command 膠水層 |
| `history.rs` | 771 | 197 | 掃描工具本地會話補登用量 |
| `models.rs` | 630 | 275 | 模型目錄、Codex catalog 產出 |
| `quota.rs` | 504 | 42 | 訂閱額度查詢（Claude／Codex／opencode 私有 API） |
| `db.rs` | 446 | **0** | Schema + migration（**資料真相層**） |
| `keys.rs` | 361 | 14 | `sk-local-` 簽發、雜湊、配額 |
| `price_extract.rs` | 362 | 12 | LLM 抓取定價頁 |
| `providers.rs` | 241 | 88 | 上游 CRUD |
| `fsutil.rs` | 52 | 0 | 原子寫入、備份輪替 |
| **合計** | **11,430** | **3,975** | 總計 15,405 行 |

**請求生命週期**（`proxy.rs:774` `proxy_handler`）：

```
客戶端 (Codex/Claude Code/…)
   │  POST 127.0.0.1:15722/v1/…
   ▼
① 鑑權     bearer → keys::auth_key（本地 sk-local-）→ 失敗回退 auth_direct
② 限流     RateLimiter（per-key 60 秒滑動窗，**進程內記憶體**）
③ 讀 body  上限 32 MB
④ 模型     body.model → 白名單檢查 → 路徑回填（Gemini）
⑤ 路由     Key 綁定來源 → 未命中則 resolve_model_provider() 跨來源
⑥ 格式矩陣 detect_in_format(path) × provider.api_format → TransKind
⑦ 翻譯     anthropic_to_openai / responses_to_openai（含 sanitize_chat_messages）
⑧ 剝離     已記憶欄位 → strip_unsupported_tools → 歷史配對修復
⑨ 轉發     reqwest（timeout 600 s）
⑩ 400 處理 parse_unknown_fields → 剝離 → remember_stripped → 重送一次
⑪ 用量     SSE 邊轉邊解析 / 非流式讀 usage
⑫ 落庫     request_logs
```

### 1.4 前端結構

`App.tsx`(287) 是薄殼：5 個分頁 + `ProxyPill` 啟停控制 + 5 顆環境光暈 orb。

| 檔案 | 行 | 內容 |
|---|---:|---|
| `components/Providers.tsx` | **2,766** | ⚠️ 單檔 20 個元件 |
| `components/Usage.tsx` | **1,605** | ⚠️ 7 個 lens + 圖表 + 額度環 |
| `components/Keys.tsx` | 838 | Key 簽發／輪換 |
| `components/Calc.tsx` | 780 | A/B 試算 + 網站比價 |
| `index.css` | 727 | macOS 27 風格設計系統 |
| `lib/api.ts` | 636 | 61 個 command 的 TS 綁定 |

### 1.5 健康度基線（實測，非推測）

**Phase 0 基線（重構前）**

| 檢查 | 指令 | 結果 |
|---|---|---|
| 後端測試 | `cargo test --offline` | ✅ **146 passed / 0 failed / 8 ignored**，exit 0 |
| 前端型別 | `npx tsc --noEmit` | ✅ **0 錯誤**，exit 0 |
| 編譯警告 | `cargo test` | ⚠️ 3 個（皆 `dead_code`） |

**Phase 1 結束（現況）**

| 檢查 | 指令 | 結果 |
|---|---|---|
| 後端測試 | `cargo test --offline` | ✅ **179 passed / 0 failed / 8 ignored**，exit 0 |
| 前端型別 | `npx tsc --noEmit` | ✅ **0 錯誤**，exit 0 |
| 前端建置 | `npx vite build` | ✅ 成功（主 chunk 542 kB，gzip 150 kB） |
| Clippy（含測試） | `cargo clippy --offline --all-targets` | ✅ exit 0；**`dead_code` 歸零**，餘 5 條為既有的風格提示（`tools.rs` ×4、`price_extract.rs` ×1，非本次新增） |
| App 二進位 | `cargo build` | ✅ 成功 |

> Phase 2 每一步結束都重新確認前三項，數字維持不變（179/0、0 錯誤、
> 5 條既有警告）—— 這是「純重構」的證明。第 5～9 步（抽 `finish_response`／
> `prelude`／`resolve_model`／`prepare_request`／`request_meta`）亦同。

**Phase 2 結束（handler 部分，9 步）**

| 檢查 | 指令 | 結果 |
|---|---|---|
| 後端測試 | `cargo test --offline` | ✅ **179 passed / 0 failed / 8 ignored**，exit 0（**測試一行未改**） |
| 前端型別 | `npx tsc --noEmit` | ✅ **0 錯誤** |
| Clippy（含測試） | `cargo clippy --offline --all-targets` | ✅ exit 0；仍為同 5 條既有風格提示 |
| `proxy_handler` | 實測 | 909 → **86 行**（目標 < 80，❌ 差 6 行） |
| `proxy.rs` 總行數 | 實測 | 3,098（Phase 0）→ **4,195**（實作 2,294 + 測試 1,901）❌ |

> ⚠️ **注意 `proxy.rs` 變大了，這是預期結果、不是退步。** 抽函式解決的是
> 「單一 869 行的巨型函式」，不是「巨型檔案」。每個抽出的函式都自帶簽章與
> 解釋「為什麼」的註解，而本專案註解密度極高（§2 註記：註解是優點，不該
> 為行數刪減）。**檔案行數只能靠拆模組解決，而拆模組尚未開始。**

8 個 ignored 測試是標記 `#[ignore]` 的 **live 測試**，會真的寫入 `~/.claude/settings.json`、`opencode.json`、hermes `.env`（跑法：`cargo test live_ -- --ignored --test-threads=1`）。

**這個基線是重構的安全網，必須在每個階段結束時重新確認不退步。**

---

## 2. 問題一：過肥與冗餘檔案

### 2.1 真正的肥檔（依嚴重度排序）

| # | 位置 | 規模 | 問題本質 | 建議拆法 |
|---|---|---|---|---|
| **F1** | `proxy.rs:774-1642` `proxy_handler` | ~~單一函式 868 行~~ → **86 行** | 鑑權／路由／翻譯／剝離／轉發／重試／SSE 全在一個函式；8 處重複 `log_reject + err_json` 樣板；5 個 `return err_json(400, …)` 分支 | ✅ **Phase 2 已完成**：拆為 `prelude` / `request_meta` / `resolve_model` / `prepare_request` / `upstream_for`+`send_with_strip_retry` / `finish_response` 六段，`proxy_handler` 只做編排（實際 86 行，目標 < 80 差 6 行） |
| **F1b** | `proxy.rs` 整個檔案 | **4,195 行**（實作 2,294 + 測試 1,901） | F1 修完後才看得出來的**真正問題**：抽出的小函式全部留在同一檔，檔案行數不減反增 | ⏳ **尚未開始**：拆 `proxy/{mod,auth,route,matrix,forward,stream,strip}.rs` + 測試移到 `proxy/tests.rs`，目標每檔 < 600 行 |
| **F2** | `tools.rs`（2,500 行） | 7 個工具 × (偵測／計畫／套用／還原) | 全部塞一檔；`apply_switch` 巨型 match（L1874-1930） | 拆 `tools/{claude,codex,opencode,hermes,dsh,cursor,antigravity}.rs` + `tools/mod.rs` 共用 trait |
| **F3** | `Providers.tsx`（2,766 行） | `SwitchDialog` 單元件 **528 行**、`ProviderForm` 328 行 | 表單、定價、時段、模型面板、接管對話框全在一檔 | 拆 `providers/` 目錄，7 個檔案 |
| **F4** | `Usage.tsx`（1,605 行） | 32 個頂層函式 | 7 個 lens + 貢獻日曆 + 額度環 + 匯入對話框 | 拆 `usage/` 目錄，每個 lens 一檔 |
| **F5** | `commands.rs`（801 行） | **0 測試** | 純膠水層，但承載 61 個 API 邊界 | 不需拆檔；需**補測試**（見 §3 B8） |
| **F6** | `db.rs`（446 行） | **0 測試** | Schema + migration，**資料真相層** | 不需拆檔；需**補 migration 測試** |

> **註解也是行數來源。** 本專案的註解極其詳盡且多半在解釋「為什麼」（例如 `proxy.rs:469-473` 解釋兩種上游錯誤口徑的來源與日期），這是**優點**，不建議為了行數刪減。拆檔時應**連同註解一起搬到新模組**。

### 2.2 已清理的垃圾（本階段已執行，已由 zip 快照保全）

| 類別 | 數量 | 說明 |
|---|---:|---|
| 根目錄一次性 patch 腳本 | 9 | `add_header.py`、`check.py`、`dump_keys.py`、`find.py`、`find_keys.py`、`fix_fields.py`、`fix_keys.py`、`fix_keys2.py`、`sync_anim.py` |
| `token-gateway/*.log` | 22 | 18 個 `gw*.log`（內容幾乎相同，688 bytes）+ 4 個 0 byte（`app_err`、`app_out`、`dev`、`gw_stdout`） |
| 0 byte 空檔 | 1 | 根目錄名為 `=` 的檔案 |

**為何可安全刪除**：那 9 個腳本是**編碼災難（mojibake）時期**用來批次改字串的臨時工具，腳本內還留著 `濡?flatkey`、`鐢ㄩ噯鎺у埗` 這類**本身就是壞字串**的比對目標。我已掃描 `src/` 與 `src-tauri/src/` 全部 `.rs/.ts/.tsx/.css`，**確認 mojibake 已完全修復、無殘留**，故這些工具已無用途。

### 2.3 殘留清理（已依確認執行）

| 項目 | 處置 |
|---|---|
| `tauri-dev.log`（32 KB） | ✅ 已刪除。內容僅為帶 ANSI 色碼的開發期 Rust 編譯錯誤，無 runtime 證據 |
| `gen_icons.py`（根目錄） | ✅ 已刪除。輸出目錄 `icon-candidates/` 已不存在，是死碼 |
| `gw_init.txt`、`gw_req_debug.txt`（根目錄 + `token-gateway/` 共 4 份） | ✅ 已刪除。全部是「已移除的 debug 版本」殘留輸出；其中兩份**內容不同**且具診斷價值，已先保全至 `docs/evidence/` |
| `check_mapping.py`、`test_anthropic.py`、`test_body.json` | ✅ 已移至 `scripts/`。這三個是手動 E2E 驗證工具，對 §5 的驗證工作有直接價值 |

**證據保全結果**（`docs/evidence/`）——原本看似重複，實為三筆獨立樣本，且**互相矛盾**，這正是 §5.2 推論的基礎：

| 檔案 | 來源 | 價值 |
|---|---|---|
| `2026-09-26-body-parse-OK.txt` | 根目錄 `gw_req_debug.txt` | 成功案例（`bytes_len=132` ＝ 原始長度） |
| `2026-09-26-body-parse-FAILED.log` | `token-gateway/gw_debug.log` | 失敗案例一（`bytes_len=107` ＝ 去引號長度） |
| `2026-09-26-body-parse-FAILED-openai.log` | `token-gateway/gw_req_debug.txt` | 失敗案例二（`bytes_len=69` ＝ 去引號長度） |

> ✅ **已於 Phase 1 處理**：`scripts/test_anthropic.py` 原本硬編了一把
> `sk-local-…` 金鑰（以 `git grep` 掃描確認它是**唯一**含該實值的追蹤檔案）。
> 已改為從環境變數 `TOKEN_GATEWAY_KEY`（本專案既有的標準變數名）或命令列
> 參數讀取，並在缺金鑰時給出可行動提示。
>
> 同時改良了該腳本：回應不是 JSON 時，印出原始 bytes 的 hex 而非直接
> `json.loads` 崩潰 —— 這正好對應 §5.2 的需求（先前無法結案就是因為
> 沒有留下原始 bytes）。
>
> 註：該金鑰是僅在本機有效的閘道 Key（離開本機的 `app.db` 即無用），
> 且已在快照與 git 歷史中；若你曾在別處重用過同一把字串，建議在
> 「本地 Key」頁輪換它。

---

## 3. 問題二：Bug 清單

### 3.1 分級總表

| ID | 嚴重度 | 位置 | 症狀 | 階段 | 狀態 |
|---|---|---|---|---|---|
| **B1** | 🔴 高 | `proxy.rs`（缺失） | `encrypted_content` 直通上游 → `400 reasoning encrypted_content was not issued to this caller` | Phase 1 | ✅ 已修 |
| **B2** | 🟠 中 | `proxy.rs`（`stripped` 記憶體） | **每次重啟網關，每個渠道都要重踩一次 400 + 重試** | Phase 1 | ✅ 已修 |
| **B3** | 🔴 高 | `tools.rs:999-1036` | 硬編 `state_5.sqlite` + **所有失敗路徑靜默回空** → 舊 Codex 會話 provider 別名段消失 | Phase 3 | ⏳ 待處理 |
| **B4** | 🟠 中 | `tools.rs:1110` | 無差別強制 `wire_api = "responses"`，強迫所有 Codex 流量走最複雜翻譯路徑 | Phase 2 | ⏳ 待處理 |
| **B5** | 🟡 低 | `proxy.rs:966-1024` | 格式矩陣正確性依賴 match **分支順序**，重排即靜默改行為 | Phase 1 | ✅ 已修 |
| **B6** | 🟡 低 | `proxy.rs:1023` | `("openai","openai-responses")` 落入 `_ => TransKind::None` 直通 → chat 請求打到 responses 端點必失敗 | Phase 1 | ✅ 已修 |
| **B7** | 🟡 低 | `tools.rs` | 2 個 `dead_code`：`restore_backup_to`、`restore_backup` 只被測試呼叫，lib 建置下恆為未使用 | Phase 1 | ✅ 已處置 |
| **B8** | 🟠 中 | `commands.rs`、`db.rs` | 801 + 446 行 **零測試**，卻是 API 邊界與資料真相層 | Phase 1（`db.rs`）/ 4（`commands.rs`） | 🟡 `db.rs` 已補 |
| **B9** | 🟡 低 | `proxy.rs:1646` | `reqwest` timeout **600 秒**且未設 connect_timeout → 上游卡死會佔用請求 10 分鐘 | Phase 1 | ✅ 已修 |

> 狀態於 Phase 1 結束時更新。詳見 §3.2 的完成記錄。

---

### 3.2 Phase 1 完成記錄（實作後回填）

**⚠️ 一處判斷修正**：B1–B9 的初稿把 B7 寫成「3 個 dead_code」且推測是
「未註冊為 command」。實際是 **2 個**，且真正原因是它們**只被測試呼叫**，
因此在 lib 建置（無 `cfg(test)`）下才是 dead code —— 這也解釋了為何
`cargo test` 從不抱怨、只有 `cargo clippy --all-targets` 看得見。

更關鍵的是：我原先認為 `restore_backup_to` 與 `restore_backup_to_port` 是
「同義重複（只差埠預設值）」，**這是錯的**。兩者真正的差異是 `migrate` 參數：

| 函式 | port | migrate |
|---|---|---|
| `restore_backup_to`（只被測試用） | `DEFAULT_GATEWAY_PORT` | **false** |
| `restore_backup_to_port`（生產路徑） | 呼叫端提供 | **true** |
| `restore_backup`（只被測試用） | `DEFAULT_GATEWAY_PORT` | true（經 `_port`） |

第一版改寫讓 `restore_backup_to` 的測試 helper 轉呼叫 `_port` 版本，
7 個測試立刻失敗 —— 它們測的正是「不做 baseline 遷移」的還原路徑。
這個失敗本身就是證據。最終處置：helper 照抄原行為（`migrate: false`），
生產 API 只留帶 port 的版本。

**各項實際改動**

| ID | 改動 | 測試 |
|---|---|---|
| B1 | 新增 `proxy.rs::strip_encrypted_content()`，遞迴移除所有 `encrypted_content`（含巢狀／陣列），以位元組快掃短路避免多一次 JSON parse | 5 個單測 |
| B2 | `ProxyCtx.stripped`（進程 HashMap）→ `provider_stripped_fields` 表；移除 `ProxyCtx` 欄位與舊 `remember_stripped()` | 持久化跨連線回歸測試 |
| B5 | `detect_in_format`/字串 match → `InFmt`/`TargetFmt` enum + `resolve_trans_kind()`，30 格全列舉、**無萬用字元**（新增變體時編譯器強制補齊） | `trans_kind_matrix_is_fully_pinned` 釘死 30 格 |
| B6 | `(OpenAi, OpenAiResponses)` 由靜默直通改為明確 400 + 可行動建議 | 單測 + e2e 回歸 |
| B7 | 兩個包裝降為測試模組內 helper | 179→179 全綠 |
| B9 | 加 `connect_timeout(10s)`，整體 timeout 維持 600s（抽出常數） | — |
| B8 | `db.rs` 補 10 個 migration 測試（詳見下） | 10 個 |

**B6 修正當場暴露了一個既有測試的假陽性**：`e2e_forward_auth_and_log`
的 fixture 把渠道宣告成 `openai-responses`，卻在測 chat 端點轉發 ——
它長期綠燈**正是因為 B6 的靜默直通**掩蓋了這個不一致。已修正 fixture
（改 `openai-chat`），並補上 B6 的端到端回歸斷言。

**可觀測性（§5.3 第 0 層，本次新增）**

- 新增 `trace.rs`（`proxy.rs` 已 1,660 行，不讓它繼續膨脹）。
- 新增 `db.rs` 的 `proxy_trace` 表，只寫異常請求（正常請求不寫，避免日誌洪水），
  網關啟動時裁剪至 5000 筆。
- 四類留痕：① 上游 400 且成功剝離（記下剝了哪些欄位 + 上游完整原文）
  ② 剝離後重試仍失敗（標為「未解決」）③ 上游 400 但解析不出欄位名
  （標為「相容策略失效」）④ **body 解析失敗 → 存原始位元組 hex 前綴（512 bytes）**。
- ④ 是 §5.2 懸案的解藥：該處三個證據樣本的 `bytes_len` 用了兩套算法
  （成功案例是原始長度、失敗案例是去引號後長度），無法區分「真解析失敗」
  與「debug 儀器弄壞 body」。現在失敗當下會留下原始 hex，可直接定案。
- 順帶修正該 400 的訊息：原本無論 Anthropic 或 Responses 入站都說
  「Anthropic 請求體不是 JSON」。
- 前端新增側欄「診斷」分頁（`Diagnostics.tsx`）：總覽統計、上游能力記憶
  （可逐渠道重設）、請求追蹤（只看異常／全部、展開看完整上下文、可複製 hex）。

**`db.rs` 遷移測試覆蓋（B8 部分）**

全新庫版本、冪等重開、舊庫自動補表、v1→v2（`provider_id`）與
v6→v7（`key_plain`）的 ALTER 路徑、v5→v6（`source`/`import_path`）、
v3→v4 峰谷定價 → `pricing_periods` 遷移（含非 `tou` 列不遷移、原列保留）、
`settings` 種子不覆蓋使用者值、使用者刪除種子後不復活、
以及「種子渠道不得內建任何金鑰」。

---

### B1 🔴 `encrypted_content` 剝離從未被實作

**證據**
你的 `tokengw-plan.md` 第二節明確規劃了修復：

> **2. proxy.rs — 新增 `strip_encrypted_content` 函數** … 在 `proxy_handler` 的 body 處理階段調用 … 在 `strip_unsupported_tools` 之後調用

但我 grep 整個後端，`strip_encrypted_content` **不存在**：

```
$ grep -rn "strip_encrypted|encrypted_content" src-tauri/src
translate.rs:795  /// reasoning item → 思考原文。優先解我們自己寫入的 `encrypted_content` 標記
translate.rs:801      if let Some(ec) = item.get("encrypted_content")...
translate.rs:2296/2300  （測試）
```

`encrypted_content` 只出現在 `reasoning_text_of()`（`translate.rs:800`），用來讀取**網關自己寫入的** marker `tg-openai-reasoning-v1:`。**沒有任何地方會剝離它。**

**根因**：兩條翻譯路徑其實都安全，但**直通路徑裸奔**——

| 路徑 | 條件 | `encrypted_content` 是否會上送 |
|---|---|---|
| Anthropic → Chat | `anthropic` × `openai-chat`/`mixed` | ✅ 安全。`anthropic_to_openai` 用**白名單重建** body（`translate.rs:470-514`），只放 `model`/`messages`/`max_tokens`/… 等 10 個欄位，`thinking` 根本不會被帶出去 |
| Responses → Chat | `responses` × `openai-chat`/`mixed` | ✅ 安全。`reasoning` item 只取文字（`translate.rs:929-931`） |
| **Responses 直通** | `responses` × `openai-responses` | ❌ **裸奔**。`TransKind::None` → body 原封不動轉發 |

**修法**（`proxy.rs`，在 L1195 `strip_unsupported_tools` 附近加入）

```rust
/// 遞迴移除所有 `encrypted_content` 欄位（含 reasoning item 內）。
/// 目的：Codex 在 ChatGPT 授權模式下產生的推理簽章只對 OpenAI 有效，
/// 送到任何第三方 responses 端點都會被 400 拒收
/// （`reasoning \`encrypted_content\` was not issued to this caller`）。
/// 回傳 None 表示未命中（原文一字不動）。
fn strip_encrypted_content(body: &[u8]) -> Option<Vec<u8>> {
    fn walk(v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(o) => {
                let mut hit = o.remove("encrypted_content").is_some();
                for (_, child) in o.iter_mut() {
                    hit |= walk(child);
                }
                hit
            }
            serde_json::Value::Array(a) => a.iter_mut().fold(false, |acc, c| walk(c) | acc),
            _ => false,
        }
    }
    let mut v: serde_json::Value = parse_body_json(body)?;
    if walk(&mut v) { serde_json::to_vec(&v).ok() } else { None }
}
```

呼叫點（僅直通分支，與既有剝離並列）：

```rust
    if !translated {
        if let Some(stripped) = strip_unsupported_tools(&body_bytes) {
            body_bytes = stripped;
        }
        // 新增：Responses 直通時剝離 OpenAI 專屬推理簽章
        if let Some(stripped) = strip_encrypted_content(&body_bytes) {
            body_bytes = stripped;
        }
        if let Some(fixed) = sanitize_passthrough_chat_body(&body_bytes, in_fmt, translated) {
            body_bytes = fixed;
        }
    }
```

**驗證**：新增單元測試（餵入含 `encrypted_content` 的巢狀 responses body，斷言輸出不含該鍵且其餘欄位不變）+ 整合測試（mock 上游回 400，斷言第二次轉發體已無該欄位）。
**風險**：低。純函式、只刪欄位、不命中就回 `None` 保持原文。

---

### B2 🟠 「剝離記憶」只存在進程記憶體，每次重啟歸零

**證據**

```rust
// proxy.rs:28-35
pub(crate) struct ProxyCtx {
    ...
    /// 每渠道已知的上游拒收欄位（命中一次後記住，後續請求預先剝離）。
    pub stripped: Arc<Mutex<HashMap<i64, Vec<String>>>>,
}

// proxy.rs:1642-1651  —— serve() 每次啟動都新建
pub async fn serve(db_path: PathBuf, listener: TcpListener) -> Result<(), String> {
    let ctx = ProxyCtx {
        ...
        stripped: Default::default(),   // ← 空白
    };
```

```rust
// commands.rs:128-134
pub fn proxy_stop(proxy: State<ProxyState>) -> Result<(), String> {
    let mut inner = proxy.inner.lock()...;
    if let Some(r) = inner.take() {
        r.handle.abort();               // ← ctx 被 drop，記憶消失
    }
    Ok(())
}
```

**影響**：`remember_stripped()`（`proxy.rs:618`）學到的東西**在網關停止時全部丟失**。也就是說，**每次你按下「啟動」，每個渠道的第一個請求都會再吃一次 400、再重試一次**。如果你經常開關網關，這個成本是每次重複支付的。

**這是 §5.2 架構批評的具體證據**，不只是理論問題。

**修法**：見 §5.3 的「能力設定檔持久化」。過渡期最小改動是把 `stripped` 寫進 SQLite：

```sql
CREATE TABLE provider_stripped_fields (
  provider_id INTEGER NOT NULL,
  field       TEXT    NOT NULL,
  learned_at  INTEGER NOT NULL,
  PRIMARY KEY (provider_id, field)
);
```

---

### B3 🔴 Codex 舊會話 provider 別名段會靜默消失

**這是「無法繼續使用舊的對話 session」最可能的技術根因。**

**機制**：Codex 的 rollout 記錄（`~/.codex/sessions/**/*.jsonl`）與 `threads` 表都會記住該會話用的 `model_provider` 名。恢復舊會話時，Codex 會拿這個名字去 `config.toml` 找 `[model_providers.<name>]` 段；**找不到就報 `Model provider not found`，會話開不起來**。

網關的對策（設計意圖正確）：接管時寫 `model_provider = "custom"`（共享段名，cc-switch 同款），並**逐一補建所有歷史 provider 名的別名段**，讓舊會話仍能解析。程式碼註解（`tools.rs:20-22`）自己就寫明了這個目標：

> 會話歷史按段名引用供應商，段名必須永遠存在且指向網關，舊會話才不會因段被刪而「Model provider not found」。

**漏洞**：別名段的清單來自 `tools.rs:1001` `codex_legacy_providers()`：

```rust
pub fn codex_legacy_providers(codex_home: &Path) -> Vec<String> {
    let db = codex_home.join("state_5.sqlite");          // ← ① 硬編檔名
    if !db.exists() { return vec![]; }                    // ← ② 靜默
    let conn = match rusqlite::Connection::open_with_flags(
        &db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY, // ← ③ 無 busy_timeout
    ) { Ok(c) => c, Err(_) => return vec![] };            // ← ② 靜默
    let mut stmt = match conn.prepare("SELECT DISTINCT model_provider FROM threads") {
        Ok(s) => s, Err(_) => return vec![],              // ← ② 靜默（schema 一改就失效）
    };
    let rows = match stmt.query_map([], |r| r.get::<_, String>(0)) {
        Ok(r) => r, Err(_) => return vec![],              // ← ② 靜默
    };
```

**四個獨立的失效點，而且每一個都靜默回傳空向量**：

**① 檔名硬編 —— 我用 `codex/` 上游原始碼核對過**

```rust
// codex/codex-rs/state/src/sqlite.rs:29-34
const LOGS_DB_FILENAME:            &str = "logs_2.sqlite";
const GOALS_DB_FILENAME:           &str = "goals_1.sqlite";
const MEMORIES_DB_FILENAME:        &str = "memories_1.sqlite";
const QUEUE_DB_FILENAME:           &str = "queue_1.sqlite";
const STATE_DB_FILENAME:           &str = "state_5.sqlite";        // ← 今天剛好命中
const THREAD_HISTORY_DB_FILENAME:  &str = "thread_history_1.sqlite"; // ← 新獨立 DB
```

Codex 現在有 **7 個 SQLite**，檔名**全部帶遞增版本號**，而且已經把 thread history 拆到 `thread_history_1.sqlite`。`state_5.sqlite` **今天剛好還是對的**，但這是一個隨時會變的私有實作細節 —— Codex 升版變成 `state_6.sqlite` 的那一刻，別名段清單就變成空陣列，**所有舊會話全部開不起來，而且沒有任何錯誤訊息**。

**② 全部失敗路徑靜默回空**：`vec![]` 與「真的沒有任何歷史 provider」**無法區分**。使用者看到的是「接管成功」，但實際上舊會話已經壞了。`codex_doctor`（`tools.rs:2039`）也呼叫同一個函式，所以**連診斷工具都無法揭露這個問題**。

**③ 沒有 `busy_timeout`**：對比 `proxy.rs:96` 的 `open_conn()` 有設 `PRAGMA busy_timeout=5000`，這個 read-only 連線沒設。Codex 正在執行時持有寫鎖，這個讀取可能拿到 `SQLITE_BUSY` → 靜默回空。

**④ schema 硬編**：`threads` 表名與 `model_provider` 欄名一旦調整即失效。

**修法（Phase 3）**：
1. **多檔探測**：依序嘗試 `state_*.sqlite`（取版本號最大者）＋ glob `*.sqlite` 掃描含 `threads` 表的檔案，而非硬編單一檔名。
2. **改用 Codex 的公開介面優先**：rollout `.jsonl` 檔本身也記錄 provider 名（`history.rs:240` 已在解析這些檔案），可作為 DB 之外的**第二來源**；兩者取聯集。
3. **失敗要大聲**：`codex_legacy_providers` 改回傳 `Result<Vec<String>, String>`，並在 UI／`codex_doctor` 明確顯示「無法讀取 Codex 狀態庫，舊會話可能無法恢復」。
4. **加 `busy_timeout`** 與唯讀開啟（保留現有的 `SQLITE_OPEN_READ_ONLY`，這點是對的）。
5. **別名段改為「只增不減」的持久記錄**：一旦補建過某個別名段，就記在網關自己的 DB（`codex_provider_aliases` 表），之後即使讀不到 Codex 狀態庫也照樣還原。

**驗證**：把 `state_5.sqlite` 改名為 `state_6.sqlite` 的測試案例必須通過（證明不再硬編）；新增「`threads` 表缺失時應回報錯誤而非靜默」的測試。

**關於「無法封存對話」**：我需要先澄清一件事——**網關不會寫入 Codex 的資料庫**（`codex_legacy_providers` 用 `SQLITE_OPEN_READ_ONLY`，且全檔僅有讀取，我已 grep 確認無 `INSERT`/`UPDATE`/`DELETE`）。所以封存失敗**不可能**是網關直接改壞 DB 造成的。合理的假設有兩個：
- **(a)** 封存動作會觸及 thread 的 provider 解析 → 與 B3 同源；
- **(b)** 是 Codex 自身的問題（例如 `thread_history_1.sqlite` 遷移未完成）。

**這需要你提供 Codex 端的原始錯誤訊息才能定位。** 建議在重現時用 `RUST_LOG=debug codex …` 或查看 `~/.codex/logs_2.sqlite`。**我不會在不確定根因的情況下先改程式碼。**

---

### B4 🟠 無差別強制 `wire_api = "responses"`

**證據**（`tools.rs:1110`，位於 `gateway_section()` 內，供所有被接管的 Codex provider 使用）：

```rust
tbl["wire_api"] = toml_edit::value("responses");
```

**影響**：不論上游實際是什麼協定，Codex 都被設定成「用 Responses 協議講話」。於是**每一個 Codex 請求都必須經過 Responses→Chat 翻譯**（`TransKind::ResponsesToChat`）——這是整個系統最複雜、最容易出 400 的路徑（涉及 reasoning item 還原、custom tool 還原、孤兒 tool_call 配對修復）。

程式碼裡其實**已經知道**這個設定的危險性，`tools.rs:2478-2480` 有相關警告：

```rust
// 直連第三方 https 但 wire_api=responses：多數第三方只說 Chat 協議，Codex 將 404。
```

但那只在**直連模式**警告，**接管模式下並未依上游 `api_format` 區分**。

**修法**：`wire_api` 依 `provider.api_format` 決定——
- `api_format = "openai-chat"` / `"mixed"` → 仍需 `responses`（因為 Codex 只會講 Responses，網關負責翻譯）。**所以這裡其實不能簡單改掉。**

> ⚠️ **修正我的初步判斷**：Codex 的 `wire_api` 只支援 `responses` 與 `chat` 兩種。若設成 `chat`，Codex 會直接發 `/chat/completions`，網關就**不需要翻譯**，400 風險大幅下降。**這是一個值得實測的選項**：對 `api_format = "openai-chat"` 的上游，把 `wire_api` 設為 `"chat"`，讓網關走純透通路徑。
>
> 但這會改變 Codex 的功能面（`wire_api = "chat"` 可能失去 reasoning summary 等 Responses 專屬能力）。**建議列為 Phase 2 的 A/B 實驗項目，需你實測後決定。**

**驗證**：對同一個 chat-only 上游，分別用 `wire_api = "responses"` 與 `"chat"` 各跑一輪 Codex 對話（含工具調用與舊會話續用），比較 400 發生率與功能完整性。

---

### B5 🟡 格式矩陣依賴 match 分支順序

**證據**（`proxy.rs:966-1024`）：

```rust
let kind = match (in_fmt, target) {
    ("anthropic", "anthropic") => TransKind::None,
    ("anthropic", "openai-chat") | ("anthropic", "mixed") => TransKind::AnthropicToChat,
    ("responses", "openai-chat") | ("responses", "mixed") => TransKind::ResponsesToChat,
    ("anthropic", _) => { /* 400 */ }
    ("openai", "anthropic") | ("responses", "anthropic") => { /* 400 */ }
    ("gemini", "gemini") => TransKind::None,
    ("gemini", _) => { /* 400 */ }
    (_, "gemini") => { /* 400 */ }
    _ => TransKind::None,
};
```

**問題**：`("anthropic", _)` 排在**第 4 個分支**，它會攔截所有 `("anthropic", "gemini")` 等組合。而 `("gemini", "gemini")` 之所以可達，純粹是因為它排在 `(_, "gemini")` 之前。**任何人重排這些分支（例如 rustfmt 或「整理一下」）都會靜默改變行為，且沒有任何測試會抓到。**（我逐一驗證過目前**邏輯是正確的**，這不是現存 bug，是**脆弱性**。）

**修法**：改為**窮舉式**的明確對照（`in_fmt` 4 值 × `target` 5 值 = 20 格全列出），或抽出 `fn resolve_trans_kind(in_fmt: &str, target: &str) -> Result<TransKind, &'static str>` 並為**全部 20 種組合**寫一個表驅動測試。這樣重排分支會直接編譯失敗或測試失敗。**成本極低，建議 Phase 1 就做。**

---

### B6 🟡 `("openai", "openai-responses")` 誤直通

前端 `Providers.tsx:29` 提供 `API_FORMATS = ["openai-chat", "openai-responses", "mixed", "anthropic", "gemini"]`，但 `proxy.rs` 的矩陣中沒有 `("openai", "openai-responses")` 這格 → 落入 `_ => TransKind::None` → **把 chat 格式的請求原樣打到 responses 端點**，必然失敗。

**修法**：加入 `("openai", "openai-responses") => { /* 400 + 明確訊息：需 Chat 兼容渠道 */ }`，或實作反向翻譯（OpenAI → Responses）。至少要先**明確報錯而非靜默直通**。

---

### B7 🟡 未註冊的 `pub` 函式

`cargo test` 的 3 個警告：

```
warning: function `restore_backup_to` is never used
warning: function `restore_backup` is never used
```

`tools.rs:2296`、`tools.rs:2489` 是 `pub fn`，但 `lib.rs` 的 `invoke_handler!` 清單（61 個 command）裡沒有對應 command。**這是「功能寫完了但沒接上 UI」的半成品**，或已廢棄的死碼。

**修法**：確認意圖後二選一 —— 註冊為 `switch_restore` 的變體（附 port 參數），或降為 `pub(crate)` / 移除。目前 `switch_restore` 走的是 `restore_backup_port`（`tools.rs:2493`），所以 `restore_backup` / `restore_backup_to` 可能是**被取代的舊 API 殘留**。

---

### B8 🟠 `commands.rs` 與 `db.rs` 零測試

| 檔案 | 行數 | 測試 | 風險 |
|---|---:|---:|---|
| `commands.rs` | 801 | **0** | 61 個 API 邊界的參數驗證、錯誤映射完全無保護 |
| `db.rs` | 446 | **0** | **Schema 與 migration 是資料真相層**——migration 寫錯等於使用者資料損毀 |

有趣的是 `usage.rs:1724` 有一個 `v3_tou_migrates_to_periods` 測試與 `seeds_apply_once_and_stay_deleted`，說明部分 migration 邏輯有測到，但 `db.rs` 本身的 schema 建立與版本升級沒有。

**修法（Phase 4）**：用 `tempfile`（已在 dev-dependencies）為 `db.rs` 建立「空 DB → `init_db` → 斷言 schema 版本與表齊全」以及「舊版 schema → 升級 → 資料不遺失」的測試。`commands.rs` 至少補參數驗證的單元測試（range 解析 `range_start`/`range_bucket_span` 是純函式，最好測）。

---

### B9 🟡 上游 timeout 600 秒且無 connect timeout

**證據**（`proxy.rs:1645-1647`）：

```rust
client: Client::builder()
    .timeout(std::time::Duration::from_secs(600))   // 10 分鐘
    .build()
```

`reqwest` 的 `timeout` 是**整體請求**上限（含串流讀取），沒有設 `connect_timeout`、也沒有 `read_timeout`。若上游 TCP 連得上但一直不回資料，請求會被佔用**整整 10 分鐘**，且 `RateLimiter` 的計數已消耗。多個卡死請求會耗盡連線。

**修法**：加 `.connect_timeout(Duration::from_secs(10))`，並考慮對串流設定 `read_timeout`（`reqwest` 0.12 支援），或改用 `tokio::time::timeout` 包裹首個位元組的等待。

---

## 4. 問題三：Git 版控與備份 —— ✅ 已完成

### 4.1 為何必須先做

重構的每一步都需要「可回滾」與「可 diff」。原本工作區**完全沒有版控**（`codex/.git` 是上游 clone 帶來的，與本專案無關），一旦改壞無法還原。

### 4.2 已執行內容

| 步驟 | 內容 |
|---|---|
| **① 離線快照** | `_backup/token-counter-snapshot-20260926-195436.zip`<br>9,909 項 / 56.7 MB / tar exit 0<br>**在清理之前建立**，是真正的「原始狀態」還原點<br>排除：`src-tauri/target/`（29 GB）、`node_modules/`（0.43 GB）、`_backup/` 自身<br>**已驗證零洩漏**，且關鍵檔案（含被刪的垃圾）皆在內 |
| **② 清理垃圾** | 9 個 patch 腳本 + 22 個 log + 1 個空檔（詳見 §2.2） |
| **③ `.gitignore`** | 排除建置產物、`codex/`、`icon-lab/`、`icons/concepts/`、`__pycache__/`、`.workbuddy/tmp/` |
| **④ `.gitattributes`** | 倉庫內統一 LF；Windows 腳本保留 CRLF；二進位標註；`pnpm-lock.yaml`／`Cargo.lock` 停止 diff |
| **⑤ 基準線 commit** | `38f060a`，**121 檔 / 1.6 MB**，工作樹乾淨<br>已驗證 `codex/`、`icon-lab/`、`target/`、`node_modules/`、`_backup/`、`*.log`、`icons/concepts/` **全部零洩漏** |

### 4.3 為何 29 GB 沒有進 git

`token-gateway/.gitignore` 原本只排除 `node_modules`、`dist`、`*.log`，**沒有排除 `src-tauri/target/`**（子目錄的 `src-tauri/.gitignore` 有 `/target/`，但依賴 git 已啟動才生效）。若直接 `git init`，第一次 `git add` 會嘗試 stage **29 GB / 26,697 檔**，必然卡死。這一步已預先處理。

### 4.4 備份策略建議

| 層級 | 機制 | 用途 |
|---|---|---|
| 逐次回滾 | Git（`38f060a` 起） | 每個重構步驟一個 commit，可 `git revert` / `git diff` |
| 離線保險 | `_backup/*.zip` | 磁碟整個工作區的完整還原點 |
| **建議新增** | 每個 Phase 結束時 tag（`v0.1.0-phase1`） | 讓「回到 Phase 1 結束的狀態」變成一個指令 |

**建議的 commit 慣例**：`refactor(proxy): 抽出 forward 階段` / `fix(translate): 剝離 encrypted_content`，並**在改動前先跑一次 `cargo test` 確認基線**。

---

## 5. 問題四專章：400 連線錯誤與 Codex 會話續用

> 你對這一項回答「不確定要怎麼做」。以下是**我的具體建議**。

### 5.1 先把兩個症狀分開

| 症狀 | 現象 | 判定歸屬 |
|---|---|---|
| **A. 連線錯誤（通常 400）** | 工具跳出 400，訊息可能是中文（網關產生）或英文（上游原文透出） | 翻譯／相容層（`proxy.rs` + `translate.rs`） |
| **B. Codex 舊會話無法續用／無法封存** | Codex 報 `Model provider not found` 或直接開不起來 | 接管層（`tools.rs`，見 B3） |

**兩者不是同一個 bug，但都源自同一個設計哲學問題**（見 §5.2）。**建議分開修，但用同一套觀測設施。**

### 5.2 核心診斷：目前的相容策略本質上是脆弱的

網關面對「上游不認某個欄位就回 400」的做法是**事後剝離重試**（`proxy.rs:1219-1240`）：

```
送請求 → 400 → 用 regex 從錯誤訊息猜出欄位名 → 剝掉 → 記在記憶體 → 重送一次
```

**這有五個結構性問題**：

| # | 問題 | 證據 |
|---|---|---|
| 1 | **記憶不持久，每次重啟歸零** | `stripped` 是 `ProxyCtx` 的記憶體 `HashMap`（`proxy.rs:34`），`serve()` 每次 `Default::default()`（`proxy.rs:1650`）。**每次啟動網關，每個渠道都要重踩一次 400**（見 B2） |
| 2 | **依賴上游錯誤訊息格式** | `parse_unknown_fields()` 只認兩種口徑：`unknown field "x"`（New-API 系）與 `Unsupported parameter(s): 'x'`（OpenRouter）。**第三種口徑就完全失效**，直接把 400 透給使用者 |
| 3 | **是「失去能力」而非「協商能力」** | 剝掉 `prompt_cache_key` → 失去前綴快取路由；剝掉 `stream_options` → 失去用量統計。**功能靜默降級，使用者不知道** |
| 4 | **重複成本** | 第一次請求已經完整送到上游（可能已計費、已進入推理），失敗後再送一次 → 雙倍延遲 |
| 5 | **不可觀測** | 剝離了什麼只存在記憶體、沒有落庫。事後無法回答「這個 400 到底剝了什麼、剝完成功了沒」 |

**另外，「body 解析失敗」這條線索經細算後，我判定它極可能是假警報 —— 而不是真 bug。** 這個推理過程值得完整記錄，因為它示範了為什麼第 0 層必須先做。

我手上有**三份**目測矛盾的 debug 輸出：

| 檔案 | `in_fmt` | `body_json_is_some` | 日誌中的 `bytes_len` | 該 body 的**真實**長度 |
|---|---|---|---:|---:|
| `...-body-parse-OK.txt` | `anthropic` | **true** | **132** | 132 ✅ 吻合 |
| `...-body-parse-FAILED.log` | `anthropic` | false | **107** | 123 ✗ |
| `...-body-parse-FAILED-openai.log` | `openai` | false | **69** | 83 ✗ |

**關鍵在於：三者的 `bytes_len` 用了兩套不同的算法。**

- **成功案例**：`test_anthropic.py` 經 `json.dumps` 產生的 body，我把每個 token 逐一加總（含 `": "` 與 `", "` 分隔符）＝ **正好 132** → 與日誌吻合 ⇒ 這個 `bytes_len` 是**原始位元組長度**。
- **失敗案例一**：`test_body.json` 為 123 bytes；扣掉 16 個雙引號字元 ＝ **正好 107** → 與日誌吻合 ⇒ 這個 `bytes_len` 是**剝掉引號後的長度**。
- **失敗案例二**：`{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"What is 2+2?"}]}` 為 83 bytes；扣掉 14 個引號 ＝ **正好 69** → 與日誌吻合 ⇒ 同樣是**剝掉引號後的長度**。

**兩個獨立的失敗樣本都呈現「`bytes_len` ＝ 去引號後長度」，而唯一的成功樣本用的是原始長度。** 這強烈指向：**產生那兩筆失敗日誌的 debug 版本，是在剝掉引號之後（或直接對被剝掉的緩衝區）才做 JSON 解析判定** —— 也就是說，**是那段已被刪除的 debug 程式碼自己把 body 弄壞的，不是 `parse_body_json` 有 bug。**

**結論修正**：
- ❌ 我**不再**把這個 400 列為「已證實的 body 解析 bug」。
- ✅ 它極可能是**已被移除的 debug instrumentation 造成的假警報**。
- ⚠️ 但這仍是**推論而非證明**（也有可能那段 debug 碼只影響輸出、而真 body 另有問題，例如後半段含非法 UTF-8 —— 前 130 字的預覽看不出來）。

**因此修法不變，而且更有必要**：不要猜，**下次失敗時把原始位元組以 hex/base64 落庫**。這正是 §5.3 第 0 層的核心。**在拿到 hex 之前，我不會針對這個 400 改任何程式碼。**

> 📎 三份證據已全部保全於 `docs/evidence/`。**注意**：這些是「已刪除的 debug 版本」的產物，**不代表當前程式碼有 bug** —— 現行程式碼已無這些 `GW-DBG` 輸出。

### 5.3 我的建議：四層處理，由外而內

**核心原則：先讓問題看得見，再修它。** 目前所有 400 都是黑箱，直接改翻譯邏輯會變成盲猜。

---

#### 第 0 層：可觀測性（先做這個，且不改任何既有行為）⭐

**為什麼先做**：這一層不改變任何請求處理邏輯，因此**零回歸風險**，但它是後面三層的前提。沒有它，你無法確認任何修復是否真的有效。

**要做什麼**：

1. **新增請求追蹤表**（`db.rs` migration）：
   ```sql
   CREATE TABLE proxy_trace (
     id             INTEGER PRIMARY KEY,
     ts             INTEGER NOT NULL,
     app            TEXT,
     model          TEXT,
     in_fmt         TEXT,      -- detect_in_format 結果
     target_fmt     TEXT,      -- provider.api_format
     trans_kind     TEXT,      -- none / anthropic_to_chat / responses_to_chat
     upstream_status INTEGER,
     stripped_fields TEXT,     -- JSON 陣列：這次剝了什麼
     retry_count    INTEGER,
     body_sha256    TEXT,      -- 請求體指紋（不存明文，保護隱私）
     latency_ms     INTEGER,
     trace_level    TEXT       -- info / debug
   );
   ```
2. **失敗時記錄原始位元組**：當 `body_json_is_some == false`，把 body 的**前 512 bytes 以 hex 落庫**（不落明文，避免金鑰外洩），並記 `Content-Length`、`Content-Type`、`Content-Encoding`。**這直接解決 §5.2 那個無法判定的 400。**
3. **上游原文完整落庫**：目前 `upstream_err_text()` 截斷 300 字（`proxy.rs:636`），且只在錯誤時透出。改為**完整存進 `proxy_trace`**（UI 顯示時再截斷）。
4. **擴充 `codex_doctor` / 新增「網關診斷」頁**：一鍵對每個啟用渠道發一次最小探測請求，顯示：可達性、耗時、狀態碼、上游原文、被剝離欄位。

**驗收標準**：能回答「過去 24 小時所有 4xx 請求，各自剝了什麼欄位、上游原文是什麼、第幾次重試成功」。

**實作結果（Phase 1 ✅）**：已落地，與上面的草案有幾處刻意偏離：

- 表名與欄位以實作為準（見 `db.rs` 的 `proxy_trace`）。多存了
  `content_length` / `content_type` / `body_hex` / `upstream_error` / `note`，
  `level` 而非 `debug`；`model` 拆成 `model_raw`。**沒有**存
  `Content-Encoding`（`reqwest` 已是 `identity`，無意義）。
- 第 1 點的 `trace_level` 最終只用了 `info` / `warn` 兩級：warn＝確實出錯，
  info＝有剝離或重試但最終成功。
- **只寫異常請求**（草案未言明）。正常請求不寫，避免日誌洪水；
  網關啟動時裁剪至最近 5000 筆。
- 第 4 點的「主動探測每個渠道」**未做** —— 它會對上游產生真實流量與費用，
  且屬於 Phase 2 的能力宣告（§5.4）範疇。目前只做被動記錄。
- 診斷頁（`Diagnostics.tsx`）已上線：總覽、上游能力記憶（可逐渠道重設）、
  請求追蹤（可展開看完整上下文與可複製的 hex）。

**尚未定案**：§5.2 的 body 解析失敗 400 仍需**真實重現一次**才能定案 ——
觀測設施已就位，但修正後的網關還沒遇到該情況，故診斷頁上
「body 解析失敗」目前應為 0。這正是這一層的用途：下次它出現時會直接給出答案。

---

#### 第 1 層：低風險直接命中（純函式 + 單測）—— ✅ 已於 Phase 1 完成

| 項目 | 改動 | 風險 | 狀態 |
|---|---|---|---|
| B1 `strip_encrypted_content` | 新增純函式 + 呼叫點（見 §3 B1，程式碼已備妥） | 極低 | ✅ |
| B2 剝離記憶持久化 | `stripped` 改讀寫 `provider_stripped_fields` 表 | 低 | ✅ |
| B5 格式矩陣窮舉化 | 表驅動 + **30 格**全測（原估 20 格，實際是 5 入站 × 6 渠道協議） | 極低 | ✅ |
| B6 補 `("openai","openai-responses")` 明確報錯 | 一個 match 分支 | 極低 | ✅ |
| B9 加 `connect_timeout` | 一行 | 低 | ✅ |
| B7 清理 dead code | 確認後刪除或降級可見性（採「降為測試 helper」） | 極低 | ✅ |

**驗收結果**：`cargo test --offline` **179 passed / 0 failed / 8 ignored**（≥ 146 ✅）。
`cargo clippy --all-targets` exit 0，`dead_code` 歸零（餘 5 條為既有風格提示，非新增）。

---

#### 第 2 層：翻譯層健壯化（需第 0 層支撐）

1. **`wire_api` A/B 實驗**（見 B4）：對 `api_format = "openai-chat"` 的上游，實測 `wire_api = "chat"`（純透通）vs `"responses"`（強制翻譯）的 400 發生率。**這是可能直接消滅大部分 400 的一個開關。**
2. **把「猜」換成「宣告」**（見下方策略建議）。
3. **Responses↔Chat 翻譯補整合測試**：用 mock 上游重現三種真實 400 口徑，鎖住行為。

---

#### 第 3 層：Codex 會話（需你提供錯誤訊息才能定位 B3 的封存問題）

1. B3 修法全部執行（多檔探測、聯集 rollout、失敗要大聲、`busy_timeout`、別名持久化）。
2. 為「`state_5.sqlite` 改名為 `state_6.sqlite`」寫測試案例。
3. 「無法封存對話」需你重現時提供 Codex 原始錯誤 → 再決定是網關側還是 Codex 側。

---

### 5.4 策略建議：從「事後猜測」改為「能力宣告」

這是我對這整個問題**最重要的一條建議**。

**現狀**：網關用「送了才知道、錯了才剝」來適配上游。這把主導權交給了上游的錯誤訊息格式。

**建議**：改為**持久化的能力設定檔（capability profile）**。

```
① 在 providers 表新增 capabilities TEXT（JSON）
   或在 UI 提供「上游能力」面板，讓使用者明確勾選：
     □ 支援 stream_options.include_usage
     □ 支援 prompt_cache_key
     □ 支援 reasoning_effort
     □ 支援 parallel_tool_calls
     □ 支援 custom tools
     □ 接受 encrypted_content

② 首次探測結果（或使用者手動設定）持久化到 DB
   → 重啟不再重踩 400（解決 B2）

③ 請求組裝時一次組對，而不是先送錯再剝
   → 消除問題 3（能力靜默喪失）與問題 4（雙倍延遲）

④ 剝離事件仍寫入 proxy_trace（第 0 層）
   → 可事後稽核「這家上游到底少了什麼能力」

⑤ 保留「事後剝離」作為 fallback，但把學到的結果寫回 DB
   → 漸進式遷移，不需要一次改完
```

**為什麼這個方向對**：它把「上游能力的真相」從**易失的進程記憶體**搬到**持久化的資料庫**，並且從「被動猜測」變成「主動宣告 + 可稽核」。同時它讓使用者能**手動修正**（當某家上游的錯誤訊息口徑特殊時，不必等網關支援）。

**成本**：Phase 1 的持久化是這個方向的第 ①⑤ 步，其餘可漸進。**不需要一次做完。**

---

## 6. 重構路線圖

> 依你的選擇，**Phase 0（本階段）只產出文件、不動程式碼**。以下為後續規劃。

### Phase 0 ✅ 已完成（本階段）

- [x] 完整架構與 bug 診斷（本文件）
- [x] zip 離線快照 + Git 基準線（`38f060a`）
- [x] 垃圾清理、`.gitignore` / `.gitattributes`
- [x] 保全診斷證據到 `docs/evidence/`（3 份，含推導出 §5.2 結論的關鍵矛盾）
- [x] 殘留清理（§2.3）＋ 驗證腳本整理進 `scripts/`
- [x] 決策：`target/` 保留（理由見 §6.1）

### Phase 1 ✅ 已完成：低風險命中 + 觀測設施

**目標**：把所有 400 變成可見，並修掉零風險的 bug。**不拆檔。**（未拆檔 ✅）

- [x] §5.3 第 0 層：`proxy_trace` 表 + body hex 落庫 + 上游原文全存 + 診斷頁
- [x] §5.3 第 1 層：B1（`encrypted_content`）、B2（記憶持久化）、
      B5（矩陣編譯器窮舉）、B6（chat→responses 明確報錯）、
      B7（dead code 處置）、B9（connect_timeout）
- [x] 補 `db.rs` migration 測試（B8 的一部分，10 個）
- [x] 新增 `provider_stripped_fields` 表（B2 的落地處）
- [x] `provider_delete` 一併清掉該渠道的能力記憶（避免孤兒列）
- [x] 前端「診斷」分頁（總覽 / 上游能力記憶 / 請求追蹤）

**驗收結果**：`cargo test --offline` **179 passed / 0 failed / 8 ignored**（≥ 146 ✅）；
`npx tsc --noEmit` 0 錯誤；`npx vite build` 成功；`cargo clippy --all-targets`
exit 0 且 `dead_code` 歸零；能查詢任一 400 的完整上下文（含原始 hex）✅

**提交紀錄**（每個邏輯單元一顆，皆可獨立建置）

| commit | 內容 |
|---|---|
| `b4561eb` | `db`: schema v8 + 10 個 migration 測試 |
| `373a586` | `proxy`: B1/B2/B5/B6/B9 + 追蹤寫入路徑 |
| `1c87a24` | `commands`: 診斷中心 API |
| `f2118f5` | `providers`: 刪除渠道時清掉能力記憶 |
| `8f2b81d` | `web`: 診斷中心頁面 |
| `ab7ee6a` | `tools`: B7 dead code 處置 |

**過程中發現、且值得記下的事**

1. **B6 的修正當場抓到一個假陽性測試**（詳見 §3.2）。
2. **我對 B7 的初判有錯**：它不是「同義重複」，`restore_backup_to` 傳
   `migrate: false`、生產路徑傳 `migrate: true`，是語義差異。改寫後 7 個
   測試立刻失敗即為證據（詳見 §3.2）。
3. **尚未定案**：§5.2 的 body 解析失敗 400。觀測設施已就位，但需要
   **真實重現一次**才能定案 —— 目前資料庫裡 `with_body_hex` 應為 0，
   因為修正後的網關還沒遇到該情況。下一步是請你在實際使用中留意診斷頁
   「body 解析失敗」那格是否變為非 0。

### Phase 2：拆 `proxy.rs`（最大技術債）—— ✅ 完成（12 步）

**目標**：`proxy_handler` 從 868 行降到 < 80 行。**行為完全不變**，只重構結構。

**目前進度：`proxy_handler` 909 → 86 行**（實測；起點比原估的 868 行更長），
九個步驟各自獨立成一次提交，每步都跑完整測試後才提交。

| 步驟 | 提交 | 內容 | handler |
|---|---|---|---|
| 1 | `7602086` | `reject()` 統一 11 處「記日誌 + 回錯誤 JSON」；`infer_app` 只算一次；`provider_name()` | 909 → 870 |
| 2 | `e6a7a8e` | `relay_sse` 通用逐行 SSE 轉送 + `StreamLog` 記帳 + `sse_response` | 870 → 655 |
| 3 | `7c312dc` | `translate_forward_body` + `strip_for_upstream`（`BodyPrep` 而非 `Result<_, Response>`） | 655 → 589 |
| 4 | `f946937` | `Upstream` 連線物件 + `RetryCtx` + `send_with_strip_retry` | 589 → 467 |
| 5 | `859b8fc` | `FinishCtx` + `finish_response`（標頭轉發、三種串流分支、非流式整包解析、記帳） | 467 → 278 |
| 6 | `22d353a` | `Prelude` + `prelude`（Bearer → 本地 Key／直連回退 → 限流 → 讀請求體；5 個拒絕出口） | 278 → 237 |
| 7 | `b6dc4ce` | `resolve_model`（模型／來源白名單 + 跨來源路由；4 個拒絕出口） | 237 → 165 |
| 8 | `8c408e3` | `PrepareInput`／`Prepared` + `prepare_request`（矩陣判定 → 轉譯 → 剝離） | 165 → 101 |
| 9 | `dce9321` | `ReqMeta`／`request_meta` + `upstream_for`（請求元資料、上游連線物件） | 101 → **86** |
| 10 | `35d7eac` | 測試 1,898 行移到 `proxy/tests.rs`（`proxy.rs` + `proxy/` 子目錄形式，**不需要 `mod.rs`**） | 86（不變） |
| 11 | `c4b626f` | 拆出 `proxy/matrix.rs`（`TransKind`／`InFmt`／`TargetFmt`／5 個 `E_*`／`resolve_trans_kind`） | 86（不變） |
| 12 | `ded205c` | 拆完其餘八個子模組（`strip`／`util`／`logging`／`stream`／`forward`／`retry`／`finish`／`pipeline`） | 86（不變） |

`proxy_handler` 現在是一條六段具名管線，每段一個函式、各自有 doc：

```
prelude  →  request_meta  →  resolve_model  →  prepare_request
         →  upstream_for + send_with_strip_retry  →  finish_response
鑑權限流    請求元資料       白名單與路由      矩陣/轉譯/剝離    送出與重試        收尾與記帳
```

**實作階段發現、值得記下的四件事**

1. **`&rusqlite::Connection` 不能跨越 `await`。** 抽出 `send_with_strip_retry`
   時第一版傳入 `conn: &Connection`：`cargo check` 過了，但 axum 的
   `Handler` 突然不成立。原因是 `Connection` 是 `Send` 但**不是 `Sync`**，
   故 `&Connection` 不是 `Send`，整個 handler future 退化成 `!Send`。
   編譯器的錯誤訊息完全沒提 Send，只說「Handler trait 未滿足」——
   這是日後拆檔時最容易再踩一次的坑。改傳 `db_path`，寫入時開短命連線；
   需要跨階段持有連線時，**傳所有權**（見 `Prelude.conn`）。
   反過來說，**同步**函式（如 `resolve_model`、`prepare_request`）借用
   `&Connection` 完全沒問題——這個區別已寫進這兩個函式的 doc。
2. **`clippy::result_large_err`**：`Result<_, Response>` 的 Err 有 128 bytes，
   改 `Box<Response>`。
3. **留痕必須記「原始」body**：重構中 `nb`（剝完的）與 `body`（原始的）
   極易混淆；若記成剝完的，「這次剝了什麼」就永遠看不出來，整個觀測設施
   的意義歸零。
4. **`tokio::spawn` 要求 `'static`，借用脈絡不能搬進 task。**
   `finish_response` 的三個串流分支都必須在 `async move` **之外**先
   `f.app.to_string()` 等做出 owned 字串。把「借用脈絡」與「spawn」放進
   同一個函式時一定會遇到，已寫進該函式 doc。

**最終落地（第十二步實測）**

`proxy.rs`：4,195 → **270 行**（不含空行 245）✅ 目標 < 600 達成。

| 檔案 | 行數 | |
|---|---:|---|
| `proxy.rs` | 270 | 常數、狀態型別、`proxy_handler`、`serve`、子模組地圖 |
| `proxy/tests.rs` | 1,907 | ⚠️ **唯一超標的檔案**（見下） |
| `proxy/pipeline.rs` | 393 | |
| `proxy/forward.rs` | 261 | |
| `proxy/finish.rs` | 258 | |
| `proxy/logging.rs` | 231 | |
| `proxy/strip.rs` | 210 | |
| `proxy/util.rs` | 204 | |
| `proxy/stream.rs` | 191 | |
| `proxy/matrix.rs` | 181 | |
| `proxy/retry.rs` | 179 | |

**唯一未達標項：`proxy/tests.rs` 1,907 行 > 600。** 該檔有 37 個測試、**沒有
共用 helper**（每個 e2e 測試自建 mock 上游），所以可以無痛再拆成
`proxy/tests/{unit,e2e_basic,e2e_translate,e2e_strict}.rs`。列為後續可選工作
—— 測試檔的導航成本遠低於實作檔，故未列入本次驗收。

**剩下的 Phase 2 附帶項**：§5.3 第 2 層（`wire_api` A/B、能力宣告）——
需要你實測後才能決定方向。

#### 拆檔配方（後續模組一律照這個做，已用 `matrix.rs` 驗證過）

使用 Rust 2018 的 **`proxy.rs` + `proxy/` 子目錄**形式：`proxy.rs` 仍是模組根，
子模組放 `proxy/xxx.rs`，**不需要 `proxy/mod.rs`**（也不需要搬動 `proxy.rs`）。

1. 把要搬的**連續區塊**切到 `proxy/xxx.rs`。
2. 對區塊內**頂層**的 `enum` / `fn` / `const` 加 `pub(super) ` 前綴。
3. ⚠️ **`impl` 區塊不能加可見性限定符** —— `pub(super) impl X {}` 會編譯失敗
   （`error[E0449]: visibility qualifiers are not permitted here`）。
   可見性要加在 **impl 內的方法**上（`    pub(super) fn from_path(…)`）。
4. 子模組開頭視需要加 `use super::*;`（子模組可存取父模組的私有項目，
   glob import 也會帶入父層的私有 `use` 別名）。若該模組完全自足則不要加，
   否則會多一條 `unused_imports`。
5. ⚠️ **父模組要用 `use xxx::{…};` 把搬走的項目重新引進自己的 scope**，
   否則 `proxy.rs` 內所有引用都會「找不到」。
6. ⚠️ **孫模組的項目不會被 `use super::*` 帶進子模組。** `proxy/tests.rs` 是
   `proxy` 的子模組，`use super::*` 只涵蓋 `proxy` 自身的綁定；若常數下移到
   `proxy::matrix`，測試必須明確 `use super::matrix::{…}`。

#### 當時的模組切法規劃（依依賴關係由外而內）—— 已於第十二步執行

| 模組 | 內容 | 粗估 |
|---|---|---|
| `proxy/strip.rs` | `parse_unknown_fields`／`parse_body_json`／`strip_json_field`／`strip_unsupported_tools`／`strip_encrypted_content`／`sanitize_passthrough_chat_body`／`upstream_err_text` | ~210 |
| `proxy/util.rs` | `open_conn`／`normalize_model`／`infer_app`／`bearer`／`join_upstream`／`is_hop_header`／`should_inject_usage`／`strip_key_param`／`model_from_path`／`check_port` | ~250 |
| `proxy/log.rs` | `extract_usage`／`SseAcc`／`insert_log`／`LogRow`／`recent_logs`／`err_json`／`reject`／`log_reject` | ~200 |
| `proxy/stream.rs` | `responses_line_events`／`relay_sse`／`StreamLog`／`sse_response`／`ResponsesRelay`／`AnthropicRelay` | ~190 |
| `proxy/forward.rs` | `build_upstream_target`／`provider_name`／`resolve_model_provider`／`ReroutedProvider`／`TransSpec`／`ForwardBody`／`BodyPrep`／`translate_forward_body`／`strip_for_upstream`／`Upstream`／`RetryCtx`／`send_with_strip_retry`／`FinishCtx`／`finish_response` | ~560 |
| `proxy/pipeline.rs` | `Prelude`／`prelude`／`resolve_model`／`PrepareInput`／`Prepared`／`prepare_request`／`ReqMeta`／`request_meta`／`upstream_for` | ~390 |

`proxy.rs` 最後留下：三個 const、`ProxyCtx`／`RateLimiter`／`ProxyState`／
`RunningProxy`、`proxy_handler`（86 行）、`serve`，加上 `mod` 宣告與 `use`
清單 —— 預期落在 **450–600 行**，剛好達到目標。


**為何 handler 停在 86 行、不再往下壓**

剩下的 6 行全部來自同一處：`PrepareInput`／`RetryCtx`／`FinishCtx` 三個階段
脈絡各自重複列出 `ctx`/`started`/`app`/`model_raw`（`RetryCtx` 還多一個
`content_type`）。唯一正確的解法是引入共用的 `ReqCtx` 讓三者內嵌它，但那會
牽動約 **56 處欄位存取**（`f.ctx` → `f.req.ctx`、`input.app` → `input.req.app`、
`rc.started` → `rc.req.started` …），換來 **7 行**。

判斷：**這是行數高爾夫，不是設計改進，故不做。** 86 行已經是一條六段具名
管線，原本「單一 869 行函式」的可讀性問題已經解決。若日後真的要滿足這個
硬指標，共用的 `ReqCtx` 是唯一正確路徑。

**驗收**：179 個測試**一行不改**全部通過（十二個步驟皆如此 ✅）；
`cargo clippy --all-targets` exit 0，僅餘 5 條既有警告（`price_extract.rs` 1 條、
`tools.rs` 4 條，皆為 Phase 1 前就存在）✅；`cargo build` exit 0 ✅；
`npx tsc --noEmit` 0 錯誤 ✅；
`proxy.rs` < 600 行 → **270 行 ✅**；
`proxy_handler` < 80 行 → **86 行 ❌**（差 6 行；理由見上，判斷為不值得的代價）。
`proxy/tests.rs` 1,907 行 > 600 → **❌ 未達標**（可選後續工作，見上）。

### Phase 3：拆 `tools.rs` + 修 Codex 會話（B3）

- 拆為 `tools/{mod,claude,codex,opencode,hermes,dsh,cursor,antigravity}.rs`
- B3 五項修法全部執行
- 需要你提供「封存對話」的 Codex 原始錯誤訊息

**驗收**：`state_5` → `state_6` 改名測試通過；`tools.rs` < 400 行/檔。

### Phase 4：補測試 + 拆前端

- `commands.rs`、`db.rs` 測試（B8）
- `Providers.tsx`（2,766 行）→ `providers/` 目錄，`SwitchDialog` 單獨一檔
- `Usage.tsx`（1,605 行）→ `usage/` 目錄，每個 lens 一檔

**驗收**：`tsc --noEmit` 0 錯誤；每個新檔案 < 400 行。

### Phase 5（選配）：架構收斂

- 統一錯誤型別（目前全部是 `Result<_, String>`，61 個 command 全靠字串約定）
- `translate.rs` 依格式拆為 `translate/{anthropic,responses,chat}.rs`

---

### 6.1 已決定的兩件事（✅ 已結案）

1. **`src-tauri/target/`（29 GB）→ 決定保留。** 因為要立即進入 Phase 1，而 Phase 1 第一步就是 `cargo test`；清掉 target 會白花一次 10 分鐘以上的完整重建。**待 Phase 1 完成、進入穩定期後再釋放。**
2. **§2.3 殘留清理 → 決定全部執行**（`tauri-dev.log`、根目錄 `gen_icons.py`、4 份重複 debug 檔），並把 3 個驗證腳本整理進 `scripts/`。**已於本次完成**，其中兩份 debug 檔的差異內容已先保全至 `docs/evidence/`（見 §2.3）。

**當前狀態**：Phase 0 全部結案 → **進入 Phase 1**。

---

## 7. 風險與回滾策略

| 風險 | 緩解 |
|---|---|
| 重構改壞翻譯邏輯 | 146 個測試是安全網；每個 Phase 結束必須確認 **不低於 146 passed** |
| 純重構不小心改了行為 | Phase 2 的驗收標準是「測試一行不改就通過」——若必須改測試，代表不是純重構 |
| 上游行為無法在 CI 重現 | 用 mock 上游（`proxy.rs` 已有 `axum` mock 測試基礎設施，見 L1885 起的 `#[tokio::test]`） |
| 誤刪 Codex／icon 資產 | 已全部在 zip 快照內；`codex/` 與 `icon-lab/` 僅是 gitignore，**檔案仍在磁碟** |
| 改壞使用者真實設定 | `apply_switch` 前自動備份到 `backups/<app>/`（保留 10 份，`tools.rs:25`）；8 個 live 測試會真的寫入 `~/.claude` 等，**執行前務必確認備份存在** |
| 資料庫 migration 出錯 | `db.rs` fail-fast（`lib.rs:27`）；Phase 1 起補 migration 測試 |

**回滾指令**：
```powershell
# 回到基準線
git -C 'D:\token counter' reset --hard 38f060a

# 從 zip 還原單一檔案
tar -xf '_backup\token-counter-snapshot-20260926-195436.zip' './token-gateway/src-tauri/src/proxy.rs'
```

---

## 8. 附錄

### 8.1 證據檔案

| 檔案 | 內容 |
|---|---|
| `docs/evidence/2026-09-26-body-parse-OK.txt` | `bytes_len=132`（＝原始長度）、`body_json_is_some=true` → 成功對照組 |
| `docs/evidence/2026-09-26-body-parse-FAILED.log` | `bytes_len=107`（＝去引號長度）、`body_json_is_some=false`、`translated but fwd_value=None!` → 失敗案例一 |
| `docs/evidence/2026-09-26-body-parse-FAILED-openai.log` | `bytes_len=69`（＝去引號長度）、`body_json_is_some=false`、`in_fmt=openai` → 失敗案例二 |

> 三者的 `bytes_len` 用了兩套算法（成功＝原始、失敗＝去引號），此矛盾是 §5.2 判定「假警報」的依據。

### 8.2 重現指令

```powershell
# 健康度基線
cd 'D:\token counter\token-gateway\src-tauri'; cargo test --offline

# 前端型別檢查
cd 'D:\token counter\token-gateway'; npx tsc --noEmit

# live 測試（會寫入真實工具設定，請先確認備份）
cd 'D:\token counter\token-gateway\src-tauri'; cargo test live_ -- --ignored --test-threads=1

# 手動 E2E（Anthropic 格式；需先啟動網關並設定 TOKEN_GATEWAY_KEY）
cd 'D:\token counter\scripts'; python test_anthropic.py
```

### 8.3 關鍵座標速查

| 項目 | 位置 |
|---|---|
| `proxy_handler` 巨型函式 | `proxy.rs:774-1642` |
| 格式矩陣 | `proxy.rs:966-1024` |
| 400 剝離重試 | `proxy.rs:1219-1240` |
| `parse_unknown_fields` | `proxy.rs:474-514` |
| `strip_encrypted_content`（**不存在，待新增**） | `proxy.rs` ~L1195 |
| 剝離記憶（易失） | `proxy.rs:34`、`proxy.rs:1650` |
| HTTP client timeout | `proxy.rs:1645` |
| `codex_legacy_providers` | `tools.rs:999-1036` |
| `wire_api` 寫入 | `tools.rs:1110` |
| 共享段名常數 | `tools.rs:22`（`CODEX_SHARED_PROVIDER_ID = "custom"`） |
| `stripped` 相關函式 | `proxy.rs:618` `remember_stripped` |
| Codex SQLite 檔名（上游） | `codex/codex-rs/state/src/sqlite.rs:29-34` |

### 8.4 本次分析使用的方法

所有結論均基於**實際執行**（`cargo test`、`npx tsc`、`grep`、`git`）與**原始碼閱讀**，非推測。凡屬推測者均已明確標註（例如 §3 B3 的「無法封存對話」根因、§3 B4 的 `wire_api` 修正方向）。**凡無法確定者，本文件選擇標示不確定，而非給出未經驗證的結論**（參見 §5.2 對 `gw_debug.log` 的重新評估）。

---

*本計畫書為 Phase 0 交付物。下一步請你決定 §6.1 的兩件事，以及是否進入 Phase 1。*
