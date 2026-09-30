//! 最小 tar 讀取器（P3.3）。
//!
//! GitHub 的技能下載用 tarball（`.tar.gz`）：一次請求就能拿到整個資料夾，
//! 比用 contents API 逐檔抓省得多（後者還會撞到每小時 60 次的匿名限制）。
//!
//! 為什麼自己寫而不用 `tar` crate：**離線快取裡沒有它**（`flate2` 有，
//! 所以 gunzip 用現成的）。tar 的格式很單純 —— 每個檔案一個 512 bytes 的
//! header（檔名、大小是八進位字串…），後面接內容補齊到 512 的倍數 ——
//! 我們只需要「讀 header、拿內容」，不需要產生 tar，所以約 60 行就夠。
//!
//! 支援：一般檔案與目錄、GNU longname（`typeflag = 'L'`，GitHub 的長路徑會用到）。
//! 忽略：符號連結、硬連結、裝置檔（技能就是一堆檔案，不需要那些）。

use std::io::Read;

/// 從 tar 位元組取出所有一般檔案：`(相對路徑, 內容)`。
///
/// 路徑會去掉 tar 最外層的那個目錄前綴（GitHub 的 tarball 是
/// `{repo}-{ref}/...`），呼叫端拿到的是 `skills/foo/SKILL.md` 這種相對路徑。
pub fn read_tar(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = vec![];
    let mut pos = 0usize;
    let mut prefix_strip: Option<String> = None;
    let mut pending_longname: Option<String> = None;
    while pos + 512 <= bytes.len() {
        let header = &bytes[pos..pos + 512];
        // 全零的 header＝結束（tar 會補兩個 512 的空區塊）
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let name = read_str(&header[0..100]);
        let size = read_octal(&header[124..136])?;
        let typeflag = header[156] as char;
        let data_start = pos + 512;
        let data_end = data_start + size;
        if data_end > bytes.len() {
            return Err(format!("tar 內容被截斷（{name}：宣告 {size} bytes）"));
        }
        let data = &bytes[data_start..data_end];
        // 下一個 header 要對齊 512
        pos = data_start + size.div_ceil(512) * 512;

        match typeflag {
            'L' => {
                // GNU longname：這筆的內容是下一個項目的完整檔名
                pending_longname = Some(String::from_utf8_lossy(data).trim_end_matches('\0').to_string());
                continue;
            }
            '0' | '\0' => {}
            _ => continue, // 目錄／連結等一律跳過
        }
        let full = pending_longname
            .take()
            .unwrap_or_else(|| name.clone());
        let rel = strip_root(&full, &mut prefix_strip);
        if let Some(rel) = rel {
            out.push((rel, data.to_vec()));
        }
    }
    Ok(out)
}

/// 去掉 tarball 最外層的 `{repo}-{ref}/` 前綴（只做一次，之後沿用）。
fn strip_root(full: &str, cache: &mut Option<String>) -> Option<String> {
    let prefix = match cache {
        Some(p) => p.clone(),
        None => {
            let p = full.split('/').next().unwrap_or("").to_string();
            *cache = Some(p.clone());
            p
        }
    };
    let rest = full.strip_prefix(&format!("{prefix}/")).unwrap_or(full);
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn read_str(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// tar 的數字欄位是「八進位字串」（可能前面有空白、後面有 NUL）。
fn read_octal(b: &[u8]) -> Result<usize, String> {
    let s = read_str(b);
    if s.is_empty() {
        return Ok(0);
    }
    usize::from_str_radix(s.trim(), 8).map_err(|e| format!("tar 大小欄位「{s}」不是八進位：{e}"))
}

/// gunzip（GitHub 的 tarball 是 gzip 壓縮）。
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut d = flate2::read::GzDecoder::new(bytes);
    let mut out = Vec::new();
    d.read_to_end(&mut out).map_err(|e| format!("解壓失敗：{e}"))?;
    Ok(out)
}

/// 便利函式：`.tar.gz` → 檔案清單。
pub fn read_tar_gz(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    read_tar(&gunzip(bytes)?)
}

/// 組一個 tar（**只給測試用**：單元測試要能憑空造出 tarball 來驗證解析）。
#[cfg(test)]
pub(crate) fn write_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in entries {
        let mut h = [0u8; 512];
        h[..name.len().min(100)].copy_from_slice(&name.as_bytes()[..name.len().min(100)]);
        // 權限 0644
        h[100..108].copy_from_slice(b"0000644\0");
        let size = format!("{:011o}\0", data.len());
        h[124..136].copy_from_slice(size.as_bytes());
        h[156] = b'0';
        // checksum：先填空白，算完再寫回
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        let cs = format!("{:06o}\0 ", sum);
        h[148..156].copy_from_slice(cs.as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_tarball_with_nested_files() {
        let tar = write_tar(&[
            ("repo-main/skills/demo/SKILL.md", b"# Demo\n"),
            ("repo-main/skills/demo/python/run.py", b"print(1)\n"),
            ("repo-main/README.md", b"root\n"),
        ]);
        let files = read_tar(&tar).unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "skills/demo/SKILL.md",
                "skills/demo/python/run.py",
                "README.md"
            ],
            "最外層的 repo-ref/ 要去掉"
        );
        assert_eq!(files[0].1, b"# Demo\n");
        assert_eq!(files[1].1, b"print(1)\n");
    }

    #[test]
    fn gunzip_roundtrip_via_flate2() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let tar = write_tar(&[("r-m/a/b.txt", b"hello")]);
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&tar).unwrap();
        let gz = enc.finish().unwrap();
        let files = read_tar_gz(&gz).unwrap();
        assert_eq!(files, vec![("a/b.txt".to_string(), b"hello".to_vec())]);
    }

    #[test]
    fn rejects_truncated_input() {
        let mut tar = write_tar(&[("r-m/big.txt", &vec![b'x'; 2048])]);
        tar.truncate(700); // 砍掉內容
        assert!(read_tar(&tar).unwrap_err().contains("截斷"));
    }

    #[test]
    fn empty_tar_is_empty_list() {
        assert!(read_tar(&vec![0u8; 1024]).unwrap().is_empty());
        assert!(read_tar(&[]).unwrap().is_empty());
    }

    #[test]
    fn bad_octal_is_reported() {
        let mut tar = write_tar(&[("r-m/a.txt", b"x")]);
        tar[124..136].copy_from_slice(b"zzzzzzzzzzz\0");
        assert!(read_tar(&tar).unwrap_err().contains("八進位"));
    }
}
