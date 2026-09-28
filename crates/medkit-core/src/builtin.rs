//! 由 Rust 实现的内置检测（`probe: { builtin: … }`）。
//! 名字要同时登记在 `catalog::BUILTIN_PROBES` 里。

use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use crate::platform::{Display, Displays, KeyboardAids, OsInfo, Platform};

/// 内置检测能用到的信息。
pub struct Env<'a> {
    pub os: &'a OsInfo,
    /// 电脑上现在的时间（本机时区；读不到时区时是 UTC）
    pub now: OffsetDateTime,
    pub platform: &'a dyn Platform,
}

pub fn run(name: &str, env: &Env<'_>) -> Result<Value, String> {
    match name {
        "cpu-features" => Ok(cpu_features(env.os)),
        "clock" => Ok(clock(env.now.date(), build_date())),
        "keyboard-aids" => env.platform.keyboard_aids().map(keyboard_aids).map_err(|e| e.to_string()),
        "winsock" => env.platform.winsock_catalog().map(|c| crate::winsock::verdict(&c)).map_err(|e| e.to_string()),
        "display-resolution" => env.platform.displays().map(|d| display_resolution(&d)).map_err(|e| e.to_string()),
        _ => Err(format!("不认识的内置检测：{name}")),
    }
}

/// 键盘的辅助功能开着哪一项。几项都开着时按「最像键盘坏了」的顺序报：筛选键（短按全被忽略）、
/// 粘滞键（Shift、Ctrl 按一下就算「按着」）、鼠标键（小键盘不出数字）；三项的状态都记在事实里。
fn keyboard_aids(aids: KeyboardAids) -> Value {
    let result = if aids.filter_keys {
        "filter-keys"
    } else if aids.sticky_keys {
        "sticky-keys"
    } else if aids.mouse_keys {
        "mouse-keys"
    } else {
        "ok"
    };
    json!({
        "result": result,
        "facts": { "filter_keys": aids.filter_keys, "sticky_keys": aids.sticky_keys, "mouse_keys": aids.mouse_keys }
    })
}

/// 分辨率是不是显示器推荐的那一项。比推荐的低（宽或者高小一些，宽高比不一样的也算）时画面要拉伸，字和图标会发虚、
/// 变形，常见的是嫌字小把分辨率调低了、显卡驱动没装好；比推荐的高（显卡的「超级分辨率」）不算。几个屏幕显示同一个
/// 画面（复制）时分辨率只能选大家都支持的，另报一种结果；几个显示器都低于推荐的时，先报不在「复制」里的那个（改得了）。
/// 远程桌面里、读不到推荐的分辨率时不下结论。
/// 事实：count（显示器个数）、summary（每个显示器现在的分辨率）、displays（每个显示器一行）；
/// 有显示器不是推荐的分辨率时，还有第一个这样的显示器的 name、current、recommended。
fn display_resolution(d: &Displays) -> Value {
    let count = d.list.len();
    let low = d.list.iter().enumerate().filter(|(_, x)| below_preferred(x)).min_by_key(|(_, x)| x.cloned);
    let result = if d.remote {
        "remote"
    } else if count == 0 {
        "no-display"
    } else if let Some((_, x)) = low {
        if x.cloned { "cloned" } else { "not-recommended" }
    } else if d.list.iter().any(|x| x.preferred.is_some()) {
        "ok"
    } else {
        "unknown"
    };
    let size = |(w, h): (u32, u32)| format!("{w}×{h}");
    let rows: Vec<String> = d
        .list
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let recommended =
                x.preferred.map_or_else(|| "读不到推荐的分辨率".to_owned(), |p| format!("推荐 {}", size(p)));
            let cloned = if x.cloned { "，和别的屏幕显示同一个画面" } else { "" };
            format!("{}：{}（{recommended}{cloned}）", display_name(i, x, count), size((x.width, x.height)))
        })
        .collect();
    let summary: Vec<String> = d.list.iter().map(|x| size((x.width, x.height))).collect();
    let mut facts = json!({ "count": count, "summary": summary.join("、"), "displays": rows });
    if let Some((i, x)) = low {
        facts["name"] = json!(display_name(i, x, count));
        facts["current"] = json!(size((x.width, x.height)));
        facts["recommended"] = json!(x.preferred.map(size));
    }
    json!({ "result": result, "facts": facts })
}

/// 比推荐的分辨率低：宽或者高比推荐的小。
fn below_preferred(d: &Display) -> bool {
    d.preferred.is_some_and(|(w, h)| d.width < w || d.height < h)
}

/// 说给用户听的显示器名字：有型号名的用型号名，笔记本自带的屏幕说「电脑自带的屏幕」，都没有时按顺序叫。
fn display_name(index: usize, d: &Display, count: usize) -> String {
    match &d.name {
        Some(name) => format!("显示器「{name}」"),
        None if d.internal => "电脑自带的屏幕".to_owned(),
        None if count > 1 => format!("第 {} 个显示器", index + 1),
        None => "显示器".to_owned(),
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
    fn the_keyboard_aid_most_like_a_broken_keyboard_is_reported_first() {
        let aids = |filter_keys, sticky_keys, mouse_keys| KeyboardAids { filter_keys, sticky_keys, mouse_keys };
        assert_eq!(keyboard_aids(aids(false, false, false))["result"], "ok");
        assert_eq!(keyboard_aids(aids(true, true, true))["result"], "filter-keys");
        assert_eq!(keyboard_aids(aids(false, true, true))["result"], "sticky-keys");
        let v = keyboard_aids(aids(false, false, true));
        assert_eq!(v["result"], "mouse-keys");
        assert_eq!(v["facts"], json!({ "filter_keys": false, "sticky_keys": false, "mouse_keys": true }));
    }

    fn display(name: Option<&str>, internal: bool, now: (u32, u32), preferred: Option<(u32, u32)>) -> Display {
        Display { name: name.map(str::to_owned), internal, width: now.0, height: now.1, preferred, cloned: false }
    }

    fn displays(list: Vec<Display>) -> Displays {
        Displays { remote: false, list }
    }

    #[test]
    fn a_resolution_below_the_recommended_one_is_reported() {
        // 嫌字小把笔记本屏幕调到了 1366×768
        let v = display_resolution(&displays(vec![display(None, true, (1366, 768), Some((1920, 1080)))]));
        assert_eq!(v["result"], "not-recommended");
        assert_eq!(v["facts"]["name"], "电脑自带的屏幕");
        assert_eq!(v["facts"]["current"], "1366×768");
        assert_eq!(v["facts"]["recommended"], "1920×1080");
        assert_eq!(v["facts"]["count"], 1);
        assert_eq!(v["facts"]["displays"], json!(["电脑自带的屏幕：1366×768（推荐 1920×1080）"]));
        // 宽高比不一样（16:10 的屏幕用了 16:9 的分辨率）也算
        let v =
            display_resolution(&displays(vec![display(Some("DELL U2415"), false, (1920, 1080), Some((1920, 1200)))]));
        assert_eq!(v["result"], "not-recommended");
        assert_eq!(v["facts"]["name"], "显示器「DELL U2415」");
    }

    #[test]
    fn the_recommended_or_a_higher_resolution_is_fine() {
        let v = display_resolution(&displays(vec![
            display(None, true, (1920, 1080), Some((1920, 1080))),
            // 显卡的「超级分辨率」：比推荐的还高，不算问题
            display(Some("LG ULTRAGEAR"), false, (3840, 2160), Some((2560, 1440))),
        ]));
        assert_eq!(v["result"], "ok");
        assert_eq!(v["facts"]["summary"], "1920×1080、3840×2160");
        assert_eq!(v["facts"].get("name"), None);
        // 推荐的读不到的显示器不算，只要有一个读得到
        let v = display_resolution(&displays(vec![
            display(None, false, (1024, 768), None),
            display(None, false, (1920, 1080), Some((1920, 1080))),
        ]));
        assert_eq!(v["result"], "ok");
        assert_eq!(
            v["facts"]["displays"],
            json!(["第 1 个显示器：1024×768（读不到推荐的分辨率）", "第 2 个显示器：1920×1080（推荐 1920×1080）"])
        );
    }

    #[test]
    fn cloned_screens_remote_sessions_and_unknown_screens_are_told_apart() {
        // 笔记本接投影仪选了「复制」：投影仪推荐的分辨率更高，但只能用两个都支持的
        let mut laptop = display(None, true, (1920, 1080), Some((1920, 1080)));
        let mut projector = display(None, false, (1920, 1080), Some((3840, 2160)));
        laptop.cloned = true;
        projector.cloned = true;
        let v = display_resolution(&displays(vec![laptop, projector]));
        assert_eq!(v["result"], "cloned");
        assert_eq!(v["facts"]["name"], "第 2 个显示器");
        assert_eq!(v["facts"]["displays"][1], "第 2 个显示器：1920×1080（推荐 3840×2160，和别的屏幕显示同一个画面）");

        // 复制着的两个屏幕之外还有一个扩展出去的屏幕也低于推荐：先报这个能直接改好的
        let mut a = display(None, true, (1920, 1080), Some((1920, 1080)));
        let mut b = display(None, false, (1920, 1080), Some((3840, 2160)));
        a.cloned = true;
        b.cloned = true;
        let v = display_resolution(&displays(vec![
            a,
            b,
            display(Some("AOC 24G2"), false, (1280, 720), Some((1920, 1080))),
        ]));
        assert_eq!(v["result"], "not-recommended");
        assert_eq!(v["facts"]["name"], "显示器「AOC 24G2」");

        let mut remote = displays(vec![display(None, false, (1280, 720), Some((1920, 1080)))]);
        remote.remote = true;
        assert_eq!(display_resolution(&remote)["result"], "remote");
        assert_eq!(display_resolution(&displays(Vec::new()))["result"], "no-display");
        let v = display_resolution(&displays(vec![display(None, false, (1024, 768), None)]));
        assert_eq!(v["result"], "unknown");
        assert_eq!(v["facts"]["displays"], json!(["显示器：1024×768（读不到推荐的分辨率）"]));
    }

    #[test]
    fn the_build_date_is_not_in_the_future() {
        // 构建日期晚于真实日期的话，所有用户都会被误报「时间不对」
        assert!(build_date() <= OffsetDateTime::now_utc().date().next_day().unwrap());
    }
}
