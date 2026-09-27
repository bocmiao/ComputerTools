//! U 盘里的文件不见了：U 盘病毒（Autorun 一类）把原来的文件、文件夹设成「隐藏」和「系统」属性（有的还加上「只读」），
//! 再放一堆同名的快捷方式，双击快捷方式就会运行病毒。文件其实都还在，只是看不见（「显示隐藏的文件」也看不见带「系统」
//! 属性的）。这里把它们找出来、显示回来：
//! - 文件夹只能由后端的系统选择框取得；Windows 所在的盘（系统盘）不给用，那里的隐藏文件大多是系统自己的；
//! - 列出所选文件夹里直接的、带「隐藏」或「系统」属性的文件和文件夹（回收站、System Volume Information、desktop.ini
//!   这类系统自己的不列），文件夹里一共有多少东西、藏起来多少也数出来；
//! - 程序和脚本文件（.exe、.vbs、.js、.bat、autorun.inf 这类）多半就是病毒本身：不给显示出来，单独列出来提醒别点；
//! - 快捷方式：和被藏起来的东西同名的，或者里面写着 cmd、wscript、rundll32、powershell、mshta 这类程序的，列为「病毒放的
//!   快捷方式」，只提醒、不删；
//! - 「显示出来」：勾选的文件、文件夹（连同文件夹里面的一切）去掉「隐藏」「系统」属性，原来带着这两个之一的也去掉「只读」；
//!   程序和脚本文件照样藏着。每一处原来的属性都记下来，能一键改回去。不跟着符号链接、目录联接走；最多改 MAX_CHANGES 处、
//!   看 MAX_VISITS 个文件和文件夹。
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_HIDDEN: u32 = 0x2;
pub const ATTR_SYSTEM: u32 = 0x4;
pub const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_NORMAL: u32 = 0x80;
const ATTR_REPARSE_POINT: u32 = 0x400;
/// SetFileAttributesW 能设的：只读、隐藏、系统、存档、临时、脱机、不建索引、网盘的「始终保留」「仅在线」
const SETTABLE: u32 = 0x1 | 0x2 | 0x4 | 0x20 | 0x100 | 0x1000 | 0x2000 | 0x80000 | 0x100000;

/// 「显示出来」最多改多少处、最多看多少个文件和文件夹
pub const MAX_CHANGES: usize = 100_000;
const MAX_VISITS: usize = 1_000_000;
/// 数文件夹里面有多少东西时，最多数多少个、多少秒
const COUNT_LIMIT: usize = 100_000;
const COUNT_TIME: Duration = Duration::from_secs(10);
/// 快捷方式最多读多少字节来看它指向什么
const LNK_READ: u64 = 64 * 1024;

/// 程序和脚本：病毒本身多半是这些，不给显示出来
const PROGRAM_EXTENSIONS: &[&str] = &[
    "exe", "com", "scr", "pif", "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "wsh", "ps1", "hta", "dll", "cpl",
    "msi", "jar", "lnk", "inf", "reg",
];
/// 系统自己的隐藏文件和文件夹（名字，小写）：不列、不动
const SYSTEM_NAMES: &[&str] =
    &["system volume information", "$recycle.bin", "recycler", "recycled", "desktop.ini", "thumbs.db", "$windows.~bt"];
/// 快捷方式里出现这些字（不分大小写），就是在偷偷运行程序
const SUSPICIOUS_TARGETS: &[&str] = &[
    "cmd.exe",
    "wscript",
    "cscript",
    "rundll32",
    "powershell",
    "mshta",
    "regsvr32",
    ".vbs",
    ".vbe",
    ".jse",
    ".wsf",
    ".bat",
    ".cmd",
];

/// 读、写文件属性。测试里换成假的。
pub trait Attrs {
    fn get(&self, path: &Path) -> Option<u32>;
    fn set(&self, path: &Path, value: u32) -> std::io::Result<()>;
}

/// 真的文件系统：Windows 上是 GetFileAttributesW / SetFileAttributesW（在 medkit-core 里，Windows 测试会真的设一遍），
/// 别的系统上没有这些属性。
pub struct SystemAttrs;

#[cfg(windows)]
impl Attrs for SystemAttrs {
    fn get(&self, path: &Path) -> Option<u32> {
        medkit_core::platform::windows::file_attributes(path)
    }

    fn set(&self, path: &Path, value: u32) -> std::io::Result<()> {
        medkit_core::platform::windows::set_file_attributes(path, value)
    }
}

#[cfg(not(windows))]
impl Attrs for SystemAttrs {
    fn get(&self, path: &Path) -> Option<u32> {
        let meta = fs::symlink_metadata(path).ok()?;
        Some(if meta.is_dir() { ATTR_DIRECTORY } else { ATTR_NORMAL })
    }

    fn set(&self, _path: &Path, _value: u32) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "只有在 Windows 上才能改文件属性"))
    }
}

#[derive(Debug, Default)]
pub struct HiddenState {
    /// 用户选的文件夹
    pub root: Option<PathBuf>,
    /// 最近一次列出的项目，编号就是下标
    pub shown: Vec<PathBuf>,
    /// 上一次「显示出来」改了哪些、原来的属性（撤销用）
    pub changed: Vec<(PathBuf, u32)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenReport {
    /// 选的文件夹（用户自己选的，显示给他看）
    pub folder: String,
    /// 被藏起来的文件和文件夹（能显示出来的）
    pub items: Vec<HiddenItem>,
    /// 被藏起来的程序和脚本文件：多半是病毒，不给显示出来
    pub programs: Vec<String>,
    /// 病毒放的快捷方式
    pub shortcuts: Vec<String>,
    /// 上一次「显示出来」改了多少处，能撤销
    pub can_undo: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenItem {
    /// 「显示出来」时传回来
    pub id: usize,
    pub name: String,
    pub is_dir: bool,
    /// 文件的大小（文件夹是 0）
    pub size: u64,
    /// 文件夹里一共有多少个文件和文件夹
    pub inside: usize,
    /// 其中藏起来的有多少
    pub hidden_inside: usize,
    /// 没数完（太多了或者时间到了）
    pub counted_all: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    /// 改了属性的文件和文件夹
    pub changed: usize,
    /// 文件夹里的程序和脚本文件，照样藏着
    pub kept_programs: usize,
    /// 改不了的
    pub failed: usize,
    /// 太多了，没改完（到了 MAX_CHANGES 处或者 MAX_VISITS 个）
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub restored: usize,
    pub failed: usize,
}

/// 「显示出来」做了什么，和重新查的结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenRestore {
    pub result: RestoreResult,
    pub report: HiddenReport,
}

/// 撤销做了什么，和重新查的结果（还没选过文件夹时没有）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenUndo {
    pub result: UndoResult,
    pub report: Option<HiddenReport>,
}

fn lower_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn is_program(path: &Path) -> bool {
    let name = lower_name(path);
    name.rsplit_once('.').is_some_and(|(_, ext)| PROGRAM_EXTENSIONS.contains(&ext))
}

fn is_hidden(attrs: u32) -> bool {
    attrs & (ATTR_HIDDEN | ATTR_SYSTEM) != 0
}

/// 符号链接、目录联接：不跟着走，也不改
fn is_link(path: &Path, attrs: u32) -> bool {
    attrs & ATTR_REPARSE_POINT != 0 || fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(true)
}

/// Windows 所在的盘（环境变量 SystemDrive）：`folder` 在它上面就不给用
pub fn on_system_drive(folder: &Path) -> bool {
    std::env::var_os("SystemDrive").is_some_and(|d| on_drive(folder, &d.to_string_lossy()))
}

/// `folder` 在 `drive`（「C:」）上
fn on_drive(folder: &Path, drive: &str) -> bool {
    let drive = drive.trim_end_matches(['\\', '/']).to_lowercase();
    if drive.is_empty() {
        return false;
    }
    let text = folder.to_string_lossy().to_lowercase();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text == drive || text.starts_with(&format!("{drive}\\")) || text.starts_with(&format!("{drive}/"))
}

/// 快捷方式里有没有写着偷偷运行程序的字（ASCII 或者 UTF-16 的都看）
fn suspicious_shortcut(path: &Path) -> bool {
    use std::io::Read;
    let Ok(file) = fs::File::open(path) else { return false };
    let mut bytes = Vec::new();
    if file.take(LNK_READ).read_to_end(&mut bytes).is_err() {
        return false;
    }
    let ascii: String = bytes.iter().map(|&b| (b as char).to_ascii_lowercase()).collect();
    let wide: String = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .map(|u| char::from_u32(u32::from(u)).unwrap_or('\0').to_ascii_lowercase())
        .collect();
    // UTF-16 的字符串可能从奇数位置开始
    let shifted: String = bytes
        .get(1..)
        .unwrap_or_default()
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .map(|u| char::from_u32(u32::from(u)).unwrap_or('\0').to_ascii_lowercase())
        .collect();
    SUSPICIOUS_TARGETS.iter().any(|t| ascii.contains(t) || wide.contains(t) || shifted.contains(t))
}

struct Count {
    inside: usize,
    hidden: usize,
    all: bool,
}

/// 数文件夹里面有多少东西、藏起来多少（不跟着链接走）
fn count_inside(dir: &Path, attrs: &dyn Attrs, deadline: Instant) -> Count {
    let mut count = Count { inside: 0, hidden: 0, all: true };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(list) = fs::read_dir(&d) else { continue };
        for entry in list.flatten() {
            if count.inside >= COUNT_LIMIT || Instant::now() > deadline {
                count.all = false;
                return count;
            }
            let path = entry.path();
            let a = attrs.get(&path).unwrap_or(0);
            if is_link(&path, a) {
                continue;
            }
            count.inside += 1;
            if is_hidden(a) {
                count.hidden += 1;
            }
            if a & ATTR_DIRECTORY != 0 {
                stack.push(path);
            }
        }
    }
    count
}

/// 列出 `root` 里被藏起来的东西。返回结果和结果里列出的项目（编号就是下标）。
pub fn scan(root: &Path, attrs: &dyn Attrs, can_undo: usize) -> Result<(HiddenReport, Vec<PathBuf>), String> {
    let list = fs::read_dir(root).map_err(|e| format!("打不开这个文件夹：{e}"))?;
    let mut entries: Vec<(PathBuf, u32)> = list
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| attrs.get(&p).map(|a| (p, a)))
        .filter(|(p, a)| !is_link(p, *a))
        .collect();
    entries.sort_by(|a, b| lower_name(&a.0).cmp(&lower_name(&b.0)));

    let hidden_names: Vec<String> = entries
        .iter()
        .filter(|(p, a)| is_hidden(*a) && !SYSTEM_NAMES.contains(&lower_name(p).as_str()))
        .map(|(p, _)| lower_name(p))
        .collect();
    let deadline = Instant::now() + COUNT_TIME;
    let mut shown = Vec::new();
    let mut items = Vec::new();
    let mut programs = Vec::new();
    let mut shortcuts = Vec::new();
    for (path, a) in &entries {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let lower = lower_name(path);
        if SYSTEM_NAMES.contains(&lower.as_str()) {
            continue;
        }
        let is_dir = a & ATTR_DIRECTORY != 0;
        if lower.ends_with(".lnk") && !is_dir {
            let stem = &lower[..lower.len() - 4];
            if hidden_names.iter().any(|h| h == stem) || suspicious_shortcut(path) {
                shortcuts.push(name);
            }
            continue;
        }
        if !is_hidden(*a) {
            continue;
        }
        if !is_dir && is_program(path) {
            programs.push(name);
            continue;
        }
        let (inside, hidden_inside, counted_all) = if is_dir {
            let c = count_inside(path, attrs, deadline);
            (c.inside, c.hidden, c.all)
        } else {
            (0, 0, true)
        };
        let size = if is_dir { 0 } else { fs::metadata(path).map(|m| m.len()).unwrap_or(0) };
        items.push(HiddenItem { id: shown.len(), name, is_dir, size, inside, hidden_inside, counted_all });
        shown.push(path.clone());
    }
    let report = HiddenReport { folder: root.display().to_string(), items, programs, shortcuts, can_undo };
    Ok((report, shown))
}

/// 去掉隐藏、系统属性以后的属性；不用改时返回 None
fn shown_attrs(attrs: u32) -> Option<u32> {
    if !is_hidden(attrs) {
        return None;
    }
    let value = attrs & SETTABLE & !(ATTR_HIDDEN | ATTR_SYSTEM | ATTR_READONLY);
    Some(if value == 0 { ATTR_NORMAL } else { value })
}

/// 把 `targets`（文件夹连同里面的一切）显示出来，改过的记进 `changed`。
pub fn restore(targets: &[PathBuf], attrs: &dyn Attrs, changed: &mut Vec<(PathBuf, u32)>) -> RestoreResult {
    let mut result = RestoreResult { changed: 0, kept_programs: 0, failed: 0, truncated: false };
    let mut stack: Vec<PathBuf> = targets.to_vec();
    let mut visits = 0;
    while let Some(path) = stack.pop() {
        visits += 1;
        if visits > MAX_VISITS {
            result.truncated = true;
            break;
        }
        let Some(a) = attrs.get(&path) else {
            result.failed += 1;
            continue;
        };
        if is_link(&path, a) {
            continue;
        }
        let is_dir = a & ATTR_DIRECTORY != 0;
        if !is_dir && is_program(&path) {
            if is_hidden(a) {
                result.kept_programs += 1;
            }
            continue;
        }
        if let Some(value) = shown_attrs(a) {
            if changed.len() >= MAX_CHANGES {
                result.truncated = true;
                break;
            }
            match attrs.set(&path, value) {
                Ok(()) => {
                    changed.push((path.clone(), a));
                    result.changed += 1;
                }
                Err(_) => result.failed += 1,
            }
        }
        if is_dir && let Ok(list) = fs::read_dir(&path) {
            stack.extend(list.flatten().map(|e| e.path()));
        }
    }
    result
}

/// 把上一次「显示出来」改过的属性都改回去（后改的先改回去）
pub fn undo(changed: &mut Vec<(PathBuf, u32)>, attrs: &dyn Attrs) -> UndoResult {
    let mut result = UndoResult { restored: 0, failed: 0 };
    for (path, old) in changed.drain(..).rev() {
        let value = old & SETTABLE;
        match attrs.set(&path, if value == 0 { ATTR_NORMAL } else { value }) {
            Ok(()) => result.restored += 1,
            Err(_) => result.failed += 1,
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// 假的属性：路径 → 属性；没记的按真的文件系统算（文件夹或者普通文件）
    #[derive(Default)]
    struct FakeAttrs {
        map: RefCell<HashMap<PathBuf, u32>>,
        locked: Vec<PathBuf>,
    }
    impl FakeAttrs {
        fn mark(&self, path: &Path, extra: u32) {
            let base = if path.is_dir() { ATTR_DIRECTORY } else { 0x20 };
            self.map.borrow_mut().insert(path.to_path_buf(), base | extra);
        }
        fn of(&self, path: &Path) -> u32 {
            self.get(path).unwrap()
        }
    }
    impl Attrs for FakeAttrs {
        fn get(&self, path: &Path) -> Option<u32> {
            if let Some(a) = self.map.borrow().get(path) {
                return Some(*a);
            }
            let meta = fs::symlink_metadata(path).ok()?;
            Some(if meta.is_dir() { ATTR_DIRECTORY } else { 0x20 })
        }
        /// 和 Windows 一样：文件夹总带着「文件夹」属性；只设 NORMAL 的文件读回来是 NORMAL，文件夹读回来只有「文件夹」
        fn set(&self, path: &Path, value: u32) -> std::io::Result<()> {
            if self.locked.iter().any(|l| l == path) {
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "拒绝访问"));
            }
            assert_eq!(value & !SETTABLE & !ATTR_NORMAL, 0, "只传 SetFileAttributesW 能设的属性");
            let stored = match (path.is_dir(), value == ATTR_NORMAL) {
                (true, true) => ATTR_DIRECTORY,
                (true, false) => value | ATTR_DIRECTORY,
                (false, _) => value,
            };
            self.map.borrow_mut().insert(path.to_path_buf(), stored);
            Ok(())
        }
    }

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-hidden-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn file(&self, rel: &str, content: &[u8]) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, content).unwrap();
            p
        }
        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(&p).unwrap();
            p
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const HS: u32 = ATTR_HIDDEN | ATTR_SYSTEM;

    /// 一个中了毒的 U 盘：作业、照片两个文件夹和一个文档被藏起来，旁边是同名的快捷方式和病毒本身
    fn infected(d: &Dir, a: &FakeAttrs) {
        let homework = d.dir("作业");
        let photos = d.dir("照片");
        let inner = d.file("作业/第一章.docx", b"doc");
        let nested = d.dir("作业/图片");
        let nested_file = d.file("作业/图片/1.jpg", b"jpg");
        let doc = d.file("简历.docx", b"cv");
        let virus = d.file("DeviceConfigManager.vbs", b"virus");
        let dropped = d.file("作业/WindowsServices.exe", b"MZ");
        let autorun = d.file("autorun.inf", b"[autorun]");
        let svi = d.dir("System Volume Information");
        d.file("作业.lnk", b"L\0\0\0 target: C:\\Windows\\System32\\cmd.exe /c start _\\x.vbs");
        d.file("照片.lnk", b"L\0\0\0 plain shortcut");
        d.file("工具.lnk", "L\0\0\0 wscript".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>().as_slice());
        d.file("说明.lnk", b"L\0\0\0 C:\\Program Files\\App\\app.exe");
        d.file("看得见.txt", b"visible");
        for p in [&homework, &photos, &doc, &virus, &autorun, &svi] {
            a.mark(p, HS);
        }
        a.mark(&inner, HS);
        a.mark(&nested, ATTR_HIDDEN);
        a.mark(&nested_file, HS | ATTR_READONLY);
        a.mark(&dropped, HS);
    }

    #[test]
    fn hidden_items_programs_and_virus_shortcuts_are_told_apart() {
        let d = Dir::new("scan");
        let a = FakeAttrs::default();
        infected(&d, &a);
        let (report, shown) = scan(&d.0, &a, 0).unwrap();
        let names: Vec<&str> = report.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["作业", "照片", "简历.docx"], "系统的、看得见的不列");
        assert_eq!(shown.len(), 3);
        let homework = &report.items[0];
        assert!(homework.is_dir && homework.counted_all);
        assert_eq!((homework.inside, homework.hidden_inside), (4, 4), "第一章、图片、1.jpg、病毒程序");
        assert_eq!(report.items[2].size, 2);
        assert_eq!(report.programs, ["autorun.inf", "DeviceConfigManager.vbs"]);
        // 作业.lnk 同名又指向 cmd；照片.lnk 和被藏起来的同名；工具.lnk 里有 UTF-16 的 wscript；说明.lnk 是正常的
        assert_eq!(report.shortcuts, ["作业.lnk", "工具.lnk", "照片.lnk"]);
    }

    #[test]
    fn showing_them_keeps_programs_hidden_and_can_be_undone() {
        let d = Dir::new("restore");
        let a = FakeAttrs::default();
        infected(&d, &a);
        let (_, shown) = scan(&d.0, &a, 0).unwrap();
        let before = a.map.borrow().clone();
        let mut changed = Vec::new();
        let r = restore(&shown, &a, &mut changed);
        assert_eq!((r.changed, r.kept_programs, r.failed, r.truncated), (6, 1, 0, false), "{r:?}");
        let homework = d.0.join("作业");
        assert_eq!(a.of(&homework), ATTR_DIRECTORY, "文件夹的隐藏、系统都去掉了");
        assert_eq!(a.of(&d.0.join("作业/图片/1.jpg")), 0x20, "只读也去掉了，存档留着");
        assert_eq!(a.of(&d.0.join("作业/WindowsServices.exe")) & HS, HS, "病毒程序照样藏着");
        assert_eq!(a.of(&d.0.join("DeviceConfigManager.vbs")) & HS, HS, "没勾的不动");
        let (after, _) = scan(&d.0, &a, changed.len()).unwrap();
        assert!(after.items.is_empty() && after.can_undo == 6, "{after:?}");

        let u = undo(&mut changed, &a);
        assert_eq!((u.restored, u.failed), (6, 0));
        assert!(changed.is_empty());
        let restored = a.map.borrow().clone();
        for (path, attrs) in &before {
            assert_eq!(restored.get(path), Some(attrs), "{} 改回了原来的属性", path.display());
        }
    }

    #[test]
    fn locked_files_are_counted_and_links_are_not_followed() {
        let d = Dir::new("locked");
        let a = FakeAttrs { locked: vec![d.0.join("资料/锁住.txt")], ..Default::default() };
        let folder = d.dir("资料");
        let locked = d.file("资料/锁住.txt", b"x");
        a.mark(&folder, HS);
        a.mark(&locked, HS);
        let outside = Dir::new("locked-outside");
        let far = outside.file("别处.txt", b"far");
        a.mark(&far, HS);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside.0, d.0.join("资料/链接")).unwrap();
        let mut changed = Vec::new();
        let r = restore(std::slice::from_ref(&folder), &a, &mut changed);
        assert_eq!((r.changed, r.failed), (1, 1), "{r:?}");
        assert_eq!(a.of(&far) & HS, HS, "链接指向的地方不动");
        let (report, _) = scan(&d.0, &a, 0).unwrap();
        assert!(report.items.is_empty());
    }

    #[test]
    fn the_system_drive_is_refused() {
        assert!(on_drive(Path::new(r"C:\"), "C:"));
        assert!(on_drive(Path::new(r"c:\Users\bob"), "C:"));
        assert!(on_drive(Path::new(r"\\?\C:\Temp"), "C:"));
        assert!(on_drive(Path::new("C:"), "c:\\"));
        assert!(!on_drive(Path::new(r"E:\"), "C:"));
        assert!(!on_drive(Path::new(r"CD:\"), "C:"));
        assert!(!on_drive(Path::new(r"\\server\share"), "C:"));
        assert!(!on_drive(Path::new(r"C:\"), ""));
    }

    #[test]
    fn attributes_after_showing() {
        assert_eq!(shown_attrs(ATTR_DIRECTORY), None, "本来就看得见的不改");
        assert_eq!(shown_attrs(ATTR_READONLY | 0x20), None, "只有只读的不改");
        assert_eq!(shown_attrs(HS | ATTR_READONLY), Some(ATTR_NORMAL));
        assert_eq!(shown_attrs(HS | 0x20 | 0x2000), Some(0x20 | 0x2000));
        assert_eq!(shown_attrs(ATTR_DIRECTORY | HS), Some(ATTR_NORMAL), "文件夹的属性位不传给 SetFileAttributesW");
    }
}
