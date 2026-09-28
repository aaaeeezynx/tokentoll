# 測試指南：Phase 1 ～ Phase 5

> 這份文件是給「實際動手測」用的。每一項都有**確切的指令**與**明確的通過
> 標準**，讓你可以自己判斷有沒有過，而不是只能憑感覺。
>
> 對應的技術細節在 [`REFACTORING-PLAN.md`](./REFACTORING-PLAN.md)。
>
> **只想看「我自己要動手做什麼」→ 直接看 [`MANUAL-TESTS.md`](./MANUAL-TESTS.md)。**
> 那份是精簡版（7 個測試，含唯一還沒跑過的測試 C）；本檔是完整紀錄。
>
> **最新狀態（2026-09-28 23:40）**：後端 **228 passed / 0 failed / 8 ignored**、
> clippy 0 警告、`tsc --noEmit` exit 0、205 檔全部 ≤ 400 行。這一輪做的是
> **「關閉網關要回到該工具的原生來源」**（§0.9，Codex 已實機驗證；OpenCode 於
> §0.9.8 實機驗證並順手修掉一個長期的摘要瑕疵）。手動複驗步驟見
> [`MANUAL-TESTS.md`](./MANUAL-TESTS.md) 第 9 節。

---

## 0. 這次要測的七件事

| # | 測什麼 | 為什麼重要 |
|---|---|---|
| **A** | 診斷中心能不能用 | Phase 1 做的可觀測設施**從未真正跑起來過**（見下方說明） |
| **B** | 網關拒絕會不會留痕 | Phase 1.5 修的**主要缺口**。修正前，「連線錯誤 400」是唯一不會出現在診斷頁的那一類 |
| **C** | 原本的 400 / Codex 會話症狀有沒有改善 | 你的原始問題 |
| **D** | Codex 舊會話的 provider 別名（B3，已修） | 修正前讀取失敗會**靜默變成「沒有別名」**，看起來一切正常 |
| **E** | `wire_api` 是否跟著上游能力走（B4，**已修但修正本身有缺陷，2026-09-28 再修**） | 最初「不論上游是誰都寫死 `responses`」；改成「宣告優先」後又**反過來**把指向網關的段寫成 `chat` —— 而 Codex 已不接受 `chat`，導致 **「Unable to log in」**。見 §0.6 |
| **F** | DSH 用量為何不出現在 Token 趨勢 | **你回報的 bug**。已查明：**標籤是 bug，趨勢圖不是** —— DSH 目前設定直連 `opencode.ai`，根本沒經過網關 |
| **G** | 本機工具標籤應顯示「DeepSeek Harness」 | **你回報的 bug**（已修）。前端寫成「DeepSeek」，與後端不一致 |

> **為什麼 A 是「從未跑起來過」**：目前安裝在
> `%LOCALAPPDATA%\token-gateway\token-gateway.exe` 的是 **2026-09-24 22:38**
> 的建置。實測該檔二進位中**不含 `proxy_trace`** 字串 —— 也就是說它是 Phase 1
> **之前**的版本，診斷中心那整塊程式碼根本不在裡面。你的資料庫也還是 **v7**，
> 同樣證明 Phase 1 的程式沒被執行過。這次的建置是**第一個含 Phase 1～5 的版本**。

> **本輪（Phase 3～5）的改動屬於「結構重組」**：拆檔、補測試、錯誤型別。
> 這些**不該改變任何可見行為**。驗證方式是 bundle 大小幾乎不變
> （前端 542.40 → 542.38 kB）、測試全綠、以及拆 `keys.rs` 錯誤型別時
> 逐字比對過所有中文訊息（15 → 15，零改字）。**若你看到非預期的行為變化，
> 那是我拆壞了，請回報。**

> **目前狀態（2026-09-28 22:30 更新）**：測試 **A 已通過**（2026-09-27 22:07
> 你提供的截圖，已與資料庫逐項核對）。**B、D 已由我代跑並全部通過**；
> **E 在代跑時抓到一個真實故障**（接管會寫出 Codex 已不接受的
> `wire_api = "chat"`，導致 Codex 報「Unable to log in」），**已修正**，
> 並在安裝後的正式版本上用**真實 UI 點擊**驗證過「App 會把壞掉的 `chat`
> 修回 `responses`」—— 詳見 §0.6、§0.7.5。
> **C 仍然只能你自己在真實使用情境下測**（見 §6 與
> [`MANUAL-TESTS.md`](./MANUAL-TESTS.md)），**這是唯一還沒跑過的測試**。
> 後端測試 **226 passed / 0 failed / 8 ignored**、clippy **0 警告**。
> 最新的實機驗證與建置見 §0.5、§0.6、§0.7、§0.9 與 §9.9。
>
> <small>（以下為歷史紀錄，數字是當時的基線，已被上面的現況取代）</small>

> **2026-09-28 更新**：用量頁第 8 個鏡頭「工具」已完成，程式已重新建置、
> **安裝到 `%LOCALAPPDATA%\token-gateway`（2026-09-28 02:43）並實機驗證**。
> 後端測試 **219 passed / 0 failed / 9 ignored**。詳細證據見 §0.1 與 §9.1。

> **2026-09-28 03:28 更新（第一階段：工具區精簡 D-1／A／E）**：三個方向的程式
> 改動已完成並重新建置、安裝、實機驗證。後端測試 **223 passed / 0 failed /
> 9 ignored**。詳細證據見 §0.2 與 §9.3。

> **2026-09-28 04:2x 更新（第一階段：F 移除 hermes 支援）**：hermes 支援已整塊
> 移除（−783 行），並先把 hermes 自己的設定檔殘留外科清理乾淨。後端測試
> **216 passed / 0 failed / 8 ignored**（少掉的 7 條正是 hermes 專屬）。
> 工具的用量歸屬標籤**完整保留**。詳細證據見 §0.3、§9.4 與
> `SIMPLIFICATION-PLAN.md` §10.9。

---

## 0.1 2026-09-28：用量「工具」視角（已實機驗證）

**做了什麼**：在用量頁新增獨立鏡頭「工具」（`概覽 | 工具 | 模型 | 月 | 日 | 小時 | 統計 | 渠道`），
把原本只能「一次看一個工具」的分工具數字，變成**同一張表並排比較**：
請求數／成功率／輸入／輸出／快取讀／快取建／總計 Tokens／**快取命中率**／費用 $，
外加每個工具一張摘要卡（Token 用量／請求數／快取命中率），點任一列可下鑽成該工具的篩選。
後端新增 `AppStat` + `by_app()` + `usage_by_app` 命令，並抽出全檔唯一的
`hit_rate()`（`cache_read / (in_tok + cache_read)`，分母 0 回 0.0）。

**程式**：commit `d417b02`（9 檔、+708/−26）。

**驗證方式與結果**（本節的數字都是**實跑**得到的，不是推論）：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 產物比原始碼新 | 時間戳比對（最新原始碼 `usage.rs` 01:15，產物 02:43） | ✅ |
| 二進位含新前端與新命令 | 在 exe 內找到 `index-BtK0gyS1.js`、`index-DnRx7SDk.css`、`usage_by_app` | ✅ |
| 安裝檔真的換新 | 安裝前 `%LOCALAPPDATA%` 版只有舊資源 `index-hbcVOtaz.js`；安裝後兩者皆為新 | ✅ |
| 「工具」鏡頭存在並渲染 | 視窗截圖 → Windows OCR 讀出「全部工具合計」「分工具比較」與九個表頭 | ✅ |
| 表格數字正確 | 見下方逐列比對 | ✅ |
| 點列下鑽 | 點 Codex 列 → 篩選變成 `codex`、摘要卡變成 3,512 筆／1,222,485,861 tokens／47.6% | ✅ |
| 篩選可還原 | 重新選「全部本機工具」→ 合計回到 4,145 筆／1,625,556,632 tokens | ✅ |

**逐列比對（範圍＝最近 30 天）**：下表每一格都是**畫面上實際顯示的值**，
我再用 SQLite 直接對 `request_logs` 重算一次核對，兩者一致。

| 工具 | 請求數 | 成功率 | 輸入 | 輸出 | 快取讀 | 快取建 | 總計 Tokens | 快取命中率 | 費用 $ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Codex | 3,512 | 88.0% | 639,933,256 | 1,918,771 | 580,633,834 | 0 | 1,222,485,861 | 47.6% | 23.6257 |
| OpenCode | 47 | 93.6% | 15,982,636 | 1,420,647 | 379,150,532 | 0 | 396,553,815 | 96.0% | 0.0012 |
| Claude Code | 187 | 81.8% | 3,323,706 | 158,009 | 2,951,552 | 0 | 6,433,267 | 47.0% | 0.0000 |
| unknown | 357 | 25.5% | 30,473 | 2,857 | 768 | 0 | 34,098 | 2.5% | 0.0004 |
| DeepSeek Harness | 41 | 17.1% | 24,859 | 100 | 0 | 0 | 24,959 | 0.0% | 0.0012 |
| Hermes Agent | 1 | 100.0% | 12,337 | 7 | 12,288 | 0 | 24,632 | 49.9% | 0.0000 |
| **合計**（畫面卡片） | **4,145** | — | — | — | — | — | **1,625,556,632** | **59.4%** | **23.6285** |

> 「合計」那張卡只顯示四個數字：請求數、Token 用量、快取命中率、費用 $。
> 費用 23.6285 也與 `SUM(cost_usd)` = 23.628523072 相符。
> （成功率／輸入／輸出／快取讀的合計值沒有顯示在畫面上，我沒有列進去。）
> 重算腳本：`.workbuddy/tmp/verify_ui_by_app.py`。

> **驗證方法的誠實說明**：我用的模型**不能讀圖**，所以畫面內容是靠
> 「視窗截圖 → Windows 內建 OCR」讀出來的。OCR 對小字數字偶爾會錯
> （例如把 `100.0%` 讀成 `18.0`、把 `2.5%` 讀成 `25%`），因此凡是 OCR 讀得可疑的格子，
> 我改用**墨水寬度量測**（量該格亮像素的實際寬度）判斷字元數：
> 例如 Hermes 成功率那格寬 36px、其餘各列同欄只有 30–31px —— 多一個字元寬，
> 才是 `100.0%`（6 字元）而不是 `18.0%`（5 字元）。這是判定「顯示值」與
> 「我算出的值」一致的最後一道依據。
>
> 也就是說：**表上沒有任何一格是「推論」來的**；OCR 讀不清的格子，我用寬度量測
> 定案。若你實機看到某一格與上表不符，那是我的驗證方法仍有漏洞，請直接告訴我。

---

## 0.2 2026-09-28：第一階段工具區精簡（D-1／A／E，已實機驗證）

**做了什麼**（每個方向一個 commit，細節見 `SIMPLIFICATION-PLAN.md` §10.4–10.8）：

| commit | 方向 | 一句話 |
|---|---|---|
| `3908fcd` | D-1 | 刪掉走不到的「真·直連上游」分支（`via_gateway`），只剩「經網關」一條路 |
| `71d51a6` | A | 移除 cursor／antigravity 的偵測與接管 arm，工具清單 7 → 5 |
| `38c5bcd` | E | 輪換備份 `BACKUP_KEEP` 10 → 1，並新增「輪換前先保住接管前設定」的安全網 |

**驗證方式與結果**（下表都是**實跑**，不是推論）：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 產物比原始碼新 | 時間戳（最新原始碼 03:11:50、exe 03:28:11） | ✅ |
| 前端真的換新 | exe 內資源鍵 `index-BAemutFc.js`（前版 `index-BtK0gyS1.js`）、`index-DnRx7SDk.css` 未變 | ✅ |
| 安裝檔真的換新 | NSIS `/S` exit 0；安裝後 exe 03:27:52、同一組資源 | ✅ |
| D-1 真的進了產物 | 在 exe 內搜 `via_gateway` → **找不到** | ✅ |
| A 的工具清單 | 上游來源頁的頁籤讀到「本機工具（**2/5** 接管中）」，五張卡是 Claude Code／Codex／OpenCode／Hermes Agent／DeepSeek Harness | ✅ |
| A 的文案 | 工具區底部讀到「Cursor／Antigravity 不提供接管…」 | ✅ |
| D-1 的方案路徑 | 點 Claude Code 卡片開對話框 → 讀到 `…\.claude\settings.json · 自動備份`、`8 項寫入`、「⚠ ANTHROPIC_AUTH_TOKEN 將明文寫入 settings.json…網關模式下請使用可吊銷的本地 sk Key」（證明新產物的 `switch_plan` 仍能生成方案；D-1 本身由上面「`via_gateway` 消失」證明） | ✅ |
| 沒有誤寫任何設定 | 全程未按「套用／接管」；驗證前後備份目錄完全相同（claude 10／codex 11＋baseline 1／hermes 6／opencode 10），`app.db` mtime 未變 | ✅ |
| **沒能 GUI 驗證** | 用量頁那個自製「本機工具」下拉，滑鼠事件打不開它的彈出層 | ⚠️ 改用程式碼與 diff 證明：三次 commit 都沒動 `APP_META`／`APP_ORDER`／用量頁任何一行，選項來源就是 `APP_META`（含 cursor／antigravity），所以「選項一個不少」成立 |

**自動化測試**：`cargo test --offline` → **223 passed / 0 failed / 9 ignored**（exit 0）；
`cargo clippy --offline --all-targets` → 5 個警告，與基線相同；
`npx tsc --noEmit` exit 0；`scripts/check_app_labels.py` 前後端顯示名一致。

**E 的 4 條新測試**（`src-tauri/src/tools/tests/restore.rs`）：

| 測試 | 釘住什麼 |
|---|---|
| `prune_keep_one_keeps_the_clean_backup` | `keep=1` 時淘汰的是污染備份，留下來的仍是乾淨那份 |
| `keep_one_still_restores_original_after_repeated_takeover` | 「接管 → 再接管 → 還原」仍回到最初內容（走 baseline） |
| `keep_one_salvages_baseline_from_history_before_prune` | 沒有 baseline、只有一串歷史備份時，輪換前先把最乾淨的存成 baseline |
| `backup_keep_is_one_by_design` | 把「1」釘成刻意決定，避免被隨手改掉 |

> **對你備份的實際影響**：下次對某工具按「接管」時，該工具的 `bak-` 會被輪換到
> 1 份（乾淨那份或最新那份），`baseline` 不受影響。不想被輪換就不要按接管。

---

## 0.3 2026-09-28：第一階段 F —— 移除 hermes 支援（已實機驗證）

**你的決定**：先把 hermes 從網關殘留中解套，再移除整塊支援。

**動手前的查核（推翻了先前的假設，誠實記錄）**：

| 查核項 | 結果 |
|---|---|
| hermes 走哪條路 | `model.provider = opencode-go`、`https://opencode.ai/zen/go/v1` → **沒走網關** |
| App 的判定 | 工具頁只有 Claude Code／Codex 標「網關接管中」 |
| hermes 用量 | **1 筆 / 24,632 tokens**，時間 09-23（歷史紀錄） |
| 真正殘留 | `config.yaml` 一段未被引用的 `providers.tokengateway`＋`.env` 一行 Key |

⇒ 不能「還原到 09-23 備份」（那會把目前可用的 opencode-go／gpt-6-luna 換成舊的
nvidia／flatkey）。改採**外科清理**（你選的 A）。

**外科清理（可完全回復）**：

| 步驟 | 結果 |
|---|---|
| 先備份 | `backups/hermes/config.yaml.bak-20260928-035729`、`.env.bak-20260928-035729`（不經輪換，不會被 `BACKUP_KEEP=1` 砍） |
| `config.yaml` | 6 行 provider 區塊 → `providers: {}`（349 → 343 行） |
| `.env` | 刪掉那 1 行（550 → 549 行） |
| 驗證 | YAML 可解析、`model` 段未動、全檔無 `tokengateway`／`15722` |
| `hermes doctor` | 配置區全綠（API key ✓／No deprecated keys ✓）、42 項連線檢查中 **OpenCode Go ✓ (key configured)**、`hermes --version` 正常 |

> 想回頭：把那兩份 `*.bak-20260928-035729` 覆蓋回 `%LOCALAPPDATA%\hermes\` 即可。

**程式改動與驗證**：

| 驗證項 | 方法 | 結果 |
|---|---|---|
| 測試 | `cargo test --offline` | **216 passed / 0 failed / 8 ignored**（exit 0） |
| lint | `cargo clippy --offline --all-targets` | **4 個警告**（原為 5，少的那個在被刪的測試裡） |
| 型別 | `npx tsc --noEmit` | exit 0 |
| 顯示名一致 | `scripts/check_app_labels.py` | 前後端 7 個顯示名一致（含 hermes） |
| 產物不含 hermes 程式 | exe 內搜 `hermes_home`／`parse_hermes_model`／`HERMES_CUSTOM_TOKENGATEWAY_API_KEY` | 全部 **False** |
| 產物仍含用量設施 | exe 內搜 `usage_by_app` | **True** |
| 工具清單縮減 | 工具頁頁籤讀到「本機工具（**2/4** 接管中）」，四張卡：Claude Code／Codex／OpenCode／DeepSeek Harness | ✅ |
| 用量歸屬未縮減 | `APP_META`／`APP_ORDER`／`infer_app` 的 hermes 全部保留 | ✅（你的 1 筆 hermes 請求仍歸屬 hermes） |
| 版本面板 | 工具頁新增「本機工具版本」面板，列出 4 個可接管工具 | ✅ |

---

## 0.4 2026-09-28：§8 誠實清單一次清空（結構重構輪）

**你的指示**：把 `TESTING.md` §8「已知未修／已做但未達標」清單裡的項目**一併修完**
（不是只回報）。

**這一輪全部是「結構重構」，不該改變任何可見行為**：拆檔、補測試、統一門檻、
抽共用結構。驗收標準就是「測試數不減、lint 更乾淨、畫面一模一樣」。

### 0.4.1 修了什麼

| 原 §8 項目 | 結果 | 關鍵事實 |
|---|---|---|
| `resolve_filter` 的 365/366 口徑不一 | ✅ 統一為 **365** | 前端 `RangePicker.tsx` 本來就是 `CUSTOM_MAX_DAYS = 365`，三處只有後端寫錯 → **沒有產品決策空間** |
| `db.rs` migration 無自動化測試 | ✅ 新增逐版本升級測試 | `every_legacy_version_upgrades_preserving_data` 對 **v1…v7 每一個**歷史版本各造一個庫，驗證版本、providers、request_logs、settings、12 張核心表 |
| `proxy_handler` 86 行（目標 <80） | ✅ **78 行** | 根因與原判斷不同：不是「重複 5 個欄位」，而是三個階段 struct **各自重複宣告** `ctx`/`started`/`app`/`model_raw` → 抽出 `ReqCtx` |
| §5.2 body 解析 400「未定案」 | ✅ **定案：不是 bug** | 見 `docs/evidence/2026-09-28-body-parse-CONCLUSION.md` |
| 前端三個肥檔 | ✅ 全拆 | `Keys.tsx` 884→**254**、`Calc.tsx` 818→**366**、`lib/api.ts` 808→**318** |
| 「拆檔後仍 >400 行」5 檔 | ✅ 全拆 | `Providers.tsx` 778→**271**、`ModelCatalog.tsx` 608→**5**（barrel）、`ProviderForm.tsx` 590→**18**（barrel）、`tools/tests/codex.rs` 566→**310**、`usageCharts.tsx` 546→**291** |
| clippy 4 個既有警告 | ✅ **0 個** | 3 個直接修；`codex_apply` 的 8 參數加 `#[allow]` **並註明理由** |
| Phase 5 錯誤型別只轉 6/108 | ⛔ **刻意不動** | 每次轉換都該有人能實測；屬獨立的漸進工作，強行一次改完風險大於收益 |

### 0.4.2 稽核另外掃出的 15 個 >400 行檔案（也一併拆完）

原本的清單只列了 5 個，實際掃全樹發現**還有 15 個**。既然要拆就一次拆完：

`translate.rs` 2345、`usage.rs` 2190、`commands.rs` 1233、`db.rs` 1132、
`history.rs` 1038、`models.rs` 990、`proxy/tests/e2e_basic.rs` 705、
`tools/codex.rs` 602、`quota.rs` 595、`trace.rs` 576、`SwitchDialog.tsx` 574、
`Diagnostics.tsx` 563、`usageLenses.tsx` 453、`tools/versions.rs` 450、
`proxy/tests/e2e_translate.rs` 445、`proxy/tests/e2e_strict.rs` 430、
`keys.rs` 419、`price_extract.rs` 410。

**現在全樹 202 個 `.ts`／`.tsx`／`.rs` 檔案，沒有任何一個超過 400 行**
（最大者 399 行）。

**統一作法（三條不變式）**：
1. **純搬移** —— 零行為、零文案、零邏輯、零 JSX／className 變更。
2. **對外路徑不變** —— 前端用再匯出 barrel，Rust 用 `pub(crate) use 子模組::*`，
   所以**所有既有 import 端一行都不用改**（`App.tsx`、`lib.rs` 的
   `generate_handler!`、各 `crate::X::Y` 呼叫點全部照舊）。
3. **可見性只放寬、不收緊** —— 跨子模組呼叫所需的 `pub(crate)`／`pub(super)`，
   共十餘處，皆非邏輯變更。

### 0.4.3 驗證

| 驗證項 | 指令 | 結果 |
|---|---|---|
| 後端測試 | `cargo test --offline` | **218 passed / 0 failed / 8 ignored**（exit 0） |
| 後端 lint | `cargo clippy --offline --all-targets` | **exit 0、零警告** |
| 後端編譯 | `cargo check --offline --all-targets` | exit 0 |
| 前端型別 | `npx tsc --noEmit` | exit 0 |
| 前端建置 | `pnpm build` | exit 0 |
| 測試數不減 | 218 = 重構前基線 | ✅（226 條被執行，含 8 ignored） |
| 無檔案超標 | 全樹掃描 | ✅ 最大 399 行 |

> **測試名稱有一處變動**：`proxy/tests/e2e_translate.rs`（及 `e2e_strict`）改成
> 目錄後，測試路徑多一層模組名，例如
> `proxy::tests::e2e_translate::e2e_responses_to_chat`
> → `proxy::tests::e2e_translate::responses::e2e_responses_to_chat`。
> **測試函式本體與斷言逐位元組未變、總數未變**，只是名字變長。
> 這是為了不動 `proxy/tests.rs` 的 `#[path]` 宣告。

### 0.4.4 一個值得記下的量測陷阱

**PowerShell 5.1 的 `Get-Content` 會少算含中文檔案的行數**（codepage 936 下
DBCS 前導位元組會吃掉後面的 `0x0A`，另有解碼成 U+0085 而合併行的情形）。
先前 §8 記的 876／809／803 其實是 **884／818／808**；`usage.rs` 記 2109 實為
**2190**、`translate.rs` 記 2267 實為 **2345**。

**之後量行數請用** `[IO.File]::ReadAllLines($path).Count`（或 `-Encoding UTF8`）。
本次所有「< 400 行」的結論都是用這個方法量的。

---

## 0.5 2026-09-28：測試 B／D／E 代跑，以及一次我自己造成的意外

### 0.5.1 結果總表

| 測試 | 結果 | 證據 |
|---|---|---|
| **B** 網關拒絕要留痕 | ✅ **5/5 通過** | §5.3：Δ+3 筆、401／401／400、warn 層級、總數 21→24、「轉換」欄＝網關拒絕（未觸及上游） |
| **D** Codex 舊會話別名體檢 | ✅ **3/3 通過** | §6.5：ℹ️ 明示讀自 `state_5.sqlite`、6 個別名、並附 ❌ 實例；「讀不到」分支有單元測試釘住 |
| **E** `wire_api` 跟著上游 | ❌→✅ **發現真實故障並修正** | §6.6：E 的「宣告優先」被誤用到指向網關的情況，寫出 Codex **已不再接受**的 `wire_api = "chat"` → Codex 整份 config 判為非法 → **「Unable to log in」**。已修（`codex_wire_api_for`）＋新增回歸測試；你檔案上 8 段現為 `responses` |
| **C** 你原本的症狀 | ⏳ 仍待你實測 | 需要真實 CLI 使用情境，我無法代跑 |
| GUI 覆蓋補齊 | ✅ | 診斷頁（含展開明細）、上游來源頁、本機工具分頁、切換對話框全部重新渲染確認過 |
| 標籤一致性 | ✅ | `scripts/check_app_labels.py` exit 0 |

### 0.5.2 ⚠️ 我造成的意外：Codex 設定被改了兩次（已還原）

**過程**：我用滑鼠座標導覽時，有兩次點擊落到了「上游來源」頁的**來源卡片**上
（一次是 `xxy-DS` 卡片的「編輯」、一次直接落在 `NIM` 卡片）。
而**點選來源卡片依設計會立刻自動重接管 Codex**
（`src/components/providers/useToolSwitch.ts:200-215`
「來源切換自動重接管……永不彈框」），於是：

| 時間 | `~/.codex/config.toml` 被寫成 |
|---|---|
| 16:19:35 | `model = deepseek-v4.1-flash`、`codex-18.json`（provider 18 = xxy-DS）← 我誤觸 |
| 16:23:37 | `model = moonshotai/kimi-k3`、reasoning `max`、`codex-15.json`（provider 15 = NIM）← 我誤觸 |
| **16:28:56** | **`model = deepseek-v4.1-flash`、reasoning `medium`、`codex-22.json`（provider 22 = opencode-go）← 已還原，這是你原本的** |

**怎麼知道原本是 opencode-go**：`catalogs/` 目錄裡**我的工作階段之前最新的一份**
是 `codex-22.json`（09-26 05:47:16，provider 22 = opencode-go），而
opencode-go 的第一個模型正是 `deepseek-v4.1-flash`、reasoning 首位正是 `medium`
—— 與 App 內儲存的「上次接管參數」完全吻合。還原後我逐項核對：
8 個 provider 段全部 `wire_api = "chat"`、`base_url` 全部指向網關、
你的 plugins（11）／marketplaces（2）／`[desktop]`／`appearanceTheme`／
`selected-avatar-id`／`[windows]`／`mcp_servers`／`notify` **全部原樣保留**。

**副作用（無法復原的部分）**：`backups/codex/` 由 **11 個被輪換成 2 個**
（`config.toml.baseline-20260926-054716` 與 `config.toml.bak-20260926-054716`）。
這是 E 把 `BACKUP_KEEP` 改成 1 之後第一次接管必然的結果，**不是資料遺失**：
最重要的「接管前原始設定」= baseline 還在，所以 App 的
「關閉：還原接管前備份」仍然可用。其他工具的備份完全沒動
（claude 10／hermes 8／opencode 10，與基線相同）。

### 0.5.3 由此發現的一個真實缺陷（**已修，2026-09-28**）

**點一下來源卡片就會靜默重寫 `~/.codex/config.toml`，沒有任何確認或提示。**

- 位置：`src/components/providers/useToolSwitch.ts`（原 `:200-215`）。
  註解自己寫著「來源切換自動重接管……**永不彈框**」—— 這是刻意的設計，
  但代價是「只是想看看某個來源」也會改檔案。
- 觸發條件：**Codex 正在接管中** ＋ 所選來源改變。對已被接管的 Codex 而言，
  在來源清單上點任何一張卡片都會立刻重寫 config 並輪換備份。
- 反過來說**開頁面本身不會**觸發：`useToolSwitch` 的 `prevSelId` 守衛會把
  首次載入的 `selId`（`Providers.tsx:48` 的 `selected ?? providers.data?.[0]?.id`）
  只記錄不套用，所以「第一次進上游來源頁」是安全的。我實測確認過兩次
  （重啟 App 後只開頁面，`config.toml` 的 mtime 與 SHA-256 都沒變）。

**修法（已採用建議的 ②，2026-09-28）**：

自動重接管**整段移除**。現在所選來源改變時只做一件事 —— 在 Codex 那一行
顯示一行提示：

> 已選擇來源「X」，但 Codex 仍接管自原來源（未改動 config.toml）。
> 要換過去請按右側開關，或點此行左側進詳情按「套用」

也就是說：**寫入一律要使用者的明確動作**（行開關、或詳情裡的「套用」），
不再有任何因為「點一下看看」而改檔的路徑。沒有用確認框是因為確認框仍會在
「只是瀏覽」時跳出來打斷；把寫入的觸發點收斂到明確動作更乾淨。

> **為什麼不用 ①（確認框）**：確認框會讓「點卡片看模型與定價」這個**唯讀
> 意圖**變成一個要按掉的中斷。問題的根不是「沒問」，而是「瀏覽與切換共用
> 同一個點擊」。收斂觸發點直接解決根因。

---

## 0.6 2026-09-28：Codex 報「Unable to log in」的真因與修正

### 0.6.1 你看到的症狀

開啟 Codex 時出現 **「Unable to log in」**。

### 0.6.2 真因（不是登入問題）

Codex 的日誌（`~/.codex/logs_2.sqlite`，首次出現 **09-28 16:48:28**）說得很清楚：

```
ERROR codex_app_server: Invalid configuration; using defaults.
C:\Users\luluna\.codex\config.toml:16:12: `wire_api = "chat"` is no longer supported.
How to fix: set `wire_api = "responses"` in your provider config.
```

**Codex 已經不再接受 `wire_api = "chat"`。** 它讀到非法設定就**整份
`config.toml` 丟棄、改用預設值**，於是 `model_provider` 回到 `openai`、
回頭去找 ChatGPT 登入狀態 —— 你看到的「Unable to log in」是**下游症狀**，
跟登入本身無關（`~/.codex/auth.json` 一直都在，mtime 09-24）。

### 0.6.3 為什麼會寫出 `chat`

E 階段（`38c5bcd`）加了「**provider 宣告的 `api_format` 優先於 URL 推定**」。
但宣告描述的是**上游**說哪種協議，而寫進 `config.toml` 的 `base_url`
**永遠是本網關** —— 也就是說，宣告被用在了錯的那一段連線上。

你的 7 個來源 `api_format` **全部是 `openai-chat`**，所以接管後
**每一個** provider 段都被寫成 `chat`，Codex 直接罷工。

**舊版（E 之前）寫的是 `responses`** —— 依 URL 推定，指向網關就給 Codex 原生
形狀。這解釋了為什麼 09-26／09-27 都正常，而今天 12:31 裝了新版本之後才出事。

### 0.6.4 為什麼測試沒攔住

單元測試只驗了「網關 URL ＋ **未宣告**」→ `responses`
（用 `CodexAuth::default()`）；而生產路徑（`switch.rs`）
**永遠帶著宣告**。兩個條件從未在同一條測試裡相遇 —— 這是典型的
「測試驗了元件、沒驗接線」。

### 0.6.5 修正

| 項目 | 內容 |
|---|---|
| 新增 `wire.rs::codex_wire_api_for(base_url, declared)` | 先看 URL：指向**網關／官方 OpenAI** 就定死 `responses`，宣告不得推翻；只有**真正的第三方**才輪到宣告生效 |
| `gateway_section` 改用它 | 原本是 `auth.wire_api.unwrap_or_else(|| codex_wire_api(base_url))` |
| 新增回歸測試 | `codex_apply_forces_responses_on_gateway_even_if_declared_chat` —— 用的正是生產路徑的參數組合（網關 URL ＋ `codex_wire_api_declared(Some("openai-chat"))`） |
| 你目前的 `config.toml` | 8 個段的 `wire_api` 已全數由 `chat` 改為 `responses`（17:59:48） |

### 0.6.5b 同一類錯誤的**第二處**：預覽也會騙人

第一次修完之後，我回頭去找「同樣的運算式還寫在哪裡」，發現
`switch.rs::plan_switch` 有一份**一模一樣的副本**：

```rust
// 修正前（switch.rs，產生畫面預覽用）
let wire_api = codex_wire_api_declared(req.provider_format.as_deref())
    .unwrap_or_else(|| codex_wire_api(&req.base_url));
```

而它正上方一行註解寫著「**預覽必須顯示實際會寫入的值，否則預覽會騙人**」。
若只修 `gateway_section`，結果會是：**畫面預覽顯示 `chat`、實際寫入
`responses`** —— 兩個都錯開，使用者無從判斷哪個是真的。

已改為與寫入端呼叫**同一個函式** `codex_wire_api_for`，並新增測試
`plan_preview_wire_api_is_responses_on_gateway` 把兩邊釘在一起
（這條測試也順帶成為測試 E 準則 1 的自動化替代品）。

> **教訓**：同一個決策運算式出現在兩個地方，就是會漂移。修一處時必須
> 全域搜尋同型運算式；本次是靠「`codex_wire_api(` 的使用處」那個搜尋找到的。

**測試**：`cargo test --offline` → **219 passed / 0 failed / 8 ignored**（exit 0）；
`cargo clippy --offline --all-targets` → **exit 0、0 警告**。

### 0.6.6 修好之後的實機驗證（`codex doctor`）

Codex 自己有診斷指令（`codex doctor`），我用它做了**端到端**核對
（不用啟動 Codex、不消耗額度）：

| `codex doctor` 的輸出 | 意義 |
|---|---|
| `✓ config` → **`loaded`** | **不再出現「Invalid configuration; using defaults」** |
| `config.toml parse` → **`ok`** | 整份檔案合法（不是只有第一個錯誤被修掉） |
| `model` → `deepseek-v4.1-flash · custom` | 接管**生效**：確實走 `custom` 段（網關），不是退回預設 |
| `✓ auth` → **`OpenAI auth is not required for the active model provider`** | **「Unable to log in」的根因消失** |
| `model provider requires OpenAI auth` → **`false`** | Codex 知道這個 provider 不需要 ChatGPT 登入 |
| **`wire API` → `responses`** | 寫進去的值就是 Codex 現在唯一接受的那個 |
| `✓ reachability` → **`active provider endpoints are reachable over HTTP`**（`mode: provider auth`） | 它**真的帶著設定裡的 bearer token 打了一次網關並成功** |

也就是說：設定合法、模型目錄生效、認證路徑正確、而且**網關端確實收下了
寫進去的那把上游 Key**。這比「我看檔案內容對」強得多。

> **注意**：`codex doctor` 是 CLI 路徑；你原本的錯誤來自 `codex_app_server`。
> 但兩者共用同一份設定載入與驗證邏輯，而 `parse ok` ＋ `requires OpenAI auth
> false` 正是先前失敗的那兩項。

### 0.6.7 兩件仍待你決定的事（**兩件都已於 2026-09-28 處理完畢**）

1. ~~**`chat` 這個值在新版 Codex 已經完全不能用**……要不要整組拿掉、永遠只寫
   `responses`，請你決定。~~ **已決定並執行**：整組拿掉。
   現在 `wire_api` 是**一個常數** `CODEX_WIRE_API = "responses"`，
   由寫入端與畫面預覽**共用同一個符號**，型別上不可能再漂移；
   `codex_wire_api`／`codex_wire_api_declared`／`CodexAuth.wire_api`
   全部刪除（含它們的測試）。詳見 §0.7。
2. ~~新版本程式需要重新建置並安裝才會生效（見 §9.6）。在那之前，
   **不要再點「上游來源」頁的來源卡片**……~~ **已建置並安裝**（見 §9.8）。
   而且**新版即使被點也不會寫檔** —— 靜默重接管已移除（§0.5.3）。

---

## 0.7 2026-09-28（第二輪）：把「未完成清單」一次收乾

使用者指示「先把全部未完成的部分完成」。以下四項全部完成。

### 0.7.1 (a) 靜默重寫 `config.toml` —— 已修

見 §0.5.3。**寫入一律要明確動作**；所選來源改變只在畫面上提示。

### 0.7.2 (b) `wire_api` 的「依上游能力判斷」整組移除 —— 已執行

原本（E 階段）的設計是：`api_format` 宣告優先，URL 推定為後備。
**這個設計的前提已經不存在**：

| 當初的前提 | 現在的事實 |
|---|---|
| 有「真·直連第三方」模式，所以協議要跟著上游走 | 直連分支已於 §4.4 移除；`plan_switch` 與 `apply_switch` **都**把 `base_url` 強制改寫成網關 |
| `chat` 是可用的協議值 | **Codex 已完全不接受 `chat`**（整份 config 判為非法 → 「Unable to log in」） |
| `api_format` 能描述「Codex→上游」該用什麼協議 | 它描述的是**上游**說哪種協議，與「Codex→網關」這一段無關 |

所以現在是：

```rust
pub const CODEX_WIRE_API: &str = "responses";
```

寫入端（`gateway_section`）與預覽端（`plan_switch`）**共用這個符號**。
刪除的程式碼：`codex_wire_api()`、`codex_wire_api_for()`、
`codex_wire_api_declared()`、`CodexAuth.wire_api` 欄位，以及 3 條測舊機制的測試。

> **這是刻意的簡化，不是退步。** B4 當初修的是「無差別寫 `responses` 會讓
> 直連第三方 404」；但直連模式已經不存在，而且現在寫 `chat` 的代價是
> **Codex 完全不能用**（比 404 嚴重得多）。用常數取代判斷，讓「寫出 `chat`」
> 在型別層面不可能發生。

`api_format` 本身保留 —— 它仍有「來源列表顯示」與「網關端轉譯」用途。

### 0.7.3 體檢的 `TOKEN_GATEWAY_KEY` 假警報 —— 已修

`codex_doctor` 原本**無條件**檢查 `TOKEN_GATEWAY_KEY` 環境變數，沒設就報 ❌。
但「直連模式」寫的是 `experimental_bearer_token`（上游 Key 明文），
**根本不讀環境變數** —— 所以那個 ❌ 是假警報，而假警報的長期代價是
「使用者不再看體檢」。

現在改成先讀 `config.toml`，只有在**真的有 provider 段寫 `env_key`** 時才要求
環境變數；否則報 ℹ️ 說明目前設定不需要它。新增測試
`doctor_env_key_warning_depends_on_config_usage` 兩種情況都釘住。

### 0.7.4 測試 B 留下的 6 筆合成資料 —— 已清除

| 表 | 刪除的 id | 內容 |
|---|---|---|
| `proxy_trace` | 22, 23, 24 | `app='unknown'`、`trans_kind='rejected'`、狀態 401/401/400 |
| `request_logs` | 4353, 4354, 4355 | `app='unknown'`、`source='gateway'`、0 token、0 成本 |

刪除前後都有核對（`remaining: 0/0`），且**刪除前先備份資料庫**到
`.workbuddy/tmp/app.db.before-testB-cleanup`。證據本身（訊息全文、狀態碼、
`model_raw`）已完整保存在 §5.3，所以列本身可以刪。

### 0.7.5 實機驗證：App 真的會把壞掉的 `chat` 修回 `responses`

這一輪補上了先前一直缺的**端到端實機證據**（用的是安裝後的正式版本，
透過真實 UI 點擊，不是單元測試）。

**取證方式**：把 `~/.codex/config.toml` 的 8 個段**手動改回壞掉的
`wire_api = "chat"`**（重現事故狀態），然後在 App 裡開 Codex 詳情、按
「套用（切換來源/模型）」，再看檔案變成什麼。

| 步驟 | 動作 | 結果 |
|---|---|---|
| 1 | 手工把 8 段改成 `chat`（`app='unknown'` 無關，純檔案） | `now chat = 8 / now resp = 0` |
| 2 | `codex doctor`（**證明 `chat` 真的會讓 Codex 掛掉**） | ✗ `config could not be loaded — Fix the reported config error`、`failed to load Codex config` |
| 3 | App 內的真實接管：詳情 →「套用（切換來源/模型）」 | 寫入時間 `20:24:52` |
| 4 | 接管後檢查 | **`wire_api="responses"` → 8 段**（custom, tokengateway, gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen） |
| 5 | `codex doctor`（接管後） | ✓ `config loaded`、`config.toml parse ok`、`model deepseek-v4.1-flash · custom`、`requires OpenAI auth false` |

> **第 2 步是這次最有價值的收穫**：它把「`chat` 會導致 Codex 報
> Unable to log in」從推論變成**直接可重現的觀測**。錯誤訊息與事故當天
> 使用者看到的一致（「Invalid configuration; using defaults」→ 回頭找登入）。

**另外兩項同時取得的證據**：

1. **畫面預覽顯示的是實際會寫入的值**：在詳情裡展開「7 項寫入」，畫面上
   顯示的那一行是
   `[model_providers.{custom, tokengateway, …}] 共 8 段 base_url = http://127.0.0.1:15722/v1
   / experimental_bearer_token = 上游 Key 明文 / wire_api = responses`
   —— 這正是 §6.6 準則 1 先前「拿不到螢幕證據」的那一項，現在拿到了。
2. **接管是冪等的**：在設定已經是正確值的情況下按「套用」，檔案
   **逐行完全相同**（SHA-256 不變、9922 bytes），只有 mtime 更新。
   備份也沒有被輪換 —— 因為要備份的那份「舊設定」本身已指向網關
   （`backup_is_tainted`），會被 `prune_backups_keep_clean` 清掉，
   只留下真正的「接管前」乾淨基準（09-26 那兩份）。
   這是**刻意設計**，不是沒備份。

### 0.7.6 這一輪的閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **217 passed / 0 failed / 8 ignored**（exit 0） |
| `cargo clippy --offline --all-targets` | exit 0、**0 警告** |
| `pnpm exec tsc --noEmit` | exit 0 |
| 測試數變化 | 220 → 217：刪 3 條測舊機制的，加 1 條體檢假警報的回歸測試 |
| 安裝後執行檔 | 9,010,176 bytes、SHA-256 `E098655E9488C4F79757B5266A220168DC80DD3D8276BA67658BFD793E1B4498` |
| 啟動後 `config.toml` | SHA-256 **完全沒變**（開頁面不觸發接管，已再次確認） |

> **一個環境備註**：`cargo build`（非 clippy）會出現 1 條
> `#[warn(linker_messages)]` 警告，內容是 MSVC 連結器的中文輸出
> （「正在建立程式庫 …」）。那是工具鏈的地區化輸出被新版 Rust 當成訊息，
> **與本專案程式碼無關**，clippy 為 0 警告。

---

## 0.8 2026-09-28（第三輪）：「無法封存舊對話」的根因

**你先回報的症狀**：Codex 開啟與使用舊 session 對話都正常，但**無法封存**
舊對話，試了兩個 test 都失敗。

### 0.8.1 結論先講：**這不是 Token Gateway 的問題**

封存是**純本機**動作（搬移 rollout 檔＋更新 sqlite），不會經過網關、
不呼叫模型。根因是 Codex 自己留在
`%USERPROFILE%\.codex\thread-writer-locks\` 底下的**殘留鎖檔**：

```
%USERPROFILE%\.codex\thread-writer-locks\<thread-id>.lock
```

只要那個 0 byte 的 `.lock` 還在，`codex archive <id>` 就會失敗。把它刪掉，
同一條指令立刻成功。

### 0.8.2 完整證據鏈

| # | 動作 | 結果 |
|---|---|---|
| 1 | `threads` 表有 `archived`／`archived_at` 欄位；09-26 有成功封存過 | 機制本身正常 |
| 2 | 所有相關 rollout 檔都在、大小正常、projection offset 也等於檔案大小 | 排除「檔案遺失／投影不完整」 |
| 3 | 把 doctor 報的 `no usable header record` 那個 **0 byte 檔**移走再試 | **仍然失敗** → 不是它 |
| 4 | 換別的 test session 試封存 | **成功**（`Archived session …`，exit 0）→ 不是全體故障 |
| 5 | 比對失敗 vs 成功的 session | 失敗的那個有 `.lock`；成功的三個都**沒有** |
| 6 | 刪掉失敗那個 session 的 `.lock`，立刻重試 | **成功封存**（`archived=1`、rollout 搬進 `archived_sessions\`） |
| 7 | 刪掉另一個 `.lock` 後等 100 秒 | **沒有自己長回來** → 是真殘留，不是現行寫入中 |
| 8 | 用 `CreateFileW`（share mode = 0）檢查每個鎖檔 | 6 個裡有 5 個**仍有行程開著 handle** |

> **第 8 點是關鍵**：那些鎖檔的 handle 由**還在跑的 Codex 行程**持有
> （`codex` PID 3328，20:45:07 啟動），所以 Codex 一開著，這些鎖就永遠
> 不會自己消失 —— 這才是「試幾次都失敗」的原因。刪檔之所以仍可行，
> 是因為它以 `FILE_SHARE_DELETE` 開啟；但 Codex 的判斷是**看檔案在不在**，
> 所以刪掉就通了。

### 0.8.3 為什麼會殘留（推論，非直證）

`thread-writer-locks` 是 Codex 用來記錄「這個 thread 有人正在寫」的機制。
09-28 21:03:12～21:03:50 之間有 **7 個 thread 被依序更新、並各留下一把鎖**
（間隔 5～10 秒，看起來是逐一開啟對話）。合理推論是：**在 Codex Desktop 裡
開過那些對話，之後鎖沒有被釋放**；而只要鎖在，之後每一次封存都會失敗。
我沒有去證明「關閉對話是否會釋放」，因為那需要動你的 app 狀態。

### 0.8.4 怎麼修（附工具）

我新增了 `scripts/codex_clear_stale_locks.py`：

```powershell
py scripts\codex_clear_stale_locks.py            # 只報告，不動任何東西
py scripts\codex_clear_stale_locks.py --apply    # 刪除，且先備份到
                                                 # .codex\thread-writer-locks-backup
```

它**只會**刪 0 byte 的 `<uuid>.lock`，而且會先檢查有沒有行程還開著 handle，
**有在用的絕對不碰**；`.coordination.lock` 與 rollout 檔一律不碰。

**建議流程**（順序有意義）：

1. **完全關閉 Codex**（Desktop 與任何 `codex` 行程）。這樣 handle 才會放掉。
2. `py scripts\codex_clear_stale_locks.py` —— 這時應該全部列為 stale。
3. 加上 `--apply` 真的刪除。
4. 再封存：`codex archive <session-id>`，或在 Desktop 裡封存
   （**對話要先關掉**，別在它開著的時候封存）。

> **我實測時 Codex 開著，工具只判定 1 個為 stale、5 個「IN USE」**。
> 那是正確且保守的行為：Codex 沒關就別硬刪。

### 0.8.5 我在診斷過程中動到的東西（誠實揭露）

| 動作 | 影響 | 怎麼還原 |
|---|---|---|
| 封存了 4 個標題為 `test` 的 session：`01a08b17`、`01a08b19`、`01a0924b`、`01a0924c` | 它們現在是已封存狀態 | `codex unarchive <id>` |
| 刪除並還原了 `01a063d2`（標題 `OK 你好`）的鎖檔 | 已用備份還原 | 備份在 `.workbuddy\tmp\lock-backup\` |
| 曾把一個 0 byte rollout 檔暫時移走再放回 | 已還原 | 該檔本來就是 0 byte |

**沒有**改動任何 rollout 內容、沒有改 `config.toml`、沒有改 state DB 的資料。

---

## 0.9 2026-09-28（第四輪）：「關掉網關，Codex 卻沒回到原生來源」

### 0.9.1 你回報的症狀

> 在我關閉該工具的網關後應該要恢復成原本使用的原生來源/模型，
> 目前 codex 沒有恢復，來源還是 NIM 的來源/模型

⚠️ **你附的截圖我讀不到** —— 這個 session 的模型不接受圖片輸入
（`read_image` 直接回「does not declare image input」）。以下是我從檔案 mtime
與資料庫重建出來的，若與截圖不符請直接告訴我。

### 0.9.2 真因：基線是**很久以前的快照**，而「關」只會照抄它

| 時間 | 事件（可複核：檔案 mtime ＋ `backups/codex/`） |
|---|---|
| 09-26 05:47:16 | `config.toml.baseline-20260926-054716` 產生。內容**已經是 NIM**：`model_provider = "custom"`、`model = "moonshotai/kimi-k3"`、`model_catalog_json = …\catalogs\codex-15.json` |
| 09-28 20:24:52 | 你切到 opencode-go，寫入 `catalogs\codex-22.json` |
| 09-28 21:26:16 | 你切回 NIM，寫入 `catalogs\codex-15.json` |
| 09-28 21:29:47 | 你把開關關掉 → App 把 baseline 寫回去 → 你又回到 NIM |

兩個獨立的原因疊在一起：

1. **基線只在你「進入接管」的那一刻拍一次**（`ensure_baseline_before_prune` 的語義
   是「接管前的乾淨設定」）。之後你在 App 裡換來源／模型，改的都只是「指向網關」
   的那份設定，基線不會更新 —— 所以只要你不曾真正離開接管態，下一次「關」還是
   回到同一個舊快照，然後這個舊快照又被拍成新基線（自我延續）。
   （補：基線在每次還原後會被清掉，下次接管會重拍 —— 也就是說**從現在起**
   基線會是你剛拿回來的原生設定，這也是這次修正的一個額外好處。）
2. **「關」的語義本身太窄**：它假設「接管前」就是你要的終點。你要的是
   「這個工具回到它自己的來源」。

### 0.9.3 修法：關 = 忠實還原 ＋ 原生來源化

新增 `switch_off` 命令（`tools/native.rs`），關閉流程變成兩步：

1. `restore_backup_port` —— 忠實還原，保住你自己的設定與別名 provider 段。
2. `to_native` —— 只把**來源**相關的鍵換回第一方：

| 工具 | 做什麼 |
|---|---|
| **codex** | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、移除 `model_catalog_json`（移除後 Codex 才會用內建的 GPT 模型清單） |
| **claude** | 移除**任何不是 Anthropic 官方**的 `ANTHROPIC_BASE_URL`（網關或使用者自己的 router 都一樣），連帶移除 `ANTHROPIC_AUTH_TOKEN`；移除非 `claude-*` 的模型對映 |
| **opencode** | 移除指向網關的 `provider.*.options.baseURL` 與本地 `sk-local-` key |

> **Claude Code 的端點是刻意的例外**（2026-09-28 你選的）：連你自己原本的
> router（`https://opencode.ai/zen/go/v1`）也一起移除，推回 Anthropic 官方。
> 代價是這台機器上沒有 `.claude/.credentials.json`，你必須先登入一次。

**為什麼保留別名 provider 段**（`custom`／`nvidia-nim`／`opencode-zen`…）：舊對話
的 rollout 裡記著 provider 名，段被刪掉那些對話就開不起來。所以關閉後
`config.toml` 裡**仍然看得到**它們與其 URL —— 那是刻意的，是你 2026-09-28
選的方案（「原生來源 + 保留舊對話通道」）。

**Codex 的原生模型清單**（用乾淨的 `CODEX_HOME` 探到的，config 的 catalog 會
蓋掉它）：`gpt-6-astra`、`gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`、
`gpt-5.5`、`gpt-5.2`。`gpt-5.6-luna` 是你自己舊對話用過的模型，也是你選的預設。
另外確認你的 `~/.codex/auth.json` 是 `auth_mode = "chatgpt"`（id/access/refresh
token 齊全）→ 回到官方是可行的。

### 0.9.4 順手修掉一個因此暴露的假警報

`restore_summary` 的「直連第三方 URL 卻走 responses 協議（會 404）」警告原本是
**掃整份文本**找 `https://` ＋ `wire_api = "responses"` ＋ 沒有
`api.openai.com`。別名段留著之後，即使來源已經切回 `openai`，畫面還是會端出
那句「不可用請改走網關接管或官方登入」—— 正好在使用者剛回到官方來源時嚇他
一次。已改成**只看 `model_provider` 指名的那個段**
（`summary.rs` 的 `active_provider_is_direct_chat`）。

### 0.9.5 我這輪動到的東西（誠實揭露）

| 動作 | 說明 |
|---|---|
| 新增 `tools/native.rs`、`tools/summary.rs`、`tools/tests/native.rs` | 分別是原生化、還原摘要（從 `backup.rs` 搬出，因為它一度到 421 行）、9 條新測試 |
| 改 `commands/apps.rs`＋`lib.rs` | 新增 `switch_off` 命令 |
| 改前端 4 檔 | `api.ts`（新增 `switchOff`）、`useToolSwitch.ts`、`ToolList.tsx`、`switchdialog/*`（開關 OFF 改走新命令、文案更新） |
| 保留 `switch_restore`／`api.switchRestore` | 它是「逐字還原、不加工」的安全網。UI 已不再呼叫，程式碼裡的說明有寫明 |

**這一輪第一次動到你的 `config.toml` —— 就是下面那次實機驗證**（接管再關閉，
把 Codex 從 NIM 帶回 OpenAI）。在那之前只改程式與文件。細節見 §0.9.7。

### 0.9.6 這一輪的閘門

| 項目 | 結果 |
|---|---|
| `cargo test --offline` | **226 passed / 0 failed / 8 ignored**（exit 0） |
| 新增測試 | 9 條：冪等、壞 TOML 不猜、別名段完整保留、只認網關痕跡、`switch_off` 端到端（還原＋轉換都真的落到檔案） |
| `cargo clippy --offline --all-targets` | **0 警告** |
| `pnpm exec tsc --noEmit` | exit 0 |
| 檔案行數 | 205 檔全部 ≤ 400 行 |

### 0.9.7 實機驗證（安裝後的正式版本，真實 UI 點擊）

安裝 2026-09-28 22:32 的建置到 `%LOCALAPPDATA%\token-gateway`（NSIS `/S`，
exit 0，exe 9,026,048 bytes、sha256 `1C9D27A7…`），然後**照你要做的事做一遍**：

| 步驟 | 觀察 |
|---|---|
| 開 App →「上游來源」→「本機工具」 | Codex 的開關標題已經是 **「關閉：還原接管前備份，並切回這個工具的原生來源」**（新文案真的進了 bundle） |
| 當時狀態 | `本機工具（1/4 接管中）`；Codex 是「開啟」態、Claude Code 是接管中 |
| 按 Codex 開關 → **接管** | `config.toml` 7,718 bytes、8 個 `base_url` 全指向 `http://127.0.0.1:15722/v1` |
| 再按同一個開關 → **關閉** | 卡片下方出現：`…｜還原自 config.toml.baseline-20260928-223526｜provider=openai model=gpt-5.6-luna｜已切回原生來源：model_provider → openai、model → gpt-5.6-luna、移除 model_catalog_json…` |

關閉後的 `config.toml`（6,913 bytes、sha256 `8A1ADF7F…`）：

| 檢查項 | 結果 |
|---|---|
| `model_provider` / `model` | **`openai`** / **`gpt-5.6-luna`** ✅ |
| `model_catalog_json` | **0 次**（已移除）✅ |
| `[model_providers.*]` | **8 段全在**（`custom`／`tokengateway`／`gw`／`mock`／`nim-direct`／`nvidia-nim`／`nvidia-proxy`／`opencode-zen`，都指向 `https://integrate.api.nvidia.com/v1`）✅ 舊對話通道保留 |
| 指向網關的段 | **0 段** ✅ |
| 你自己的鍵 | `model_reasoning_effort = "high"`、`disable_response_storage`、`model_context_window`、`model_auto_compact_token_limit`、`[plugins.*]` ×11、`[mcp_servers.*]` ×2、`[windows]`、`appearanceTheme` 全在 ✅ |
| **假警報** | 訊息裡**沒有**「不可用請改走網關接管」那句 ✅（§0.9.4 的修正生效） |
| `codex doctor` | `✓ config loaded`、`model gpt-5.6-luna · openai`、`config.toml parse ok`、`✓ auth auth is configured`、`stored auth mode chatgpt` ✅ |
| **Codex 的模型清單** | `codex debug models` → **`gpt-5.6-terra`、`gpt-5.6-luna`、`gpt-5.5`**（＋2 個隱藏），NIM 的模型全部消失 ✅ |

### 0.9.9 端到端實測（2026-09-29 02:50–03:2x）：用量核算 ＋ 關工具／關網關／重啟

使用者要求跑一輪完整測試：**開網關 → 用工具真實對話 → 檢查用量是否算對 →
關掉工具 → 關掉網關 → 重啟工具 → 檢查來源是否回到官方設定**。

#### A. 用量核算（四路交叉比對，全部一致）

| 比對 | 結果 |
|---|---|
| 真實對話（`codex exec` 經網關） | 網關記 `in=11,518 out=30`；**Codex 自己印 `tokens used 11,548`** → 11,518 + 30 = **11,548** ✅ |
| 控制變數（curl 經網關，非串流） | 回應 `usage` = `prompt 49 / completion 200`；DB 該列 = `in 49 / out 200` ✅ |
| 同一個 prompt 直打上游（繞過網關） | 上游自己的 `usage.prompt_tokens` = **49**，與網關一致 ✅（輸出 164 vs 200 是取樣與 `max_tokens` 上限，屬正常） |
| 本地 key 配額 | `local_keys.used_tokens` 由 133,951 → 134,006，增加 **55** = 該列 `in 35 + out 20` ✅ |
| 工具歸屬 | 帶 `x-tg-app: codex` 時 `app=codex` ✅；不帶時 `app=unknown`（未標識的客戶端，非退化） |

**約束二（用量資訊必須完整）複驗**：本機工具篩選清單實測列出 **8 個選項** ——
全部本機工具／Claude Code／OpenAI Codex／opencode OpenCode／**Hermes Agent**／
DeepSeek Harness／**Cursor**／**Antigravity**，沒有任何選項消失 ✅

#### B. 關工具 → 關網關 → 重啟工具

**這一步抓到一個真的 bug，而且是「關閉網關」第二次以後才會遇到**：

| 步驟 | 結果 |
|---|---|
| 接管 Codex | 設定變成 8 個 `base_url` 全指向網關、`model_provider = "custom"`、`model = "moonshotai/kimi-k3"` ✅ |
| 真實對話（kimi-k3） | **失敗**：`#4404 st=504 303,004ms` —— 那個模型在 NIM 上要不到（見下方「已知問題」） |
| 真實對話（改 `-m deepseek-ai/deepseek-v4.1-flash`） | 成功，8 秒內回覆，用量正確 ✅ |
| **關閉 Codex** | **失敗**：卡片顯示「所有備份都還含網關配置，無法還原到接管前」❌ |
| 檢查備份 | `config.toml.bak-20260929-025022`（6,911 bytes）**一個網關位址都沒有**、`model_provider = "openai"` —— 它明明是乾淨的 ❌ |

根因（`backup.rs::backup_has_known_route`）：它要求「這份設定裡找得到一個
URL」，但**原生設定正好沒有 URL** ——

- Codex 的第一方 `openai` provider 是**內建**的，沒有 `[model_providers.openai]`
  段 → `codex_text_base_url` 回 `None`；
- Claude 回到官方時沒有 `ANTHROPIC_BASE_URL`。

`restore_backup_to_inner` 會先呼叫 `migrate_gateway_baseline`，而它在沒有
baseline 時會把該函式的 `Err` 直接往外丟 → 使用者看到一句指責備份含網關配置的
訊息，但備份是乾淨的。

**為什麼以前沒事**：在 §0.9 的原生還原上線前，「關閉」還原的是**接管前那份**
（`model_provider = "custom"` ＋ NIM URL，有 URL → 有來源）。是「回到原生」讓
基線／備份變成沒有 URL 的長相，才踩到這個判斷。

修法：`backup_has_known_route` 改成認得原生長相 ——
Codex：有段就必須有 URL（`model_provider = "custom"` 卻沒有段＝壞設定，不算），
沒有段時只有**內建** provider（`openai`）才算有來源；
Claude／OpenCode：只要是合法的非空設定物件就算有來源（有沒有被指到網關由
`backup_is_tainted` 另行判斷，兩者職責分開）。

回歸測試：`switch_off_accepts_a_native_codex_backup`、
`switch_off_accepts_a_native_claude_backup`（兩條都先重現失敗再修）。

修好後重建安裝（exe 9,038,336 bytes、sha256 `3561A32B…`），再跑一次同一個流程：

| 步驟 | 結果 |
|---|---|
| 關閉 Codex（修正後） | `(On -> Off)` ✅、設定 7,718 → **6,913 bytes** |
| 訊息 | `…｜還原自 config.toml.baseline-20260929-032145｜provider=openai model=gpt-5.6-luna｜已切回原生來源：model → gpt-5.6-luna` ✅ |
| 設定內容 | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、**指向網關 0 次**、8 個別名段全在 ✅ |
| **關閉網關** | App 內按「停止」→ `15722 LISTEN = 0`、`gw` 行程消失、按鈕變「啟動」✅（App 本身仍在跑，這是正確的） |
| **重啟工具** | 網關關著的情況下 `codex exec` → 14 秒回覆「官方來源正常。」、exit 0 ✅ |
| **證明沒走網關** | 對話前後 `request_logs` 的 max id **都是 4407**（網關若收到請求一定會寫一列）✅ |
| `codex doctor` | `✓ config loaded`、`model gpt-5.6-luna · openai`、`default model provider openai`、`✓ auth is configured` ✅ |
| 模型清單 | `gpt-5.6-terra`／`gpt-5.6-luna`／`gpt-5.5`（＋2 隱藏）✅ |

**這一輪我改動的檔案**：`~/.codex/config.toml`（接管 → 關閉，最終為原生設定，指紋
6,913 bytes／`7CA78C88…`）、`~/.config/opencode/opencode.json`（§0.9.8）、
`~/.claude/settings.json`**沒動**（仍在網關接管中）。備份都在
`%APPDATA%\com.tokencounter.gateway\backups\`。

**⚠️ 一個要你決定的行為**：原生還原會把 `model` **覆寫**成 `gpt-5.6-luna`
（你 2026-09-28 選的預設）。這次它把你原本的 `gpt-6-luna` 換掉了（訊息裡那句
「model → gpt-5.6-luna」就是這個動作）。如果你希望「已經是 GPT 系列就別動」，
告訴我一聲就改。

#### 已知問題（測試中發現，尚未處理）

- **`moonshotai/kimi-k3` 在 NIM 上 504**（303 秒後逾時）。那是 App 為 Codex
  記住的來源／模型配對，所以「接管 Codex 之後直接用」會失敗。這不是網關算錯，
  是那個上游要不到那個模型；要不要換掉預設配對請你決定。


### 0.9.8 實機驗證（OpenCode）＋ 一個順手抓到的長期瑕疵

照使用者的要求，把 OpenCode 的開關也實機走一遍（接管 → 關閉）：

| 步驟 | 觀察 |
|---|---|
| 接管 OpenCode | `opencode.json` 改成 `provider.tokengateway.options.baseURL = http://127.0.0.1:15722/v1`、`apiKey` 寫入上游 Key 明文 |
| 關閉 OpenCode | 訊息出現：`…｜還原自 opencode.json.baseline-20260928-232249｜**provider=未知 model=未知｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入**｜已切回原生來源：移除 provider.anthropic.options.baseURL（原本指向本網關）、移除 provider.anthropic.options.apiKey（網關的本地 key）` |
| 結果 | `opencode.json` 570 bytes、**指向網關 0 次**、`provider.anthropic.options = {}`（回到 OpenCode 自己的登入）、`provider.tokengateway` 段留著（舊對話通道）✅ |
| 它自己的登入 | `~/.local/share/opencode/auth.json` 內有 `nvidia`／`opencode`／`openrouter`／`opencode-go` ✅ 所以離開網關之後仍可用 |

**那串粗體是錯的，而且不是這一輪造成的**：`restore_summary` 原本不分工具都用
Codex 的 TOML 掃描，於是還原 OpenCode 這種 **JSON** 設定時，畫面端出
「provider=未知 model=未知｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入」
—— 三句全是錯的（它不是 TOML，也跟 Codex 無關）。已改成依工具產生：

| 工具 | 摘要長相 |
|---|---|
| codex | `provider=custom model=moonshotai/kimi-k3`（＋TOML／死鏈警告，照舊） |
| claude | `endpoint=opencode.ai model=claude-opus-5`；沒有覆寫端點時 `endpoint=api.anthropic.com（未覆寫，官方）` |
| opencode | `providers=anthropic,tokengateway` |

JSON 壞掉時也改成講對工具（`⚠️ 還原內容不是合法 JSON，OpenCode 可能無法載入`）。

**第二個瑕疵（同一輪修掉）**：修好摘要之後再實機跑一次，畫面變成
`…｜providers=anthropic,tokengateway｜⚠️ 設定裡沒有網關痕跡，維持原樣` ——
**摘要對了，但那個 ⚠️ 是錯的語氣**。第二次關閉時設定本來就已經乾淨（上一次
已經清過），所以「沒有東西要改」；對一個健康的設定示警，會讓人以為壞了。
`NativeOutcome` 因此把兩件事分開：

| 欄位 | 意思 | 畫面 |
|---|---|---|
| `changes` | 我改了什麼 | `｜已切回原生來源：…` |
| `note` | **沒有東西要改**（正常） | `｜設定裡沒有網關痕跡，維持原樣`（無 ⚠️） |
| `warning` | **做不到**（TOML／JSON 壞掉、工具不支援） | `｜⚠️ …` |

> **Claude Code 沒有做實機點擊**：你選了「連端點一起推回 Anthropic 官方」，而這台
> 機器上沒有 `.claude/.credentials.json` —— 一按下去 Claude Code 就會停用到你登入
> 為止。那條路徑由單元測試釘住（`claude_native_strips_users_own_third_party_endpoint`
> 等 3 條），要啟用請先 `claude` 登入一次，再按開關。

> **我實際改了你的東西**：`~/.codex/config.toml` 現在是原生設定（§0.9.7 那份）。
> 接管前的那份備份在
> `%APPDATA%\com.tokencounter.gateway\backups\codex\config.toml.bak-20260928-223526`
> （7,022 bytes），要退回 NIM 就把它複製回去。基線在還原時被清掉了（那是既有
> 行為），所以**下一次接管會拿現在這份原生設定當基線** —— 這是好事。
> 另外驗證過程中**有一件事不是我做的**：22:29:03 Codex Desktop 自己改了
> `config.toml`（更新它自己的 runtime 路徑、版本 `26.917.62051`→`26.924.22138`、
> 補上 `enabled-reasoning-efforts`）。這個檔案是共用的，不是你我在搶它。

---

## 1. 產物位置

| 產物 | 路徑 |
|---|---|
| 執行檔（免安裝） | `token-gateway\src-tauri\target\release\token-gateway.exe` |
| NSIS 安裝檔（你目前用的形式） | `token-gateway\src-tauri\target\release\bundle\nsis\token-gateway_0.1.0_x64-setup.exe` |
| MSI 安裝檔 | `token-gateway\src-tauri\target\release\bundle\msi\token-gateway_0.1.0_x64_en-US.msi` |

三者都是同一次建置的產物（確切時間與大小見本文件末尾的「建置資訊」）。

---

## 2. 開始之前：務必先關掉舊版

目前有一個**舊版行程正在執行**：

```
pid  8252
path C:\Users\luluna\AppData\Local\token-gateway\token-gateway.exe
```

關掉它（存檔、關視窗，或）：

```powershell
Stop-Process -Id 8252
```

**然後才**安裝或執行新版。不關掉的話：安裝檔無法覆蓋舊檔，而且新版會去
升級一個正被舊版寫入的資料庫。

> **不要**在舊版還開著的時候啟動新版。兩個行程同時寫同一個 SQLite 檔不是好主意。

---

## 3. 第一次啟動會做什麼（已對你的真實資料庫驗證過）

啟動時會把資料庫從 **v7 升級到 v8**，新增兩張表：`proxy_trace`、
`provider_stripped_fields`。這是**純新增**，不動任何既有表。

我已經對**你的真實資料庫的副本**（熱拷貝，含 `-wal`，先驗
`PRAGMA integrity_check` = ok）實跑過升級：

```text
升級前：version=7 providers=7 request_logs=4111
升級後：version=8 providers=7 request_logs=4111 proxy_trace=true provider_stripped_fields=true
```

資料零減損。升級前後你可以自己再確認一次（唯讀，不會改動資料庫）：

```powershell
py scripts\dump_traces.py
```

第一次跑會看到 `proxy_trace: **不存在**`（因為還沒啟動新版），
啟動過新版之後再跑，就會出現 `proxy_trace : <筆數>`。

---

## 4. 測試 A：診斷中心

**狀態：✅ 已通過（2026-09-27 22:07 由使用者實測截圖確認）。**

截圖內容與資料庫實際值逐項核對，全部一致：

| 畫面顯示 | 資料庫實際 | 結果 |
|---|---|---|
| 網關 `127.0.0.1:15722` | 監聽 `127.0.0.1:15722` | ✅ |
| 追蹤筆數上限 5,000 | `TRACE_KEEP = 5000` | ✅ |
| 兩筆時間戳 `20:28:40` / `20:28:17` | `id=2` → `20:28:40`、`id=1` → `20:28:17` | ✅ **精確對應** |
| 層級 `warn` | 當時 2 筆皆為 `trace_level=warn` | ✅ |
| 類型「拒絕」 | 2 筆皆為 `trans_kind=rejected` | ✅ |
| 「尚未觀察到上游拒收欄位」 | `provider_stripped_fields` 為 0 筆 | ✅ 空是正確的 |
| 上游能力記憶說明文字 | 對應 `§5.3` 第 2 層設計 | ✅ |

> **那兩筆是什麼**：是我在本輪為了驗證「網關拒絕要留痕」（測試 B）而送出的
> 頭兩個請求 —— 無 Authorization 與無效金鑰，皆回 401。截圖當時只顯示 2 筆
> 是正確的，因為 22:07 時資料庫確實只有這 2 筆；之後我為驗證 UA 歸屬又送了
> 更多測試請求。

**這解決了先前的空白**：在此之前「診斷中心」這整塊程式碼**從未在任何 GUI 中
實際渲染過**。現在確認版面正常、數字正確、與資料庫一致。

1. 啟動新版，左側導覽應出現 **「診斷」**。
2. 點進去，應看到三塊面板：**網關診斷**、以及剝離欄位與請求追蹤相關區塊。

**通過標準**：頁面正常渲染、沒有錯誤橫幅、三個查詢都回得出資料
（剛升級完追蹤數為 0 是正常的，因為追蹤只在異常時寫入）。

---

## 5. 測試 B：網關拒絕要留痕（本次修正的重點）

### 5.1 先確認網關已啟動

在 App 裡把**網關啟動**（預設埠 **15722**；若你改過，以下指令請代換）。
以下指令都在 **PowerShell** 裡跑。

### 5.2 製造兩個「網關層拒絕」

這兩種拒絕**根本不會碰到上游**，所以修正前完全不會留下追蹤。

```powershell
# ① 完全沒有 Authorization → 應回 401
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Body '{"model":"ghost"}'
} catch { "① status = " + $_.Exception.Response.StatusCode.value__ }
```

```powershell
# ② 帶一個無效的金鑰 → 應回 401（"無效的 Key"）
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Headers @{ Authorization = "Bearer not-a-real-key" } `
    -Body '{"model":"ghost"}'
} catch { "② status = " + $_.Exception.Response.StatusCode.value__ }
```

若你有真實金鑰（`$env:TOKEN_GATEWAY_KEY`），再製造一個 **400**：

```powershell
# ③ 真實金鑰 + 未登記的模型 → 應回 400（模型不在清單內）
$k = $env:TOKEN_GATEWAY_KEY
try {
  Invoke-RestMethod -Uri http://127.0.0.1:15722/v1/chat/completions -Method Post `
    -ContentType 'application/json' -Headers @{ Authorization = "Bearer $k" } `
    -Body '{"model":"definitely-not-registered"}'
} catch { "③ status = " + $_.Exception.Response.StatusCode.value__ }
```

### 5.3 回診斷頁看

**通過標準**（這是這次修正的驗收）—— **2026-09-28 已由我代跑，五項全過**：

- [x] 「請求追蹤」出現對應筆數，且每一筆的展開明細中，「轉換」欄顯示
      **「網關拒絕（未觸及上游）」**（修正前這裡會顯示成 ` → （rejected）`，
      因為這一類沒有格式轉換也沒有上游）
- [x] 這幾筆的狀態碼分別是 401／401／400
- [x] 它們也出現在**問題追蹤**清單裡（因為記為 `warn` 等級）
- [x] 「追蹤總數」相應增加
- [x] 明細中的 **note** 讀得出被拒原因（例如「模型不在白名單」），
      且**不含任何金鑰字串**

**實跑結果**（2026-09-28，網關 15722；`proxy_trace` 由 21 筆 → 24 筆，Δ = **+3**）：

| 送出的請求 | 狀態碼 | 追蹤 ID | 明細「轉換」欄 | 說明欄 |
|---|---|---|---|---|
| 完全沒有 `Authorization` | **401** | 22 | 網關拒絕（未觸及上游） | 缺少 Authorization：請填本地 sk-local-… 或該渠道的上游 Key |
| 帶無效金鑰 `Bearer not-a-real-key` | **401** | 23 | 網關拒絕（未觸及上游） | 無效的 Key（既非本地 Key，也非已登記渠道的上游 Key） |
| 真實本地 Key ＋ 未登記模型 `zzz-not-registered-xyz` | **400** | 24 | 網關拒絕（未觸及上游） | 模型 zzz-not-registered-xyz 不在該 Key 綁定的來源「NIM」模型清單內，請求不會轉發… |

診斷頁讀到：**追蹤總數 24／異常 24／warn 層級／近 24 小時 24**、
**上游狀態碼分佈 401 → 23、400 → 1**，與資料庫逐筆一致。
展開第 24 筆的明細：追蹤 ID 24、時間 2026-09-28 16:15:31、應用 unknown、
模型（原始）`zzz-not-registered-xyz`、**轉換「網關拒絕（未觸及上游）」**、
延遲 6 ms、重試次數 0、請求體 —、body SHA-256 —，最後一列是完整「說明」。

> **注意**：這三筆是**合成測試**，會在 `proxy_trace` 與 `request_logs` 各留 3 筆
> （`app=unknown`、0 token、0 成本），讓「總請求」多 3。這是網關的正常行為
> （被拒請求本來就會留痕），但如果你想清掉，刪掉 `proxy_trace` id 22–24 與
> `request_logs` id 4353–4355 即可。

> **修正前的行為**：以上三筆在診斷頁**一筆都不會出現**。你若想看對照，可以
> 先用舊版跑一次 5.2 的指令，再開診斷頁 —— 但舊版根本沒有診斷頁，所以這個
> 對照其實做不了。這正是問題所在。

---

## 6. 測試 C：原本的症狀（400 與 Codex 會話）

這一項**需要你在真實使用情境下測**，我無法代跑。

1. 如常使用你的 CLI 工具（Claude Code／Codex…）經過本網關。
2. 若再遇到 **「連線錯誤（通常是 400）」**：
   - 立刻開診斷頁，看「問題追蹤」有沒有對應紀錄
   - 特別看 **`body 解析失敗`** 這個計數是否變成非 0
   - 若有，展開明細會看到 **`body_hex`**（請求體前 512 bytes 的十六進位）
     —— 這是判定「究竟是上游回傳 XML/HTML 錯誤頁、空 body，還是合法 JSON
     被誤判」的關鍵證據（**§5.2 已於 2026-09-28 定案：那兩筆是真的非法 JSON，
     不是網關 bug**，見 §5.2，所以這裡不再是懸案，只是留著給未來的新案例）
3. 若再遇到 **Codex 無法續用舊會話／無法封存對話**：
   - 這是 **B3，已修**（別名段聯集「只增不減」＋體檢誠實回報，見 §6.5）
   - 但**成因有四條**（計畫書 Phase 2.5），本次只保證其中兩條的機制；
     若你還是遇到，請把 Codex 的**原始錯誤文字**給我 —— 我目前只能從程式碼
     推論，缺這份文字無法定位到具體分支

---

## 6.5 測試 D：Codex 舊會話別名（B3，已修）

這一項針對「**無法繼續使用舊的 conversation session**」。修好的四條成因見
計畫書 Phase 2.5，但**你要測的只有一件事**：體檢會不會誠實回報。

1. **完全關閉 Codex**（重要 —— 成因之一就是 Codex 正在寫 DB 時的競爭）。
2. 在 App 的 Codex 接管頁跑**接管前體檢**。

**通過標準** —— **2026-09-28 已由我代跑，三項全過**（Codex 行程確認未執行）：

- [x] 若一切正常，會看到
      **「ℹ️ 歷史會話用過 N 個別名 provider（讀自 state_5.sqlite）：…」**
      ——注意它會**標明讀自哪個檔案**
- [x] 若讀不到，會看到
      **「❌ 讀不到 Codex 歷史會話的 provider 名：…」** 並附原因與處置建議
- [x] **最關鍵**：絕對不會在讀不到的情況下看到
      「✅ 歷史會話無第三方 provider 殘留」

**實跑結果**（2026-09-28，於「上游來源 → 本機工具 → Codex」按「接管前體檢」）：

| 讀到的訊息 | 判定 |
|---|---|
| `ℹ️ 歷史會話用過 6 個別名 provider（讀自 state_5.sqlite）：gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen —— 接管將全寫為網關別名段` | ✅ 明示來源檔名、數量與清單 |
| `❌ TOKEN_GATEWAY_KEY 未設定：Codex 行程繼承不到 Key 會 401，請先設為用戶環境變數` | ✅ 失敗附原因＋處置 |
| `✅ 15721 空閒（cc-switch 代理未運行）` | ✅ 真通過 |

**6 這個數字是對的**：`SELECT DISTINCT model_provider FROM threads` 實測回傳
8 個（`custom, gw, mock, nim-direct, nvidia-nim, nvidia-proxy, opencode-zen,
tokengateway`），體檢**刻意扣掉 `custom` 與 `tokengateway`**（網關自己的段名），
程式碼 `doctor.rs:71` 的文案也寫明了「custom / tokengateway 之外」。
那 8 個別名在 `config.toml` 裡**每一個都有對應段**，所以舊會話不會斷。

**第三條（最關鍵那條）為何成立**：`codex_legacy_providers_report` 回傳的是
`LegacyProviders::Ok { .. }` / `Failed { .. }` 這個 enum，體檢用 `match` 分開處理
——「讀不到」走 `Failed` 分支印 ❌，**寫不到 ✅ 那條路上**。這條界線有
單元測試釘住（`tools/tests/codex_legacy.rs`：完全沒有 state DB、以及有 DB 但
缺 `threads` 表，兩者都必須是 `Failed` 而不是 `Ok`）。

第三條是這次修的核心。**修正前，讀取失敗會顯示那個 ✅** —— 唯一的診斷工具
在真正的失敗上給你綠色勾勾。

若要自己確認資料面是否一致（唯讀）：

```powershell
py -c "import sqlite3,os;h=os.path.join(os.environ['USERPROFILE'],'.codex');c=sqlite3.connect(f'file:{h}/state_5.sqlite?mode=ro',uri=True);print(sorted({r[0] for r in c.execute('SELECT DISTINCT model_provider FROM threads')}))"
```

把印出的 provider 名與 `~/.codex/config.toml` 裡的 `[model_providers.*]` 段名
對照：**thread 用過的每一個都應該在 config.toml 裡有對應段**。少任何一個，
那個舊會話就會失效。

## 6.6 測試 E：`wire_api` 是否跟著上游（B4，已修）

這一項只在**直連模式**（接管頁取消勾選「經由網關」，`base_url` 直接指向第三
方）下有意義。

1. 選一個**第三方**渠道（例如 `https://integrate.api.nvidia.com/v1`），走直連。
2. 看**接管預覽**的文字。

**通過標準** —— **2026-09-28 已由我逐項核對**（準則 1 未做成螢幕證據，理由見下）：

- [x] 預覽顯示 `wire_api = chat`（**不是** `responses`）—— 由
      `codex_wire_api_prefers_declared_format` 與
      `codex_apply_picks_wire_api_per_upstream` 兩條測試釘住（見下）
- [x] 接管後打開 `~/.codex/config.toml`，每個 `[model_providers.*]` 段都是
      `wire_api = "chat"` —— **在你真實的 config.toml 上實測通過**
- [x] 換成經由網關接管時，同一欄位是 `wire_api = "responses"`
- [x] 若該渠道的 `api_format` 被宣告為 `openai-responses`，即使 base_url 是
      第三方，也應該寫 `responses`（**宣告優先於 URL 猜測**）

> ## ❌ 這一項原本是**程式錯、文件對**，而且造成了真實故障
>
> **2026-09-28 17:xx 追查「Codex 開啟時報 Unable to log in」時發現：**
> 實作把「宣告優先」用到了**指向網關**的情況上 —— 你的 7 個來源
> `api_format` **全部是 `openai-chat`**，於是接管後**每個** provider 段
> 都被寫成 `wire_api = "chat"`。
>
> 而 **Codex 已經不再接受 `wire_api = "chat"`**。它的日誌寫得很清楚
> （`~/.codex/logs_2.sqlite`）：
>
> ```
> ERROR codex_app_server: Invalid configuration; using defaults.
> C:\Users\luluna\.codex\config.toml:16:12: `wire_api = "chat"` is no longer supported.
> How to fix: set `wire_api = "responses"` in your provider config.
> ```
>
> Codex 讀到非法設定就**整份 config 丟棄改用預設值**，於是回頭找 ChatGPT
> 登入 —— 使用者看到的錯誤是 **「Unable to log in」**，
> **完全看不出真正的起因**。這正是本節準則 3 要防的事。
>
> **為什麼會漏掉**：單元測試只驗了
> 「網關 URL ＋ **未宣告**」→ `responses`（`CodexAuth::default()`），
> 而生產路徑**永遠帶著宣告**（`switch.rs` 傳
> `codex_wire_api_declared(provider_format)`）。兩者從未在同一條測試裡相遇。
>
> **修法**（`wire.rs` 新增 `codex_wire_api_for`）：先看 URL ——
> 指向**網關／官方 OpenAI** 就定死 `responses`，宣告只在**真正的第三方**
> 才發揮作用。並補上回歸測試
> `codex_apply_forces_responses_on_gateway_even_if_declared_chat`，
> 用的正是生產路徑的參數組合。

**在你真實檔案上的實測**（2026-09-28，接管中、來源 opencode-go；修正後）：

| 檢查 | 結果 |
|---|---|
| `[model_providers.*]` 段數 | 8（`custom`／`tokengateway`／`gw`／`mock`／`nim-direct`／`nvidia-nim`／`nvidia-proxy`／`opencode-zen`） |
| 每一段的 `wire_api` | **全部 `responses`** ✓ |
| 每一段的 `base_url` | **全部 `http://127.0.0.1:15722/v1`** ✓ |
| 有沒有段落漏寫 `wire_api` | 無 ✓ |

**釘住這件事的測試**（`src-tauri/src/tools/tests/codex_wire_api.rs`）：

| 測試 | 釘住什麼 |
|---|---|
| `codex_wire_api_prefers_declared_format` | `openai-chat`→`chat`；`openai-responses`／`mixed`→`responses`；`anthropic`／`gemini`／`None`／空字串→`None`（交 URL 推定）；且**同一個第三方 URL 下，宣告能推翻 URL 推定** |
| `codex_wire_api_falls_back_to_url_when_undeclared` | 沒宣告時一定回退到 URL 推定，不會變成無值 |
| `codex_apply_picks_wire_api_per_upstream` | 直連第三方 → **每個**段都 `chat`；走網關且未宣告 → **每個**段都 `responses`；別名段也必須存在 |
| **`codex_apply_forces_responses_on_gateway_even_if_declared_chat`** | **新增（回歸）**：網關 URL ＋ 宣告 `openai-chat` → **必須** `responses`；官方 OpenAI 同理；真正的第三方才輪到宣告生效 |

> **準則 1 為何沒做成螢幕截圖**：接管預覽要展開「N 項寫入」才會露出內容，
> 而那個展開控制項在 UI Automation 裡不是可 Invoke 的元素；我用座標點它時
> 反而把對話框關掉了。改以「測試 + 真實檔案」舉證。**沒有捏造螢幕證據。**

`wire_api` 決定 Codex 用哪種協議打 `base_url`。寫錯的話 Codex 會打到不存在的
端點而 **404** —— 修正前直連第三方一律被寫成 `responses`，明知會壞還照寫。

## 6.7 測試 F：DSH 用量為何不出現在 Token 趨勢（**已查明，非程式 bug**）

**你回報**：「Token 趨勢中還是沒有出現 DeepSeek Harness 的使用量」、
「本機工具應該顯示 DSH 或 DeepSeek Harness，而不是 DeepSeek」。

### 結論先講

| 你的回報 | 查證結果 |
|---|---|
| 標籤顯示「DeepSeek」 | ✅ **是真 bug**，已修（前端寫錯，後端本來就對），見 §6.8 |
| Token 趨勢沒有 DSH 用量 | ⚠️ **不是程式 bug** —— DSH 目前的設定**根本沒有經過網關**，所以網關看不到它的用量 |
| 「DSH 用量被記成 Codex」 | ❌ **我先前搞錯了**，見下方「更正」 |

### 為什麼網關看不到 DSH 的用量

`~/.dsh/settings.yaml`：

```yaml
llm-deepseek:
  baseURL: http://127.0.0.1:15722/v1   # 指向網關
  models: []                            # ← 沒有設定任何模型，等於沒在用
llm-pi-ai:
  providers:
    oc-go:
      baseURL: https://opencode.ai/zen/go/v1   # ← 直連，繞過網關
agent-default-model:
  provider: oc-go                       # ← DSH 實際用的就是這個
  model: deepseek-v4.1-flash
```

DSH 實際使用的 provider 是 `oc-go`，它**直接連到 `opencode.ai`**，不經過網關。
指向網關的 `llm-deepseek` 反而 `models: []`（沒有模型）。

**實測佐證**：我本身就跑在 DSH 裡。在 22:00～23:35 之間我進行了數十個回合，
`request_logs` 裡**完全沒有**這些請求 —— 網關最後一次收到真實流量是 17:09。
若 DSH 經過網關，我自己的每個回合都會留下紀錄。

### 要讓 DSH 用量被統計，你把 `oc-go` 的 `baseURL` 改成網關即可

```yaml
llm-pi-ai:
  providers:
    oc-go:
      baseURL: http://127.0.0.1:15722/v1   # 改成網關
```

改完後 DSH 的請求就會被記錄。歸屬會是正確的 `dsh`（見下方 UA 查證）。

### 更正：我先前「1,990 筆被記成 Codex」的說法是錯的

我在 `97e5911` 的訊息裡寫了「`app='codex'` 的 3,505 筆中有 1,990 筆
（56.8%）其實是 DSH」。**這個結論是錯的**，原因是我只看模型名稱就推論來源。

錯在哪：

1. **`~/.codex/config.toml` 的 `model = "deepseek-v4.1-flash"`** —— Codex 自己
   就設定了這個模型，它的 `models = [...]` 清單裡還包含
   `muse-spark-1.3-contributor`、`space-bunny-free`、`omen-alpha` 等
   我誤稱為「DSH 專屬」的模型。**兩個工具用的是同一批模型名稱。**
2. **`source='import'` 的 1,142 筆來自 `~/.codex/sessions/*.jsonl`** ——
   那些是 Codex 自己的會話檔，本來就是 Codex。其中確實包含
   `glm-5.3-flash`（523 筆）等模型，證明 Codex 真的用過它們。
3. **舊版 `infer_app` 對 DSH 的真實 UA 本來就會回 `dsh`。** 我核對過
   baseline `38f060a` 的原始碼：`deepseek-harness/...` 這個字串不含
   `codex`／`opencode`／`hermes`／`dsh`，會命中 `deepseek` → 回 `dsh`。
   所以**從來沒有把 DSH 記成 Codex**。

### UA 查證（這部分仍然有效）

DSH 的 `dsh-llm` 與 `dsh-llm-pi-ai` 都會用 `attributionHeaders()` 送出：

```
deepseek-harness/<version> (+https://github.com/deepseek-ai/deepseek-harness)
```

`dsh-llm-pi-ai` 的 `requestHeaders()` 會**移除大小寫不敏感的衝突欄位**再套用
attribution，所以即使底層 SDK 想設自己的 UA 也會被覆蓋。DSH 不可能送出含
`codex` 的 UA。

### 那 `97e5911` 的 `infer_app` 改動要留嗎？

**留。** 它對真實流量是**行為等價**的（所有真實 UA 的判定結果都相同），
只在「UA 同時含多個工具字樣」時更穩健。我補的 7 條測試也把行為釘住了。
但它**不是**你症狀的原因 —— 這點我必須講清楚。

---

## 6.8 測試 G：本機工具標籤（已修，`43930a4`）

前端 `logos.tsx` 的 `APP_META` 把 `dsh` 標成 `"DeepSeek"`，後端
`tools/consts.rs` 的 `APPS` 早就寫 `"DeepSeek Harness"` —— 兩份清單各寫各的。

這一格指的是**本機工具**（DeepSeek Harness CLI），不是模型或廠商名稱，
寫成 `DeepSeek` 會讓人以為那是模型。

**通過標準**：Usage 頁的來源篩選、Token 趨勢圖例、診斷頁明細，凡顯示來源
名稱處都應為「DeepSeek Harness」。

**已做的防護**：
- Rust 測試 3 條釘住 `APPS` 顯示名（含「不可簡寫成 DeepSeek」）
- `py scripts/check_app_labels.py` 比對前後端兩份清單，不一致則 exit 1

---

## 7. 回報方式

跑這個（**唯讀開啟，不會改動資料庫**）：

```powershell
py scripts\dump_traces.py -n 50
```

會同時印到螢幕並寫入 `trace_dump.txt`。把那個檔給我即可。它包含：
schema 版本、各表筆數、最近追蹤的完整欄位（含 `body_hex` 還原）、
`trans_kind`／狀態碼分布、以及已學會的剝離欄位。

只想看問題的話：

```powershell
py scripts\dump_traces.py --problems -n 100
```

> 腳本以 `mode=ro` 開啟資料庫。它刻意不寫入任何東西。

---

## 8. 已知未修 / 未驗證的事（誠實清單）

**未修**（尚未授權或需產品決策）：

- ~~**B3**：Codex 舊會話 provider 別名段靜默消失。~~ **已修** —— 見測試 D。
  共 4 條成因（檔名寫死 `state_5.sqlite`、缺 `busy_timeout`、管理清單會縮小、
  `codex_doctor` 把失敗報成 ✅）加上第 5 條 `rows.flatten()` 靜默吞錯。
- ~~**B4**：無差別強制 `wire_api = "responses"`。~~ **已修，而後整個機制已移除。**
  過程是：B4 的修法（宣告優先）本身有缺陷 —— 它把指向網關的每個段都寫成
  Codex 已不接受的 `chat`，造成「Unable to log in」（2026-09-28，§0.6）。
  接著發現**那套設計的前提全部失效**（直連模式已移除、Codex 不接受 `chat`、
  `api_format` 描述的是上游而非 Codex→網關），於是**整組拿掉**：
  現在 `wire_api` 是單一常數 `CODEX_WIRE_API = "responses"`，見 §0.7.2。
- ~~**§5.3 第 2 層**：能力宣告。~~ **已實作，而後移除。**
  協議選擇改成宣告優先之後被證明會寫出 `chat` 而讓 Codex 打不開，
  最終於 2026-09-28 移除整個宣告機制（同上一條）。
  `providers.api_format` 欄位保留，但只剩「來源列表顯示」與「網關端轉譯」用途。
- ~~**§5.2 的 body 解析 400**：仍未定案。~~ **已定案（2026-09-28）** ——
  **不是網關的 bug**。把三筆證據的 body 與宣稱的 `bytes_len` 逐一對齊
  （69／107／132 全部相符，儀器沒有弄壞 body），再實際丟給 JSON 解析器：
  兩個 FAILED 的 body **key 都沒加引號**（`{model:...}` 而非 `{"model":...}`），
  那是 JS 物件字面值、本來就不是合法 JSON，`serde_json` 拒絕它們完全正確；
  OK 那筆是合法 JSON 且成功。網關回 400 是對的行為。完整推理見
  `docs/evidence/2026-09-28-body-parse-CONCLUSION.md`。
- ~~**`resolve_filter` 的「365 天」訊息與「366 天」判斷式不一致**~~ **已修** ——
  前端 `RangePicker.tsx` 的 `CUSTOM_MAX_DAYS` 本來就是 365，三處裡只有後端
  判斷式是錯的，因此**沒有產品決策空間**。已統一為 365，測試改為
  `resolve_filter_span_limit_is_365_days`。
- ~~**`db.rs` migration 仍無自動化測試**（§2.1 F6）。~~ **已補** ——
  新增 `every_legacy_version_upgrades_preserving_data`：對 v1…v7
  **每一個歷史版本**各造一個資料庫，確認升級後版本正確、providers／
  request_logs 一列不少、settings 既有值不變、12 張核心表齊全。
  原本那條需人工觸發的 `live_migrate_real_db_copy` 保留作為出貨前的額外確認。
- ~~**前端三個肥檔未動**：`Keys.tsx` 876、`Calc.tsx` 809、`lib/api.ts` 803。~~
  **已拆**（2026-09-28），見下方「拆檔」段落。

**已做但未達標**：

- ~~`proxy_handler` **86 行**（目標 < 80）。~~ **已達標：78 行** ——
  根因不是「重複 5 個欄位」而是 `RetryCtx`／`FinishCtx`／`PrepareInput`
  **各自重複宣告** `ctx`／`started`／`app`／`model_raw`。新增
  `proxy/reqctx.rs` 的 `ReqCtx`，三個階段改為嵌入它，handler 只建一次。
- ~~**拆檔後仍 > 400 行的檔案**~~ **已全部拆完**（2026-09-28）。
- ~~**Phase 5 錯誤型別只轉了 6/108 處**~~ —— 這一項**維持不動**：`keys.rs`
  是刻意選的試點，轉換路線已證明可行且不傷文案，但**每次轉換都應有人能
  實測**。剩下的 102 處屬獨立的漸進工作，不建議一次改完。

### 8.1 拆檔成果（2026-09-28）

> **行數一律用 `[IO.File]::ReadAllLines()` 量**。PowerShell 5.1 的
> `Get-Content` 對含中文的檔案會少算（見 §0.4.4），下面的「原行數」是
> 真實值，會比 §8 舊文裡引用的數字大。

**原本 §8 列出的 5 個檔案**：

| 原檔 | 原行數 | 現況 |
|---|---:|---|
| `Providers.tsx` | 778 | **274**（抽 SourceCard／SourceDetail／ToolList ＋ useDragSort／useToolSwitch） |
| `providers/ModelCatalog.tsx` | 608 | **5**（barrel；`modelcatalog/` 4 檔，最大 238） |
| `providers/ProviderForm.tsx` | 590 | **18**（barrel；`providerform/` 4 檔，最大 356） |
| `tools/tests/codex.rs` | 566 | **326**（再拆 `codex_wire_api.rs`、`codex_legacy.rs`） |
| `usage/usageCharts.tsx` | 546 | **295**（日誌元件搬到 `usageLogs.tsx`） |

**稽核時另外掃出的、原本不在清單上的檔案**（既然要拆就一併處理）：

| 檔案 | 原行數 | 現況 |
|---|---:|---|
| `Keys.tsx` | 884 | **254**（+ `keys/` 6 檔） |
| `Calc.tsx` | 818 | **366**（+ `calc/` 5 檔，最大 225） |
| `lib/api.ts` | 808 | **318**（型別拆到 `apiTypes.ts` 303 ＋ `apiTypesPricing.ts` 172 ＋ `apiTypesDiagnostics.ts` 70） |
| `translate.rs` | 2,345 | `translate/` 13 檔，最大 **341** |
| `usage.rs` | 2,190 | `usage/` 14 檔，最大 **369** |
| `commands.rs` | 1,233 | `commands/` 12 檔，最大 **224** |
| `db.rs` | 1,132 | `db/` 9 檔，最大 **296** |
| `history.rs` | 1,038 | `history/` 6 檔，最大 **273** |
| `models.rs` | 990 | `models/` 5 檔，最大 **360** |
| `proxy/tests/e2e_basic.rs` | 705 | 23 行 root ＋ `e2e_basic/` 3 檔，最大 **300** |
| `tools/codex.rs` | 602 | `tools/codex/` 4 檔，最大 **274** |
| `quota.rs` | 595 | `quota/` 6 檔，最大 **153** |
| `trace.rs` | 576 | `trace/` 5 檔，最大 **242** |
| `SwitchDialog.tsx` | 574 | **9**（barrel；`switchdialog/` 3 檔，最大 353） |
| `Diagnostics.tsx` | 563 | **6**（barrel；`diagnostics/` 7 檔，最大 291） |
| `usage/usageLenses.tsx` | 453 | **8**（barrel；`lenses/` 4 檔，最大 210） |
| `tools/versions.rs` | 450 | `tools/versions/` 4 檔，最大 **184** |
| `proxy/tests/e2e_translate.rs` | 445 | 23 行 root ＋ 3 檔，最大 **162** |
| `proxy/tests/e2e_strict.rs` | 430 | 23 行 root ＋ 3 檔，最大 **194** |
| `keys.rs` | 419 | `keys/` 2 檔，最大 **363** |
| `price_extract.rs` | 410 | `price_extract/` 2 檔，最大 **359** |

> **方法與保證（三條不變式）**：
> 1. **純搬移** —— 零行為、零文案、零邏輯、零 JSX／className 變更。多個子代理
>    各自做了逐位元組／多重集比對（例如 `Calc.tsx`：142 個中文片段 697 字元
>    零差異；`ProviderForm.tsx`：597 行中僅 7 行是表頭與 import 路徑）。
> 2. **對外路徑不變** —— 前端用再匯出 barrel，Rust 用 `pub(crate) use 子模組::*`，
>    所以**所有既有 import 端一行都不用改**（`App.tsx`、`lib.rs` 的
>    `generate_handler!`、各 `crate::X::Y` 呼叫點全部照舊）。
> 3. **可見性只放寬、不收緊** —— 跨子模組呼叫所需的 `pub(crate)`／`pub(super)`，
>    共十餘處，皆非邏輯變更。
>
> 過程中以 `cargo test` 通過數（218）、`cargo clippy`（0 警告）、
> `npx tsc --noEmit` 與 `pnpm build` 逐步把關。

**順手修掉的一個真實缺陷**：`Providers.tsx` 裡我先前加的「本機工具版本」面板
同時出現在內嵌區塊與收合區，等於**重複渲染**。拆檔時只保留收合區那一份。

**已驗證但你可能想自己再看一次**：

- `cargo test --offline` → **218 passed / 0 failed / 8 ignored**（exit 0）
- `cargo clippy --offline --all-targets` → **exit 0、零警告**（原為 4 條）
- `cargo check --offline --all-targets` → exit 0
- `npx tsc --noEmit` → 0 錯誤
- `pnpm build` → 成功，bundle 553.18 kB（gzip 152.36 kB）
- **全樹 202 個 `.ts`/`.tsx`/`.rs` 檔案，最大 399 行**，無一超過 400
- **拆 `keys.rs` 錯誤型別時的中文訊息逐字比對**：15 條 → 15 條，零遺漏、
  零新增、零改字（腳本：`.workbuddy/tmp/verify_msg_text.py`）

> **我沒有實際啟動過 GUI**。二進位的正確性是用「建置成功 + 二進位內含修正
> 字串 + 前端 bundle 內含新文案」間接確認的，升級路徑是用真實資料庫副本
> 實跑的。GUI 本身要由你第一次啟動時驗證 —— 這也是測試 A 的意義。
>
> **2026-09-28 補註**：這一輪（用量「工具」視角）我**實際啟動了 GUI**，
> 以「視窗截圖 → OCR → 與資料庫逐項核對」驗證過畫面，方法與證據見 §0.1。
> 上面這段描述的是 09-27 那次建置的驗證方式，保留作為對照。

---

## 9. 建置資訊

### 9.5 前一次建置（2026-09-28 12:31，§8 誠實清單清空輪，已被 9.6 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 12:31:27**（release 編譯 7 分 33 秒） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,830,299 bytes |
| 後端測試 | **218 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告**（`cargo clippy --offline --all-targets`，exit 0） |
| 前端型別 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-Dy7Y8xkU.js`、`index-DnRx7SDk.css` |

**產物內容抽查**（讀 exe 位元組）：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-Dy7Y8xkU.js` / `index-DnRx7SDk.css` | ✅ 存在，與 `dist/` 一致 | 內嵌的確實是本次建置的前端 |
| `usage_by_app` | ✅ 存在 | 第二階段的分工具用量仍在 |
| `via_gateway` | ❌ 不存在 | D-1（單一模式）沒有被拆檔改回去 |
| `hermes_home` / `parse_hermes_model` | ❌ 不存在 | F（移除 hermes 支援）仍然成立 |

**安裝驗證**：

| 步驟 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 9,009,152 bytes、04:34:36（前一版） |
| 執行 NSIS `/S` | exit code 0 |
| 安裝後同一路徑 | **9,010,176 bytes、12:31:18**，內含 `index-Dy7Y8xkU.js` ✅ |

**實機 GUI 驗證（本輪新增，逐項都是螢幕 OCR 讀出來的）**：

| 驗證項 | 結果 |
|---|---|
| 應用程式啟動、用量頁渲染 | ✅ 側邊欄、摘要卡、八個鏡頭頁籤（工具／模型／月度／時段…）全部讀到 |
| **要求二：本機工具篩選保留全部選項** | ✅ **展開下拉後 8 個選項全部在畫面上**：全部本機工具、Claude Code、Codex、OpenCode、**Hermes Agent**、**DeepSeek Harness**、**Cursor**、**Antigravity** |
| DSH 標籤正確 | ✅ 顯示「DeepSeek Harness」（非舊的「DeepSeek」） |
| 導航可用 | ✅ 點側邊欄可切到「設定」頁（監聽連接埠 15722、強調色等區塊都正確渲染） |
| **未寫入任何設定** | ✅ `app.db` 仍為 09-27 17:09:05、1,425,408 bytes；`settings.json`／`config.toml`／`opencode.json` 時間戳皆未變 |
| **備份未被輪換** | ✅ claude 10／codex 11／hermes 8／opencode 10，與驗證前基線完全相同 |
| 下拉未選擇任何項目 | ✅ 驗證後按 ESC 關閉，未觸發 `onChange` |

> **一個操作上的坑（給下次的自己）**：這台機器的 WebView2 會**吃掉前幾次合成滑鼠
> 點擊**（第一次點擊只用來讓 webview 取得焦點）。純 Win32 程式（Notepad 測試）
> 不受影響，所以別把「沒反應」誤判成 UI 壞了。本輪最後是「同一點連點 3 次」
> 才穩定生效；且 OCR 座標與點擊座標存在固定偏移，需先做一次校準。

---

### 9.10 最新建置（2026-09-29 03:20，端到端測試輪：修好「原生備份無法還原」）—— **你目前安裝的就是這一個**

使用者要求跑一輪完整端到端測試（開網關 → 對話 → 檢查用量 → 關工具 → 關網關 →
重啟工具 → 檢查來源），過程中抓到並修掉 `backup_has_known_route` 的失效
（§0.9.9），因此重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-29 03:1x 起算 → 產物 03:20:10 |
| 執行檔大小 | 9,038,336 bytes |
| NSIS 大小 | 3,843,545 bytes |
| **安裝後執行檔 SHA-256** | **`3561A32B5CDE3F50C29BC573991A9ED80DE0CB09A727E9C0E4EC1B5392E71E6D`**（前一版 `66339F9E…`） |
| 後端測試 | **231 passed / 0 failed / 8 ignored**（+2 條回歸測試） |
| clippy | **0 個警告** |
| 檔案行數 | 全部 ≤ 400 行 |
| 實機驗證 | §0.9.9 全流程（含「網關關著仍能對話、DB 不新增列」） |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/backup.rs` | **修**：`backup_has_known_route` 認得原生長相（Codex 內建 `openai`、Claude 無 base URL） |
| `src-tauri/src/tools/tests/native.rs` | 新增 2 條回歸測試（codex／claude 原生備份必須可還原） |
| 文件 | 本檔 §0.9.9／§9.10 |

---

### 9.9 前一次建置（2026-09-28 23:42，「關閉網關回到原生來源」輪，已被 9.10 取代）

這一輪做 §0.9：新增 `switch_off`（還原 ＋ 原生來源化），並修掉因此暴露的
還原摘要假警報。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-28 23:36 起算 → 產物 23:42:27（release 約 6 分鐘） |
| 執行檔大小 | 9,038,848 bytes |
| NSIS 大小 | 3,844,624 bytes |
| MSI 大小 | 7,356,416 bytes |
| **安裝後執行檔 SHA-256** | **`66339F9E3519947AC2C6D9E228DB1110123BAD4705CB8BD65ACEF4D0346C907F`**（前一版 `E098655E…`） |
| 後端測試 | **229 passed / 0 failed / 8 ignored**（+12） |
| clippy | **0 個警告** |
| 前端 | `tsc --noEmit` exit 0 |
| 檔案行數 | 205 檔全部 ≤ 400 行 |
| 實機驗證 | §0.9.7（Codex 回到 `openai` ＋ `gpt-5.6-luna`，模型清單變回 GPT）、§0.9.8（OpenCode 回到自己的登入，摘要有依工具產生） |

> 這一輪共建置 4 次（`switch_off` → Claude 端點推回官方 → 摘要依工具 → note／warning
> 分離）。上面是那 4 次的最後一次，**已被 9.10 取代**。

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/native.rs` | **新增**：`to_native`（codex／claude／opencode 的原生來源化）＋ `restore_native_to_port`／`restore_native_port` |
| `src-tauri/src/tools/summary.rs` | **新增**：`restore_summary` 從 `backup.rs` 搬出（該檔一度 421 行）；新增 `active_provider_is_direct_chat` |
| `src-tauri/src/tools/tests/native.rs` | **新增**：10 條測試（＋1 條 note／warning 分離） |
| `src-tauri/src/tools/tests/restore.rs` | 新增 `summary_is_per_app_not_codex_only` |
| `src-tauri/src/tools/backup.rs` | 摘要邏輯搬走（行為不變） |
| `src-tauri/src/tools.rs`／`tools/tests.rs` | 模組與再匯出清單同步 |
| `src-tauri/src/commands/apps.rs`／`src/lib.rs` | 新增 `switch_off` 命令 |
| `src/lib/api.ts` | 新增 `switchOff`；`switchRestore` 保留為「逐字還原」安全網（UI 不再呼叫） |
| `src/components/providers/useToolSwitch.ts` | 行開關 OFF 改走 `switchOff` |
| `src/components/providers/ToolList.tsx` | 開關 tooltip 更新 |
| `src/components/providers/switchdialog/useSwitchDialog.ts`／`SwitchDialog.tsx` | 詳情開關 OFF 改走 `switchOff`；確認框與說明文案更新 |
| 文件 | 本檔 §0.9／§9.9、`SIMPLIFICATION-PLAN.md` §10.15、`MANUAL-TESTS.md` 第 9 節（新增） |

---

### 9.8 前一次建置（2026-09-28 20:15，未完成清單收乾輪，已被 9.9 取代）

這一輪把 §0.7 的四項全部做完（靜默重接管、`wire_api` 機制移除、
體檢假警報、測試 B 殘留資料），因此重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | 2026-09-28 20:09:41 起算 → 產物 20:15:12 |
| 執行檔大小 | 9,010,176 bytes |
| NSIS 大小 | 3,827,867 bytes |
| MSI 大小 | 7,344,128 bytes |
| **安裝後執行檔 SHA-256** | **`E098655E9488C4F79757B5266A220168DC80DD3D8276BA67658BFD793E1B4498`**（前一版 `9AD41461…`） |
| 後端測試 | **217 passed / 0 failed / 8 ignored** |
| clippy | **0 個警告** |
| 前端 | `tsc --noEmit` exit 0 |
| 啟動後 | 15722 LISTEN；`config.toml` SHA-256 不變 |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src/components/providers/useToolSwitch.ts` | **移除**來源切換自動重接管（改為只提示）；刪 `applyCurrentSelection` |
| `src-tauri/src/tools/codex/wire.rs` | 加 `CODEX_WIRE_API` 常數；刪 `codex_wire_api`／`codex_wire_api_for`／`codex_wire_api_declared`／`CodexAuth.wire_api` |
| `src-tauri/src/tools/switch.rs` | 預覽改用常數；`CodexAuth` 建構簡化 |
| `src-tauri/src/tools/codex/doctor.rs` | 體檢的 `TOKEN_GATEWAY_KEY` 改為依設定檔實際使用情況判定 |
| `src-tauri/src/tools.rs`／`tests.rs` | 再匯出清單同步 |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 改寫：3 條舊機制測試刪除，2 條回歸測試保留並強化 |
| `src-tauri/src/tools/tests/codex.rs` | 刪重複的舊測試；新增體檢假警報回歸測試 |
| 文件 | 本檔 §0.5.3／§0.6.7／§0.7／§9.8、`SIMPLIFICATION-PLAN.md` §10.13、`MANUAL-TESTS.md`（新增） |

---

### 9.7 前一次建置（2026-09-28 18:45，`wire_api` 預覽修正輪，已被 9.8 取代）

**為什麼要再建一次**：§0.6.5b 發現 `plan_switch`（畫面預覽）有**第二份**
同樣的運算式，會顯示 `chat` 而實際寫 `responses`。這處修正在 Rust 後端，
所以要重新建置。

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 18:38:30 起算 → 產物 18:45:28**（release 編譯約 7 分） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,829,430 bytes |
| **安裝後執行檔 SHA-256** | **`9AD41461C3293FDBD344B30FCBC9FE23748264729E5F31709F5E8EF2C85BF0EE`**（前一版 `758F03DF…`，已換掉） |
| 後端測試 | **220 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告** |
| 新增測試 | `plan_preview_wire_api_is_responses_on_gateway`（＋前一版的 `codex_apply_forces_responses_on_gateway_even_if_declared_chat`） |
| 啟動後 | 15722 LISTEN ✓；`config.toml` **SHA-256 完全沒變**（開頁面不會觸發接管） |

**這一輪改到的檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/codex/wire.rs` | 新增 `codex_wire_api_for()`；`gateway_section()` 改用它 |
| `src-tauri/src/tools/switch.rs` | `plan_switch` 預覽改用**同一個函式**（第二份副本） |
| `src-tauri/src/tools.rs` | 再匯出清單：`codex_wire_api` → `codex_wire_api_for`（前者已無模組外使用者） |
| `src-tauri/src/tools/tests.rs` | 測試模組再匯出清單同步 |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 兩條回歸測試 |
| `docs/TESTING.md`、`docs/SIMPLIFICATION-PLAN.md`、`docs/evidence/…` | §0.6／§9.6／§9.7、§10.12 |

---

### 9.6 前一次建置（2026-09-28 18:14，`wire_api` 修正輪，已被 9.7 取代）

**為什麼要重建**：§0.6 的 `wire_api` 修正動到 Rust 後端。**舊版執行檔仍會把
`wire_api` 寫成 Codex 已不接受的 `chat`** —— 在裝上這一版之前，只要點
「上游來源」頁的來源卡片，Codex 就會再次罷工。

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 18:05:49 起算 → 產物 18:14:53**（release 編譯 7 分 30 秒） |
| 執行檔大小 | 9,010,176 bytes |
| MSI 大小 | 7,344,128 bytes |
| NSIS 大小 | 3,827,231 bytes |
| **安裝後執行檔 SHA-256 前綴** | **`758F03DFB3E69A73941FA2B0`**（前一次安裝的是 `25CCCF4F0942ACBB17A3B5B8`，**已確實換掉**） |
| 後端測試 | **219 passed / 0 failed / 8 ignored**（實跑 `cargo test --offline`，exit 0） |
| clippy | **0 個警告**（`cargo clippy --offline --all-targets`，exit 0） |
| 新增測試 | `tools::tests::codex_wire_api::codex_apply_forces_responses_on_gateway_even_if_declared_chat` |
| 前端 | **未改動**（`src/` 一行都沒動） |
| 啟動後 | 15722 LISTEN ✓；`config.toml` **mtime（17:59:48）與 SHA-256 都沒變** —— 開頁面不會觸發接管 |

**這次的程式改動共 4 個檔案**：

| 檔案 | 改動 |
|---|---|
| `src-tauri/src/tools/codex/wire.rs` | 新增 `codex_wire_api_for()`；`gateway_section()` 改用它 |
| `src-tauri/src/tools/tests.rs` | 測試模組的再匯出清單加一行 `codex_wire_api_for` |
| `src-tauri/src/tools/tests/codex_wire_api.rs` | 新增回歸測試（生產路徑的參數組合） |
| `docs/TESTING.md`、`docs/evidence/2026-09-28-codex-config-incident.md` | 本節與 §0.6 |

---

### 9.4 前一次建置（2026-09-28 04:34，第一階段 F，已被 9.5 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 04:34:43** |
| 執行檔大小 | 9,009,152 bytes（8,798 KB） |
| MSI 大小 | 7,344,128 bytes（7,172 KB） |
| NSIS 大小 | 3,830,837 bytes（3,741 KB） |
| 後端測試 | **216 passed / 0 failed / 8 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| clippy | **4 個警告**（原為 5；少的那個在被刪的 hermes 測試裡） |
| 前端型別檢查 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-B1Ek3VaH.js`（前版 `index-BAemutFc.js`）、`index-DnRx7SDk.css`（未變） |

**已驗證產物比所有原始碼新**（最新原始碼 `Providers.tsx` 為 `04:11:21`，產物 `04:34:43`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-B1Ek3VaH.js` | ✅ 存在 | 內嵌的是本次建置的前端（含工具頁版本面板） |
| `hermes_home` / `parse_hermes_model` / `HERMES_CUSTOM_TOKENGATEWAY_API_KEY` | ❌ **全部不存在** | F 真的編進去了（hermes 支援整塊消失） |
| `usage_by_app` | ✅ 存在 | 用量設施仍在（要求二未被犧牲） |

**安裝驗證**：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-28 03:27:52，內含 `index-BAemutFc.js`（前一版） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 04:34:36、9,009,152 bytes，內含 `index-B1Ek3VaH.js`、且無 hermes 字串 ✅ |
| 實機 GUI | 見 §0.3（工具清單 4 個、頁籤讀「2/4 接管中」、全程未寫入任何設定） |

### 9.3 前一次建置（2026-09-28 03:28，第一階段 D-1／A／E，已被 9.4 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 03:28**（release 編譯 12 分 14 秒） |
| 執行檔大小 | 9,046,528 bytes（8,835 KB） |
| MSI 大小 | 7,364,608 bytes（7,192 KB） |
| NSIS 大小 | 3,842,393 bytes（3,752 KB） |
| 後端測試 | **223 passed / 0 failed / 9 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| clippy | 5 個警告，與基線相同（皆在既有程式碼） |
| 前端型別檢查 | `npx tsc --noEmit` exit 0 |
| 前端資源指紋 | `index-BAemutFc.js`（前版 `index-BtK0gyS1.js`）、`index-DnRx7SDk.css`（未變） |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `tools.rs` 為 `03:11:50`，產物 `03:28:11`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-BAemutFc.js` / `index-DnRx7SDk.css` | ✅ 存在 | 內嵌的是本次建置的前端（含 A 的文案改動） |
| `via_gateway` | ❌ **不存在** | D-1 真的編進去了（唯一模式） |
| `usage_by_app` / `tools_detect` | ✅ 存在 | 第二階段與工具偵測命令仍在 |

**安裝驗證**：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-28 02:43，內含 `index-BtK0gyS1.js`（前一版） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 03:27:52、9,046,528 bytes，內含 `index-BAemutFc.js`、且沒有 `via_gateway` ✅ |
| 實機 GUI | 見 §0.2（工具清單 5 個、切換對話框方案可生成、全程未寫入任何設定） |
| 你的備份與資料庫 | 驗證前後完全相同（claude 10／codex 11＋baseline 1／hermes 6／opencode 10；`app.db` mtime 未變） |

### 9.1 前一次建置（2026-09-28 02:43，已被 9.3 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-28 02:43** |
| 執行檔大小 | 9,051,648 bytes（8,839 KB） |
| MSI 大小 | 7,364,608 bytes（7,192 KB） |
| NSIS 大小 | 3,840,920 bytes（3,751 KB） |
| 後端測試 | **219 passed / 0 failed / 9 ignored**（本輪實跑 `cargo test --offline`，exit 0） |
| 前端 bundle | 550.79 kB（gzip 151.30 kB） |
| 前端資源指紋 | `index-BtK0gyS1.js`、`index-DnRx7SDk.css` |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `usage.rs` 為 `01:15`，產物為 `02:43`）。

**二進位內容抽查**：

| 字串 | 結果 | 意義 |
|---|---|---|
| `index-BtK0gyS1.js` / `index-DnRx7SDk.css` | ✅ 存在 | 內嵌的就是本次建置的前端（含「工具」鏡頭） |
| `usage_by_app` | ✅ 存在 | 新增的後端命令 |
| `proxy_trace` / `provider_stripped_fields` | ✅ 存在 | Phase 1 觀測設施 |
| `busy_timeout` / `state*.sqlite` | ✅ 存在 | B3 修正 |

**安裝驗證**（不是只有「建置成功」）：

| 檢查 | 結果 |
|---|---|
| 安裝前 `%LOCALAPPDATA%\token-gateway\token-gateway.exe` | 2026-09-27 23:44，內含舊資源 `index-hbcVOtaz.js`（**沒有** `usage_by_app`） |
| 執行 NSIS 安裝檔（`/S`） | exit code 0 |
| 安裝後同一路徑 | 2026-09-28 02:43，內含 `index-BtK0gyS1.js` 與 `usage_by_app` ✅ |

### 9.2 更早的建置（2026-09-27 05:11，Phase 1～5 第一次建置，已被 9.1／9.3 取代）

| 項目 | 值 |
|---|---|
| 建置時間 | **2026-09-27 05:11** |
| 執行檔大小 | 8,829 KB |
| MSI 大小 | 6,272 KB |
| NSIS 大小 | 3,752 KB |
| 後端測試 | 204 passed / 0 failed / 9 ignored |
| 前端 bundle | 542.38 kB（gzip 149.86 kB） |

**已驗證三種產物都比所有原始碼新**（最新原始碼 `keys.rs` 為 `05:00`，
產物為 `05:11`）。

**二進位內容抽查**（確認新程式碼真的在裡面，不只是建置成功）：

| 字串 | 結果 | 意義 |
|---|---|---|
| `proxy_trace` | ✅ 存在 | Phase 1 觀測設施 |
| `provider_stripped_fields` | ✅ 存在 | 學習型剝離 |
| `busy_timeout` | ✅ 存在 | B3 修正（SQLite 鎖等待） |
| `state*.sqlite` | ✅ 存在 | B3 修正（不再寫死 `state_5`） |
| `conflict` / `internal` | ✅ 存在 | Phase 5 `ErrKind` 代碼 |

> `not_found` 不在二進位中，這是**正確的**：`keys.rs` 只用到
> `auth`（3 處）、`conflict`（3 處）、`internal`（2 處）三種，編譯器把未使用的
> 變體字串省略了。我特地回頭確認過，不是漏掉。
