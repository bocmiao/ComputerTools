//! 由 Rust 实现的内置检测（`probe: { builtin: … }`）。
//! 名字要同时登记在 `catalog::BUILTIN_PROBES` 里。

use serde_json::{Value, json};

use crate::platform::OsInfo;

pub fn run(name: &str, os: &OsInfo) -> Result<Value, String> {
    match name {
        "cpu-features" => Ok(cpu_features(os)),
        _ => Err(format!("不认识的内置检测：{name}")),
    }
}

/// 已经装了 Windows 11（版本号 22000 起；服务器版不算）。
fn is_windows11(os: &OsInfo) -> bool {
    os.build >= 22000 && !os.edition_id.starts_with("Server")
}

/// Win11 24H2 起，CPU 必须支持 POPCNT 和 SSE4.2，否则即使绕过检查也装不上、开不了机。
fn cpu_features(os: &OsInfo) -> Value {
    let windows11 = is_windows11(os);
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let popcnt = std::arch::is_x86_feature_detected!("popcnt");
        let sse42 = std::arch::is_x86_feature_detected!("sse4.2");
        json!({
            "result": cpu_verdict(popcnt && sse42, windows11),
            "facts": { "popcnt": popcnt, "sse42": sse42, "windows11": windows11 }
        })
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // ARM 电脑没有这两条 x86 指令的问题。
        json!({
            "result": "ok",
            "facts": { "popcnt": true, "sse42": true, "windows11": windows11, "arch": std::env::consts::ARCH }
        })
    }
}

/// 缺指令只在已经装了 Windows 11 的电脑上算问题（以后的大版本更新装不上）；
/// Windows 10 上只影响以后能不能升级，用带 -win10 的结果代码，只做提示。
#[cfg_attr(not(any(target_arch = "x86", target_arch = "x86_64")), allow(dead_code))]
fn cpu_verdict(supported: bool, windows11: bool) -> &'static str {
    match (supported, windows11) {
        (true, _) => "ok",
        (false, true) => "missing",
        (false, false) => "missing-win10",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(build: u32, edition_id: &str) -> OsInfo {
        OsInfo {
            caption: String::new(),
            build,
            display_version: String::new(),
            edition_id: edition_id.to_owned(),
            edition: None,
            computer_name: String::new(),
        }
    }

    #[test]
    fn windows11_is_told_apart_from_windows10_and_server() {
        assert!(is_windows11(&os(26100, "CoreCountrySpecific")));
        assert!(!is_windows11(&os(19045, "Professional")));
        assert!(!is_windows11(&os(26100, "ServerDatacenter")));
    }

    #[test]
    fn missing_instructions_only_matter_on_windows11() {
        assert_eq!(cpu_verdict(true, true), "ok");
        assert_eq!(cpu_verdict(true, false), "ok");
        assert_eq!(cpu_verdict(false, true), "missing");
        assert_eq!(cpu_verdict(false, false), "missing-win10");
    }
}
