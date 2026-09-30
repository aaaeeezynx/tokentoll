//! 從上游回應裡取出數字（P2.1 的「extractor」）。
//!
//! ## 為什麼不是 JavaScript
//!
//! cc-switch 的自訂查詢是**使用者寫的 JS**（`({request, extractor})`）。我們刻意
//! 不做那一格，改成**宣告式**的抽取規格，理由三條：
//!
//! 1. **不想為了這個塞一個 JS 引擎**：純 Rust 的引擎（boa 之類）是好幾 MB 的
//!    相依與編譯時間，而這台機器的建置是 `--offline`。
//! 2. **文件裡的每個例子都是「打一個網址、從 JSON 取幾個數字」**：cc-switch 自己的
//!    範例（Generic、New API）就是 `url` ＋ `headers` ＋ `response.xxx`，
//!    真正需要任意程式邏輯的案例很少。
//! 3. **可測試**：宣告式規格是資料，可以逐條寫測試；使用者寫的 JS 只能靠執行才知道。
//!
//! 所以規格長這樣（每個數字欄位可以是字串路徑，或一個帶運算的小物件）：
//!
//! ```json
//! {
//!   "remaining": "balance_infos.0.total_balance",
//!   "used": { "path": "used_quota", "divide": 500000 },
//!   "total": { "path": "quota", "divide": 500000 },
//!   "unit": "USD",
//!   "planName": "data.group",
//!   "validPath": "is_available",
//!   "invalidMessagePath": "message",
//!   "extra": "data.email"
//! }
//! ```
//!
//! 路徑語法：`.` 分隔，數字段＝陣列索引（`balance_infos.0.total_balance`）。
//! 找不到一律回 `None`（不猜、不補 0）——「查不到」與「是 0」是不同的事。

use serde_json::Value;

/// 一個數字欄位的規格：路徑 ＋ 可選運算。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumSpec {
    pub path: String,
    /// 減掉另一個路徑的值（OpenRouter 的「額度 − 已用」就是這種）
    pub subtract: Option<String>,
    pub divide: Option<f64>,
    pub multiply: Option<f64>,
}

impl NumSpec {
    /// 從 JSON 值解析：字串＝只要路徑；物件＝帶運算。
    pub fn parse(v: &Value) -> Option<Self> {
        match v {
            Value::String(s) => Some(NumSpec {
                path: s.clone(),
                ..Default::default()
            }),
            Value::Object(o) => {
                let path = o.get("path").and_then(|p| p.as_str())?.to_string();
                Some(NumSpec {
                    path,
                    subtract: o
                        .get("subtract")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string()),
                    divide: o.get("divide").and_then(|d| d.as_f64()),
                    multiply: o.get("multiply").and_then(|m| m.as_f64()),
                })
            }
            _ => None,
        }
    }

    /// 依規格算值。任何一步缺資料就回 `None`。
    pub fn eval(&self, root: &Value) -> Option<f64> {
        let mut v = lookup_num(root, &self.path)?;
        if let Some(sub) = &self.subtract {
            v -= lookup_num(root, sub)?;
        }
        if let Some(d) = self.divide {
            if d == 0.0 || !d.is_finite() {
                return None;
            }
            v /= d;
        }
        if let Some(m) = self.multiply {
            v *= m;
        }
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }
}

/// 依點分路徑取值；支援陣列索引。
pub fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        cur = match cur {
            Value::Object(o) => o.get(seg)?,
            Value::Array(a) => {
                let i: usize = seg.parse().ok()?;
                a.get(i)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// 取值並轉成數字。**字串型數字也接受**（上游很常回 `"110.00"`）。
pub fn lookup_num(root: &Value, path: &str) -> Option<f64> {
    match lookup(root, path)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// 取字串（數字也轉成字串，方便顯示）。
pub fn lookup_str(root: &Value, path: &str) -> Option<String> {
    match lookup(root, path)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn lookup_supports_objects_arrays_and_indices() {
        let j = v(r#"{"a":{"b":[{"c":7}]},"s":"12.5"}"#);
        assert_eq!(lookup_num(&j, "a.b.0.c"), Some(7.0));
        assert_eq!(lookup_num(&j, "s"), Some(12.5), "字串型數字要接受");
        assert_eq!(lookup_str(&j, "s").as_deref(), Some("12.5"));
        // 不存在的路徑回 None —— 「查不到」不等於 0
        assert_eq!(lookup_num(&j, "a.b.9.c"), None);
        assert_eq!(lookup_num(&j, "nope"), None);
        assert_eq!(lookup_num(&j, ""), None);
        // 型別不對也是 None
        assert_eq!(lookup_num(&j, "a"), None);
        assert_eq!(lookup_num(&v(r#"{"x":"abc"}"#), "x"), None);
    }

    #[test]
    fn num_spec_parses_both_shapes() {
        let plain = NumSpec::parse(&Value::String("balance".into())).unwrap();
        assert_eq!(plain.path, "balance");
        assert_eq!(plain.divide, None);

        let full = NumSpec::parse(&v(
            r#"{"path":"quota","subtract":"used","divide":500000,"multiply":2}"#,
        ))
        .unwrap();
        assert_eq!(full.path, "quota");
        assert_eq!(full.subtract.as_deref(), Some("used"));
        assert_eq!(full.divide, Some(500000.0));
        assert_eq!(full.multiply, Some(2.0));

        // 沒有 path 的物件無效
        assert!(NumSpec::parse(&v(r#"{"divide":2}"#)).is_none());
        assert!(NumSpec::parse(&Value::Bool(true)).is_none());
    }

    #[test]
    fn num_spec_arithmetic() {
        let j = v(r#"{"total_credits":10.0,"total_usage":2.5,"quota":500000}"#);
        // New API 的 /500000
        let div = NumSpec::parse(&v(r#"{"path":"quota","divide":500000}"#)).unwrap();
        assert_eq!(div.eval(&j), Some(1.0));
        // OpenRouter 的「額度 − 已用」
        let sub = NumSpec::parse(&v(r#"{"path":"total_credits","subtract":"total_usage"}"#))
            .unwrap();
        assert_eq!(sub.eval(&j), Some(7.5));
        // 除以 0 要回 None，不是 inf
        let bad = NumSpec::parse(&v(r#"{"path":"quota","divide":0}"#)).unwrap();
        assert_eq!(bad.eval(&j), None);
        // 減項缺資料 → None（不假裝算得出來）
        let missing =
            NumSpec::parse(&v(r#"{"path":"quota","subtract":"nope"}"#)).unwrap();
        assert_eq!(missing.eval(&j), None);
    }
}
