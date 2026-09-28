//! 回收站坏了：「X:\ 上的回收站已损坏。是否清空该驱动器上的回收站?」点了「是」还是一再弹出来时，照微软《The Recycle Bin is
//! corrupted》的办法：删掉那个盘根目录下的 `$Recycle.Bin` 文件夹（文章里是在管理员命令提示符里 `RD X:\$Recycle.bin /s /q`），
//! 再重启电脑，Windows 会重新建一个。
//!
//! 这会清空那个盘上所有账户的回收站，删了找不回来，所以先数一数里面有多少东西，界面上说清楚再动手。
//! 只删 `<盘的根目录>\$Recycle.Bin` 这一个位置；它是链接（符号链接、目录联接）时只删链接本身，不跟着删链接指向的地方。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 回收站文件夹的名字（每个盘的根目录下一个）。
pub const FOLDER: &str = "$Recycle.Bin";

/// 一个盘的回收站里有多少东西。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Contents {
    pub files: u64,
    pub bytes: u64,
    /// 数完了（文件太多、太慢时只数了一部分，「至少这么多」）
    pub complete: bool,
}

/// 删完以后怎么样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// 本来就没有回收站文件夹
    Absent,
    /// 删干净了
    Done,
    /// 有的文件删不掉（正在用、没有权限），剩下这么多
    Partly { left: u64 },
}

/// 盘的根目录下的回收站文件夹。
pub fn folder(root: &Path) -> PathBuf {
    root.join(FOLDER)
}

/// 数一数回收站里有多少文件、一共多大：最多数 `limit` 这么久、`max_files` 个。没有回收站文件夹时是 `None`。
/// 读不了的文件夹跳过（没有权限的别的账户的回收站）；链接（符号链接、目录联接）不跟着走，也不算。
pub fn measure(root: &Path, limit: Duration, max_files: u64) -> Option<Contents> {
    let top = folder(root);
    let meta = fs::symlink_metadata(&top).ok()?;
    let mut c = Contents { complete: true, ..Default::default() };
    if !meta.is_dir() {
        // 不是文件夹（坏了的一种样子），也算有东西要删
        c.files = 1;
        c.bytes = meta.len();
        return Some(c);
    }
    let start = Instant::now();
    let mut stack = vec![top];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if start.elapsed() > limit || c.files >= max_files {
                c.complete = false;
                return Some(c);
            }
            let Ok(meta) = entry.path().symlink_metadata() else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                c.files += 1;
                c.bytes += meta.len();
            }
        }
    }
    Some(c)
}

/// 删掉回收站文件夹（相当于 `RD <盘>\$Recycle.bin /s /q`）。删不干净时再数一次还剩多少。
pub fn remove(root: &Path) -> io::Result<Removed> {
    let top = folder(root);
    let meta = match fs::symlink_metadata(&top) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Removed::Absent),
        Err(e) => return Err(e),
    };
    let result = if meta.file_type().is_symlink() {
        // 符号链接、目录联接：只删链接本身（Windows 上指向文件夹的链接要当文件夹删）
        fs::remove_file(&top).or_else(|_| fs::remove_dir(&top))
    } else if meta.is_dir() {
        // 里面的链接也不跟着走。删不掉时都去掉只读再删一次（NTFS 上会忽略只读直接删，FAT32、exFAT 的盘上不会）
        fs::remove_dir_all(&top).or_else(|_| {
            clear_readonly(&top);
            fs::remove_dir_all(&top)
        })
    } else {
        remove_file(&top, &meta)
    };
    match result {
        Ok(()) => Ok(Removed::Done),
        Err(e) => match measure(root, Duration::from_secs(5), 100_000) {
            None => Ok(Removed::Done),
            Some(left) if left.files == 0 && !top.exists() => Ok(Removed::Done),
            Some(left) => {
                if left.files == 0 {
                    // 只剩空文件夹也删不掉：照实报错
                    return Err(e);
                }
                Ok(Removed::Partly { left: left.files })
            }
        },
    }
}

/// 把文件夹和里面的文件、文件夹都去掉只读（不跟着链接走）。
fn clear_readonly(top: &Path) {
    let mut stack = vec![top.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = path.symlink_metadata() else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.permissions().readonly() {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(&path, perms);
        }
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(&path)
        {
            stack.extend(entries.flatten().map(|e| e.path()));
        }
    }
}

/// 删一个文件，只读的先去掉只读。
fn remove_file(path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let mut perms = meta.permissions();
    if perms.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    #[test]
    fn contents_are_counted_and_removed() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(measure(root.path(), Duration::from_secs(5), 100), None);
        assert_eq!(remove(root.path()).unwrap(), Removed::Absent);

        let bin = folder(root.path());
        write(&bin.join("S-1-5-21-1-2-3-1001").join("$IABC123.txt"), 10);
        write(&bin.join("S-1-5-21-1-2-3-1001").join("$RABC123.txt"), 1000);
        write(&bin.join("S-1-5-21-1-2-3-1002").join("$RDEF456").join("inner.doc"), 500);
        let c = measure(root.path(), Duration::from_secs(5), 100).unwrap();
        assert_eq!(c, Contents { files: 3, bytes: 1510, complete: true });
        let c = measure(root.path(), Duration::from_secs(5), 2).unwrap();
        assert!(!c.complete && c.files == 2, "{c:?}");

        assert_eq!(remove(root.path()).unwrap(), Removed::Done);
        assert!(!bin.exists());
        // 盘上别的东西不动
        write(&root.path().join("keep.txt"), 1);
        write(&bin.join("x"), 1);
        remove(root.path()).unwrap();
        assert!(root.path().join("keep.txt").exists());
    }

    #[test]
    fn a_file_in_place_of_the_folder_is_removed_too() {
        let root = tempfile::tempdir().unwrap();
        write(&folder(root.path()), 42);
        assert_eq!(measure(root.path(), Duration::from_secs(5), 100).unwrap().bytes, 42);
        assert_eq!(remove(root.path()).unwrap(), Removed::Done);
        assert!(!folder(root.path()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_removed_without_touching_what_it_points_to() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&elsewhere.path().join("precious.txt"), 5);
        std::os::unix::fs::symlink(elsewhere.path(), folder(root.path())).unwrap();
        remove(root.path()).unwrap();
        assert!(!folder(root.path()).exists());
        assert!(elsewhere.path().join("precious.txt").exists());

        // 回收站里面的链接：不数、不跟着删
        write(&folder(root.path()).join("S-1").join("$RA.txt"), 3);
        std::os::unix::fs::symlink(elsewhere.path(), folder(root.path()).join("S-1").join("link")).unwrap();
        assert_eq!(
            measure(root.path(), Duration::from_secs(5), 100),
            Some(Contents { files: 1, bytes: 3, complete: true })
        );
        assert_eq!(remove(root.path()).unwrap(), Removed::Done);
        assert!(!folder(root.path()).exists());
        assert!(elsewhere.path().join("precious.txt").exists());
    }
}
