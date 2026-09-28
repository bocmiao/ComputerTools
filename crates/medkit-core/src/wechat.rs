//! 微信占 C 盘：找出微信 3.x、4.x 每个账号的文件夹，挑出「缓存和临时文件」「聊天里的旧文件」，交给外壳放进回收站。
//!
//! 目录规则照 blackboxo/CleanMyWechat（MIT）的 `get_fileNum`（main.py）和 `selectVersion.py`；4.x 各个文件夹里放什么，
//! 参考 wener.me 的《WeChat Inside》笔记：
//! - 4.x：`xwechat_files\<账号>\`（账号文件夹里有 msg）。缓存：cache（按月放消息气泡、缩略图、临时图片、表情、朋友圈、
//!   小程序图标的缓存）、temp、apm_record、business\InputTemp、business\emoticon\Temp、business\emoticon\Thumb、
//!   business\xweb。聊天里的文件：msg\file、msg\attach（聊天图片、语音、附件）、msg\video。
//! - 3.x：`WeChat Files\<wxid>\`（账号文件夹里有 FileStorage）。缓存：FileStorage\Cache。聊天里的文件：FileStorage\File、
//!   FileStorage\Image、FileStorage\Video、FileStorage\MsgAttach。
//!
//! 安全边界（改这里要先想清楚）：
//! - 只看上面白名单里的子文件夹，别的一律不碰：聊天记录数据库（4.x 的 db_storage、3.x 的 Msg）、配置、收藏，还有账号
//!   文件夹和 xwechat_files、WeChat Files 本身都不在白名单里；
//! - 白名单文件夹里的数据库、程序文件（[`SKIP_EXTENSIONS`]）也跳过；
//! - 不跟着符号链接、目录联接走（链接指向别的地方，删了会删到别处），链接本身也不算；
//! - 缓存只挑 [`CACHE_MIN_DAYS`] 天以前的（微信刚写的、正在用的不动），聊天里的文件只挑用户选的 N 天以前的；
//! - 只挑文件，不删文件夹；放进回收站由外壳做（和在资源管理器里按 Delete 一样），这里不删任何东西。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// 缓存只挑多少天以前的
pub const CACHE_MIN_DAYS: u32 = 7;

/// 聊天里的文件最少按多少天、最多按多少天以前算
pub const CHAT_DAYS_RANGE: std::ops::RangeInclusive<u32> = 30..=3650;

/// 这些扩展名的文件在哪里都不挑（照 CleanMyWechat 的 SAFE_SKIP_EXTS，加上 db-journal）：数据库和程序、脚本文件。
pub const SKIP_EXTENSIONS: &[&str] = &[
    "db",
    "sqlite",
    "sqlite3",
    "db-shm",
    "db-wal",
    "db-journal",
    "ldb",
    "sst",
    "dll",
    "exe",
    "msi",
    "sys",
    "ocx",
    "pyd",
    "so",
    "dylib",
    "bat",
    "cmd",
    "ps1",
    "vbs",
    "js",
    "jar",
    "pak",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Version {
    /// 微信 3.x（`WeChat Files`）
    V3,
    /// 微信 4.x（`xwechat_files`）
    V4,
}

impl Version {
    pub fn label(self) -> &'static str {
        match self {
            Version::V3 => "3.x",
            Version::V4 => "4.x",
        }
    }

    /// 放账号文件夹的那个文件夹的名字
    pub fn root_name(self) -> &'static str {
        match self {
            Version::V3 => "WeChat Files",
            Version::V4 => "xwechat_files",
        }
    }

    /// 账号文件夹里一定有的子文件夹（用来认出账号文件夹）
    fn account_marker(self) -> &'static str {
        match self {
            Version::V3 => "FileStorage",
            Version::V4 => "msg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// 缓存和临时文件
    Cache,
    /// 聊天里的图片、视频、文件
    Chat,
}

/// 一类文件在账号文件夹里的白名单子文件夹（相对路径，用 `/` 分隔）。
pub fn folders(version: Version, category: Category) -> &'static [&'static str] {
    match (version, category) {
        (Version::V4, Category::Cache) => &[
            "cache",
            "temp",
            "apm_record",
            "business/InputTemp",
            "business/emoticon/Temp",
            "business/emoticon/Thumb",
            "business/xweb",
        ],
        (Version::V4, Category::Chat) => &["msg/file", "msg/attach", "msg/video"],
        (Version::V3, Category::Cache) => &["FileStorage/Cache"],
        (Version::V3, Category::Chat) => {
            &["FileStorage/File", "FileStorage/Image", "FileStorage/Video", "FileStorage/MsgAttach"]
        }
    }
}

/// `root`（xwechat_files、WeChat Files）下面的账号文件夹：里面有 [`Version::account_marker`] 那个子文件夹的。
/// 链接不算；读不了的返回空。按路径排好。
pub fn accounts(root: &Path, version: Version) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| real_dir(p) && real_dir(&p.join(version.account_marker())))
        .collect();
    out.sort();
    out
}

/// 是文件夹，而且不是链接（符号链接、目录联接）。
fn real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

fn skipped_extension(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| SKIP_EXTENSIONS.iter().any(|s| s.eq_ignore_ascii_case(e)))
}

/// 挑出来的文件。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picked {
    /// 文件和它的大小
    pub files: Vec<(PathBuf, u64)>,
    pub bytes: u64,
    /// 这些文件夹里最新的一个文件的修改时间（不管挑没挑上；「最近收到文件」用）
    pub newest: Option<SystemTime>,
    /// 数完了（到了时间、个数上限时停下，是「至少这么多」）
    pub complete: bool,
}

/// 在一个账号文件夹里挑一类文件：白名单子文件夹里、修改时间早于 `before` 的普通文件，跳过 [`SKIP_EXTENSIONS`] 和链接。
/// 最多花到 `deadline`、挑 `max_files` 个。
pub fn pick(
    account: &Path,
    version: Version,
    category: Category,
    before: SystemTime,
    deadline: Instant,
    max_files: usize,
) -> Picked {
    let mut p = Picked { complete: true, ..Default::default() };
    for rel in folders(version, category) {
        let top = rel.split('/').fold(account.to_path_buf(), |acc, part| acc.join(part));
        // 白名单文件夹自己是链接的也不进去
        if !real_dir(&top) {
            continue;
        }
        let mut stack = vec![top];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                if Instant::now() > deadline || p.files.len() >= max_files {
                    p.complete = false;
                    return p;
                }
                let path = entry.path();
                let Ok(meta) = fs::symlink_metadata(&path) else { continue };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().ok();
                if let Some(m) = modified {
                    p.newest = Some(p.newest.map_or(m, |n| n.max(m)));
                }
                if skipped_extension(&path) || modified.is_none_or(|m| m >= before) {
                    continue;
                }
                p.bytes += meta.len();
                p.files.push((path, meta.len()));
            }
        }
    }
    p
}

/// `now` 往前 `days` 天。
pub fn days_before(now: SystemTime, days: u32) -> SystemTime {
    now.checked_sub(Duration::from_secs(u64::from(days) * 24 * 3600)).unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 3600);

    fn file(path: &Path, bytes: usize, age_days: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
        let t = SystemTime::now() - DAY * u32::try_from(age_days).unwrap();
        fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    /// 一个 4.x 账号：白名单里外、新的旧的、数据库都有
    fn v4_account(root: &Path) -> PathBuf {
        let a = root.join("xwechat_files").join("wxid_test_1234");
        file(&a.join("cache/2025-01/Message/abc/Thumb/1.dat"), 100, 400);
        file(&a.join("cache/2026-09/Message/abc/Thumb/2.dat"), 10, 1);
        file(&a.join("temp/x.tmp"), 20, 30);
        file(&a.join("business/emoticon/Thumb/e.png"), 30, 30);
        file(&a.join("business/emoticon/Persist/keep.png"), 40, 400);
        file(&a.join("msg/attach/h/2025-01/Img/old.dat"), 1000, 400);
        file(&a.join("msg/attach/h/2026-09/Img/new.dat"), 1000, 2);
        file(&a.join("msg/file/2025-01/report.docx"), 500, 400);
        file(&a.join("msg/video/2025-01/v.mp4"), 700, 200);
        file(&a.join("msg/file/2025-01/tool.exe"), 50, 400);
        file(&a.join("db_storage/message/message_0.db"), 5000, 400);
        file(&a.join("cache/leftover.db"), 5000, 400);
        file(&a.join("config/settings.ini"), 5, 400);
        file(&a.join("top-level.dat"), 5, 400);
        a
    }

    fn names(p: &Picked) -> Vec<String> {
        let mut v: Vec<String> =
            p.files.iter().map(|(f, _)| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn accounts_are_folders_with_the_marker_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let a = v4_account(dir.path());
        let root = dir.path().join("xwechat_files");
        fs::create_dir_all(root.join("all_users/config")).unwrap();
        fs::write(root.join("note.txt"), b"x").unwrap();
        assert_eq!(accounts(&root, Version::V4), vec![a]);
        assert!(accounts(&root, Version::V3).is_empty(), "4.x 的账号里没有 FileStorage");
        assert!(accounts(&dir.path().join("missing"), Version::V4).is_empty());

        let v3 = dir.path().join("WeChat Files");
        fs::create_dir_all(v3.join("wxid_old/FileStorage/Cache")).unwrap();
        fs::create_dir_all(v3.join("All Users/config")).unwrap();
        assert_eq!(accounts(&v3, Version::V3), vec![v3.join("wxid_old")]);
    }

    #[test]
    fn only_old_files_in_the_whitelisted_folders_are_picked() {
        let dir = tempfile::tempdir().unwrap();
        let a = v4_account(dir.path());
        let now = SystemTime::now();
        let cache = pick(&a, Version::V4, Category::Cache, days_before(now, CACHE_MIN_DAYS), far(), usize::MAX);
        assert!(cache.complete);
        // 新的缩略图（1 天）不动，数据库不动，白名单外的 Persist 不动
        assert_eq!(names(&cache), ["1.dat", "e.png", "x.tmp"]);
        assert_eq!(cache.bytes, 150);
        let chat = pick(&a, Version::V4, Category::Chat, days_before(now, 365), far(), usize::MAX);
        // 365 天以前的：old.dat、report.docx；200 天的视频、2 天的图片、程序文件不挑
        assert_eq!(names(&chat), ["old.dat", "report.docx"]);
        assert_eq!(chat.bytes, 1500);
        let newest = chat.newest.expect("记下了最新的文件");
        assert!(now.duration_since(newest).unwrap() < DAY * 3, "最新的是两天前收到的图片");
        let chat180 = pick(&a, Version::V4, Category::Chat, days_before(now, 180), far(), usize::MAX);
        assert_eq!(names(&chat180), ["old.dat", "report.docx", "v.mp4"]);
        for (f, _) in cache.files.iter().chain(&chat180.files) {
            let rel = f.strip_prefix(&a).unwrap().to_string_lossy().replace('\\', "/");
            assert!(
                folders(Version::V4, Category::Cache)
                    .iter()
                    .chain(folders(Version::V4, Category::Chat))
                    .any(|w| rel.starts_with(&format!("{w}/"))),
                "{rel} 不在白名单里"
            );
        }
    }

    #[test]
    fn version_3_uses_its_own_folders() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("WeChat Files").join("wxid_old");
        file(&a.join("FileStorage/Cache/2020-01/c.jpg"), 10, 400);
        file(&a.join("FileStorage/MsgAttach/h/Image/2020-01/i.dat"), 20, 400);
        file(&a.join("FileStorage/File/2020-01/f.pdf"), 30, 400);
        file(&a.join("Msg/Multi/MSG0.db"), 40, 400);
        file(&a.join("FileStorage/Fav/keep.dat"), 50, 400);
        let now = SystemTime::now();
        let cache = pick(&a, Version::V3, Category::Cache, days_before(now, CACHE_MIN_DAYS), far(), usize::MAX);
        assert_eq!(names(&cache), ["c.jpg"]);
        let chat = pick(&a, Version::V3, Category::Chat, days_before(now, 365), far(), usize::MAX);
        assert_eq!(names(&chat), ["f.pdf", "i.dat"]);
    }

    /// 规则里不能有对账号文件夹、xwechat_files、WeChat Files、Tencent Files 本身的递归：每一条都是账号文件夹下面具体的
    /// 子文件夹，不能是空的、「.」「..」、绝对路径，也不能是放聊天记录数据库的文件夹。
    #[test]
    fn the_whitelist_never_reaches_the_account_root_or_the_databases() {
        for version in [Version::V3, Version::V4] {
            for category in [Category::Cache, Category::Chat] {
                for rel in folders(version, category) {
                    assert!(
                        !rel.is_empty() && !rel.starts_with('/') && !rel.contains('\\') && !rel.contains(':'),
                        "{rel}"
                    );
                    for part in rel.split('/') {
                        assert!(!part.is_empty() && part != "." && part != "..", "{rel}");
                        let lower = part.to_ascii_lowercase();
                        assert!(
                            ![
                                "db_storage",
                                "config",
                                "favorite",
                                "fav",
                                "xwechat_files",
                                "wechat files",
                                "tencent files"
                            ]
                            .contains(&lower.as_str()),
                            "{rel}"
                        );
                    }
                    // 4.x 的 msg、3.x 的 Msg 下面有聊天记录数据库，FileStorage 下面有收藏：只能是它们下面具体的子文件夹
                    assert!(
                        !rel.eq_ignore_ascii_case("msg") && !rel.eq_ignore_ascii_case("FileStorage"),
                        "{rel} 太大了"
                    );
                }
            }
        }
    }

    #[test]
    fn database_and_program_files_are_never_picked() {
        for name in ["a.db", "a.DB", "b.db-wal", "c.sqlite", "tool.exe", "x.dll", "s.ps1", "q.db-journal"] {
            assert!(skipped_extension(Path::new(name)), "{name}");
        }
        for name in ["a.dat", "b.jpg", "c.docx", "d.mp4", "noext"] {
            assert!(!skipped_extension(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn limits_stop_early_and_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let a = v4_account(dir.path());
        let now = SystemTime::now();
        let p = pick(&a, Version::V4, Category::Cache, days_before(now, CACHE_MIN_DAYS), far(), 1);
        assert!(!p.complete);
        assert_eq!(p.files.len(), 1);
        let p = pick(
            &a,
            Version::V4,
            Category::Cache,
            days_before(now, CACHE_MIN_DAYS),
            Instant::now() - Duration::from_secs(1),
            usize::MAX,
        );
        assert!(!p.complete);
        assert!(p.files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let a = v4_account(dir.path());
        let outside = dir.path().join("outside");
        file(&outside.join("precious.jpg"), 10, 400);
        std::os::unix::fs::symlink(&outside, a.join("cache/linked")).unwrap();
        std::os::unix::fs::symlink(outside.join("precious.jpg"), a.join("temp/linked.jpg")).unwrap();
        // 白名单文件夹本身是链接的也不进去
        fs::remove_dir_all(a.join("msg/video")).unwrap();
        std::os::unix::fs::symlink(&outside, a.join("msg/video")).unwrap();
        let now = SystemTime::now();
        for category in [Category::Cache, Category::Chat] {
            let p = pick(&a, Version::V4, category, days_before(now, CACHE_MIN_DAYS), far(), usize::MAX);
            assert!(
                p.files.iter().all(|(f, _)| !f.to_string_lossy().contains("linked") && !f.starts_with(&outside)),
                "{p:?}"
            );
            assert!(!names(&p).contains(&"precious.jpg".to_owned()));
        }
        // 账号文件夹是链接的不算账号
        let root = dir.path().join("xwechat_files");
        std::os::unix::fs::symlink(&a, root.join("wxid_link")).unwrap();
        assert_eq!(accounts(&root, Version::V4), vec![a]);
    }

    #[test]
    fn days_before_counts_whole_days() {
        let now = SystemTime::UNIX_EPOCH + DAY * 1000;
        assert_eq!(days_before(now, 7), SystemTime::UNIX_EPOCH + DAY * 993);
        assert!(days_before(SystemTime::UNIX_EPOCH + DAY, 30) < SystemTime::UNIX_EPOCH + DAY);
    }
}
