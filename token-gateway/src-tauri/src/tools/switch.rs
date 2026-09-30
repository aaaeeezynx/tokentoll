//! 接管計畫與套用（plan_switch／apply_switch）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

#[derive(Debug, Clone, Deserialize)]
pub struct SwitchRequest {
    pub app: String,
    /// 目標 base_url。**前端送來的值只是佔位**：`plan_switch`／`apply_switch`
    /// 一律覆寫成 `gateway_url(port, app)`。
    ///
    /// 原本還有一個「真·直連上游」模式（`via_gateway = false` 時把工具設定檔
    /// 的網址直接寫成廠商地址），但前端從來沒有送過 `false`（`buildReq()` 的
    /// `via` 參數三個呼叫點全是 `true`），那條路走不到，已於 2026-09-28 連同
    /// `via_gateway` 欄位一起刪除。見 `docs/SIMPLIFICATION-PLAN.md` §4.4。
    pub base_url: String,
    /// 要使用的密鑰取值：網關模式=本地 sk key；Key 欄留空時＝上游 Key
    /// （見 `direct_upstream`）。僅 codex/opencode/dsh 用於生成環境變量指引；
    /// claude 寫入文件（見警告）。
    pub api_key: String,
    pub model: String,
    /// 選中的渠道（產生 Codex 目錄時必填）。
    pub provider_id: Option<i64>,
    /// 選中渠道的格式（計劃頁做相容提示用）。
    pub provider_format: Option<String>,
    /// 選中模型的思考等級（unset/None = 不動配置文件現值）。
    pub reasoning: Option<String>,
    /// 選中模型的上下文視窗（None = 不寫；僅 opencode/Codex 目錄生效）。
    pub context_window: Option<i64>,
    /// 是否同時產生 Codex 模型目錄（僅 codex 有效）。
    pub gen_catalog: bool,
    /// Codex 目錄是否包含其他來源模型（聯合目錄；默认僅當前來源）。
    #[serde(default)]
    pub catalog_union: bool,
    /// 直連上游模式（Key 欄留空時置 true）：後端以來源的上游 Key 直連
    /// （經網關=不經本地 Key 轉發；直連=直接寫入工具配置）；用量照常記錄
    /// （僅不計本地 Key 配額）、僅允許該來源。
    #[serde(default)]
    pub direct_upstream: bool,
    /// 前端下拉記憶的本地 Key id（後端以 api_key 為準，此欄僅回填選項用）。
    #[serde(default)]
    pub key_id: Option<i64>,
    /// Claude Code 模型映射（預設/haiku/sonnet/opus/fable；
    /// 寫入 settings.json 的 ANTHROPIC_MODEL / ANTHROPIC_DEFAULT_*；僅 claude 有效）。
    #[serde(default)]
    pub claude_map: Option<ClaudeModelMap>,
}


#[derive(Debug, Clone, Serialize)]
pub struct SwitchPlan {
    pub app: String,
    pub config_path: String,
    pub supported: bool,
    pub will_backup: bool,
    pub edits: Vec<String>,
    pub env_setup: Vec<String>,
    pub warnings: Vec<String>,
}


#[derive(Debug, Clone, Serialize)]
pub struct SwitchResult {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub extra_files: Vec<String>,
}


pub fn plan_switch(req: &SwitchRequest, port: u16) -> Result<SwitchPlan, String> {
    if !APPS.iter().any(|(a, _)| *a == req.app) {
        return Err(format!("未知工具：{}", req.app));
    }
    if req.model.trim().is_empty() {
        return Err("model 不能為空".to_string());
    }
    // base_url 一律由後端決定（唯一的模式就是經網關）。
    let mut normalized = req.clone();
    normalized.base_url = gateway_url(port, &normalized.app);
    let req = &normalized;
    let path = config_path_for(&req.app);
    let mut plan = SwitchPlan {
        app: req.app.clone(),
        config_path: path
            .as_deref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        supported: true,
        will_backup: path.as_ref().is_ok_and(|p| p.exists()),
        edits: vec![],
        env_setup: vec![],
        warnings: vec![],
    };
    // 註：原本這裡有一段「該工具正被本地代理接管（疑似 cc-switch）」警告，只在
    // `via_gateway = false`（真·直連）時才會出現；那條路走不到，已隨 D-1 刪除。
    match req.app.as_str() {
        "claude" => {
            plan.edits.push(format!("env.ANTHROPIC_BASE_URL = {}", req.base_url));
            plan.edits.push("env.ANTHROPIC_AUTH_TOKEN = ***（明文寫入 settings.json）".into());
            if let Some(m) = req.claude_map.as_ref() {
                for (k, v) in m.pairs() {
                    let v = v.trim();
                    if !v.is_empty() {
                        plan.edits.push(format!("env.{k} = {v}"));
                    }
                }
            }
            if let Some(cw) = req.context_window.filter(|n| *n > 0) {
                plan.edits.push(format!(
                    "env.CLAUDE_CODE_MAX_CONTEXT_TOKENS = {cw}（Claude Code 對不認識的模型只假設 200k，會提早自動壓縮）"
                ));
            }
            plan.warnings.push(
                "ANTHROPIC_AUTH_TOKEN 將明文寫入 settings.json：網關模式下請使用可吊銷的本地 sk key，不要放上游長效 key"
                    .to_string(),
            );
        }
        "codex" => {
            plan.edits.push(format!("model = {}", req.model));
            plan.edits.push(format!(
                "model_provider = {CODEX_SHARED_PROVIDER_ID}（共享段名，custom / tokengateway / 歷史別名段同時指向網關，舊會話可繼續）"
            ));
            // 預覽歷史別名。刻意用 report 版：**讀失敗要在接管前就讓使用者
            // 看到**，而不是等到舊會話失效、回頭懷疑是網關弄壞了什麼。
            let aliases = match path
                .as_ref()
                .ok()
                .and_then(|p| p.parent())
                .map(codex_legacy_providers_report)
            {
                Some(LegacyProviders::Ok { providers, .. }) => providers,
                Some(LegacyProviders::Failed { reason }) => {
                    plan.warnings.push(format!(
                        "讀不到 Codex 歷史會話的 provider 名：{reason}。接管可能無法補寫別名段，\
                         仍在使用舊會話的 provider 可能失效（請先完全結束 Codex 再接管）"
                    ));
                    vec![]
                }
                None => vec![],
            };
            let auth_desc = if req.direct_upstream {
                "experimental_bearer_token = 上游 Key 明文"
            } else {
                "env_key = TOKEN_GATEWAY_KEY"
            };
            // 協議形狀：**一律** `CODEX_WIRE_API`，與 [`codex_apply`] 共用
            // 同一個常數（先前兩邊各算一次，預覽因此顯示過 `chat` 而實際
            // 寫 `responses` —— 預覽騙人）。指向網關是唯一模式，宣告與此無關。
            let wire_api = CODEX_WIRE_API;
            if aliases.is_empty() {
                plan.edits.push(format!(
                    "[model_providers.{CODEX_SHARED_PROVIDER_ID}] 與 [model_providers.{GATEWAY_PROVIDER_ID}] base_url = {} / {auth_desc} / wire_api = {wire_api}",
                    req.base_url
                ));
            } else {
                plan.edits.push(format!(
                    "[model_providers.{{custom, tokengateway{}}}] 共 {} 段 base_url = {} / {auth_desc} / wire_api = {wire_api}",
                    if aliases.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", aliases.join(", "))
                    },
                    aliases.len() + 2,
                    req.base_url
                ));
            }
            plan.edits.push(
                "provider 段內聯 models（桌面版選擇器顯示用）＋接管結果回驗（託管段恰好一次，異常拒寫）"
                    .to_string(),
            );
            if let Some(r) = req.reasoning.as_deref() {
                if !r.trim().is_empty() && r != "unset" {
                    plan.edits.push(format!("model_reasoning_effort = {r}"));
                }
            }
            if req.gen_catalog {
                plan.edits.push(format!(
                    "按可用模型表產生 Codex 模型目錄（{}）並寫入 model_catalog_json",
                    if req.catalog_union {
                        "聯合：含其他啟用來源模型"
                    } else {
                        "僅當前來源模型"
                    }
                ));
            }
            if req.direct_upstream {
                plan.edits.push(
                    "直連模式：各段寫 experimental_bearer_token = 上游 Key 明文（不寫 env_key）"
                        .to_string(),
                );
                plan.warnings.push(
                    "直連模式：上游 Key 明文寫入 config.toml，僅本機可讀；流量仍經網關轉發，用量照常記錄（僅不計本地 Key 配額）、僅允許該來源登記的模型、不做跨來源路由"
                        .to_string(),
                );
            } else {
                plan.env_setup.push(ps_export(GATEWAY_ENV_KEY));
                plan.warnings.push(
                    "金鑰不寫入檔案：Codex 官方要求經 env_key 引用的環境變量提供（experimental_bearer_token 已被官方明確不鼓勵）"
                        .to_string(),
                );
            }
        }
        "opencode" => {
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.options.baseURL = {}",
                req.base_url
            ));
            let apikey_desc = if req.direct_upstream {
                "上游 Key 明文（直連模式）".to_string()
            } else {
                format!("{{env:{GATEWAY_ENV_KEY}}}（引用，不落地）")
            };
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.options.apiKey = {apikey_desc}",
            ));
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.models 登記 {}",
                req.model
            ));
            if !req.model.trim().is_empty() {
                plan.edits.push(format!(
                    "model = {GATEWAY_PROVIDER_ID}/{}（接管時一併指定）",
                    req.model
                ));
            }
            if let Some(cw) = req.context_window {
                if cw > 0 {
                    plan.edits.push(format!(
                        "provider.{GATEWAY_PROVIDER_ID}.models.{}.limit.context = {cw}（上文上限）",
                        req.model
                    ));
                }
            }
            if req.direct_upstream {
                plan.warnings.push(
                    "直連模式：上游 Key 明文寫入 opencode.json，僅本機可讀；流量仍經網關轉發，用量照常記錄（僅不計本地 Key 配額）、僅允許該來源登記的模型、不做跨來源路由"
                        .to_string(),
                );
            } else {
                plan.env_setup.push(ps_export(GATEWAY_ENV_KEY));
            }
            if !req.direct_upstream {
                plan.warnings.push(
                    "變量必須存在於 opencode 行程環境中：請在啟動 opencode 的同一終端機先執行上面命令".to_string(),
                );
            }
            plan.warnings.push(format!(
                "預設模型已釘在 {GATEWAY_PROVIDER_ID}/{}：`opencode run` 直接用這個模型，\
                 不必再手選（要換模型就回到這裡重新接管）",
                req.model
            ));
        }
        "dsh" => {
            plan.supported = false;
            plan.will_backup = false;
            plan.env_setup
                .push("$env:DEEPSEEK_API_KEY = \"<把金鑰粘貼在這裡>\"".to_string());
            plan.warnings.push(
                "DSH 的 endpoint 由 profile bundle（deepseek-official）決定，M2 僅管理 DEEPSEEK_API_KEY，網關接管待 M3 驗證"
                    .to_string(),
            );
            return Ok(plan);
        }
        // cursor／antigravity／hermes：仍列在 APPS（用量歸屬與篩選要用），但
        // **不提供自動接管**，而且已不再出現在工具偵測清單裡
        // （cursor／antigravity 為第一階段 A，hermes 為第一階段 F）。
        // 原本這裡各有專屬的接管/提示分支，那幾條路在 UI 上已走不到，
        // 一併刪除，改成這一條誠實的拒絕。
        app => {
            plan.supported = false;
            plan.will_backup = false;
            plan.warnings.push(format!(
                "{app} 不支援自動接管：本 App 只把它當作用量歸屬標籤顯示，不會寫入它的設定檔"
            ));
            return Ok(plan);
        }
    }
    // 格式相容提示（`direct_upstream` 只是 Key 的來源不同，流量仍經網關）
    if let Some(pf) = req.provider_format.as_deref() {
        match (tool_format(&req.app), pf) {
            ("anthropic", "openai-chat") | ("anthropic", "mixed") => {
                plan.edits.push(
                    "將自動做 Anthropic→OpenAI 轉換（模型按可用模型表映射，無映射原樣透傳）"
                        .to_string(),
                );
            }
            ("anthropic", "anthropic") => {
                plan.edits.push("Anthropic 原生透傳".to_string());
            }
            (_, "anthropic") => {
                plan.warnings.push(
                    "格式不相容：該工具不說 Anthropic，請求會被網關拒絕（反向轉換未實現）"
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    Ok(plan)
}


pub fn apply_switch(
    app_data: &Path,
    mut req: SwitchRequest,
    port: u16,
    catalog: Option<String>,
) -> Result<SwitchResult, String> {
    req.base_url = gateway_url(port, &req.app);
    // 先出方案做合法性校驗（dsh 等直接在此拒絕）。
    let plan = plan_switch(&req, port)?;
    if !plan.supported {
        return Err("該工具暫不支持自動寫入，請按 env_setup 指引手工配置".to_string());
    }
    let backups_root = app_data.join("backups");
    let cfg = config_path_for(&req.app)?;
    let existing = read_text(&cfg);
    let reasoning = req.reasoning.as_deref();
    let new_text = match req.app.as_str() {
        "claude" => {
            claude_apply(
                existing.as_deref(),
                &req.base_url,
                &req.api_key,
                req.claude_map.as_ref(),
                req.context_window,
            )?
        }
        "codex" => {
            // 歷史會話用過的 provider 名全寫為網關別名段（只讀 threads，不寫 DB）。
            //
            // 再與 config.toml 裡**已指向本網關**的別名段聯集，讓管理清單只增
            // 不減：若清單只來自 DB，一旦 threads 變少（例如使用者**封存對話**
            // 之後該 provider 不再出現在 `SELECT DISTINCT model_provider`），
            // 舊的別名段就會被 `codex_apply` 剔除，舊會話的 provider 段因而
            // 消失。
            //
            // ⚠️ 2026-09-28 更正：這裡原本還寫著「這正是舊會話無法續用／無法
            // 封存的可能成因」——**那個推論是錯的**。實際追查後，封存失敗與
            // provider 段完全無關（失敗的 session 其 provider 段一切正常），
            // 真因是 Codex 自己的殘留鎖檔
            // `~/.codex/thread-writer-locks/<thread-id>.lock`，見
            // docs/TESTING.md §0.8。保留「只增不減」是因為它對**舊會話續用**
            // 仍然必要（provider 段不見了，舊 session 就真的開不起來），
            // 但不要再把它當成封存問題的線索。
            let gw_url = gateway_url(port, "codex");
            let db_aliases = match cfg.parent().map(codex_legacy_providers_report) {
                Some(LegacyProviders::Ok { providers, .. }) => providers,
                // 讀失敗時 `codex_alias_ids` 仍會保住既有的網關別名段。
                _ => vec![],
            };
            let aliases = codex_alias_ids(existing.as_deref().unwrap_or(""), &db_aliases, &gw_url);
            let inline_models = codex_inline_models(catalog.as_deref());
            // 直連上游：api_key 即 switch_apply 命令層注入的上游 Key（明文寫段內）
            let direct_key = if req.direct_upstream && !req.api_key.trim().is_empty() {
                Some(req.api_key.trim())
            } else {
                None
            };
            codex_apply(
                existing.as_deref().unwrap_or(""),
                &req.base_url,
                &req.model,
                reasoning,
                catalog.as_deref(),
                &aliases,
                &inline_models,
                // 協議形狀不由這裡決定：`gateway_section` 一律寫
                // `CODEX_WIRE_API`（指向網關是唯一模式，Codex 也已不接受
                // `chat`）。這裡只需交代認證方式。
                CodexAuth { direct_key },
            )?
        }
        "opencode" => {
            // 直連上游：api_key 即 switch_apply 命令層注入的上游 Key（明文寫 options.apiKey，
            // 與 Codex 的 experimental_bearer_token 對等；缺省仍走 {env:} 引用）。
            let direct_key = if req.direct_upstream && !req.api_key.trim().is_empty() {
                Some(req.api_key.trim())
            } else {
                None
            };
            opencode_apply(existing.as_deref(), &req.base_url, &req.model, req.context_window, direct_key)?
        }
        _ => unreachable!(),
    };

    let mut result = SwitchResult {
        config_path: cfg.to_string_lossy().to_string(),
        backup_path: None,
        extra_files: vec![],
    };
    // 備份（同名 stem 前綴輪換；第一階段 E 起每個工具只留 `BACKUP_KEEP`＝1 份，
    // 所以輪換前先確保「接管前」基準備份存在——見 `ensure_baseline_before_prune`）。
    if let Some(old) = existing.as_deref() {
        let dir = backups_root.join(&req.app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = cfg
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");
        // 第一階段 E：輪換只留 1 份，砍之前先保住「接管前」的乾淨設定
        // （當前乾淨就用當前；當前是接管態就從歷史備份遷移一份）。
        ensure_baseline_before_prune(app_data, &req.app, &cfg, old, port);
        let name = format!("{stem}.bak-{}", crate::fsutil::backup_stamp());
        let dest = crate::fsutil::unique_backup_name(&dir, &name);
        std::fs::write(&dest, old).map_err(|e| format!("備份失敗：{e}"))?;
        prune_backups_keep_clean(&dir, &req.app, stem, BACKUP_KEEP, port)
            .map_err(|e| e.to_string())?;
        result.backup_path = Some(dest.to_string_lossy().to_string());
    }
    crate::fsutil::atomic_write(&cfg, new_text.as_bytes()).map_err(|e| format!("寫入失敗：{e}"))?;
    Ok(result)
}
