//! 版本字串比較與提純。

/// 版本段式比較（`1.2.10 > 1.2.9`；無後綴 > 有後綴，如 `0.1.2 > 0.1.2-rc.1`）。
pub(crate) fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |s: &str| {
        let (nums, pre) = match s.split_once('-') {
            Some((n, p)) => (n, Some(p.to_string())),
            None => (s, None),
        };
        let ns: Vec<u64> = nums.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        (ns, pre)
    };
    let (mut an, ap) = split(a.trim());
    let (mut bn, bp) = split(b.trim());
    let n = an.len().max(bn.len());
    an.resize(n, 0);
    bn.resize(n, 0);
    match an.cmp(&bn) {
        Ordering::Equal => match (ap, bp) {
            (None, None) => Ordering::Equal,
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(x), Some(y)) => x.cmp(&y),
        },
        ord => ord,
    }
}


/// 從人類可讀版本行提純番號（`codex-cli 0.153.4`→`0.153.4`，
/// `0.1.2-rc.1` 保留後綴）。
pub(crate) fn clean_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            if j < b.len() && b[j] == b'-' {
                let mut k = j + 1;
                while k < b.len()
                    && (b[k].is_ascii_alphanumeric() || b[k] == b'.' || b[k] == b'-')
                {
                    k += 1;
                }
                if k > j + 1 {
                    j = k;
                }
            }
            let cand = &s[i..j];
            if cand.contains('.') {
                return Some(cand.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}
