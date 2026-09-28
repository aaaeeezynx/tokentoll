# 手動測試指南

> 這份文件只講**要你自己動手做的測試**。已經自動化或我已代跑的部分，
> 見 [`TESTING.md`](./TESTING.md)（§0.7 是最近的完整結果）。
>
> 你目前安裝的版本：**2026-09-28 20:15 建置**
> （`token-gateway.exe` 9,010,176 bytes，
> SHA-256 `E098655E9488C4F79757B5266A220168DC80DD3D8276BA67658BFD793E1B4498`）。

---

## 0. 開始前：確認你跑的是對的版本

```powershell
$inst = "$env:LOCALAPPDATA\token-gateway\token-gateway.exe"
(Get-Item $inst).Length                       # 應該是 9010176
(Get-FileHash $inst -Algorithm SHA256).Hash   # 應該以 E098655E 開頭
```

兩個都對，才繼續。不對就代表安裝沒成功，先告訴我。

**開關 App 的正確方式**：直接關視窗即可（它會一併停掉內部的 `gw` sidecar）。
重開就是再執行上面那個 `$inst`。

---

## 1. 【最重要】測試 C：原本的 400 與 Codex 會話

**這是唯一還沒有人跑過的測試**，也是你最初回報的問題。

### 為什麼只有你能跑
它需要在**真實使用情境**下發生 —— 用你平常的方式操作 Codex（真的聊天、
真的用舊會話），而不是我憑空造一個請求。

### 步驟

1. **記下現在的狀態**（這樣出事才能比對）：
   ```powershell
   $cfg = "$env:USERPROFILE\.codex\config.toml"
   (Get-FileHash $cfg -Algorithm SHA256).Hash
   (Get-Item $cfg).LastWriteTime
   ```
2. 確認 App 開著、Codex 那張卡顯示「**網關接管中**」。
3. **完全關閉 Codex**（不是關視窗，是確定行程結束：
   `Get-Process codex -ErrorAction SilentlyContinue` 應該沒有輸出）。
4. 重新啟動 Codex，**開一個你以前就在用的舊會話**，送一則訊息。
5. 再用一次**新開的會話**送一則訊息。

### 通過標準

| # | 應該看到 |
|---|---|
| 1 | Codex 啟動時**沒有**「Unable to log in」或任何設定錯誤 |
| 2 | 舊會話能正常續用（沒有「provider 不存在」之類的錯誤） |
| 3 | 兩則訊息都有回應，**沒有** 400 連線錯誤 |
| 4 | App 的**診斷頁**看得到這兩筆請求，且**沒有** `轉換 = 網關拒絕` |
| 5 | 用量頁的請求數／token 有增加 |

### 如果失敗，請把這些給我

- Codex 視窗裡的**原始錯誤文字**（整段複製，不要只說「壞了」）
- App 診斷頁那幾筆的**明細全文**
- 上一步記下的 `config.toml` hash（用來確認檔案有沒有被動過）

---

## 2. 測試：點來源卡片**不該**再改動 `config.toml`

這是 2026-09-28 修掉的缺陷（我當天就是踩到它才改了你的設定兩次）。

### 步驟

1. 先記錄檔案的「指紋」：
   ```powershell
   $cfg = "$env:USERPROFILE\.codex\config.toml"
   (Get-FileHash $cfg -Algorithm SHA256).Hash
   ```
2. 到「**上游來源**」頁 → 點「**來源（7）**」分頁。
3. 依序**點每一張來源卡片**（NIM、or、aihubmix、xxy-DS、xxy-GLM、xxy-GPT 272k…），
   每張都點一下就好，**不要**按任何「編輯」或「刪除」。
4. 回到 PowerShell，**再算一次** hash。

### 通過標準

- 兩次 hash **完全相同**（檔案沒被改、mtime 也沒變）。
- 點卡片之後，Codex 那一行會出現一行灰字提示：
  「已選擇來源「X」，但 Codex 仍接管自原來源（未改動 config.toml）。
  要換過去請按右側開關，或點此行左側進詳情按「套用」」。

> **注意**：這個提示只在「你選了跟上次接管不同的來源」時才出現。
> 如果你只是點回同一個來源，不會有提示，這是正常的。

### 舊版行為（對照用，不要期待）
以前點卡片會**立刻重寫** `config.toml` 並輪換備份，畫面只閃一行訊息。

---

## 3. 測試：接管預覽要顯示 `wire_api = responses`

（這一項我已經在 20:15 版本上取得螢幕證據，但你可以自己複驗。）

### 步驟

1. 「上游來源」頁 →「**本機工具**」分頁。
2. 點 **Codex** 那張卡（點卡片左側文字區域）→ 跳出「切換 Codex」對話框。
3. 找到下方顯示路徑 `C:\Users\luluna\.codex\config.toml` 的區塊，
   旁邊有一個「**N 項寫入**」的可展開列 → **點它展開**。
4. 找這一行：

   ```
   [model_providers.{custom, tokengateway, …}] 共 8 段
   base_url = http://127.0.0.1:15722/v1 / experimental_bearer_token = 上游 Key 明文
   / wire_api = responses
   ```

### 通過標準

- `wire_api =` 後面是 **`responses`**。
- **絕對不可以**是 `chat` —— 那就是 2026-09-28 讓 Codex 完全打不開的那個值
  （見下方第 4 項的說明）。

---

## 4. 測試：真的一次接管，然後確認 Codex 還活著

**這會真的重寫 `~/.codex/config.toml`**（App 會自動備份）。想先保險就複製一份：

```powershell
Copy-Item "$env:USERPROFILE\.codex\config.toml" "$env:TEMP\config.backup-before-test.toml"
```

### 步驟

1. 承上，對話框還開著。確認「來源」是你想要的那個（例如 `opencode-go`）、
   模型是你想要的。
2. 按「**套用（切換來源/模型）**」。
3. 等它顯示成功訊息。
4. 確認寫進去的值：
   ```powershell
   Select-String -Path "$env:USERPROFILE\.codex\config.toml" -Pattern 'wire_api'
   ```
   應該看到 8 行，**全部**是 `wire_api = "responses"`。
5. 確認 Codex 還能用：
   ```powershell
   codex doctor
   ```

### 通過標準

| # | 應該看到 |
|---|---|
| 1 | 8 行 `wire_api` 全部是 `responses` |
| 2 | `codex doctor` 顯示 `✓ config loaded`、`config.toml parse ok` |
| 3 | `codex doctor` 顯示 `config.toml` 的 model 是你選的那個 |
| 4 | `codex doctor` 顯示 `model provider requires OpenAI auth false` |

### 如果第 2 步之後 Codex 掛了
```powershell
Copy-Item "$env:TEMP\config.backup-before-test.toml" "$env:USERPROFILE\.codex\config.toml" -Force
```
然後把 App 的畫面與 `codex doctor` 的**全文**給我。

---

## 5. 測試：接管前體檢不該再出現假警報

### 步驟

1. 「本機工具」分頁 → Codex 卡片上的「**接管前體檢**」按鈕。
2. 看第一行關於 `TOKEN_GATEWAY_KEY` 的結論。

### 通過標準

- 應該是 **ℹ️ 目前的設定不需要 TOKEN_GATEWAY_KEY（各段用
  experimental_bearer_token 直接寫入上游 Key，不讀環境變數）**。
- **不該**再是 ❌ ——以前它無條件報 ❌，即使你的設定根本沒用到那個環境變數。
- 其餘幾項（15721 是否被佔用、歷史別名 provider、config 重複段）維持原樣。

> **什麼情況下才該出現 ❌**：設定檔裡有 provider 段寫 `env_key = "TOKEN_GATEWAY_KEY"`
> （也就是你改用「本地 Key」模式）而該環境變數沒設。那時 ❌ 是真的。

---

## 6. 測試：網關拒絕要留痕（測試 B 的人工複驗）

我已經代跑過（`TESTING.md` §5.3），但你可以自己再跑一次確認。

### 步驟

1. 確認 App 開著、網關在跑。
2. 送一個**故意錯的**請求：
   ```powershell
   Invoke-WebRequest -Uri "http://127.0.0.1:15722/v1/chat/completions" `
     -Method POST -Headers @{ "Authorization" = "Bearer not-a-real-key"; "Content-Type" = "application/json" } `
     -Body '{"model":"test","messages":[{"role":"user","content":"hi"}]}' `
     -ErrorAction SilentlyContinue | Select-Object StatusCode
   ```
3. 到 App 的「**診斷**」頁，重新整理。

### 通過標準

- 有一筆新的紀錄，級別 **warn**、`轉換 = 網關拒絕（未觸及上游）`、
  上游狀態碼 **401**。
- 「追蹤總數」與「異常」都 +1。

> **這正是 Phase 1.5 修的缺口**：修正前，「連線錯誤 400」是唯一不會出現在
> 診斷頁的那一類。

---

## 7. 測試：用量頁的「本機工具」篩選要**完整**

這是不可違反的約束：**用量資訊必須完整**，任何歷史用量都不得退化成 `unknown`。

### 步驟

1. 到「**用量**」頁。
2. 點上方的「**全部本機工具**」篩選。
3. 展開選項清單。

### 通過標準

清單裡**必須同時包含**這些選項（一個都不能少）：

- Claude Code
- Codex
- OpenCode
- **DeepSeek Harness**
- **Cursor**
- **Antigravity**
- **Hermes** ← 這一項特別重要：hermes 的**接管支援**已移除，但它的
  **用量標籤必須留著**，否則歷史資料會歸屬不明。

---

## 8. 測試：封存舊對話（2026-09-28 新增）

你回報「無法封存舊的對話，封存兩個 test 都失敗」。根因已查明 ——
**不是網關的問題**，是 Codex 自己留下的鎖檔（完整證據在 `TESTING.md` §0.8）。

### 快速確認是不是同一個原因

```powershell
Get-ChildItem "$env:USERPROFILE\.codex\thread-writer-locks" -Force |
  ForEach-Object { "  " + $_.Name + "  " + $_.LastWriteTime }
```

只要清單裡有 `<一串 UUID>.lock`，而你想封存的對話正好是那個 UUID，
那就是同一個原因。

### 修法

```powershell
# 1) 先完全關閉 Codex（Desktop 和任何 codex 行程都要關）
Get-Process codex -ErrorAction SilentlyContinue

# 2) 只報告，不動任何東西
py scripts\codex_clear_stale_locks.py

# 3) 確認列出來的都是你想處理的，再真的刪（會先備份）
py scripts\codex_clear_stale_locks.py --apply

# 4) 封存
codex archive <session-id>
```

### 通過標準

| # | 應該看到 |
|---|---|
| 1 | 第 2 步把那些鎖列為 **stale**（若顯示「IN USE」，代表 Codex 還開著，回第 1 步） |
| 2 | `codex archive <id>` 印出 `Archived session <id>.`、exit code 0 |
| 3 | rollout 檔從 `.codex\sessions\` 搬到 `.codex\archived_sessions\` |
| 4 | 在 Codex Desktop 的對話清單裡，那個對話移到「已封存」 |

### 注意

- **不要在對話開著的時候封存它。** 那會失敗，而且會在
  `thread-writer-locks\` 留下一把鎖，讓之後每次封存都失敗。
- 工具**只**刪 0 byte 的 `<uuid>.lock`，而且**有行程開著 handle 的一律不碰**
  （`.coordination.lock` 和 rollout 檔也一律不碰）。

---

## 9. 測試：關掉網關要回到**原生來源**（2026-09-28 新增）

以前的「關」只做一件事：把**接管前的那份備份**逐字寫回去。問題是那份備份可能
是兩天前的快照 —— 你的實例是基線停在 09-26 的 NIM 設定，所以關掉網關之後
Codex 停在 NIM 的來源與模型，而不是 GPT。

現在「關」= 還原備份 **＋ 切回這個工具的原生來源**：

| 工具 | 關閉後會變成 |
|---|---|
| **Codex** | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、移除 `model_catalog_json` → 模型清單回到 GPT |
| **Claude Code** | 移除指向網關的 `ANTHROPIC_BASE_URL` 與 `ANTHROPIC_AUTH_TOKEN`，並移除不是 `claude-*` 的模型對映 |
| **OpenCode** | 移除指向網關的 `provider.*.options.baseURL` 與本地 `sk-local-` key |

**刻意保留**：你自己的設定，以及 `.codex` 裡那些別名 provider 段（`custom`、
`nvidia-nim`、`opencode-zen`…）。刪掉它們會讓舊對話開不起來（rollout 裡記著
provider 名）。所以關閉後 `config.toml` 裡**仍然看得到**那些段和它們的 URL ——
那是正常的，它們已經不是現在的路由。

### 步驟

1. 首頁 →「本機工具」→ 點 Codex 那一行左側進詳情。
2. 右上開關**打開**（寫入網關接管），等它顯示接管成功。
3. 再**關掉**同一個開關。確認框應該寫「…並把來源切回這個工具的原生來源…」。
4. 卡片下方訊息應該出現 `已切回原生來源：model_provider → openai、…`。

> **如果你現在的 Codex 已經停在 NIM（開關本來就是關的）**：先把開關打開、
> 再關掉，才會走過這條新路徑。或者用第 10 節的手動還原。

### 通過標準

```powershell
$c = "$env:USERPROFILE\.codex\config.toml"
Select-String -Path $c -Pattern '^(model_provider|model) ='   # openai / gpt-5.6-luna
Select-String -Path $c -Pattern 'model_catalog_json'          # 沒有輸出
Select-String -Path $c -Pattern '^\[model_providers\.'        # 別名段還在（正常）
codex doctor                                                  # config 全綠、沒有 ❌
```

Codex 打開後，模型選擇器裡應該是 **GPT-5.x / GPT-6**，不是 NIM 的模型。

### 已知取捨

- Codex 回到官方之後，**舊對話若原本走第三方模型**（例如 `moonshotai/kimi-k3`）
  仍然開得起來，但送出的請求要看那個別名段指向哪裡 —— 它在關閉後指向你最後
  一次接管前的上游，不是 OpenAI。
- Claude Code 還原後如果沒有官方登入（這台機器上找不到
  `.claude/.credentials.json`），它會要你先登入 —— 那就是「原生」的樣子。
- 關閉**不會**動你原本就有的 `model_context_window`、`disable_response_storage`
  之類的鍵（那些不是 App 寫的）。

---

## 10. 若出現問題：怎麼回復

| 想回復什麼 | 怎麼做 |
|---|---|
| **Codex 設定** | App 的「本機工具」頁 → 把 Codex 的開關**關掉**（還原接管前備份**並**切回原生來源） |
| 只想逐字還原備份（不要切回原生） | 備份在 `%APPDATA%\com.tokencounter.gateway\backups\codex\`，挑 `config.toml.bak-*` 複製回 `~/.codex/config.toml` |
| 手動還原單一檔案 | 備份在 `%APPDATA%\com.tokencounter.gateway\backups\codex\`，挑 `config.toml.bak-*` 複製回 `~/.codex/config.toml` |
| 資料庫 | 每次動資料庫前我都會先備份（例如 `.workbuddy/tmp/app.db.before-*`） |
| 誤封存的對話 | `codex unarchive <session-id>` |
| 被刪掉的鎖檔 | `%USERPROFILE%\.codex\thread-writer-locks-backup\`（工具會自動備份） |

**回報時請附上**：

1. 你的操作步驟（我到時候會照著重現）
2. 畫面上看到的**完整文字**（不要只說「壞了」）
3. `codex doctor` 的全文（如果跟 Codex 有關）
4. `(Get-FileHash "$env:USERPROFILE\.codex\config.toml" -Algorithm SHA256).Hash`

---

## 11. 現況一覽：還有什麼沒做

| 項目 | 狀態 |
|---|---|
| 測試 A（診斷中心） | ✅ 已通過 |
| 測試 B（網關拒絕留痕） | ✅ 我代跑 + 你可用第 6 節複驗 |
| **測試 C（原始 400 症狀）** | ⚠️ **只有你能跑 → 第 1 節** |
| 測試 D（Codex 舊會話別名） | ✅ 我代跑 |
| 測試 E（`wire_api`） | ✅ 我代跑 + 實機修復驗證 |
| 測試 F（DSH 用量不出現） | ✅ 已查明：DSH 直連 `opencode.ai`，**沒經過網關**。要統計請把 DSH 的 `oc-go` baseURL 指向網關（`TESTING.md` §6.7） |
| 測試 G（本機工具標籤） | ✅ 已修 |
| **封存舊對話** | ✅ 根因已查明（Codex 鎖檔），工具已附 → 第 8 節 |
| **關閉網關回到原生來源** | ✅ 已修（`switch_off` = 還原＋原生化）→ 第 9 節 |
| Phase 5 錯誤型別轉換 | ⛔ **刻意維持 6/108**：每次轉換都應該有人能實測，屬獨立的漸進工作 |

