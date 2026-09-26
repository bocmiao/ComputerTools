//! 诊断报告的脱敏。报告只在本地生成，由用户自己决定发不发。

use std::sync::LazyLock;

use regex::Regex;

static SID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"S-1-5-21-\d+-\d+-\d+(-\d+)?").unwrap());
static USER_DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([A-Z]:\\Users\\)[^\\\s]+").unwrap());
static IPV4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}\b").unwrap());
static MAC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b").unwrap());
static SERIAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(serial[_ ]?(number)?|序列号)\s*[:：=]\s*\S+").unwrap());

/// 把文本里的隐私信息替换掉。`secrets` 是需要额外去掉的字符串（用户名、电脑名等）。
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_owned();
    // 长的先替换，避免「xiaoming」先被替换后「MOCK-PC\xiaoming」对不上
    let mut secrets: Vec<&String> = secrets.iter().filter(|s| s.chars().count() >= 2).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for s in secrets {
        out = replace_ignore_case(&out, s, "<已隐藏>");
    }
    out = SID.replace_all(&out, "S-1-5-21-<已隐藏>").into_owned();
    out = USER_DIR.replace_all(&out, "${1}<用户>").into_owned();
    out = IPV4
        .replace_all(&out, |c: &regex::Captures<'_>| {
            let ip = &c[0];
            if ip.starts_with("127.") || ip == "0.0.0.0" { ip.to_owned() } else { "<IP>".to_owned() }
        })
        .into_owned();
    out = MAC.replace_all(&out, "<MAC>").into_owned();
    out = SERIAL.replace_all(&out, "${1}：<已隐藏>").into_owned();
    out
}

fn replace_ignore_case(haystack: &str, needle: &str, with: &str) -> String {
    let re = Regex::new(&format!("(?i){}", regex::escape(needle))).expect("转义过的正则");
    re.replace_all(haystack, regex::NoExpand(with)).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_identifiers() {
        let text = "用户 MOCK-PC\\xiaoming 在 C:\\Users\\xiaoming\\Desktop，SID S-1-5-21-1000-2000-3000-1001，\
                    IP 192.168.1.23，代理 127.0.0.1:7890，MAC 00-1A-2B-3C-4D-5E，serial_number: ABC123";
        let out = redact(text, &["MOCK-PC\\xiaoming".into(), "MOCK-PC".into(), "xiaoming".into()]);
        for leaked in ["xiaoming", "MOCK-PC", "1000-2000-3000", "192.168.1.23", "00-1A-2B", "ABC123"] {
            assert!(!out.contains(leaked), "{leaked} 没被去掉：{out}");
        }
        assert!(out.contains("127.0.0.1:7890"), "本机地址应保留：{out}");
        assert!(out.contains("C:\\Users\\<用户>"));
    }
}
