/**
 * 翻譯字典：`key → [繁中, English]`。
 *
 * 繁中是 source language（`t()` 找不到 key 時也回繁中原文），
 * 所以「繁中欄」寫的就是畫面上原本的字串 —— 不會出現兩邊對不起來的情況。
 *
 * 命名：`區域.項目`。新增翻譯時照這個規則排，
 * 覆蓋率可用 `.workbuddy/tmp/i18n_cover.py` 量（docs/TESTING.md §0.9.32）。
 */
export const DICT: Record<string, [string, string]> = {
  // ── 導覽 ──
  "nav.usage": ["用量", "Usage"],
  "nav.providers": ["上游來源", "Providers"],
  "nav.keys": ["本地 Key", "Local Keys"],
  "nav.diagnostics": ["診斷", "Diagnostics"],
  "nav.calc": ["試算", "Calculator"],
  "nav.settings": ["設定", "Settings"],

  // ── 頂列 ──
  "top.gateway": ["網關", "Gateway"],
  "top.stop": ["停止", "Stop"],
  "top.start": ["啟動", "Start"],
  "top.version": ["v2 · Tauri", "v2 · Tauri"],

  // ── 設定：網關 ──
  "settings.gateway.title": ["網關", "Gateway"],
  "settings.gateway.caption": [
    "本機代理監聽（僅 127.0.0.1）",
    "Local proxy (127.0.0.1 only)",
  ],
  "settings.gateway.port": ["監聽連接埠", "Listen port"],
  "settings.gateway.check": ["檢測", "Check"],
  "settings.gateway.autostart": ["自動啟動", "Start with app"],
  "settings.gateway.autostart.hint": [
    "應用程式啟動時自動啟動網關",
    "Start the gateway when the app launches",
  ],
  "settings.gateway.save": ["儲存", "Save"],
  "settings.gateway.hint": [
    "15721 常被 cc-switch 佔用，預設 15722。網關執行時不可改連接埠（需先停止）；改完後已切換工具需重新執行一次切換。",
    "Port 15721 is often taken by cc-switch; we default to 15722. The port can only change while the gateway is stopped; re-run a tool switch afterwards.",
  ],
  "settings.gateway.ok": ["連接埠 {port} 可用", "Port {port} is available"],
  "settings.gateway.saved": [
    "已儲存：連接埠 {port}，開機自啟{auto}",
    "Saved: port {port}, autostart {auto}",
  ],
  "settings.gateway.on": ["開", "on"],
  "settings.gateway.off": ["關", "off"],

  // ── 設定：外觀 ──
  "settings.appearance.title": ["外觀", "Appearance"],
  "settings.appearance.caption": [
    "強調色與主題即時套用全站",
    "Accent and theme apply instantly",
  ],
  "settings.appearance.accent": ["強調色", "Accent"],
  "settings.appearance.theme": ["主題", "Theme"],
  "settings.appearance.theme.system": ["跟隨系統", "Follow system"],
  "settings.appearance.theme.light": ["淺色", "Light"],
  "settings.appearance.theme.dark": ["深色", "Dark"],
  "settings.appearance.theme.now": [
    "目前：{actual}",
    "Current: {actual}",
  ],
  "settings.appearance.autostart": ["開機自啟", "Launch at login"],
  "settings.appearance.autostart.hint": [
    "登入 Windows 時自動啟動（寫 HKCU 的 Run 機碼）",
    "Start on Windows sign-in (HKCU Run key)",
  ],
  "settings.appearance.autostart.on": ["已開啟開機自啟", "Launch at login enabled"],
  "settings.appearance.autostart.off": ["已關閉開機自啟", "Launch at login disabled"],
  "settings.appearance.close": ["關閉視窗時", "On window close"],
  "settings.appearance.close.tray": ["縮到系統匣（建議）", "Minimize to tray (recommended)"],
  "settings.appearance.close.exit": ["直接結束", "Exit completely"],
  "settings.appearance.close.hint": [
    "這是本地網關 —— 縮到系統匣才不會斷掉工具的流量",
    "This is a local gateway — staying in the tray keeps your tools connected",
  ],
  "settings.appearance.close.tray.msg": [
    "關閉視窗時縮到系統匣，網關繼續運作",
    "Closing the window hides it; the gateway keeps running",
  ],
  "settings.appearance.close.exit.msg": [
    "關閉視窗時會直接結束（網關也會停）",
    "Closing the window exits the app (the gateway stops too)",
  ],
  "settings.appearance.lightweight": ["輕量模式", "Lightweight mode"],
  "settings.appearance.lightweight.btn": ["進入輕量模式", "Enter lightweight mode"],
  "settings.appearance.lightweight.hint": [
    "釋放視窗記憶體，托盤與網關照常運作",
    "Frees the window's memory; tray and gateway keep running",
  ],
  "settings.appearance.lightweight.msg": [
    "已進入輕量模式（可從托盤「開啟主視窗」或點 Deep Link 回來）",
    "Lightweight mode on (reopen from the tray or a deep link)",
  ],

  // ── 設定：用量匯出 ──
  "settings.export.title": ["用量匯出", "Usage export"],
  "settings.export.caption": [
    "請求明細 CSV（含 BOM，Excel 可直接開）",
    "Request detail CSV (BOM included, opens in Excel)",
  ],
  "settings.export.range": ["區間", "Range"],
  "settings.export.csv": ["匯出 CSV", "Export CSV"],
  "settings.export.working": ["匯出中…", "Exporting…"],
  "settings.export.done": ["已匯出 {n} 筆", "Exported {n} rows"],
  "settings.export.failed": ["匯出失敗：{e}", "Export failed: {e}"],

  // ── 設定：資料庫備份 ──
  "settings.backup.title": ["資料庫備份", "Database backups"],
  "settings.backup.caption": [
    "自動備份排程與還原（還原前會先自動備份當前資料庫）",
    "Auto-backup schedule and restore (a safety backup is taken first)",
  ],
  "settings.backup.auto": ["自動備份", "Auto backup"],
  "settings.backup.keep": ["保留", "Keep"],
  "settings.backup.keep.n": ["{n} 份", "{n}"],
  "settings.backup.interval.off": ["停用", "Off"],
  "settings.backup.interval.hours": ["每 {n} 小時", "Every {n}h"],
  "settings.backup.interval.days": ["每 {n} 天", "Every {n}d"],
  "settings.backup.now": ["立即備份", "Back up now"],
  "settings.backup.working": ["備份中…", "Backing up…"],
  "settings.backup.created": [
    "已建立備份 {name}（{size}）",
    "Created backup {name} ({size})",
  ],
  "settings.backup.last": [
    "上次備份：{time}；目錄：{dir}（超過保留份數會自動刪掉最舊的）",
    "Last backup: {time}; folder: {dir} (older ones are pruned automatically)",
  ],
  "settings.backup.empty": [
    "還沒有備份 —— 按「立即備份」建立第一份。",
    "No backups yet — press “Back up now”.",
  ],
  "settings.backup.restore": ["還原", "Restore"],
  "settings.backup.rename": ["改名", "Rename"],
  "settings.backup.delete": ["刪除", "Delete"],
  "settings.backup.deleted": ["已刪除備份", "Backup deleted"],
  "settings.backup.scheduled": ["已更新備份排程", "Backup schedule updated"],
  "settings.backup.renamed": ["已改名為 {name}", "Renamed to {name}"],

  // ── 設定：雲端同步 ──
  "settings.cloud.title": ["雲端同步", "Cloud sync"],
  "settings.cloud.caption": [
    "用 WebDAV 在裝置之間同步設定（同時只能開一種）",
    "Sync settings across devices over WebDAV (one provider at a time)",
  ],
  "settings.cloud.server": ["伺服器", "Server"],
  "settings.cloud.server.ph": [
    "伺服器網址（例如 https://dav.example.com/remote.php/dav/files/me）",
    "Server URL (e.g. https://dav.example.com/remote.php/dav/files/me)",
  ],
  "settings.cloud.user": ["帳號", "Username"],
  "settings.cloud.password": ["密碼", "Password"],
  "settings.cloud.password.ph": ["密碼／應用程式密碼", "Password / app password"],
  "settings.cloud.password.keep": [
    "密碼（留空＝不變更）",
    "Password (leave blank to keep)",
  ],
  "settings.cloud.root": ["遠端目錄", "Remote folder"],
  "settings.cloud.root.ph": [
    "遠端根目錄（預設 token-gateway-sync）",
    "Remote root (default token-gateway-sync)",
  ],
  "settings.cloud.profile": ["設定檔", "Profile"],
  "settings.cloud.profile.ph": [
    "設定檔名稱（預設 default）",
    "Profile name (default: default)",
  ],
  "settings.cloud.save": ["儲存設定", "Save settings"],
  "settings.cloud.test": ["測試連線", "Test connection"],
  "settings.cloud.testing": ["測試中…", "Testing…"],
  "settings.cloud.upload": ["上傳", "Upload"],
  "settings.cloud.uploading": ["上傳中…", "Uploading…"],
  "settings.cloud.info": ["看遠端快照", "Remote snapshot"],
  "settings.cloud.download": ["下載", "Download"],
  "settings.cloud.downloading": ["下載中…", "Downloading…"],
  "settings.cloud.auto": ["自動同步", "Auto sync"],
  "settings.cloud.saved": ["已儲存雲端同步設定", "Cloud sync settings saved"],
  "settings.cloud.snapshot": [
    "遠端快照：協定 {protocol}｜schema v{schema}｜App {app}｜{time}｜{size}",
    "Remote snapshot: {protocol} | schema v{schema} | App {app} | {time} | {size}",
  ],
  "settings.cloud.last": [
    "上次同步：{time}｜{result}",
    "Last sync: {time} | {result}",
  ],
  "settings.cloud.pending": ["（本地有變更，尚未上傳）", "(local changes not uploaded yet)"],
  "settings.cloud.auto.on": ["已開啟自動同步", "Auto sync enabled"],
  "settings.cloud.auto.off": ["已關閉自動同步", "Auto sync disabled"],

  // ── 設定：Deep Link ──
  "settings.deeplink.title": ["Deep Link 匯入", "Deep link import"],
  "settings.deeplink.caption": [
    "tokengateway:// 一鍵匯入上游來源",
    "One-click import of providers",
  ],
  "settings.deeplink.registered": [
    "已註冊（HKCU\\Software\\Classes\\tokengateway）",
    "Registered (HKCU\\Software\\Classes\\tokengateway)",
  ],
  "settings.deeplink.missing": ["尚未註冊", "Not registered"],
  "settings.deeplink.reregister": ["重新註冊", "Register again"],
  "settings.deeplink.unregister": ["取消註冊", "Unregister"],
  "settings.deeplink.done": ["已註冊", "Registered"],
  "settings.deeplink.undone": [
    "已取消註冊（之後點連結不會再打開這個 App）",
    "Unregistered (links will no longer open this app)",
  ],

  // ── 設定：關於 ──
  "settings.about.title": ["關於", "About"],
  "settings.about.caption": [
    "Token Gateway v2 · Tauri 本地網關",
    "Token Gateway v2 · local gateway for Tauri apps",
  ],
  "settings.about.datadir": ["數據目錄", "Data folder"],
  "settings.about.copy": ["複製", "Copy"],

  // ── 語言 ──
  "settings.lang.title": ["語言", "Language"],
  "settings.lang.caption": [
    "介面語言（切換即時生效，不用重啟）",
    "Interface language (applies immediately, no restart)",
  ],

  "settings.backup.restore.msg": [
    "已還原（安全備份 {name}）",
    "Restored (safety backup {name})",
  ],
  "settings.backup.restore.title": [
    "還原備份「{name}」？",
    "Restore backup “{name}”?",
  ],
  "settings.backup.restore.hint": [
    "目前的資料會被這份備份覆蓋。系統會先自動建立一份安全備份，所以後悔還救得回來。",
    "This backup will overwrite the current data. A safety backup is taken first, so you can undo it.",
  ],
  "settings.backup.restore.tip": [
    "還原這份備份（會先自動備份當前資料庫）",
    "Restore (a safety backup is taken first)",
  ],
  "settings.backup.rename.ph": ["新的備份名稱", "New backup name"],
  "settings.backup.delete.title": ["刪除備份「{name}」？", "Delete backup “{name}”?"],
  "settings.backup.delete.hint": [
    "刪除後無法復原（不影響目前資料庫）。",
    "This cannot be undone (your current database is unaffected).",
  ],

  "settings.cloud.incompatible": ["（⚠ 協定版本不符）", " (⚠ protocol mismatch)"],
  "settings.cloud.upload.title": [
    "上傳本地資料庫到雲端？",
    "Upload the local database?",
  ],
  "settings.cloud.upload.hint": [
    "上傳會覆蓋遠端的舊資料。本地不受影響。",
    "Uploading overwrites the remote copy. Your local data is untouched.",
  ],
  "settings.cloud.download.title": [
    "從雲端下載並覆蓋本地資料庫？",
    "Download and overwrite the local database?",
  ],
  "settings.cloud.download.hint": [
    "下載會覆蓋本地（遠端優先）。下載前會先自動備份本地資料庫，所以後悔還救得回來。",
    "Downloading overwrites local data (remote wins). A safety backup is taken first.",
  ],

  "settings.export.range.today": ["今日", "Today"],
  "settings.export.range.7d": ["最近 7 天", "Last 7 days"],
  "settings.export.range.30d": ["最近 30 天", "Last 30 days"],

  // ── 設定：更新 ──
  "settings.update.title": ["更新", "Updates"],
  "settings.update.caption": [
    "檢查有沒有新版本（自動安裝需要簽章更新檔，我們先做到「檢查＋下載連結」）",
    "Check for a new version (auto-install needs a signed artifact; we do check + link)",
  ],
  "settings.update.url": ["更新資訊網址", "Update manifest URL"],
  "settings.update.save": ["儲存", "Save"],
  "settings.update.check": ["檢查更新", "Check for updates"],
  "settings.update.checking": ["檢查中…", "Checking…"],
  "settings.update.current": ["目前版本 {v}", "Current version {v}"],
  "settings.update.found": ["有新版：{v}", "New version available: {v}"],
  "settings.update.latest": ["已是最新", "You're up to date"],
  "settings.update.date": ["發布於", "Published"],
  "settings.update.download": ["開啟下載頁", "Open download"],
  "settings.update.no_auto": [
    "這個更新沒有簽章，無法自動安裝 —— 請用下載連結手動安裝。",
    "This update is not signed, so it cannot be installed automatically — use the link.",
  ],

  // ── 共用 ──
  "common.cancel": ["取消", "Cancel"],
  "common.confirm": ["確定", "Confirm"],
  "common.close": ["關閉", "Close"],
  "common.loading": ["載入中…", "Loading…"],
  "common.readFailed": ["讀取失敗", "Read failed"],
};
