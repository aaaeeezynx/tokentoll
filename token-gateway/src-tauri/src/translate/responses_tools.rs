//! Responses 工具／思考輔助：自訂（freeform）工具的 function 化與還原、reasoning
//! 橋接、工具完成項，以及流式與非流式共用的工具累加器 `RToolAcc`。

use serde_json::{json, Value};

// ------------------------------------------------- Responses ⇄ Chat Completions ---

/// Codex 自訂（freeform）工具（如 `apply_patch`）被 function 化後，
/// 原始內容所放入的唯一參數名。
pub(crate) const CUSTOM_TOOL_INPUT_KEY: &str = "input";

/// 從 Responses 請求提取自訂工具名（`type == "custom"`）。
/// Codex 0.15x 把 apply_patch 宣告為 freeform 自訂工具；上游 chat 端不認
/// `type:"custom"` 會 400，故 function 化送出，回程再還原成 `custom_tool_call`。
pub(crate) fn custom_tool_names(req: &Value) -> Vec<String> {
    req.get("tools")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter(|t| t.get("type").and_then(|x| x.as_str()) == Some("custom"))
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                .filter(|n| !n.is_empty())
                .map(|n| n.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 自訂工具 function 化後的統一 parameters（單一 `input` 字串）。
pub(super) fn custom_tool_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            CUSTOM_TOOL_INPUT_KEY: {
                "type": "string",
                "description": "Raw tool input, passed through verbatim.",
            }
        },
        "required": [CUSTOM_TOOL_INPUT_KEY],
        "additionalProperties": false,
    })
}

/// 上游 function 呼叫的 arguments（JSON）→ 自訂工具原始字串。
fn custom_tool_input(args: &str) -> String {
    serde_json::from_str::<Value>(args)
        .ok()
        .and_then(|v| {
            v.get(CUSTOM_TOOL_INPUT_KEY)
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| args.to_string())
}

/// reasoning item → 思考原文。優先解我們自己寫入的 `encrypted_content` 標記，
/// 其次取 Codex 存的 summary 文本（cc-switch 同款橋接：Codex 會把 summary
/// 原樣帶回，思考模型因此保有跨回合的計畫連續性）。
pub(crate) const REASONING_MARKER: &str = "tg-openai-reasoning-v1:reasoning:";

pub(super) fn reasoning_text_of(item: &Value) -> Option<String> {
    if let Some(ec) = item.get("encrypted_content").and_then(|e| e.as_str()) {
        if let Some(rest) = ec.strip_prefix(REASONING_MARKER) {
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    for key in ["summary", "content"] {
        if let Some(parts) = item.get(key).and_then(|p| p.as_array()) {
            let text: String = parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// 工具完成項：自訂工具還原為 `custom_tool_call`，其餘維持 `function_call`。
pub(super) fn tool_item(acc: &RToolAcc, cid: &str, is_custom: bool) -> Value {
    if is_custom {
        json!({
            "type": "custom_tool_call", "id": format!("ctc_{cid}"),
            "call_id": cid, "name": acc.name,
            "input": custom_tool_input(&acc.args), "status": "completed",
        })
    } else {
        json!({
            "type": "function_call", "id": format!("fc_{cid}"),
            "call_id": cid, "name": acc.name, "arguments": acc.args,
            "status": "completed",
        })
    }
}

pub(super) fn is_custom_tool(names: &[String], name: &str) -> bool {
    names.iter().any(|n| n == name)
}

#[derive(Debug, Default)]
pub(super) struct RToolAcc {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) args: String,
}
