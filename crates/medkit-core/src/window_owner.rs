//! 「弹窗是哪个软件的」：鼠标指着的那个窗口是哪个程序的，属于「应用和功能」里的哪个软件（工具箱里的同名工具）。
//!
//! 用 [`Platform::pointed_window`] 找窗口，[`Platform::file_strings`] 读程序文件里写的说明、公司，
//! [`Platform::installed_programs`] 按程序所在的文件夹对上装好的软件（卸载信息里的安装位置、图标、卸载程序
//! 在哪个文件夹）。只读：不关窗口，不结束程序。
//!
//! 看的是鼠标指着的窗口，不是当前激活的窗口：有的广告弹窗点了也不会被激活。结果里的路径把用户文件夹名换成 `*`
//! （完整路径里常有用户名），完整路径只交给外壳，用来「打开所在的文件夹」。

use std::path::PathBuf;

use crate::platform::{InstalledProgram, PResult, Platform, ScreenRect};
use crate::views::{WindowOwnerKind, WindowOwnerReport, WindowPosition};

/// 任务栏、桌面、通知区域的窗口类名
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
];

/// 显示通知的系统程序（右下角弹出来的通知是它画的，不管是哪个软件、哪个网站发的）
const NOTIFICATION_HOSTS: &[&str] = &["shellexperiencehost.exe"];

/// 找出鼠标指着的窗口是谁的。第二项是程序的完整路径（给「打开所在的文件夹」用），是小药箱自己、任务栏这些时为空。
pub fn find(platform: &dyn Platform) -> PResult<(WindowOwnerReport, Option<PathBuf>)> {
    let Some(window) = platform.pointed_window()? else {
        return Ok((report(WindowOwnerKind::Nothing), None));
    };
    let mut out = report(WindowOwnerKind::Unreadable);
    out.position = Some(position(&window.rect, &window.screen));
    out.width = window.rect.width();
    out.height = window.rect.height();
    if window.pid == std::process::id() {
        out.kind = WindowOwnerKind::Medkit;
        return Ok((out, None));
    }
    if SHELL_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&window.class)) {
        out.kind = WindowOwnerKind::Shell;
        return Ok((out, None));
    }
    let Some(path) = window.path else {
        return Ok((out, None));
    };
    let text = path.to_string_lossy().into_owned();
    let exe = file_name(&text);
    out.kind = if NOTIFICATION_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(&exe)) {
        WindowOwnerKind::Notification
    } else if in_windows_folder(&text) {
        WindowOwnerKind::System
    } else {
        WindowOwnerKind::Program
    };
    let strings = platform.file_strings(&path);
    out.description = strings.description;
    out.company = strings.company;
    out.product = strings.product;
    out.folder = parent(&text).map(|f| display_path(&f));
    if out.kind == WindowOwnerKind::Program {
        // 读不出装了哪些程序时，照样说出是哪个程序
        let programs = platform.installed_programs().unwrap_or_default();
        if let Some(p) = owning_program(&text, &programs) {
            out.installed = Some(p.name.clone());
            out.publisher = p.publisher.clone();
        } else if normalize(&text).contains("\\windowsapps\\") {
            // 应用商店的应用不在卸载信息里，它自己就是一个「已安装的应用」
            out.installed = out.product.clone().or_else(|| out.description.clone());
            out.publisher = out.company.clone();
        }
    }
    out.exe = Some(exe);
    Ok((out, Some(path)))
}

fn report(kind: WindowOwnerKind) -> WindowOwnerReport {
    WindowOwnerReport {
        kind,
        exe: None,
        description: None,
        company: None,
        product: None,
        folder: None,
        installed: None,
        publisher: None,
        position: None,
        width: 0,
        height: 0,
    }
}

/// 窗口在屏幕的哪一块：盖住工作区九成以上的算铺满，不然按窗口的中心落在九宫格的哪一格。
pub fn position(rect: &ScreenRect, screen: &ScreenRect) -> WindowPosition {
    let (sw, sh) = (i64::from(screen.width()), i64::from(screen.height()));
    if sw == 0 || sh == 0 {
        return WindowPosition::Center;
    }
    let ow = (i64::from(rect.right.min(screen.right)) - i64::from(rect.left.max(screen.left))).max(0);
    let oh = (i64::from(rect.bottom.min(screen.bottom)) - i64::from(rect.top.max(screen.top))).max(0);
    if ow * 10 >= sw * 9 && oh * 10 >= sh * 9 {
        return WindowPosition::Full;
    }
    let cx = (i64::from(rect.left) + i64::from(rect.right)) / 2 - i64::from(screen.left);
    let cy = (i64::from(rect.top) + i64::from(rect.bottom)) / 2 - i64::from(screen.top);
    let third = |c: i64, size: i64| {
        if c * 3 < size {
            0
        } else if c * 3 < size * 2 {
            1
        } else {
            2
        }
    };
    match (third(cy, sh), third(cx, sw)) {
        (0, 0) => WindowPosition::TopLeft,
        (0, 1) => WindowPosition::Top,
        (0, _) => WindowPosition::TopRight,
        (1, 0) => WindowPosition::Left,
        (1, 1) => WindowPosition::Center,
        (1, _) => WindowPosition::Right,
        (_, 0) => WindowPosition::BottomLeft,
        (_, 1) => WindowPosition::Bottom,
        _ => WindowPosition::BottomRight,
    }
}

/// 路径按反斜杠分开的各段（去掉空段）。
fn parts(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|p| !p.is_empty()).collect()
}

fn file_name(path: &str) -> String {
    parts(path).last().map_or_else(String::new, |s| (*s).to_owned())
}

fn parent(path: &str) -> Option<String> {
    let p = parts(path);
    (p.len() >= 2).then(|| p[..p.len() - 1].join("\\"))
}

fn is_drive(part: &str) -> bool {
    let b = part.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// 用户文件夹名换成 `*`：`C:\Users\张三\AppData\…` → `C:\Users\*\AppData\…`。
pub fn display_path(path: &str) -> String {
    let mut p: Vec<&str> = parts(path);
    if p.len() >= 3 && is_drive(p[0]) && p[1].eq_ignore_ascii_case("users") {
        p[2] = "*";
    }
    if p.len() == 1 && is_drive(p[0]) {
        return format!("{}\\", p[0]);
    }
    p.join("\\")
}

/// Windows 文件夹（`%SystemRoot%`，一般是 C:\Windows）。
fn windows_folder() -> String {
    std::env::var("SystemRoot").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "C:\\Windows".to_owned())
}

fn in_windows_folder(path: &str) -> bool {
    under(&normalize(path), &normalize(&windows_folder()))
}

/// 比较用：去掉引号和首尾空白，统一成反斜杠、小写，去掉末尾的反斜杠。
fn normalize(path: &str) -> String {
    parts(path.trim().trim_matches('"')).join("\\").to_lowercase()
}

fn under(path: &str, folder: &str) -> bool {
    path.len() > folder.len() && path.starts_with(folder) && path.as_bytes()[folder.len()] == b'\\'
}

fn is_absolute(path: &str) -> bool {
    parts(path).first().is_some_and(|p| is_drive(p))
}

/// 卸载信息里的一个路径（可能带引号、带图标编号 `,0`、带参数）里的程序文件路径。
fn program_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let path = if let Some(rest) = raw.strip_prefix('"') {
        rest.split('"').next().unwrap_or("").to_owned()
    } else {
        // 没有引号：到 .exe 为止（后面是参数），不是 exe 的去掉图标编号
        let lower = raw.to_ascii_lowercase();
        match lower.find(".exe") {
            Some(i) => raw[..i + 4].to_owned(),
            None => match raw.rsplit_once(',') {
                Some((head, tail)) if tail.trim().trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => {
                    head.to_owned()
                }
                _ => raw.to_owned(),
            },
        }
    };
    is_absolute(&path).then_some(path)
}

/// 一个软件的程序可能在的文件夹：安装位置、图标所在的文件夹、卸载程序所在的文件夹。
pub fn program_folders(p: &InstalledProgram) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(loc) = p.install_location.as_deref() {
        let loc = loc.trim().trim_matches('"');
        if is_absolute(loc) {
            out.push(normalize(loc));
        }
    }
    for raw in [p.display_icon.as_deref(), p.uninstall_string.as_deref()].into_iter().flatten() {
        if let Some(folder) = program_path(raw).and_then(|path| parent(&path)) {
            out.push(normalize(&folder));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 太宽的文件夹（Program Files、用户的 AppData、Windows 文件夹这些）：里面装着各种软件，不能拿来认是哪个软件的。
/// `folder` 是 [`program_folders`] 给的样子（小写、反斜杠）。
pub fn is_generic(folder: &str) -> bool {
    if under(folder, &normalize(&windows_folder())) || folder == normalize(&windows_folder()) {
        return true;
    }
    let p = parts(folder);
    match p.as_slice() {
        [] | [_] => true,
        [_, top] => ["program files", "program files (x86)", "programdata", "users"].contains(top),
        [_, top, "common files"] if ["program files", "program files (x86)"].contains(top) => true,
        [_, "users", _] => true,
        [_, "users", _, rest @ ..] => {
            let rest = rest.join("\\");
            [
                "appdata",
                "appdata\\local",
                "appdata\\roaming",
                "appdata\\locallow",
                "appdata\\local\\programs",
                "appdata\\local\\temp",
                "desktop",
                "downloads",
                "documents",
            ]
            .contains(&rest.as_str())
        }
        _ => false,
    }
}

/// 程序属于哪个装好的软件：它在哪个软件的文件夹里（有好几个时取最具体的那个）。
pub fn owning_program<'a>(exe: &str, programs: &'a [InstalledProgram]) -> Option<&'a InstalledProgram> {
    let exe = normalize(exe);
    let mut best: Option<(&InstalledProgram, usize)> = None;
    for p in programs {
        if p.name.trim().is_empty() {
            continue;
        }
        for folder in program_folders(p) {
            if is_generic(&folder) || !under(&exe, &folder) {
                continue;
            }
            if best.is_none_or(|(_, len)| folder.len() > len) {
                best = Some((p, folder.len()));
            }
        }
    }
    best.map(|(p, _)| p)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::platform::FileStrings;
    use crate::platform::PointedWindow;
    use crate::platform::mock::MockPlatform;

    const SCREEN: ScreenRect = ScreenRect { left: 0, top: 0, right: 1920, bottom: 1040 };

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect { left, top, right, bottom }
    }

    fn window(pid: u32, class: &str, path: Option<&str>) -> PointedWindow {
        PointedWindow {
            pid,
            class: class.into(),
            rect: rect(1560, 780, 1920, 1040),
            screen: SCREEN,
            path: path.map(PathBuf::from),
        }
    }

    fn program(name: &str, location: Option<&str>, icon: Option<&str>, uninstall: Option<&str>) -> InstalledProgram {
        InstalledProgram {
            name: name.into(),
            publisher: Some(format!("{name} 公司")),
            install_location: location.map(Into::into),
            display_icon: icon.map(Into::into),
            uninstall_string: uninstall.map(Into::into),
        }
    }

    #[test]
    fn positions_on_the_screen() {
        assert_eq!(position(&rect(1560, 780, 1920, 1040), &SCREEN), WindowPosition::BottomRight);
        assert_eq!(position(&rect(0, 0, 300, 200), &SCREEN), WindowPosition::TopLeft);
        assert_eq!(position(&rect(760, 320, 1160, 720), &SCREEN), WindowPosition::Center);
        assert_eq!(position(&rect(-8, -8, 1928, 1048), &SCREEN), WindowPosition::Full);
        assert_eq!(position(&rect(0, 0, 1920, 1080), &SCREEN), WindowPosition::Full);
        assert_eq!(position(&rect(800, 900, 1100, 1040), &SCREEN), WindowPosition::Bottom);
        // 第二块屏幕在右边
        let second = rect(1920, 0, 3840, 1040);
        assert_eq!(position(&rect(3500, 800, 3840, 1040), &second), WindowPosition::BottomRight);
        assert_eq!(position(&rect(1920, 400, 2200, 600), &second), WindowPosition::Left);
        assert_eq!(position(&rect(0, 0, 10, 10), &ScreenRect::default()), WindowPosition::Center);
    }

    #[test]
    fn user_folder_names_are_hidden() {
        assert_eq!(
            display_path(r"C:\Users\张三\AppData\Roaming\2345Soft\Pic"),
            r"C:\Users\*\AppData\Roaming\2345Soft\Pic"
        );
        assert_eq!(display_path(r"d:\users\bob"), r"d:\users\*");
        assert_eq!(display_path(r"C:\Program Files (x86)\Tencent\QQ\Bin"), r"C:\Program Files (x86)\Tencent\QQ\Bin");
        assert_eq!(display_path(r"C:\"), r"C:\");
    }

    #[test]
    fn folders_come_from_the_uninstall_information() {
        let p = program(
            "看图王",
            Some(r#""C:\Program Files (x86)\2345Soft\2345Pic\""#),
            Some(r#""C:\Program Files (x86)\2345Soft\2345Pic\2345PicViewer.exe",0"#),
            Some(r#""C:\Program Files (x86)\2345Soft\2345Pic\Uninstall.exe" /S"#),
        );
        assert_eq!(program_folders(&p), vec![r"c:\program files (x86)\2345soft\2345pic".to_owned()]);
        let p =
            program("压缩", None, Some(r"C:\Tools\Zip\zip.exe,-101"), Some(r"C:\Tools\Zip\Setup\uninst.exe /quiet"));
        assert_eq!(program_folders(&p), vec![r"c:\tools\zip".to_owned(), r"c:\tools\zip\setup".to_owned()]);
        // MsiExec 的卸载命令没有路径，图标是 ico 文件
        let p = program("办公", None, Some(r"C:\ProgramData\Office\icon.ico"), Some("MsiExec.exe /X{1234}"));
        assert_eq!(program_folders(&p), vec![r"c:\programdata\office".to_owned()]);
        assert!(program_folders(&program("空的", Some(""), Some("  "), None)).is_empty());
    }

    #[test]
    fn the_most_specific_program_wins() {
        let programs = vec![
            program("腾讯全家", Some(r"C:\Program Files (x86)\Tencent"), None, None),
            program("QQ", Some(r"C:\Program Files (x86)\Tencent\QQ"), None, None),
            program("装在 Program Files 根上的", Some(r"C:\Program Files (x86)"), None, None),
            program("用系统卸载程序的", None, Some(r"C:\Windows\System32\msiexec.exe"), None),
            program("", Some(r"C:\Program Files (x86)\Tencent\QQ\Bin"), None, None),
            program("用户自己装的", Some(r"C:\Users\张三\AppData\Local\Programs\Tool"), None, None),
            program("AppData 根上的", Some(r"C:\Users\张三\AppData\Roaming"), None, None),
        ];
        let find = |exe: &str| owning_program(exe, &programs).map(|p| p.name.as_str());
        assert_eq!(find(r"C:\Program Files (x86)\Tencent\QQ\Bin\QQ.exe"), Some("QQ"));
        assert_eq!(find(r"c:\program files (x86)\tencent\qqpcmgr\tray.exe"), Some("腾讯全家"));
        assert_eq!(find(r"C:\Program Files (x86)\Other\other.exe"), None);
        assert_eq!(find(r"C:\Windows\System32\notepad.exe"), None);
        assert_eq!(find(r"C:\Users\张三\AppData\Local\Programs\Tool\tool.exe"), Some("用户自己装的"));
        assert_eq!(find(r"C:\Users\张三\AppData\Roaming\Ads\popup.exe"), None);
        // 文件夹名只是前缀相同的不算
        assert_eq!(find(r"C:\Program Files (x86)\Tencent\QQMusic\music.exe"), Some("腾讯全家"));
    }

    #[test]
    fn a_program_window_is_described() {
        let mock = MockPlatform::new();
        let exe = r"C:\Users\张三\AppData\Roaming\2345Soft\News\2345News.exe";
        mock.point_at(Some(window(4321, "PopupWnd", Some(exe))));
        mock.set_file_strings(
            Path::new(exe),
            FileStrings {
                description: Some("热点资讯".into()),
                company: Some("上海二三四五网络科技有限公司".into()),
                product: Some("2345看图王".into()),
            },
        );
        mock.set_programs(vec![program("2345看图王", Some(r"C:\Users\张三\AppData\Roaming\2345Soft"), None, None)]);
        let (r, path) = find(&mock).unwrap();
        assert_eq!(r.kind, WindowOwnerKind::Program);
        assert_eq!(r.exe.as_deref(), Some("2345News.exe"));
        assert_eq!(r.description.as_deref(), Some("热点资讯"));
        assert_eq!(r.company.as_deref(), Some("上海二三四五网络科技有限公司"));
        assert_eq!(r.folder.as_deref(), Some(r"C:\Users\*\AppData\Roaming\2345Soft\News"));
        assert_eq!(r.installed.as_deref(), Some("2345看图王"));
        assert_eq!(r.publisher.as_deref(), Some("2345看图王 公司"));
        assert_eq!(r.position, Some(WindowPosition::BottomRight));
        assert_eq!((r.width, r.height), (360, 260));
        assert_eq!(path, Some(PathBuf::from(exe)));
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("张三"), "结果里不能有用户名：{json}");
        assert!(json.contains("\"kind\":\"program\"") && json.contains("\"position\":\"bottom-right\""), "{json}");
    }

    #[test]
    fn windows_itself_the_shell_and_medkit() {
        let mock = MockPlatform::new();
        mock.set_programs(vec![program("用系统文件夹的", Some(r"C:\Windows\SystemApps"), None, None)]);
        let host = r"C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe";
        mock.point_at(Some(window(100, "Windows.UI.Core.CoreWindow", Some(host))));
        let (r, path) = find(&mock).unwrap();
        assert_eq!(r.kind, WindowOwnerKind::Notification);
        assert_eq!(r.installed, None);
        assert!(path.is_some());

        mock.point_at(Some(window(200, "Notepad", Some(r"C:\Windows\System32\notepad.exe"))));
        let (r, _) = find(&mock).unwrap();
        assert_eq!(r.kind, WindowOwnerKind::System);
        assert_eq!(r.installed, None);

        mock.point_at(Some(window(300, "Shell_TrayWnd", Some(r"C:\Windows\explorer.exe"))));
        let (r, path) = find(&mock).unwrap();
        assert_eq!((r.kind, r.exe, path), (WindowOwnerKind::Shell, None, None));

        mock.point_at(Some(window(std::process::id(), "Tauri Window", Some(r"C:\Tools\medkit.exe"))));
        let (r, path) = find(&mock).unwrap();
        assert_eq!((r.kind, path), (WindowOwnerKind::Medkit, None));

        mock.point_at(Some(window(400, "Chrome_WidgetWin_1", None)));
        let (r, path) = find(&mock).unwrap();
        assert_eq!((r.kind, r.exe, path), (WindowOwnerKind::Unreadable, None, None));
        assert_eq!(r.position, Some(WindowPosition::BottomRight));

        mock.point_at(None);
        let (r, path) = find(&mock).unwrap();
        assert_eq!((r.kind, r.position, path), (WindowOwnerKind::Nothing, None, None));
    }

    #[test]
    fn a_store_app_is_its_own_installed_app() {
        let mock = MockPlatform::new();
        let exe = r"C:\Program Files\WindowsApps\Example.News_1.0.0.0_x64__abc\News.exe";
        mock.point_at(Some(window(600, "Windows.UI.Core.CoreWindow", Some(exe))));
        mock.set_file_strings(
            Path::new(exe),
            FileStrings {
                description: Some("资讯".into()),
                company: Some("示例公司".into()),
                product: Some("示例资讯".into()),
            },
        );
        let (r, _) = find(&mock).unwrap();
        assert_eq!(r.kind, WindowOwnerKind::Program);
        assert_eq!(r.installed.as_deref(), Some("示例资讯"));
        assert_eq!(r.publisher.as_deref(), Some("示例公司"));
    }

    #[test]
    fn a_program_that_is_not_installed_is_still_named() {
        let mock = MockPlatform::new();
        let exe = r"D:\绿色软件\Tool\tool.exe";
        mock.point_at(Some(window(500, "ToolWnd", Some(exe))));
        let (r, _) = find(&mock).unwrap();
        assert_eq!(r.kind, WindowOwnerKind::Program);
        assert_eq!(r.exe.as_deref(), Some("tool.exe"));
        assert_eq!(r.folder.as_deref(), Some(r"D:\绿色软件\Tool"));
        assert_eq!((r.installed, r.description), (None, None));
    }
}
