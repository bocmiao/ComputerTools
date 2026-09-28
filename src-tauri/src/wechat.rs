//! 微信占 C 盘：找出这台电脑上微信 3.x、4.x 的账号文件夹，数一数「缓存和临时文件」「聊天里的旧文件」各有多少，把用户
//! 勾选的放进回收站（挑哪些文件的规则在 medkit_core::wechat，照 blackboxo/CleanMyWechat，MIT）。
//! - 找的地方照 CleanMyWechat 的 find_all_wechat_paths：登录用户的「文档」和用户文件夹下的 xwechat_files（4.x），「文档」下的
//!   WeChat Files（3.x），微信 3.x 在注册表里记的存储位置（HKU\<SID>\Software\Tencent\WeChat 的 FileSavePath，
//!   "MyDocument:" 表示「文档」；这个文件夹本身和它下面的 WeChat Files 都看），每个本地硬盘根目录下的 xwechat_files、
//!   WeChat Files。「文档」在哪看登录用户自己的设置（用户外壳文件夹），不看以管理员身份运行的那个账户的；
//! - 结果里没有账号名、没有路径：账号按「最近收到文件」排好叫「账号 1、账号 2」，界面只传编号；
//! - 微信开着的时候不清理（它正在用这些文件）；
//! - 放进回收站，和在资源管理器里按 Delete 一样（medkit_core::platform::windows::recycle）；清空回收站以前都能还原。
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use medkit_core::wechat::{self, CACHE_MIN_DAYS, CHAT_DAYS_RANGE, Category, Version};
use serde::Serialize;

/// 数所有账号最多花多久
const SCAN_LIMIT: Duration = Duration::from_secs(60);
/// 每个账号每一类最多挑多少个文件
const MAX_FILES: usize = 500_000;
/// 放进回收站最多花多久（文件很多时分几次清）
const CLEAN_LIMIT: Duration = Duration::from_secs(600);
/// 一次交给回收站多少个文件，交完看一次时间
const CLEAN_CHUNK: usize = 2_000;
/// 微信的程序：4.x 是 Weixin.exe，3.x 是 WeChat.exe，小程序和网页在 WeChatAppEx.exe 里
pub const WECHAT_PROCESSES: &[&str] = &["Weixin.exe", "WeChat.exe", "WeChatAppEx.exe"];

#[derive(Debug, Default)]
pub struct WechatState {
    /// 最近一次查到的账号文件夹，编号就是下标
    pub accounts: Vec<(Version, PathBuf)>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WechatAccountView {
    pub id: usize,
    /// 「4.x」「3.x」
    pub version: &'static str,
    /// 在哪个盘上（盘符）
    pub drive: String,
    /// 聊天里的文件夹里最新的一个文件的修改时间（秒，从 1970 年算），没有文件时是 null
    pub last_file_secs: Option<u64>,
    pub cache_files: u64,
    pub cache_bytes: u64,
    pub chat_files: u64,
    pub chat_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WechatReport {
    pub accounts: Vec<WechatAccountView>,
    /// 缓存只算多少天以前的
    pub cache_days: u32,
    /// 聊天里的文件按多少天以前算
    pub chat_days: u32,
    /// 微信正开着
    pub running: bool,
    /// 数完了（文件太多、太慢时是「至少这么多」）
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WechatCleanResult {
    /// 放进回收站的文件个数和大小
    pub files: u64,
    pub bytes: u64,
    /// 没放进去的（正在用、路径太长）
    pub failed: u64,
    /// 在 Windows 的提示框里点了「取消」
    pub cancelled: bool,
    /// 到了时间还没清完（再点一次接着清）
    pub partial: bool,
    /// 清完重新查的结果
    pub report: WechatReport,
}

/// 聊天里的文件按多少天以前算：只收 [`CHAT_DAYS_RANGE`] 里的。
pub fn check_days(days: u32) -> Result<u32, String> {
    if CHAT_DAYS_RANGE.contains(&days) {
        Ok(days)
    } else {
        Err(format!("天数要在 {} 到 {} 之间。", CHAT_DAYS_RANGE.start(), CHAT_DAYS_RANGE.end()))
    }
}

/// 要找的地方（没去掉重复的，也不管在不在）。
pub fn candidates(
    profile: Option<&Path>,
    documents: Option<&Path>,
    file_save_path: Option<&str>,
    drives: &[char],
) -> Vec<(Version, PathBuf)> {
    let mut out = Vec::new();
    if let Some(d) = documents {
        out.push((Version::V4, d.join(Version::V4.root_name())));
        out.push((Version::V3, d.join(Version::V3.root_name())));
    }
    if let Some(p) = profile {
        out.push((Version::V4, p.join(Version::V4.root_name())));
        out.push((Version::V4, p.join("Documents").join(Version::V4.root_name())));
        out.push((Version::V3, p.join("Documents").join(Version::V3.root_name())));
    }
    match file_save_path.map(str::trim) {
        Some("MyDocument:") => {
            if let Some(d) = documents {
                out.push((Version::V3, d.join(Version::V3.root_name())));
            }
        }
        Some(saved) if !saved.is_empty() => {
            out.push((Version::V3, PathBuf::from(saved)));
            out.push((Version::V3, Path::new(saved).join(Version::V3.root_name())));
        }
        _ => {}
    }
    for letter in drives {
        let root = PathBuf::from(format!("{letter}:\\"));
        out.push((Version::V4, root.join(Version::V4.root_name())));
        out.push((Version::V3, root.join(Version::V3.root_name())));
    }
    out
}

/// 在这些地方找账号（同一个文件夹只算一次），按最近收到文件排好：最近用过的是账号 1。
pub fn find_accounts(roots: &[(Version, PathBuf)]) -> Vec<(Version, PathBuf)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (version, root) in roots {
        for account in wechat::accounts(root, *version) {
            let key = std::fs::canonicalize(&account).unwrap_or_else(|_| account.clone());
            if seen.insert(key) {
                out.push((*version, account));
            }
        }
    }
    out
}

fn drive_of(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), Some(':')) if c.is_ascii_alphabetic() => c.to_ascii_uppercase().to_string(),
        _ => String::new(),
    }
}

fn secs(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// 数一数每个账号的缓存和聊天里的旧文件（只读）。返回界面要的结果和按编号排好的账号。
pub fn scan(
    found: Vec<(Version, PathBuf)>,
    chat_days: u32,
    now: SystemTime,
    running: bool,
) -> (WechatReport, Vec<(Version, PathBuf)>) {
    let deadline = Instant::now() + SCAN_LIMIT;
    let mut complete = true;
    let mut rows = Vec::new();
    for (version, dir) in found {
        let cache =
            wechat::pick(&dir, version, Category::Cache, wechat::days_before(now, CACHE_MIN_DAYS), deadline, MAX_FILES);
        let chat =
            wechat::pick(&dir, version, Category::Chat, wechat::days_before(now, chat_days), deadline, MAX_FILES);
        complete &= cache.complete && chat.complete;
        let view = WechatAccountView {
            id: 0,
            version: version.label(),
            drive: drive_of(&dir),
            last_file_secs: chat.newest.and_then(secs),
            cache_files: cache.files.len() as u64,
            cache_bytes: cache.bytes,
            chat_files: chat.files.len() as u64,
            chat_bytes: chat.bytes,
        };
        rows.push((view, (version, dir)));
    }
    // 最近收到文件的排前面，都没有的排后面
    rows.sort_by(|a, b| b.0.last_file_secs.cmp(&a.0.last_file_secs));
    let mut accounts = Vec::new();
    let mut views = Vec::new();
    for (i, (mut view, account)) in rows.into_iter().enumerate() {
        view.id = i;
        views.push(view);
        accounts.push(account);
    }
    (WechatReport { accounts: views, cache_days: CACHE_MIN_DAYS, chat_days, running, complete }, accounts)
}

/// 清理的结果（还没重新查）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cleaned {
    pub files: u64,
    pub bytes: u64,
    pub failed: u64,
    pub cancelled: bool,
    pub partial: bool,
}

/// 把勾选的账号里勾选的那几类文件放进回收站。`recycle` 把一批文件放进回收站，返回用户有没有点「取消」；
/// 放没放进去看文件还在不在。
pub fn clean(
    accounts: &[(Version, PathBuf)],
    ids: &[usize],
    cache: bool,
    chat: bool,
    chat_days: u32,
    now: SystemTime,
    recycle: &dyn Fn(&[PathBuf]) -> bool,
) -> Result<Cleaned, String> {
    if ids.is_empty() {
        return Err("没有勾选要清理的账号。".into());
    }
    if !cache && !chat {
        return Err("没有勾选要清理的东西。".into());
    }
    let chosen = ids
        .iter()
        .map(|&id| accounts.get(id).cloned().ok_or("有的账号不在刚才的结果里，请重新查一遍。"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut categories = Vec::new();
    if cache {
        categories.push((Category::Cache, wechat::days_before(now, CACHE_MIN_DAYS)));
    }
    if chat {
        categories.push((Category::Chat, wechat::days_before(now, chat_days)));
    }
    let deadline = Instant::now() + CLEAN_LIMIT;
    let mut done = Cleaned::default();
    for (version, dir) in chosen {
        for &(category, before) in &categories {
            let picked = wechat::pick(&dir, version, category, before, deadline, MAX_FILES);
            done.partial |= !picked.complete;
            for chunk in picked.files.chunks(CLEAN_CHUNK) {
                if Instant::now() > deadline {
                    done.partial = true;
                    return Ok(done);
                }
                let paths: Vec<PathBuf> = chunk.iter().map(|(p, _)| p.clone()).collect();
                let cancelled = recycle(&paths);
                for (path, size) in chunk {
                    if std::fs::symlink_metadata(path).is_ok() {
                        done.failed += 1;
                    } else {
                        done.files += 1;
                        done.bytes += size;
                    }
                }
                if cancelled {
                    done.cancelled = true;
                    return Ok(done);
                }
            }
        }
    }
    Ok(done)
}

/// 这台电脑上要找的地方。
#[cfg(windows)]
pub fn roots() -> Vec<(Version, PathBuf)> {
    use medkit_core::platform::Platform;
    use medkit_core::platform::windows::{WindowsPlatform, drives};
    use medkit_core::registry::{RegRoot, RegValue, is_sid};

    let platform = WindowsPlatform::new();
    let text = |root: &RegRoot, key: &str, name: &str| -> Option<String> {
        match platform.reg_get(root, key, name).ok().flatten()? {
            RegValue::String(s) | RegValue::ExpandString(s) => Some(s),
            _ => None,
        }
    };
    let user = platform.interactive_user().or_else(|| platform.process_user()).filter(|u| is_sid(&u.sid));
    let (hive, profile) = match user {
        Some(u) => {
            let key = format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\{}", u.sid);
            let profile =
                text(&RegRoot::LocalMachine, &key, "ProfileImagePath").map(|p| PathBuf::from(expand(&p, None)));
            (RegRoot::User(u.sid), profile)
        }
        None => (RegRoot::CurrentUser, std::env::var_os("USERPROFILE").map(PathBuf::from)),
    };
    let documents = text(&hive, r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders", "Personal")
        .map(|p| PathBuf::from(expand(&p, profile.as_deref())))
        .or_else(|| profile.as_ref().map(|p| p.join("Documents")));
    let saved = text(&hive, r"Software\Tencent\WeChat", "FileSavePath");
    let fixed: Vec<char> = drives().into_iter().filter(|d| !d.removable).map(|d| d.letter).collect();
    candidates(profile.as_deref(), documents.as_deref(), saved.as_deref(), &fixed)
}

#[cfg(not(windows))]
pub fn roots() -> Vec<(Version, PathBuf)> {
    Vec::new()
}

/// 把 `%NAME%` 换成值：`%USERPROFILE%` 用登录用户的用户文件夹（给了的话），别的用这个进程的环境变量；没有的原样留着。
pub fn expand(text: &str, profile: Option<&Path>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let value = if name.eq_ignore_ascii_case("USERPROFILE") {
            profile.map(|p| p.to_string_lossy().into_owned()).or_else(|| std::env::var(name).ok())
        } else if name.is_empty() {
            None
        } else {
            std::env::var(name).ok()
        };
        match value {
            Some(v) => out.push_str(&v),
            None => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 微信开着没有。
#[cfg(windows)]
pub fn running() -> bool {
    medkit_core::platform::windows::processes_running(WECHAT_PROCESSES)
}

#[cfg(not(windows))]
pub fn running() -> bool {
    false
}

/// 放进回收站。
#[cfg(windows)]
pub fn recycle(paths: &[PathBuf]) -> bool {
    medkit_core::platform::windows::recycle(paths)
}

#[cfg(not(windows))]
pub fn recycle(_paths: &[PathBuf]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const DAY: Duration = Duration::from_secs(24 * 3600);

    /// 测试用的临时文件夹，用完删掉
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-wechat-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn file(path: &Path, bytes: usize, age_days: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
        fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - DAY * age_days).unwrap();
    }

    /// 两个 4.x 账号（一个最近用过）和一个 3.x 账号
    fn machine(root: &Path) -> Vec<(Version, PathBuf)> {
        let docs = root.join("Documents");
        let busy = docs.join("xwechat_files/wxid_busy_1");
        file(&busy.join("cache/2025-01/Message/a/Thumb/t.dat"), 100, 400);
        file(&busy.join("msg/attach/a/2025-01/Img/old.dat"), 1000, 400);
        file(&busy.join("msg/attach/a/2026-09/Img/new.dat"), 1000, 1);
        file(&busy.join("db_storage/message/message_0.db"), 99, 400);
        let idle = docs.join("xwechat_files/wxid_idle_2");
        file(&idle.join("temp/x.tmp"), 10, 30);
        file(&idle.join("msg/file/2024-01/a.pdf"), 500, 700);
        let old = docs.join("WeChat Files/wxid_old");
        file(&old.join("FileStorage/Cache/2020-01/c.jpg"), 20, 2000);
        file(&old.join("FileStorage/File/2020-01/f.doc"), 30, 2000);
        find_accounts(&candidates(Some(root), Some(&docs), Some("MyDocument:"), &[]))
    }

    #[test]
    fn places_follow_clean_my_wechat() {
        let profile = Path::new("/u/me");
        let docs = Path::new("/d/Docs");
        let c = candidates(Some(profile), Some(docs), Some("MyDocument:"), &['D']);
        assert!(c.contains(&(Version::V4, docs.join("xwechat_files"))));
        assert!(c.contains(&(Version::V4, profile.join("xwechat_files"))));
        assert!(c.contains(&(Version::V3, docs.join("WeChat Files"))));
        assert!(c.contains(&(Version::V4, PathBuf::from("D:\\").join("xwechat_files"))));
        assert!(c.contains(&(Version::V3, PathBuf::from("D:\\").join("WeChat Files"))));
        let c = candidates(None, None, Some(r"E:\Chat"), &[]);
        assert_eq!(
            c,
            vec![(Version::V3, PathBuf::from(r"E:\Chat")), (Version::V3, Path::new(r"E:\Chat").join("WeChat Files"))]
        );
        assert!(candidates(None, None, Some("  "), &[]).is_empty());
    }

    #[test]
    fn accounts_are_found_once_and_listed_by_last_use_without_names() {
        let dir = Dir::new("found");
        let found = machine(dir.path());
        assert_eq!(found.len(), 3, "同一个文件夹从两处找到也只算一次：{found:?}");
        let (report, accounts) = scan(found, 365, SystemTime::now(), false);
        assert!(report.complete);
        assert_eq!(report.cache_days, CACHE_MIN_DAYS);
        assert_eq!(accounts.len(), 3);
        // 最近收到文件的（busy，1 天前）是账号 1
        assert!(accounts[0].1.ends_with("wxid_busy_1"));
        let busy = &report.accounts[0];
        assert_eq!((busy.id, busy.version), (0, "4.x"));
        assert_eq!((busy.cache_files, busy.cache_bytes, busy.chat_files, busy.chat_bytes), (1, 100, 1, 1000));
        let old = report.accounts.iter().find(|a| a.version == "3.x").unwrap();
        assert_eq!((old.cache_bytes, old.chat_bytes), (20, 30));
        // 结果里没有路径和账号名
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("wxid") && !json.contains("Documents"), "{json}");
    }

    #[test]
    fn cleaning_recycles_only_the_chosen_categories_and_counts_what_left() {
        let dir = Dir::new("clean");
        let found = machine(dir.path());
        let (_, accounts) = scan(found, 365, SystemTime::now(), false);
        let seen = std::cell::RefCell::new(Vec::new());
        // 假的回收站：删掉文件，但留下名字里有 new 的（当作正在用、放不进去）
        let fake = |paths: &[PathBuf]| -> bool {
            for p in paths {
                seen.borrow_mut().push(p.clone());
                fs::remove_file(p).unwrap();
            }
            false
        };
        let done = clean(&accounts, &[0], true, false, 365, SystemTime::now(), &fake).unwrap();
        assert_eq!(done, Cleaned { files: 1, bytes: 100, ..Default::default() });
        assert!(seen.borrow().iter().all(|p| p.to_string_lossy().contains("cache")));
        let done = clean(&accounts, &[0, 1, 2], false, true, 365, SystemTime::now(), &fake).unwrap();
        assert_eq!(done.files, 3, "{done:?}");
        assert_eq!(done.bytes, 1000 + 500 + 30);
        assert!(!seen.borrow().iter().any(|p| p.to_string_lossy().contains("db_storage") || p.ends_with("new.dat")));
        // 数据库、新图片都还在
        assert!(accounts[0].1.join("db_storage/message/message_0.db").exists());
        assert!(accounts[0].1.join("msg/attach/a/2026-09/Img/new.dat").exists());
    }

    #[test]
    fn files_that_stay_are_counted_as_failed_and_cancel_stops() {
        let dir = Dir::new("stay");
        let (_, accounts) = scan(machine(dir.path()), 365, SystemTime::now(), false);
        let keep = |_: &[PathBuf]| false;
        let done = clean(&accounts, &[0], true, true, 365, SystemTime::now(), &keep).unwrap();
        assert_eq!((done.files, done.failed), (0, 2));
        let cancel = |_: &[PathBuf]| true;
        let done = clean(&accounts, &[0], true, true, 365, SystemTime::now(), &cancel).unwrap();
        assert!(done.cancelled);
        assert_eq!(done.failed, 1, "点了取消就不再往下交");
    }

    #[test]
    fn bad_requests_are_refused() {
        let dir = Dir::new("bad");
        let (_, accounts) = scan(machine(dir.path()), 365, SystemTime::now(), false);
        let never = |_: &[PathBuf]| -> bool { panic!("不该走到放进回收站这一步") };
        let now = SystemTime::now();
        assert!(clean(&accounts, &[], true, true, 365, now, &never).unwrap_err().contains("账号"));
        assert!(clean(&accounts, &[0], false, false, 365, now, &never).unwrap_err().contains("东西"));
        assert!(clean(&accounts, &[9], true, false, 365, now, &never).unwrap_err().contains("重新查"));
        assert_eq!(check_days(365), Ok(365));
        assert!(check_days(1).is_err());
        assert!(check_days(99_999).is_err());
    }

    #[test]
    fn user_profile_is_expanded_with_the_signed_in_users_folder() {
        let p = Path::new(r"C:\Users\someone");
        assert_eq!(expand(r"%USERPROFILE%\Documents", Some(p)), r"C:\Users\someone\Documents");
        assert_eq!(expand(r"%userprofile%\Docs", Some(p)), r"C:\Users\someone\Docs");
        assert_eq!(expand(r"%NO_SUCH_VAR_MEDKIT%\x", Some(p)), r"%NO_SUCH_VAR_MEDKIT%\x");
        assert_eq!(expand("D:\\Docs", Some(p)), "D:\\Docs");
        assert_eq!(expand("50%", Some(p)), "50%");
    }

    #[test]
    fn drive_letters_come_from_the_path() {
        assert_eq!(drive_of(Path::new(r"C:\Users\x")), "C");
        assert_eq!(drive_of(Path::new(r"d:\xwechat_files")), "D");
        assert_eq!(drive_of(Path::new("/home/x")), "");
    }
}
