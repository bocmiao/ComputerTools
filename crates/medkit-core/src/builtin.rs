//! 由 Rust 实现的内置检测（`probe: { builtin: … }`）。
//! 名字要同时登记在 `catalog::BUILTIN_PROBES` 里。

use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use crate::platform::OsInfo;

/// 内置检测能用到的信息。
pub struct Env<'a> {
    pub os: &'a OsInfo,
    /// 电脑上现在的时间（本机时区；读不到时区时是 UTC）
    pub now: OffsetDateTime,
}

pub fn run(name: &str, env: &Env<'_>) -> Result<Value, String> {
    match name {
        "cpu-features" => Ok(cpu_features(env.os)),
        "clock" => Ok(clock(env.now.date(), build_date())),
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

/// 本地构建（没设 SOURCE_DATE_EPOCH）时当作构建日期的日子。发版前顺手改成最近的日期；
/// 只要不晚于真实日期就不会误报，旧一点只是查得没那么灵。
const FALLBACK_BUILD_DATE: Date = time::macros::date!(2026 - 09 - 27);

/// 这个版本的构建日期。CI 构建安装包时把 SOURCE_DATE_EPOCH 设成提交时间（可复现构建的通用约定）。
fn build_date() -> Date {
    option_env!("SOURCE_DATE_EPOCH")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok())
        .map_or(FALLBACK_BUILD_DATE, OffsetDateTime::date)
}

/// 电脑上的日期比这个版本的构建日期还早，时间肯定错了：多半是主板上的纽扣电池没电，
/// 关机以后时间回到了出厂时的日子。留一天余量，免得时区不同时误报。
/// 时间快了查不出来（不联网没有参照），所以正常时只说「看起来是对的」。
fn clock(today: Date, built: Date) -> Value {
    let floor = built.previous_day().unwrap_or(built);
    json!({
        "result": if today < floor { "behind" } else { "ok" },
        "facts": { "today": iso_date(today), "build_date": iso_date(built) }
    })
}

fn iso_date(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

#[cfg(test)]
mod tests {
    use time::macros::date;

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

    #[test]
    fn a_clock_earlier_than_the_build_is_wrong() {
        let built = date!(2026 - 09 - 27);
        let v = clock(date!(2009 - 01 - 01), built);
        assert_eq!(v["result"], "behind");
        assert_eq!(v["facts"]["today"], "2009-01-01");
        assert_eq!(v["facts"]["build_date"], "2026-09-27");
        // 时区不同，差一天不算错
        assert_eq!(clock(date!(2026 - 09 - 26), built)["result"], "ok");
        assert_eq!(clock(date!(2026 - 09 - 25), built)["result"], "behind");
        assert_eq!(clock(date!(2031 - 01 - 01), built)["result"], "ok");
    }

    #[test]
    fn the_build_date_is_not_in_the_future() {
        // 构建日期晚于真实日期的话，所有用户都会被误报「时间不对」
        assert!(build_date() <= OffsetDateTime::now_utc().date().next_day().unwrap());
    }
}
