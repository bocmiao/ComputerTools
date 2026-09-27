//! 诊断报告的脱敏。报告只在本地生成，由用户自己决定发不发。

use std::sync::LazyLock;

use regex::Regex;

// 地址前后的边界用 ASCII 的 `(?-u:\b)`：Unicode 的 `\b` 把汉字也当成单词字符，
// 「我的IP是192.168.1.5」这样紧挨着汉字的地址就认不出来了。
static SID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"S-1-5-21-\d+-\d+-\d+(-\d+)?").unwrap());
static USER_DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([A-Z]:\\Users\\)[^\\\s]+").unwrap());
static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:\b)(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}(?-u:\b)").unwrap()
});
/// 带「::」缩写的 IPv6（完整写法的 8 组也算）。只认带「::」或 8 组的，免得把「12:30:45」这种时间当成地址。
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?-u:\b)(?:[0-9a-f]{1,4}:){1,7}:(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})?|(?-u:\b)[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){7}(?-u:\b)",
    )
    .unwrap()
});
static MAC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}(?-u:\b)").unwrap());
static SERIAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(serial[_ ]?(number)?|序列号)\s*[:：=]\s*\S+").unwrap());
/// 用户自己写的问题描述里可能有的邮箱和手机号（大陆手机号：前后不紧挨着别的数字）。
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+").unwrap());
static MOBILE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^0-9])1[3-9][0-9]{9}($|[^0-9])").unwrap());

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
    out = IPV6.replace_all(&out, "<IP>").into_owned();
    out = MAC.replace_all(&out, "<MAC>").into_owned();
    out = SERIAL.replace_all(&out, "${1}：<已隐藏>").into_owned();
    out = EMAIL.replace_all(&out, "<邮箱>").into_owned();
    // 两个号码共用一个分隔符时，一遍替换不完，再来一遍
    for _ in 0..2 {
        out = MOBILE.replace_all(&out, "${1}<手机号>${2}").into_owned();
    }
    out
}

/// 不区分大小写地整词替换：前后紧挨着字母或数字的不算，
/// 免得用户名叫「Win」「HP」时把「Windows 11」、硬盘型号也改坏。
fn replace_ignore_case(haystack: &str, needle: &str, with: &str) -> String {
    let re =
        Regex::new(&format!("(?i)(^|[^A-Za-z0-9]){}($|[^A-Za-z0-9])", regex::escape(needle))).expect("转义过的正则");
    let mut out = haystack.to_owned();
    // 相邻的两处共用一个分隔符时，一遍替换不完，再来一遍
    for _ in 0..2 {
        out = re.replace_all(&out, |c: &regex::Captures<'_>| format!("{}{with}{}", &c[1], &c[2])).into_owned();
    }
    out
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

    #[test]
    fn short_names_do_not_damage_other_words() {
        let out = redact("系统：Windows 11，用户 Win 的硬盘 HP SSD，win", &["Win".into(), "HP".into()]);
        assert!(out.contains("Windows 11"), "{out}");
        assert!(!out.contains(" Win ") && !out.ends_with("win"), "{out}");
        assert!(!out.contains("HP SSD"), "{out}");
    }

    #[test]
    fn ipv6_is_redacted_but_times_and_loopback_are_kept() {
        let out = redact(
            "地址 fe80::1c2b:3d4e:5f60:7a8b%12，2409:8a55:1234:5678:9abc:def0:1234:5678，时间 12:30:45，本机 ::1",
            &[],
        );
        for leaked in ["fe80::1c2b", "2409:8a55"] {
            assert!(!out.contains(leaked), "{leaked} 没被去掉：{out}");
        }
        assert!(out.contains("12:30:45") && out.contains("::1"), "{out}");
    }

    /// 用户自己写的描述里，地址常常紧挨着汉字；邮箱、手机号也要去掉。版本号、时间、容量不能误伤。
    #[test]
    fn identifiers_next_to_chinese_text_are_redacted() {
        let out = redact(
            "我的IP是192.168.1.5，路由器MAC是00-1A-2B-3C-4D-5E，IPv6地址2409:8a55::1，\
             邮箱xiaoming.li@qq.com，电话13812345678、13987654321，版本号26100，12:30:45，16 GB",
            &[],
        );
        for leaked in ["192.168.1.5", "00-1A-2B", "2409:8a55", "xiaoming", "13812345678", "13987654321"] {
            assert!(!out.contains(leaked), "{leaked} 没被去掉：{out}");
        }
        for kept in ["版本号26100", "12:30:45", "16 GB"] {
            assert!(out.contains(kept), "{kept} 不该被改：{out}");
        }
        assert!(out.contains("<邮箱>") && out.contains("<手机号>、<手机号>"), "{out}");
    }
}
