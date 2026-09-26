# Token Gateway 計畫

## 一、Key 簽發頁 macOS 27 風格重設計

### 現狀問題
- 表單為傳統堆疊式，缺乏視覺層次
- 按鈕、輸入框、checkbox 樣式與 macOS 27 HIG 差距大
- 模型白名單區（checkbox 手動輸入）不夠直覺

### 設計方案

**整體佈局**
- 內容區最大寬度 `max-w-xl`，垂直居中，留白充足
- 每個輸入欄位獨立「玻璃卡片」区块（`glass` 類），卡片間距 16px
- 表單標題用 `text-xs font-semibold text-white/50 uppercase tracking-wider` 做欄位標籤

**輸入框**
- 全部用 `PopSelect`（上游來源）或自定義 glass-input（名稱、配額、限流）
- glass-input 樣式：`bg-white/[0.04] border border-white/[0.08] rounded-xl px-4 py-2.5 text-sm text-white/85 placeholder:text-white/25 focus:border-[var(--mac-accent)]/50 focus:ring-1 focus:ring-[var(--mac-accent)]/20 transition-all`
- focus 狀態：accent 邊框 + 微光暈（`box-shadow: 0 0 0 3px var(--mac-accent, #0a84ff)/10`）

**欄位區塊（3張卡片）**
1. **基本資訊**（Name + 上游來源）
   - 名稱輸入框（placeholder: "如 codex-主力"）
   - 上游來源 PopSelect（選擇已建立的 provider）
2. **配額與限流**（橫排兩欄）
   - 左：配額 tokens（-1不限），glass-input
   - 右：限流 次/分鐘（0不限），glass-input
3. **模型白名單**（可展開區塊）
   - 標題行 + 展開/收合 chevron（用 Segmented 切換模式）
   - 模式 A：「全部允許」（預設關閉白名單）
   - 模式 B：指定模型
     - 搜尋框（`PopSelect` searchable）+ 手動輸入 ID + 新增按鈕
     - 已選模型 Chip 列表（glass-chip，可刪除）
     - 從上游來源拉取的模型清單（checkbox 列表，搜尋過濾）
   - Chip 樣式：`bg-[var(--mac-accent)]/15 border border-[var(--mac-accent)]/30 rounded-full px-3 py-1 text-xs text-[var(--mac-accent)] hover:bg-[var(--mac-accent)]/25 transition-colors cursor-pointer`

**操作按鈕**
- 底部固定 `flex gap-3 justify-end`
- 取消：`btn-ghost`
- 儲存：`btn-primary` + `bg-[var(--mac-accent-deep)]`（WCAG AA 對比）
- 儲存中狀態：`animate-pulse` + 文字「儲存中…」

**動畫**
- 卡片進場：`anim-rise`（fade+translate）
- 開關白名單時：`acc-body` grid-rows 動畫展開

---

## 二、encrypted_content 400 錯誤修復

### 問題
Claude Code 發送帶 `thinking`/`encrypted_content` 的請求，經網關轉發到不支援此欄位的上游（如 nvidia、OpenAI 兼容），導致：
```
Error from provider (Console): Upstream request failed: [invalid_request_error] reasoning `encrypted_content` was not issued to this caller
```

### 修復方案

**1. translate.rs — `anthropic_to_openai` 函數**
在 `anthropic_content_to_openai` 中已過濾 `thinking`/`redacted_thinking` 塊，但需確認：
- `user` 消息中的 `thinking` 塊（line 130-138 過濾器）✓ 已處理
- `assistant` 消息中的 `thinking` 塊（line 172-192 過濾器）✓ 已處理
- **新增**：頂層 `thinking` 設定欄位（`req.get("thinking")`）需移除，避免上游 400

**2. proxy.rs — 新增 `strip_encrypted_content` 函數**
在轉發前（翻譯路徑和直通路徑），從請求體中移除所有 `encrypted_content` 欄位：
- 遞迴遍歷 JSON，移除所有含 `encrypted_content` 鍵的對象
- 在 `proxy_handler` 的 body 處理階段調用

**3. 具體改動**
- `translate.rs:anthropic_to_openai`：在構建 `out` 時，跳過 `req.get("thinking")`
- `proxy.rs`：新增 `fn strip_encrypted_content(body: &[u8]) -> Option<Vec<u8>>`，在 `strip_unsupported_tools` 之後調用
- `proxy.rs:proxy_handler`：在轉發前對 Anthropic 格式請求調用 `strip_encrypted_content`

---

## 驗證方式
1. `cargo test` — 所有現有測試通過 + 新增 `encrypted_content` 剝離測試
2. `cargo clippy` — 無警告
3. `pnpm build` — tsc+vite 通過
4. 手動測試：Claude Code → Gateway → nvidia，確認不再出現 400
