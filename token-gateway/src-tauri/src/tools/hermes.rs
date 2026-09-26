//! hermes 接管（providers 區塊與 .env）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

/// Hermes 家目錄：`$HERMES_HOME` → 有 config.yaml 的 `%LOCALAPPDATA%/hermes` → `~/.hermes`。
pub fn hermes_home() -> PathBuf {
    if let Ok(v) = std::env::var("HERMES_HOME") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        let p = local.join("hermes");
        if p.join("config.yaml").exists() {
            return p;
        }
    }
    user_home()
        .map(|h| h.join(".hermes"))
        .unwrap_or_else(|_| PathBuf::from(".hermes"))
}


/// 解析 hermes config.yaml 的 `model:` 段（provider / default）。
pub(crate) fn parse_hermes_model(text: &str) -> (Option<String>, Option<String>) {
    let mut in_model = false;
    let mut provider = None;
    let mut default = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_model = trimmed.trim_end_matches(':') == "model" && trimmed.ends_with(':');
            continue;
        }
        if in_model && indent == 2 {
            if let Some(v) = trimmed.strip_prefix("provider:") {
                provider = Some(strip_quotes(v));
            } else if let Some(v) = trimmed.strip_prefix("default:") {
                let v = strip_quotes(v);
                if !v.is_empty() {
                    default = Some(v);
                }
            }
        }
    }
    (provider, default)
}


/// 取 hermes `providers.<name>.base_url`。
pub(crate) fn hermes_provider_base(text: &str, name: &str) -> Option<String> {
    let mut in_providers = false;
    let mut in_block = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_providers = trimmed.trim_end_matches(':') == "providers" && trimmed.ends_with(':');
            in_block = false;
            continue;
        }
        if !in_providers {
            continue;
        }
        if indent == 2 && trimmed.ends_with(':') {
            in_block = trimmed.trim_end_matches(':') == name;
            continue;
        }
        if in_block && indent == 4 {
            if let Some(v) = trimmed.strip_prefix("base_url:") {
                return Some(strip_quotes(v));
            }
        }
    }
    None
}


/// 在 `section:` 段內設定 2 縮進標量；段內缺失則追加到段尾（段最後一個條目之後）。
pub(crate) fn set_yaml_scalar(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("{section}:");
    let key_prefix = format!("{key}:");
    let lines: Vec<&str> = text.lines().collect();
    // 找到段頭
    let mut head_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if line.len() == t.len() && t == header {
            head_idx = Some(i);
            break;
        }
    }
    let Some(hi) = head_idx else {
        // 整段缺失：末尾追加
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&format!("{header}\n  {key}: {value}\n"));
        return s;
    };
    // 段尾 = 下一個 0 縮進非空非註釋行之前
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(hi + 1) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if line.len() == t.len() {
            end = i;
            break;
        }
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    let mut changed = false;
    for (i, line) in lines.iter().enumerate() {
        if i > hi && i < end && !changed {
            let t = line.trim_start();
            let indent = line.len() - t.len();
            if indent == 2 && (t == key_prefix || t.starts_with(&format!("{key_prefix} "))) {
                out.push(format!("  {key}: {value}"));
                changed = true;
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !changed {
        // 追加到段尾（end 之前，跳過尾部空行/註釋行之上？直接插在 end 處即可）
        out.insert(end, format!("  {key}: {value}"));
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}


/// 確保 hermes `providers.tokengateway` 塊存在且 base_url / model / key_env 正確。
/// key_env 是 hermes 具名自訂 provider 的憑證指針（憑證本體在 .env），
/// 缺了它 hermes 會用 "no-key-required" 佔位 key → 網關 401。
pub(crate) fn ensure_hermes_provider(text: &str, base_url: &str, model: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut prov_head = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if line.len() == t.len() && (t == "providers:" ) {
            prov_head = Some(i);
            break;
        }
    }
    let block = [
        format!("  {GATEWAY_PROVIDER_ID}:"),
        "    name: tokengateway".to_string(),
        format!("    base_url: {base_url}"),
        format!("    model: {model}"),
        format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"),
        "    extra_headers:".to_string(),
        "      X-TG-App: hermes".to_string(),
        "    discover_models: false".to_string(),
    ];
    let Some(phi) = prov_head else {
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str("providers:\n");
        s.push_str(&block.join("\n"));
        s.push('\n');
        return s;
    };
    // 段尾
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(phi + 1) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if line.len() == t.len() {
            end = i;
            break;
        }
    }
    // 找現有 tokengateway 塊
    let mut block_idx = None;
    for (i, line) in lines.iter().enumerate().skip(phi + 1).take(end - (phi + 1)) {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        if indent == 2 && t.trim_end_matches(':') == GATEWAY_PROVIDER_ID && t.ends_with(':') {
            block_idx = Some(i);
            break;
        }
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    if let Some(bi) = block_idx {
        // 塊範圍：bi+1 起、直到縮進 <= 2 的非空非註釋行為止
        let mut block_end = out.len();
        for (i, line) in out.iter().enumerate().skip(bi + 1) {
            let t = line.trim_start();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if line.len() - t.len() <= 2 {
                block_end = i;
                break;
            }
        }
        // 逐欄位定位（4 縮進）：base_url / model / key_env / extra_headers
        let (mut base_idx, mut model_idx, mut key_idx, mut eh_idx) = (None, None, None, None);
        for (i, line) in out.iter().enumerate().take(block_end).skip(bi + 1) {
            let t = line.trim_start();
            if line.len() - t.len() != 4 {
                continue;
            }
            if t == "base_url:" || t.starts_with("base_url: ") {
                base_idx = Some(i);
            } else if t == "model:" || t.starts_with("model: ") {
                model_idx = Some(i);
            } else if t == "key_env:" || t.starts_with("key_env: ") {
                key_idx = Some(i);
            } else if t == "extra_headers:" {
                eh_idx = Some(i);
            }
        }
        if let Some(i) = base_idx {
            out[i] = format!("    base_url: {base_url}");
        }
        if let Some(i) = model_idx {
            out[i] = format!("    model: {model}");
        }
        match key_idx {
            Some(i) => out[i] = format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"),
            None => {
                let at = model_idx
                    .or(base_idx)
                    .map(|i| i + 1)
                    .unwrap_or(block_end)
                    .min(out.len());
                out.insert(at, format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"));
            }
        }
        if model_idx.is_none() {
            let at = base_idx.map(|i| i + 1).unwrap_or(block_end).min(out.len());
            out.insert(at, format!("    model: {model}"));
        }
        // X-TG-App 標頭：網關據此把用量歸到 hermes（否則 UA 是 OpenAI/Python → unknown）
        if eh_idx.is_none() {
            let at = key_idx
                .map(|i| i + 1)
                .or(model_idx.map(|i| i + 1))
                .or(base_idx.map(|i| i + 1))
                .unwrap_or(block_end)
                .min(out.len());
            out.insert(at, "      X-TG-App: hermes".to_string());
            out.insert(at, "    extra_headers:".to_string());
        }
    } else {
        let insert_at = end.min(out.len());
        for (k, b) in block.iter().enumerate() {
            out.insert(insert_at + k, b.clone());
        }
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}


pub fn hermes_apply(existing: &str, base_url: &str, model: &str) -> Result<String, String> {
    let t = ensure_hermes_provider(existing, base_url, model);
    let t = set_yaml_scalar(&t, "model", "provider", GATEWAY_PROVIDER_ID);
    let t = set_yaml_scalar(&t, "model", "default", model);
    Ok(t)
}


/// `.env` 行式 replace-or-append（保留註釋與空行）。
pub fn dotenv_set(text: &str, key: &str, value: &str) -> String {
    let mut out = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let t = line.trim_start();
        if !changed && !t.starts_with('#') && (t == key || t.starts_with(&format!("{key}="))) {
            out.push(format!("{key}={value}"));
            changed = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !changed {
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') || text.is_empty() {
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------- 方案 ---


/// Hermes `.env` 連帶還原：取最新不含我方 Key 行的 `.env.bak-*` 寫回；
/// 無任何 `.env` 備份時，若現文件僅有接管時新建的我方 Key 一行則刪除（還原"不存在"原狀）；
/// 否則保持現狀並注記（不猜、不刪用户內容）。
pub(crate) fn restore_hermes_env(app_data: &Path, cfg: &Path) -> Result<String, String> {
    let dir = app_data.join("backups").join("hermes");
    let home = cfg.parent().ok_or("hermes 配置目錄不可用".to_string())?;
    let envp = home.join(".env");
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(".env.bak-"))
                })
                .collect()
        })
        .unwrap_or_default();
    cands.sort();
    cands.reverse();
    for c in &cands {
        let text = std::fs::read_to_string(c).map_err(|e| format!(".env 備份讀取失敗：{e}"))?;
        if text.contains(HERMES_CUSTOM_ENV_KEY) {
            continue;
        }
        crate::fsutil::atomic_write(&envp, text.as_bytes())
            .map_err(|e| format!(".env 還原寫入失敗：{e}"))?;
        let name = c
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        return Ok(format!("｜.env 已還原自 {name}"));
    }
    if cands.is_empty() {
        if let Ok(cur) = std::fs::read_to_string(&envp) {
            let meaningful: Vec<&str> = cur
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            if meaningful.len() == 1
                && meaningful[0].starts_with(&format!("{HERMES_CUSTOM_ENV_KEY}="))
            {
                std::fs::remove_file(&envp).map_err(|e| format!(".env 移除失敗：{e}"))?;
                return Ok("｜.env 為接管時新建，已移除".to_string());
            }
        }
        return Ok("｜.env 無備份，保持現狀".to_string());
    }
    Ok("｜.env 無乾淨備份，保持現狀".to_string())
}
