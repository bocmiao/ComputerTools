//! 「弹窗是哪个软件的」用到的三样，都只读：鼠标指着的窗口是哪个进程的、程序文件的版本信息、「应用和功能」里的程序。
//!
//! - 窗口：GetCursorPos + WindowFromPoint，再用 GetAncestor(GA_ROOT) 找到最外层的窗口（网页、广告内容常在别的进程的
//!   子窗口里，最外层的窗口才是弹出它的那个程序的）。看鼠标下面的窗口而不是激活的窗口：有的弹窗点了也不会被激活。
//!   应用商店的应用反过来：外框 ApplicationFrameWindow 是系统进程的，取鼠标下面那个子窗口的进程。
//! - 版本信息：GetFileVersionInfoW + VerQueryValueW，只读资源，不加载、不运行这个程序。优先读简体中文的那一份。
//! - 程序：卸载信息（HKLM 64 位、32 位两份，加上登录用户自己的），不列系统组件和补丁。

use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetClassNameW, GetCursorPos, GetWindowRect, GetWindowThreadProcessId, WindowFromPoint,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

use super::{FileStrings, InstalledProgram, PResult, PlatformError, PointedWindow, ScreenRect};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
/// 版本信息里没写有哪几种语言时，依次试这几种（简体中文、英语的两种代码页、不分语言）
const FALLBACK_TRANSLATIONS: &[&str] = &["080404b0", "040904b0", "040904e4", "000004b0"];

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn to_rect(r: RECT) -> ScreenRect {
    ScreenRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

pub fn pointed_window() -> PResult<Option<PointedWindow>> {
    let mut point = POINT::default();
    // SAFETY: point 是有效的输出位置
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(PlatformError::Other(format!("读不到鼠标的位置：{}", std::io::Error::last_os_error())));
    }
    // SAFETY: 只是查询
    let hit = unsafe { WindowFromPoint(point) };
    if hit.is_null() {
        return Ok(None);
    }
    // SAFETY: hit 是窗口句柄；窗口刚好关掉时下面的查询都会失败，不会出错
    let root = unsafe { GetAncestor(hit, GA_ROOT) };
    let window = if root.is_null() { hit } else { root };
    let mut pid = window_pid(window);
    if pid == 0 {
        // 窗口已经关了
        return Ok(None);
    }
    let class = class_name(window);
    // 应用商店的应用：外框（ApplicationFrameWindow）是系统的 ApplicationFrameHost 画的，里面的内容才是应用自己的进程
    if class == "ApplicationFrameWindow" {
        let inner = window_pid(hit);
        if inner != 0 {
            pid = inner;
        }
    }
    let mut rect = RECT::default();
    // SAFETY: rect 是有效的输出位置；失败时保持全 0
    unsafe { GetWindowRect(window, &mut rect) };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: 只是查询；找不到屏幕时取离它最近的那块
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    // SAFETY: info.cbSize 已经填好
    let screen = if !monitor.is_null() && unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        to_rect(info.rcWork)
    } else {
        ScreenRect::default()
    };
    Ok(Some(PointedWindow { pid, class, rect: to_rect(rect), screen, path: process_path(pid) }))
}

fn window_pid(window: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: pid 是有效的输出位置；窗口已经关了时返回 0
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    pid
}

fn class_name(window: HWND) -> String {
    let mut class = [0u16; 256];
    // SAFETY: 缓冲区的长度如实传入
    let len = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    String::from_utf16_lossy(&class[..usize::try_from(len).unwrap_or(0).min(class.len())])
}

/// 进程的程序文件的完整路径；进程已经退出、打不开（受保护的系统进程）时为 `None`。
fn process_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: 只查询信息；打开失败返回空句柄
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    // SAFETY: buf 有 len 个元素
    let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) } != 0;
    // SAFETY: handle 来自 OpenProcess，只关一次
    unsafe { CloseHandle(handle) };
    ok.then(|| PathBuf::from(OsString::from_wide(&buf[..(len as usize).min(buf.len())])))
}

/// VerQueryValueW 查到的那一段：在 `data` 里面的字节。查不到，或者指针不在 `data` 里时为 `None`。
fn query<'a>(data: &'a [u8], sub_block: &str, chars: bool) -> Option<&'a [u8]> {
    let sub = wide(OsStr::new(sub_block));
    let mut ptr: *mut c_void = null_mut();
    let mut len = 0u32;
    // SAFETY: data 是 GetFileVersionInfoW 填好的版本信息，sub 以 NUL 结尾；返回的指针指向 data 里面
    if unsafe { VerQueryValueW(data.as_ptr().cast(), sub.as_ptr(), &mut ptr, &mut len) } == 0 || ptr.is_null() {
        return None;
    }
    let start = (ptr as usize).checked_sub(data.as_ptr() as usize)?;
    // 字符串的长度按字符算，别的按字节算
    let bytes = if chars { (len as usize).checked_mul(2)? } else { len as usize };
    let end = start.checked_add(bytes)?.min(data.len());
    (start <= end).then(|| &data[start..end])
}

fn query_string(data: &[u8], sub_block: &str) -> Option<String> {
    let bytes = query(data, sub_block, true)?;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    let text = String::from_utf16_lossy(&units[..end]).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

pub fn file_strings(path: &Path) -> FileStrings {
    let name = wide(path.as_os_str());
    let mut ignored = 0u32;
    // SAFETY: name 以 NUL 结尾
    let size = unsafe { GetFileVersionInfoSizeW(name.as_ptr(), &mut ignored) };
    if size == 0 {
        return FileStrings::default();
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: data 有 size 个字节
    if unsafe { GetFileVersionInfoW(name.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return FileStrings::default();
    }
    // \VarFileInfo\Translation：一串（语言, 代码页），各两个字节。简体中文的排在前面
    let mut listed: Vec<(u16, String)> = query(&data, r"\VarFileInfo\Translation", false)
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|p| {
            let lang = u16::from_le_bytes([p[0], p[1]]);
            let page = u16::from_le_bytes([p[2], p[3]]);
            (lang, format!("{lang:04x}{page:04x}"))
        })
        .collect();
    listed.sort_by_key(|(lang, _)| *lang != 0x0804);
    let mut order: Vec<String> = listed.into_iter().map(|(_, t)| t).collect();
    for t in FALLBACK_TRANSLATIONS {
        if !order.iter().any(|o| o == t) {
            order.push((*t).to_owned());
        }
    }
    let get = |field: &str| order.iter().find_map(|t| query_string(&data, &format!(r"\StringFileInfo\{t}\{field}")));
    FileStrings { description: get("FileDescription"), company: get("CompanyName"), product: get("ProductName") }
}

/// 「应用和功能」里的程序。`user_sid` 是登录用户（小药箱以管理员身份运行，HKCU 不一定是他的）。
pub fn installed_programs(user_sid: Option<&str>) -> Vec<InstalledProgram> {
    let mut out = Vec::new();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        if let Ok(key) = machine.open_subkey_with_flags(UNINSTALL, KEY_READ | view) {
            read_entries(&key, &mut out);
        }
    }
    let user = match user_sid {
        Some(sid) => RegKey::predef(HKEY_USERS).open_subkey_with_flags(format!(r"{sid}\{UNINSTALL}"), KEY_READ),
        None => RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(UNINSTALL, KEY_READ),
    };
    if let Ok(key) = user {
        read_entries(&key, &mut out);
    }
    out
}

fn read_entries(key: &RegKey, out: &mut Vec<InstalledProgram>) {
    for name in key.enum_keys().flatten() {
        let Ok(entry) = key.open_subkey_with_flags(&name, KEY_READ) else {
            continue;
        };
        let text = |value: &str| {
            entry.get_value::<String, _>(value).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
        };
        let Some(display_name) = text("DisplayName") else {
            continue;
        };
        // 系统组件、补丁不在「应用和功能」里
        if entry.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1 || text("ParentKeyName").is_some() {
            continue;
        }
        out.push(InstalledProgram {
            name: display_name,
            publisher: text("Publisher"),
            install_location: text("InstallLocation"),
            display_icon: text("DisplayIcon"),
            uninstall_string: text("UninstallString"),
        });
    }
}
