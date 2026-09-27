//! 找大文件和重复文件：在用户选的文件夹里，列出最大的文件和内容完全一样的文件，腾地方用。
//!
//! - 只读：不删、不改、不移动任何文件。要删的话，界面上点「在资源管理器中显示」，用户自己在资源管理器里删
//!   （删掉的先进回收站）。
//! - 文件夹只能由后端的系统选择框取得；界面拿到的是相对这个文件夹的路径，「显示」时只传结果里的编号。
//! - 不跟着符号链接、目录联接走；「仅在线」的网盘文件不占这台电脑的空间，读它还会从网上下载，不算。
//! - Windows 文件夹、Program Files、ProgramData 里的和带「系统」属性的文件标成「系统或程序的文件」：
//!   只列进大文件（让用户知道是什么），不参与找重复——系统文件夹里有大量硬链接，看着一样，删了也腾不出地方，
//!   还会弄坏系统或软件。
//! - 重复：大小一样（1 MB 以上）→ 开头 64 KB 一样 → 全部内容逐字节比较，完全一样才算，不靠哈希猜。
//! - 限时：数文件最多 60 秒、50 万个；比较内容最多 60 秒。到了就停，结果里说明。
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;

/// 最多数多少个文件
pub const MAX_FILES: usize = 500_000;
/// 列出最大的多少个文件
pub const LARGEST: usize = 50;
/// 多大的文件才找重复（小文件重复了也腾不出多少地方）
pub const MIN_DUPLICATE: u64 = 1024 * 1024;
/// 最多列出多少组重复
pub const MAX_GROUPS: usize = 50;
/// 一组里最多列出多少个文件
pub const MAX_GROUP_FILES: usize = 20;
const WALK_LIMIT: Duration = Duration::from_secs(60);
const COMPARE_LIMIT: Duration = Duration::from_secs(60);
/// 先比开头这么多字节，不一样的就不用整个读了
const HEAD: usize = 64 * 1024;
const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Default)]
pub struct SpaceState {
    /// 用户选的文件夹
    pub root: Option<PathBuf>,
    /// 最近一次结果里列出的文件，编号就是下标
    pub shown: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceReport {
    /// 选的文件夹（用户自己选的，显示给他看）
    pub folder: String,
    /// 数了多少个文件
    pub files: usize,
    pub total_bytes: u64,
    /// 没有权限打开的子文件夹
    pub skipped: usize,
    /// 没算的「仅在线」网盘文件
    pub online_only: usize,
    /// 文件太多或者时间到了，没数完
    pub truncated: bool,
    pub largest: Vec<SpaceFile>,
    pub duplicates: Vec<DuplicateGroup>,
    /// 一共找到几组重复（可能比列出来的多）
    pub duplicate_groups: usize,
    /// 每组只留一个的话，能腾出多少字节
    pub wasted_bytes: u64,
    /// 所有可能重复的文件都比较完了（时间到了就是 false）
    pub compared_all: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceFile {
    /// 「在资源管理器中显示」时传回来
    pub id: usize,
    pub name: String,
    /// 所在的文件夹，相对选的文件夹；就在选的文件夹里时是空的
    pub folder: String,
    pub size: u64,
    /// 修改时间（1970 年以来的毫秒数）
    pub modified: Option<u64>,
    /// Windows、程序自己的文件，别手动删
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// 每个文件的大小
    pub size: u64,
    /// 这一组一共几个一样的文件
    pub count: usize,
    pub files: Vec<SpaceFile>,
}

struct Entry {
    path: PathBuf,
    size: u64,
    modified: Option<u64>,
    protected: bool,
}

struct Walk {
    entries: Vec<Entry>,
    skipped: usize,
    online_only: usize,
    truncated: bool,
}

/// 数一遍 `root`，返回结果和结果里列出的文件（编号就是下标）。
pub fn scan(root: &Path) -> (SpaceReport, Vec<PathBuf>) {
    scan_with(root, &system_folders(), Instant::now() + WALK_LIMIT, COMPARE_LIMIT)
}

fn scan_with(
    root: &Path,
    system: &[PathBuf],
    walk_deadline: Instant,
    compare_limit: Duration,
) -> (SpaceReport, Vec<PathBuf>) {
    let walk = walk(root, system, walk_deadline);
    let total_bytes = walk.entries.iter().map(|e| e.size).sum();
    let (groups, compared_all) = duplicates(&walk.entries, Instant::now() + compare_limit);

    let mut shown: Vec<PathBuf> = Vec::new();
    let mut ids: HashMap<usize, usize> = HashMap::new();
    let mut row = |index: usize| -> SpaceFile {
        let e = &walk.entries[index];
        let id = *ids.entry(index).or_insert_with(|| {
            shown.push(e.path.clone());
            shown.len() - 1
        });
        SpaceFile {
            id,
            name: e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            folder: e
                .path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            size: e.size,
            modified: e.modified,
            protected: e.protected,
        }
    };

    let mut by_size: Vec<usize> = (0..walk.entries.len()).collect();
    by_size.sort_by(|&a, &b| walk.entries[b].size.cmp(&walk.entries[a].size));
    let largest: Vec<SpaceFile> = by_size.iter().take(LARGEST).map(|&i| row(i)).collect();

    let wasted_bytes = groups.iter().map(|g| walk.entries[g[0]].size * (g.len() as u64 - 1)).sum();
    let duplicate_groups = groups.len();
    let duplicates = groups
        .iter()
        .take(MAX_GROUPS)
        .map(|g| DuplicateGroup {
            size: walk.entries[g[0]].size,
            count: g.len(),
            files: g.iter().take(MAX_GROUP_FILES).map(|&i| row(i)).collect(),
        })
        .collect();

    let report = SpaceReport {
        folder: root.display().to_string(),
        files: walk.entries.len(),
        total_bytes,
        skipped: walk.skipped,
        online_only: walk.online_only,
        truncated: walk.truncated,
        largest,
        duplicates,
        duplicate_groups,
        wasted_bytes,
        compared_all,
    };
    (report, shown)
}

/// Windows 文件夹和装程序的文件夹：里面的文件别手动删。
fn system_folders() -> Vec<PathBuf> {
    ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .collect()
}

/// 在这些文件夹里面（Windows 上不区分大小写）
fn inside(path: &Path, folders: &[PathBuf]) -> bool {
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    let path = lower(path);
    folders.iter().any(|f| {
        let f = lower(f);
        let f = f.trim_end_matches(['\\', '/']);
        path.len() > f.len() && path.starts_with(f) && path[f.len()..].starts_with(['\\', '/'])
    })
}

#[cfg(windows)]
fn attributes(meta: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes(_meta: &fs::Metadata) -> u32 {
    0
}

const ATTR_SYSTEM: u32 = 0x4;
/// FILE_ATTRIBUTE_OFFLINE、RECALL_ON_OPEN、RECALL_ON_DATA_ACCESS：网盘的「仅在线」文件
const ATTR_ONLINE_ONLY: u32 = 0x1000 | 0x40000 | 0x400000;

fn walk(root: &Path, system: &[PathBuf], deadline: Instant) -> Walk {
    let mut walk = Walk { entries: Vec::new(), skipped: 0, online_only: 0, truncated: false };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(list) = fs::read_dir(&dir) else {
            walk.skipped += 1;
            continue;
        };
        for entry in list {
            if walk.entries.len() >= MAX_FILES || Instant::now() > deadline {
                walk.truncated = true;
                return walk;
            }
            let Ok(entry) = entry else { continue };
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let attrs = attributes(&meta);
            if attrs & ATTR_ONLINE_ONLY != 0 {
                if kind.is_file() {
                    walk.online_only += 1;
                }
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                let modified =
                    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
                let protected = attrs & ATTR_SYSTEM != 0 || inside(&path, system);
                walk.entries.push(Entry { path, size: meta.len(), modified, protected });
            }
        }
    }
    walk
}

/// 内容完全一样的几组文件（每组是 `entries` 的下标），能腾出地方最多的在前。第二个值：都比完了没有。
fn duplicates(entries: &[Entry], deadline: Instant) -> (Vec<Vec<usize>>, bool) {
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        if !e.protected && e.size >= MIN_DUPLICATE {
            by_size.entry(e.size).or_default().push(i);
        }
    }
    // 大的先比：同样的时间，腾出的地方最多
    let mut sizes: Vec<u64> = by_size.iter().filter(|(_, v)| v.len() >= 2).map(|(s, _)| *s).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for size in sizes {
        if Instant::now() > deadline {
            return (sorted(groups, entries), false);
        }
        let mut by_head: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
        for &i in &by_size[&size] {
            if Instant::now() > deadline {
                return (sorted(groups, entries), false);
            }
            if let Ok(head) = read_head(&entries[i].path) {
                by_head.entry(head).or_default().push(i);
            }
        }
        for (_, mut rest) in by_head {
            while rest.len() >= 2 {
                let first = rest[0];
                let mut same = vec![first];
                let mut other = Vec::new();
                for &i in &rest[1..] {
                    match same_content(&entries[first].path, &entries[i].path, deadline) {
                        Compare::Same => same.push(i),
                        Compare::Different => other.push(i),
                        Compare::Unreadable => {}
                        Compare::OutOfTime => return (sorted(groups, entries), false),
                    }
                }
                if same.len() >= 2 {
                    groups.push(same);
                }
                rest = other;
            }
        }
    }
    (sorted(groups, entries), true)
}

fn sorted(mut groups: Vec<Vec<usize>>, entries: &[Entry]) -> Vec<Vec<usize>> {
    let wasted = |g: &Vec<usize>| entries[g[0]].size * (g.len() as u64 - 1);
    groups.sort_by_key(|g| std::cmp::Reverse(wasted(g)));
    groups
}

fn read_head(path: &Path) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(HEAD);
    File::open(path)?.take(HEAD as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

enum Compare {
    Same,
    Different,
    /// 打不开或者读到一半出错（比如被别的程序独占着）：这一对不算
    Unreadable,
    OutOfTime,
}

/// 两个一样大的文件，内容是不是完全一样（逐字节比较）。
fn same_content(a: &Path, b: &Path, deadline: Instant) -> Compare {
    let (Ok(mut fa), Ok(mut fb)) = (File::open(a), File::open(b)) else { return Compare::Unreadable };
    let mut ba = vec![0u8; CHUNK];
    let mut bb = vec![0u8; CHUNK];
    loop {
        if Instant::now() > deadline {
            return Compare::OutOfTime;
        }
        let Ok(na) = read_full(&mut fa, &mut ba) else { return Compare::Unreadable };
        let Ok(nb) = read_full(&mut fb, &mut bb) else { return Compare::Unreadable };
        if na != nb || ba[..na] != bb[..nb] {
            return Compare::Different;
        }
        if na == 0 {
            return Compare::Same;
        }
    }
}

/// 读满 `buf`，除非到了文件末尾。
fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的临时文件夹，用完删掉
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-space-{name}-{}", std::process::id()));
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

    fn millis(t: std::time::SystemTime) -> u64 {
        t.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn mb(n: usize, fill: u8) -> Vec<u8> {
        vec![fill; n * 1024 * 1024]
    }

    fn far() -> Instant {
        Instant::now() + Duration::from_secs(600)
    }

    #[test]
    fn largest_files_come_first_with_relative_folders() {
        let dir = Dir::new("largest");
        write(&dir.path().join("小.txt"), b"x");
        write(&dir.path().join("视频").join("大.mp4"), &mb(3, 1));
        write(&dir.path().join("中.zip"), &mb(1, 2));
        let (r, shown) = scan_with(dir.path(), &[], far(), Duration::from_secs(600));
        assert_eq!(r.files, 3);
        assert_eq!(r.total_bytes, 4 * 1024 * 1024 + 1);
        let names: Vec<_> = r.largest.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["大.mp4", "中.zip", "小.txt"]);
        assert_eq!(r.largest[0].folder, "视频");
        assert_eq!(r.largest[1].folder, "");
        assert_eq!(shown[r.largest[0].id], dir.path().join("视频").join("大.mp4"));
        assert!(r.largest[0].modified.is_some_and(|m| m <= millis(std::time::SystemTime::now())));
        assert!(r.duplicates.is_empty() && r.compared_all);
    }

    #[test]
    fn identical_files_are_grouped_and_same_size_different_files_are_not() {
        let dir = Dir::new("dups");
        let a = mb(2, 7);
        write(&dir.path().join("照片.jpg"), &a);
        write(&dir.path().join("备份").join("照片 (1).jpg"), &a);
        write(&dir.path().join("又一份").join("照片.jpg"), &a);
        // 一样大、开头一样，只有最后一个字节不同：不是重复
        let mut b = a.clone();
        *b.last_mut().unwrap() = 8;
        write(&dir.path().join("改过的.jpg"), &b);
        // 一样大、开头就不同
        write(&dir.path().join("别的.bin"), &mb(2, 9));
        // 太小的不找重复
        write(&dir.path().join("小1.txt"), b"same");
        write(&dir.path().join("小2.txt"), b"same");

        let (r, shown) = scan_with(dir.path(), &[], far(), Duration::from_secs(600));
        assert_eq!(r.duplicate_groups, 1, "{:?}", r.duplicates);
        let g = &r.duplicates[0];
        assert_eq!(g.count, 3);
        assert_eq!(g.size, 2 * 1024 * 1024);
        assert_eq!(r.wasted_bytes, 2 * 2 * 1024 * 1024);
        let mut names: Vec<_> =
            g.files.iter().map(|f| shown[f.id].strip_prefix(dir.path()).unwrap().to_path_buf()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                PathBuf::from("又一份").join("照片.jpg"),
                PathBuf::from("备份").join("照片 (1).jpg"),
                PathBuf::from("照片.jpg")
            ]
        );
        // 大文件列表里的同一个文件，编号一样
        let big = r.largest.iter().find(|f| f.name == "照片 (1).jpg").unwrap();
        assert!(g.files.iter().any(|f| f.id == big.id));
    }

    #[test]
    fn system_files_are_listed_but_never_counted_as_duplicates() {
        let dir = Dir::new("system");
        let windows = dir.path().join("Windows");
        let a = mb(1, 3);
        write(&windows.join("System32").join("a.dll"), &a);
        write(&windows.join("WinSxS").join("a.dll"), &a);
        let (r, _) = scan_with(dir.path(), std::slice::from_ref(&windows), far(), Duration::from_secs(600));
        assert!(r.largest.iter().all(|f| f.protected));
        assert_eq!(r.duplicate_groups, 0);
        assert!(!inside(&dir.path().join("WindowsApps").join("x"), std::slice::from_ref(&windows)), "只认整个文件夹名");
    }

    #[test]
    fn time_limits_stop_early_and_say_so() {
        let dir = Dir::new("limits");
        let a = mb(1, 5);
        write(&dir.path().join("1.bin"), &a);
        write(&dir.path().join("2.bin"), &a);
        let (r, _) = scan_with(dir.path(), &[], far(), Duration::ZERO);
        assert!(!r.compared_all);
        let (r, _) = scan_with(dir.path(), &[], Instant::now() - Duration::from_secs(1), Duration::from_secs(600));
        assert!(r.truncated);
        assert_eq!(r.files, 0);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let outside = Dir::new("links-outside");
        write(&outside.path().join("外面.bin"), &mb(1, 1));
        let dir = Dir::new("links");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("链接")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("外面.bin"), dir.path().join("文件链接.bin")).unwrap();
        let (r, _) = scan_with(dir.path(), &[], far(), Duration::from_secs(600));
        assert_eq!(r.files, 0);
    }
}
