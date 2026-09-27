# 測試指南：Phase 1 ～ Phase 5

> 這份文件是給「實際動手測」用的。每一項都有**確切的指令**與**明確的通過
> 標準**，讓你可以自己判斷有沒有過，而不是只能憑感覺。
>
> 對應的技術細節在 [`REFACTORING-PLAN.md`](./REFACTORING-PLAN.md)。

---

## 0. 這次要測的七件事

| # | 測什麼 | 為什麼重要 |
|---|---|---|
| **A** | 診斷中心能不能用 | Phase 1 做的可觀測設施**從未真正跑起來過**（見下方說明） |
| **B** | 網關拒絕會不會留痕 | Phase 1.5 修的**主要缺口**。修正前，「連線錯誤 400」是唯一不會出現在診斷頁的那一類 |
| **C** | 原本的 400 / Codex 會話症狀有沒有改善 | 你的原始問題 |
| **D** | Codex 舊會話的 provider 別名（B3，已修） | 修正前讀取失敗會**靜默變成「沒有別名」**，看起來一切正常 |
| **E** | `wire_api` 是否跟著上游能力走（B4，已修） | 修正前不論上游是誰都寫死 `responses`，直連第三方會壞 |
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

> **目前狀態**：測試 **A 已通過**（2026-09-27 22:07 你提供的截圖，已與資料庫
> 逐項核對）。B～F 待你實測。後端 **210 passed / 0 failed / 9 ignored**。

> **2026-09-28 更新**：用量頁第 8 個鏡頭「工具」已完成，程式已重新建置、
> **安裝到 `%LOCALAPPDATA%\token-gateway`（2026-09-28 02:43）並實機驗證**。
> 後端測試 **219 passed / 0 failed / 9 ignored**。詳細證據見 §0.1 與 §9.1。

> **2026-09-28 03:28 更新（第一階段：工具區精簡 D-1／A／E）**：三個方向的程式
> 改動已完成並重新建置、安裝、實機驗證。後端測試 **223 passed / 0 failed /
> 9 ignored**。詳細證據見 §0.2 與 §9.3。**F（移除 hermes）尚未動**。

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

**通過標準**（這是這次修正的驗收）：

- [ ] 「請求追蹤」出現對應筆數，且每一筆的展開明細中，「轉換」欄顯示
      **「網關拒絕（未觸及上游）」**（修正前這裡會顯示成 ` → （rejected）`，
      因為這一類沒有格式轉換也沒有上游）
- [ ] 這幾筆的狀態碼分別是 401／401／400
- [ ] 它們也出現在**問題追蹤**清單裡（因為記為 `warn` 等級）
- [ ] 「追蹤總數」相應增加
- [ ] 明細中的 **note** 讀得出被拒原因（例如「模型不在白名單」），
      且**不含任何金鑰字串**

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
     被誤判」的關鍵證據，也是 §5.2 那個懸而未決問題的定案依據
3. 若再遇到 **Codex 無法續用舊會話／無法封存對話**：
   - 這是 **B3**，**本次刻意未修**（你當初指示「這次都不修，只寫進重構計畫書」）
   - 但請把 Codex 的**原始錯誤文字**給我 —— 我目前只能從程式碼推論，
     缺這份文字無法定位到具體分支

---

## 6.5 測試 D：Codex 舊會話別名（B3，已修）

這一項針對「**無法繼續使用舊的 conversation session**」。修好的四條成因見
計畫書 Phase 2.5，但**你要測的只有一件事**：體檢會不會誠實回報。

1. **完全關閉 Codex**（重要 —— 成因之一就是 Codex 正在寫 DB 時的競爭）。
2. 在 App 的 Codex 接管頁跑**接管前體檢**。

**通過標準**：

- [ ] 若一切正常，會看到
      **「ℹ️ 歷史會話用過 N 個別名 provider（讀自 state_5.sqlite）：…」**
      ——注意它會**標明讀自哪個檔案**
- [ ] 若讀不到，會看到
      **「❌ 讀不到 Codex 歷史會話的 provider 名：…」** 並附原因與處置建議
- [ ] **最關鍵**：絕對不會在讀不到的情況下看到
      「✅ 歷史會話無第三方 provider 殘留」

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

**通過標準**：

- [ ] 預覽顯示 `wire_api = chat`（**不是** `responses`）
- [ ] 接管後打開 `~/.codex/config.toml`，每個 `[model_providers.*]` 段都是
      `wire_api = "chat"`
- [ ] 換成經由網關接管時，同一欄位是 `wire_api = "responses"`
- [ ] 若該渠道的 `api_format` 被宣告為 `openai-responses`，即使 base_url 是
      第三方，也應該寫 `responses`（**宣告優先於 URL 猜測**）

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
- ~~**B4**：無差別強制 `wire_api = "responses"`。~~ **已修** —— 見測試 E。
- ~~**§5.3 第 2 層**：能力宣告。~~ **已實作**（協議選擇改為宣告優先）。
- **§5.2 的 body 解析 400**：仍未定案。三份證據用了兩種不同的
  `bytes_len` 算法，無法區分「真的解析失敗」與「舊的除錯儀器弄壞了 body」。
  觀測設施已就位，**等真實重現一次**。
- **`resolve_filter` 的「365 天」訊息與「366 天」判斷式不一致**：新測試
  `resolve_filter_span_limit_actual_behaviour` 已把**實際行為**（365 過、367 擋）
  釘住。要統一說法或統一門檻屬於產品決策，未動。
- **`db.rs` migration 仍無自動化測試**（§2.1 F6）。目前唯一驗證是需人工觸發的
  `live_migrate_real_db_copy`。
- **前端三個肥檔未動**：`Keys.tsx` 884、`Calc.tsx` 818、`lib/api.ts` 758
  （不在本輪授權範圍）。

**已做但未達標**：

- `proxy_handler` **86 行**（目標 < 80）。差的 6 行是三個階段脈絡重複列出
  同 5 個欄位，要再壓得引入共用 `ReqCtx`、牽動約 56 處存取換 7 行 ——
  判斷為行數高爾夫，不做。
- **拆檔後仍 > 400 行的檔案**：`Providers.tsx` 781、`ModelCatalog.tsx` 613、
  `tools/tests/codex.rs` 603、`ProviderForm.tsx` 600、`usage/usageCharts.tsx` 551。
  再拆需要先決定分組維度（例如 `Providers.tsx` 要先把拖拽排序抽成 hook），
  屬於獨立一步。
- **Phase 5 錯誤型別只轉了 6/108 處**。`keys.rs` 是刻意選的試點；其餘維持
  `Result<_, String>`。轉換路線已證明可行且不傷文案，但**每次轉換都應有人能
  實測**，因此沒有一次改完。

**已驗證但你可能想自己再看一次**：

- `cargo test --offline` → **204 passed / 0 failed / 9 ignored**
- `cargo clippy --offline --all-targets` → exit 0，僅 5 條**既有**風格提示
  （`tools/codex.rs` ×2、`price_extract.rs` ×1、`tools/tests/restore.rs` ×2）
- `npx tsc --noEmit` → 0 錯誤
- `npm run build` → 成功，bundle 542.38 kB
- `npx tauri build` → exe + MSI + NSIS 三種產物皆成功
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

### 9.3 最新建置（2026-09-28 03:28，第一階段 D-1／A／E）—— 你目前安裝的就是這一個

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
