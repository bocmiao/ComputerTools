//! 由 Rust 实现的内置检测（`probe: { builtin: … }`）。
//! 名字要同时登记在 `catalog::BUILTIN_PROBES` 里。

use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use crate::platform::{
    Display, Displays, Hotkey, KeyboardAids, MouseSettings, OsInfo, Platform, WifiStatus, wifi_band, wifi_channel,
};

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
        "wifi-link" => env.platform.wifi_status().map(|w| wifi_link(&w)).map_err(|e| e.to_string()),
        "mouse-settings" => env.platform.mouse_settings().map(|m| mouse_settings(&m)).map_err(|e| e.to_string()),
        "hotkeys" => {
            let candidates = hotkey_candidates();
            env.platform.hotkeys_taken(&candidates).map(|t| hotkeys(&t, candidates.len())).map_err(|e| e.to_string())
        }
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

/// 双击时间短于这个（毫秒）算太快：两下点不了这么快，双击老是变成两次单击。默认 500，「鼠标属性」的滑块是 200 到 900；
/// 这两个界限是自己定的，只提很偏的设置。
const DOUBLE_CLICK_FAST_MS: u32 = 300;
/// 双击时间长于这个算太慢：隔一会儿的两次单击也算成双击，想选中文件却打开了。
const DOUBLE_CLICK_SLOW_MS: u32 = 800;
/// 指针速度 1–20，默认 10；这么慢、这么快才提（自己定的）。
const POINTER_SLOW: u32 = 3;
const POINTER_FAST: u32 = 18;

/// 鼠标的设置。按「最像鼠标坏了」的顺序报一项：左右键互换、滚轮不滚、单击锁定、自动跳到默认按钮、指针轨迹、双击太快太慢、
/// 指针太慢太快；都正常是 ok。事实里的 summary 把每一项都列出来。
fn mouse_settings(m: &MouseSettings) -> Value {
    let result = if m.swapped {
        "swapped"
    } else if m.wheel_lines == 0 {
        "wheel-off"
    } else if m.click_lock {
        "click-lock"
    } else if m.snap_to_default {
        "snap-to-default"
    } else if m.trails > 1 {
        "trails"
    } else if m.double_click_ms < DOUBLE_CLICK_FAST_MS {
        "double-click-fast"
    } else if m.double_click_ms > DOUBLE_CLICK_SLOW_MS {
        "double-click-slow"
    } else if m.speed <= POINTER_SLOW {
        "pointer-slow"
    } else if m.speed >= POINTER_FAST {
        "pointer-fast"
    } else {
        "ok"
    };
    let on = |b: bool| if b { "开" } else { "关" };
    let wheel = match m.wheel_lines {
        0 => "不滚动".to_owned(),
        u32::MAX => "一次滚一屏".to_owned(),
        n => format!("一次滚 {n} 行"),
    };
    let summary = format!(
        "主按钮是{}；双击速度 {} 毫秒（默认 500）；指针速度第 {} 档（共 20 档，默认第 10 档）；滚轮{wheel}；单击锁定{}；\
         自动移到默认按钮{}；指针轨迹{}；提高指针精确度{}",
        if m.swapped { "右键" } else { "左键" },
        m.double_click_ms,
        m.speed,
        on(m.click_lock),
        on(m.snap_to_default),
        on(m.trails > 1),
        on(m.enhance_precision),
    );
    json!({
        "result": result,
        "facts": {
            "summary": summary,
            "double_click_ms": m.double_click_ms,
            "speed": m.speed,
            "wheel": wheel,
            "trails": m.trails,
        }
    })
}

const VK_F1: u32 = 0x70;
const VK_F3: u32 = 0x72;
const VK_F4: u32 = 0x73;
const VK_F11: u32 = 0x7A;
const VK_F12: u32 = 0x7B;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;

/// 要试的全局快捷键：Ctrl、Alt、Shift 的每一种组合（7 种）配上字母、数字、F1～F11、四个方向键（Alt + F4 除外），再加上
/// 单独按的 F1～F11，一共 367 个。照 heathhenley/windows_hotkey_checker（MIT）的枚举。不试的：
/// - 带 Win 键的（微软：归操作系统用）；
/// - F12，单独按和带修饰键的都不试（微软 RegisterHotKey：F12 一直留给调试器；CI 154 的机器上 Shift + F12 就登记不上）；
/// - Alt + F4（Windows 自己关窗口的快捷键；CI 154 的 Windows Server 上登记不上，不知道是谁占的，不能说成「被别的程序占着」）；
/// - Print Screen 和 Tab、Esc 这些系统自己处理的键。
fn hotkey_candidates() -> Vec<Hotkey> {
    const MODIFIERS: [u32; 7] = [
        Hotkey::CONTROL,
        Hotkey::ALT,
        Hotkey::SHIFT,
        Hotkey::CONTROL | Hotkey::ALT,
        Hotkey::CONTROL | Hotkey::SHIFT,
        Hotkey::ALT | Hotkey::SHIFT,
        Hotkey::CONTROL | Hotkey::ALT | Hotkey::SHIFT,
    ];
    let keys: Vec<u32> = (u32::from(b'A')..=u32::from(b'Z'))
        .chain(u32::from(b'0')..=u32::from(b'9'))
        .chain(VK_F1..=VK_F11)
        .chain([VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN])
        .collect();
    let alt_f4 = Hotkey { modifiers: Hotkey::ALT, vk: VK_F4 };
    let mut out: Vec<Hotkey> = MODIFIERS
        .iter()
        .flat_map(|&modifiers| keys.iter().map(move |&vk| Hotkey { modifiers, vk }))
        .filter(|&h| h != alt_f4)
        .collect();
    out.extend((VK_F1..=VK_F11).map(|vk| Hotkey { modifiers: 0, vk }));
    out
}

/// 快捷键的写法：「Ctrl + Alt + A」「F1」「Ctrl + Alt + ←」，修饰键按 Ctrl、Alt、Shift 的顺序。
fn hotkey_name(h: Hotkey) -> String {
    let mut parts: Vec<String> = [(Hotkey::CONTROL, "Ctrl"), (Hotkey::ALT, "Alt"), (Hotkey::SHIFT, "Shift")]
        .iter()
        .filter(|(bit, _)| h.modifiers & bit != 0)
        .map(|(_, name)| (*name).to_owned())
        .collect();
    parts.push(match h.vk {
        VK_LEFT => "←".to_owned(),
        VK_UP => "↑".to_owned(),
        VK_RIGHT => "→".to_owned(),
        VK_DOWN => "↓".to_owned(),
        vk @ VK_F1..=VK_F12 => format!("F{}", vk - VK_F1 + 1),
        vk => char::from_u32(vk).map_or_else(|| format!("键 {vk}"), |c| c.to_string()),
    });
    parts.join(" + ")
}

/// 常见软件默认用的全局快捷键（只写查证过的）：占着它的不一定就是这个软件，只是一个线索。
fn hotkey_hint(h: Hotkey) -> Option<&'static str> {
    const CTRL_ALT: u32 = Hotkey::CONTROL | Hotkey::ALT;
    match (h.modifiers, h.vk) {
        (CTRL_ALT, 0x41) => Some("QQ 截图默认用的"),
        (Hotkey::ALT, 0x41) => Some("微信截图默认用的"),
        (CTRL_ALT, 0x57) => Some("微信「显示微信窗口」默认用的"),
        (m, 0x41) if m == Hotkey::CONTROL | Hotkey::SHIFT => Some("钉钉截图默认用的"),
        (0, VK_F1) => Some("Snipaste 截图默认用的"),
        (0, VK_F3) => Some("Snipaste 贴图默认用的"),
        (CTRL_ALT, VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN) => Some("英特尔显卡旋转屏幕的快捷键"),
        (Hotkey::ALT, 0x5A) => Some("NVIDIA 游戏内覆盖默认用的"),
        _ => None,
    }
}

/// 常用的快捷键里，有哪些被别的程序登记成了全局快捷键（按下去只有占着它的程序收得到，别的软件里就「没反应」）。
/// 看不出是哪个程序占的，常见软件的默认快捷键写在后面的括号里当线索。`checked` 是试了几个。
fn hotkeys(taken: &[Hotkey], checked: usize) -> Value {
    let list: Vec<String> = taken
        .iter()
        .map(|&h| match hotkey_hint(h) {
            Some(hint) => format!("{}（{hint}）", hotkey_name(h)),
            None => hotkey_name(h),
        })
        .collect();
    json!({
        "result": if taken.is_empty() { "ok" } else { "taken" },
        "facts": {
            "taken": list.join("、"),
            "taken_count": taken.len(),
            "checked": checked,
        }
    })
}

/// 信号质量低于这个就算弱：40 相当于 -80 dBm（0 是 -100 dBm，100 是 -50 dBm，按直线换算）。
const WEAK_SIGNAL: u8 = 40;

/// WiFi 连接情况。没有 WiFi 的（读不了、没有无线网卡、没连）是 na；连着的按「信号弱 → 老的加密方式 → 没有密码」的顺序报，
/// 都没有是 ok。事实里的 summary 是一句话（信号、频段和信道、WiFi 几代、连接速率、加密），没有 WiFi 名称。
fn wifi_link(w: &WifiStatus) -> Value {
    let Some(link) = w.link else {
        let result = if !w.service {
            "no-service"
        } else if w.adapters == 0 {
            "no-adapter"
        } else {
            "disconnected"
        };
        return json!({ "result": result, "facts": { "adapters": w.adapters } });
    };
    let band = link.frequency_mhz.and_then(wifi_band);
    let channel = link.channel.or_else(|| link.frequency_mhz.and_then(wifi_channel));
    let standard = wifi_standard(link.phy, band == Some("6 GHz"));
    let security = wifi_security(link.auth, link.cipher);
    let rate = link.rx_kbps.max(link.tx_kbps) / 1000;
    let result = if link.signal < WEAK_SIGNAL {
        "weak"
    } else if wifi_old_security(link.auth, link.cipher) {
        "old-security"
    } else if link.auth == 1 && link.cipher == 0 {
        "open"
    } else {
        "ok"
    };
    let signal_text = match link.signal {
        80.. => "很好",
        60..=79 => "好",
        40..=59 => "一般",
        _ => "弱",
    };
    let mut parts = vec![match link.rssi {
        Some(rssi) => format!("信号 {}%（{signal_text}，{rssi} dBm）", link.signal),
        None => format!("信号 {}%（{signal_text}）", link.signal),
    }];
    match (band, channel) {
        (Some(b), Some(c)) => parts.push(format!("{b} 第 {c} 信道")),
        (Some(b), None) => parts.push(b.to_owned()),
        (None, Some(c)) => parts.push(format!("第 {c} 信道")),
        (None, None) => {}
    }
    parts.extend(standard.map(str::to_owned));
    if rate > 0 {
        parts.push(format!("连接速率 {rate} Mbps"));
    }
    parts.push(format!("加密方式 {security}"));
    json!({
        "result": result,
        "facts": {
            "adapters": w.adapters,
            "signal": link.signal,
            "signal_text": signal_text,
            "rssi": link.rssi,
            "band": band.unwrap_or(""),
            "channel": channel,
            "standard": standard.unwrap_or(""),
            "rx_mbps": link.rx_kbps / 1000,
            "tx_mbps": link.tx_kbps / 1000,
            "security": security,
            "summary": parts.join("，"),
        }
    })
}

/// 物理层类型（DOT11_PHY_TYPE）→ 说给用户听的「WiFi 几代」。802.11ax 在 6 GHz 上是 WiFi 6E。
fn wifi_standard(phy: i32, six_ghz: bool) -> Option<&'static str> {
    match phy {
        11 => Some("WiFi 7"),
        10 if six_ghz => Some("WiFi 6E"),
        10 => Some("WiFi 6"),
        8 => Some("WiFi 5"),
        7 => Some("WiFi 4"),
        9 => Some("802.11ad"),
        6 => Some("802.11g"),
        5 => Some("802.11b"),
        4 => Some("802.11a"),
        _ => None,
    }
}

/// 身份验证（DOT11_AUTH_ALGORITHM）和加密（DOT11_CIPHER_ALGORITHM）→ 说给用户听的加密方式。
fn wifi_security(auth: i32, cipher: i32) -> String {
    let wep = matches!(cipher, 1 | 5 | 257);
    let name = match auth {
        1 if cipher == 0 => "没有密码",
        1 | 2 if wep => "WEP",
        2 => "WEP",
        10 => "增强型开放 OWE",
        3 => "WPA 企业版",
        4 => "WPA 个人版",
        6 => "WPA2 企业版",
        7 => "WPA2 个人版",
        9 => "WPA3 个人版",
        8 | 11 => "WPA3 企业版",
        _ => "其他",
    };
    let cipher_name = match cipher {
        4 | 10 => "（AES）",
        8 | 9 => "（GCMP）",
        2 => "（TKIP）",
        _ => "",
    };
    format!("{name}{cipher_name}")
}

/// 老的加密方式：WEP、TKIP、第一代 WPA。微软《Wi-Fi network not secure in Windows》说 WEP、TKIP 有已知的漏洞；
/// Intel《Data Rate Won't Exceed 54 Mbps When WEP or TKIP Encryption is Configured》：用它们时 802.11n 起的高速率用不了。
fn wifi_old_security(auth: i32, cipher: i32) -> bool {
    matches!(cipher, 1 | 2 | 5 | 257) || matches!(auth, 2..=4)
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

    fn wifi(signal: u8, mhz: Option<u32>, channel: Option<u32>, phy: i32, auth: i32, cipher: i32) -> WifiStatus {
        WifiStatus {
            service: true,
            adapters: 1,
            link: Some(crate::platform::WifiLink {
                signal,
                rssi: Some(-56),
                frequency_mhz: mhz,
                channel,
                phy,
                rx_kbps: 1_201_000,
                tx_kbps: 864_000,
                auth,
                cipher,
            }),
        }
    }

    #[test]
    fn wifi_link_is_told_in_one_sentence_without_the_network_name() {
        // WPA2 个人版（AES），5 GHz、WiFi 6、信号很好
        let v = wifi_link(&wifi(88, Some(5745), Some(149), 10, 7, 4));
        assert_eq!(v["result"], "ok");
        assert_eq!(
            v["facts"]["summary"],
            "信号 88%（很好，-56 dBm），5 GHz 第 149 信道，WiFi 6，连接速率 1201 Mbps，加密方式 WPA2 个人版（AES）"
        );
        assert_eq!(v["facts"]["rx_mbps"], 1201);
        assert_eq!(v["facts"]["tx_mbps"], 864);

        // 6 GHz 上的 802.11ax 是 WiFi 6E；网卡没报信道时按频率算
        let v = wifi_link(&wifi(70, Some(6115), None, 10, 9, 4));
        assert_eq!(v["result"], "ok");
        assert_eq!(v["facts"]["band"], "6 GHz");
        assert_eq!(v["facts"]["channel"], 33);
        assert_eq!(v["facts"]["standard"], "WiFi 6E");
        assert_eq!(v["facts"]["security"], "WPA3 个人版（AES）");

        // 信号弱先报（哪怕加密也老）；40 以下算弱
        let weak = wifi_link(&wifi(39, Some(2437), Some(6), 7, 7, 2));
        assert_eq!(weak["result"], "weak");
        assert_eq!(weak["facts"]["signal_text"], "弱");
        assert_eq!(wifi_link(&wifi(40, Some(2437), Some(6), 7, 7, 4))["result"], "ok");

        // WPA2 + TKIP、第一代 WPA、WEP 都算老的加密方式
        let tkip = wifi_link(&wifi(80, Some(2437), Some(6), 6, 7, 2));
        assert_eq!(tkip["result"], "old-security");
        assert_eq!(tkip["facts"]["security"], "WPA2 个人版（TKIP）");
        assert_eq!(tkip["facts"]["standard"], "802.11g");
        assert_eq!(wifi_link(&wifi(80, None, Some(6), 7, 4, 4))["result"], "old-security");
        let wep = wifi_link(&wifi(80, None, Some(6), 7, 1, 5));
        assert_eq!(wep["result"], "old-security");
        assert_eq!(wep["facts"]["security"], "WEP");

        // 没有密码的开放网络；增强型开放（OWE）有加密，不算
        let open = wifi_link(&wifi(80, Some(2412), Some(1), 7, 1, 0));
        assert_eq!(open["result"], "open");
        assert_eq!(open["facts"]["security"], "没有密码");
        assert_eq!(wifi_link(&wifi(80, Some(2412), Some(1), 7, 10, 4))["result"], "ok");

        // 读不到频率、信道、物理层类型、速率时，这几段不说
        let mut bare = wifi(65, None, None, 0, 7, 4);
        if let Some(link) = bare.link.as_mut() {
            link.rssi = None;
            link.rx_kbps = 0;
            link.tx_kbps = 0;
        }
        let v = wifi_link(&bare);
        assert_eq!(v["facts"]["summary"], "信号 65%（好），加密方式 WPA2 个人版（AES）");
        assert_eq!(v["facts"]["channel"], Value::Null);
        assert_eq!(v["facts"]["band"], "");

        // 没有 WiFi 的三种情况
        let none = |service, adapters| WifiStatus { service, adapters, link: None };
        assert_eq!(wifi_link(&none(false, 0))["result"], "no-service");
        assert_eq!(wifi_link(&none(true, 0))["result"], "no-adapter");
        assert_eq!(wifi_link(&none(true, 2))["result"], "disconnected");
    }

    #[test]
    fn mouse_settings_report_the_most_confusing_one_first() {
        let normal = MouseSettings::default();
        let v = mouse_settings(&normal);
        assert_eq!(v["result"], "ok");
        assert_eq!(
            v["facts"]["summary"],
            "主按钮是左键；双击速度 500 毫秒（默认 500）；指针速度第 10 档（共 20 档，默认第 10 档）；滚轮一次滚 3 行；\
             单击锁定关；自动移到默认按钮关；指针轨迹关；提高指针精确度开"
        );
        let with = |f: &dyn Fn(&mut MouseSettings)| {
            let mut m = normal;
            f(&mut m);
            mouse_settings(&m)["result"].as_str().unwrap_or_default().to_owned()
        };
        // 几项同时不对时先报左右键互换
        assert_eq!(
            with(&|m| {
                m.swapped = true;
                m.click_lock = true;
                m.trails = 7;
            }),
            "swapped"
        );
        assert_eq!(with(&|m| m.wheel_lines = 0), "wheel-off");
        assert_eq!(with(&|m| m.click_lock = true), "click-lock");
        assert_eq!(with(&|m| m.snap_to_default = true), "snap-to-default");
        assert_eq!(with(&|m| m.trails = 7), "trails");
        // 轨迹是 0 或者 1 都是关着
        assert_eq!(with(&|m| m.trails = 1), "ok");
        assert_eq!(with(&|m| m.double_click_ms = 200), "double-click-fast");
        assert_eq!(with(&|m| m.double_click_ms = 300), "ok");
        assert_eq!(with(&|m| m.double_click_ms = 900), "double-click-slow");
        assert_eq!(with(&|m| m.double_click_ms = 800), "ok");
        assert_eq!(with(&|m| m.speed = 1), "pointer-slow");
        assert_eq!(with(&|m| m.speed = 4), "ok");
        assert_eq!(with(&|m| m.speed = 20), "pointer-fast");
        assert_eq!(with(&|m| m.speed = 17), "ok");
        // 一次滚一屏不算坏
        let mut page = normal;
        page.wheel_lines = u32::MAX;
        let v = mouse_settings(&page);
        assert_eq!(v["result"], "ok");
        assert_eq!(v["facts"]["wheel"], "一次滚一屏");
    }

    #[test]
    fn hotkeys_are_tried_without_win_or_a_lone_f12_and_written_the_usual_way() {
        let all = hotkey_candidates();
        assert_eq!(all.len(), 7 * (26 + 10 + 11 + 4) - 1 + 11);
        let unique: std::collections::HashSet<Hotkey> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "不能重复");
        let ctrl_alt = Hotkey::CONTROL | Hotkey::ALT;
        assert!(all.contains(&Hotkey { modifiers: ctrl_alt, vk: u32::from(b'A') }));
        assert!(all.contains(&Hotkey { modifiers: 0, vk: VK_F1 }));
        assert!(all.iter().all(|h| h.vk != VK_F12), "F12 留给调试器，带修饰键的也不试");
        assert!(!all.contains(&Hotkey { modifiers: Hotkey::ALT, vk: VK_F4 }), "Alt + F4 是 Windows 自己关窗口的");
        assert!(all.contains(&Hotkey { modifiers: Hotkey::CONTROL, vk: VK_F4 }));
        assert!(all.iter().all(|h| h.modifiers & !(Hotkey::CONTROL | Hotkey::ALT | Hotkey::SHIFT) == 0), "不试 Win 键");
        assert!(
            all.iter().filter(|h| h.modifiers == 0).all(|h| (VK_F1..=VK_F11).contains(&h.vk)),
            "单独按的只有 F1～F11"
        );

        let name = |modifiers, vk| hotkey_name(Hotkey { modifiers, vk });
        assert_eq!(name(ctrl_alt, u32::from(b'A')), "Ctrl + Alt + A");
        assert_eq!(name(Hotkey::ALT | Hotkey::SHIFT, u32::from(b'1')), "Alt + Shift + 1");
        assert_eq!(name(Hotkey::SHIFT | Hotkey::CONTROL | Hotkey::ALT, VK_F12), "Ctrl + Alt + Shift + F12");
        assert_eq!(name(ctrl_alt, VK_LEFT), "Ctrl + Alt + ←");
        assert_eq!(name(0, VK_F1), "F1");

        let v = hotkeys(&[], all.len());
        assert_eq!(v["result"], "ok");
        assert_eq!(v["facts"]["taken"], "");
        assert_eq!(v["facts"]["checked"], 367);
        let v = hotkeys(
            &[
                Hotkey { modifiers: ctrl_alt, vk: u32::from(b'A') },
                Hotkey { modifiers: Hotkey::CONTROL | Hotkey::SHIFT, vk: u32::from(b'Q') },
                Hotkey { modifiers: ctrl_alt, vk: VK_DOWN },
            ],
            all.len(),
        );
        assert_eq!(v["result"], "taken");
        assert_eq!(v["facts"]["taken_count"], 3);
        assert_eq!(
            v["facts"]["taken"],
            "Ctrl + Alt + A（QQ 截图默认用的）、Ctrl + Shift + Q、Ctrl + Alt + ↓（英特尔显卡旋转屏幕的快捷键）"
        );
    }

    #[test]
    fn the_build_date_is_not_in_the_future() {
        // 构建日期晚于真实日期的话，所有用户都会被误报「时间不对」
        assert!(build_date() <= OffsetDateTime::now_utc().date().next_day().unwrap());
    }
}
