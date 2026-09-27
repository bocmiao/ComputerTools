//! 诊断报告的脱敏。报告只在本地生成，由用户自己决定发不发。

use std::sync::LazyLock;

use regex::Regex;

// 地址不靠「单词边界」来找：紧挨着汉字（Unicode 的 \b 把汉字也算单词字符）、字母、下划线的地址
// （「我的IP是192.168.1.5」「IP192.168.1.5」「ip_10.0.0.1」）都要认出来。所以先找出整段数字和点、
// 整段十六进制数字和冒号，再判断这一段是不是地址。
static SID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"S-1-5-21-\d+-\d+-\d+(-\d+)?").unwrap());
static USER_DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)([A-Z]:\\Users\\)[^\\\s]+").unwrap());
/// 数字和点组成的一段：正好四段、每段 0–255 的是 IPv4；五段以上的是版本号，不动。
static DOTTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+(?:\.[0-9]+)+").unwrap());
/// 十六进制数字和冒号组成的一段：带「::」或者有 7 个冒号、至少 3 个十六进制数字的是 IPv6。
/// 「12:30:45」这种时间、「00:1a:…」这种 MAC 地址、代码里的「std::vector」都不算；「::1」是本机，不动。
static COLONED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9A-Fa-f:]*:[0-9A-Fa-f:]*").unwrap());
/// MAC 地址：六组两位十六进制数，用冒号或短横线隔开。前后不看边界（「MAC00-1A-…」也要认出来）。
static MAC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9A-Fa-f]{2}(?:[:-][0-9A-Fa-f]{2}){5}").unwrap());
static SERIAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(serial[_ ]?(number)?|序列号)\s*[:：=]\s*\S+").unwrap());
/// 用户自己写的问题描述里可能有的邮箱：最后一段必须是字母（「vue@3.4.21」这种版本号不算）。
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}").unwrap());
/// 大陆手机号：可以带 +86 / 86，可以用空格、短横线分成 3-4-4；前后不紧挨着别的数字。
static MOBILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^0-9])(?:\+?86[ -]?)?1[3-9][0-9](?:[ -]?[0-9]{4}){2}($|[^0-9])").unwrap());

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.len() <= 3 && p.parse::<u16>().is_ok_and(|n| n <= 255))
}

fn is_ipv6(s: &str) -> bool {
    let hex = s.chars().filter(char::is_ascii_hexdigit).count();
    (s.contains("::") || s.matches(':').count() == 7) && hex >= 3
}

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
    out = DOTTED
        .replace_all(&out, |c: &regex::Captures<'_>| {
            let run = &c[0];
            // 本机地址说明的是「本机有个代理软件」，不涉及隐私
            let local = run.starts_with("127.") || run == "0.0.0.0";
            if is_ipv4(run) && !local { "<IP>".to_owned() } else { run.to_owned() }
        })
        .into_owned();
    out = COLONED
        .replace_all(&out, |c: &regex::Captures<'_>| {
            let run = &c[0];
            if is_ipv6(run) { "<IP>".to_owned() } else { run.to_owned() }
        })
        .into_owned();
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

    /// 地址紧挨着字母、下划线也要认出来；手机号的各种写法也要去掉。版本号、时间、代码不能误伤。
    #[test]
    fn identifiers_glued_to_letters_and_other_phone_formats_are_redacted() {
        let out = redact(
            "IP192.168.1.5 ip_10.0.0.1 MAC00-1A-2B-3C-4D-5E IPv62409:8a55::1 地址192.168.1.9。\
             +8613812345678 138-1234-5678 138 1234 5678 +86 139 8765 4321",
            &[],
        );
        for leaked in ["192.168", "10.0.0.1", "00-1A", "2409", "8a55", "13812345678", "1234-5678", "1234 5678", "8765"]
        {
            assert!(!out.contains(leaked), "{leaked} 没被去掉：{out}");
        }
        let kept = "vue@3.4.21、node@18.2.0、std::vector、[System.IO.File]::Exists、12:30:45、10.0.26100.1742、\
                    本机 ::1、代理 127.0.0.1:7890、C:\\Windows、0.0.1";
        assert_eq!(redact(kept, &[]), kept);
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
