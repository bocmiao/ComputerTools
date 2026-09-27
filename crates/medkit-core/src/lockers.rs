//! 文件删不掉的时候，看是谁占着（工具箱里的「文件删不掉：是谁占着」）。
//!
//! 用 [`Platform::file_users`]（Windows 上是重启管理器）查，只读：不关程序，不动文件。
//! - 选了几个文件：一个一个查，说得出哪个程序占着哪个文件；
//! - 选了一个文件夹：重启管理器只收文件，所以把里面的文件一起登记、查一次（不跟着符号链接和目录联接
//!   走到文件夹外面去）；文件不多时再一个一个查，说出是哪几个文件。
//!
//! 文件夹本身被占着（比如命令行窗口停在这个文件夹里）查不出来，界面上有说明。
//! 结果里只有文件名（文件夹里的用相对路径），不带完整路径：完整路径里常有用户名。

use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::{FileUser, PResult, Platform};
use crate::views::{FileLockMode, FileLockReport, FileLockUser};

/// 最多查几个选中的文件
pub const MAX_FILES: usize = 100;
/// 文件夹里最多查多少个文件
pub const MAX_FOLDER_FILES: usize = 5000;
/// 文件夹里的文件不超过这么多时，一个一个查出是哪几个文件被占着
pub const MAP_FOLDER_FILES: usize = 100;
/// 每个程序最多列几个文件名
pub const MAX_NAMES: usize = 20;

/// 要查什么：几个文件，或者一个文件夹。路径只来自后端的选择框。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockTarget {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
}

/// 查到的一个进程和它在用的文件（显示用的名字）。
struct Found {
    user: FileUser,
    files: Vec<String>,
}

pub fn find(platform: &dyn Platform, target: &LockTarget) -> PResult<FileLockReport> {
    match target {
        LockTarget::Files(paths) => {
            let truncated = paths.len() > MAX_FILES;
            let paths = &paths[..paths.len().min(MAX_FILES)];
            let mut missing = Vec::new();
            let mut present = Vec::new();
            for path in paths {
                if fs::symlink_metadata(path).is_ok() {
                    present.push((path.clone(), name_of(path)));
                } else {
                    missing.push(name_of(path));
                }
            }
            let mut found = Vec::new();
            let failed = one_by_one(platform, &present, &mut found)?;
            Ok(FileLockReport {
                mode: FileLockMode::Files,
                targets: paths.iter().map(|p| name_of(p)).collect(),
                checked: present.len() - failed.len(),
                missing,
                failed,
                truncated,
                unreadable: 0,
                users: users(found),
            })
        }
        LockTarget::Folder(root) => {
            let listing = walk(root);
            let files: Vec<(PathBuf, String)> = listing
                .files
                .into_iter()
                .map(|p| {
                    let label = p.strip_prefix(root).map(|r| r.display().to_string()).unwrap_or_else(|_| name_of(&p));
                    (p, label)
                })
                .collect();
            let mut found = Vec::new();
            let mut failed = Vec::new();
            if !files.is_empty() {
                let paths: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
                match platform.file_users(&paths) {
                    // 有人在用、文件又不多：一个一个查，说出是哪几个文件
                    Ok(list) if !list.is_empty() && files.len() <= MAP_FOLDER_FILES => {
                        failed = one_by_one(platform, &files, &mut found)?;
                    }
                    Ok(list) => list.into_iter().for_each(|u| add(&mut found, u, None)),
                    // 一起查失败（比如其中一个路径太长），文件不多时一个一个查，跳过查不了的
                    Err(_) if files.len() <= MAP_FOLDER_FILES => {
                        failed = one_by_one(platform, &files, &mut found)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(FileLockReport {
                mode: FileLockMode::Folder,
                targets: vec![name_of(root)],
                checked: files.len() - failed.len(),
                missing: Vec::new(),
                failed,
                truncated: listing.truncated,
                unreadable: listing.unreadable,
                users: users(found),
            })
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// 一个一个文件地查，记下每个进程在用哪些文件。返回没能查的文件；一个都没能查时返回第一个错误。
fn one_by_one(platform: &dyn Platform, files: &[(PathBuf, String)], found: &mut Vec<Found>) -> PResult<Vec<String>> {
    let mut failed = Vec::new();
    let mut first_error = None;
    for (path, label) in files {
        match platform.file_users(std::slice::from_ref(path)) {
            Ok(list) => list.into_iter().for_each(|u| add(found, u, Some(label))),
            Err(e) => {
                failed.push(label.clone());
                first_error.get_or_insert(e);
            }
        }
    }
    match first_error {
        Some(e) if failed.len() == files.len() => Err(e),
        _ => Ok(failed),
    }
}

/// 同一个进程（进程号和启动时间都一样）只记一次，文件名合在一起。
fn add(found: &mut Vec<Found>, user: FileUser, file: Option<&str>) {
    let index = match found.iter().position(|f| f.user.pid == user.pid && f.user.started == user.started) {
        Some(i) => i,
        None => {
            found.push(Found { user, files: Vec::new() });
            found.len() - 1
        }
    };
    let entry = &mut found[index];
    if let Some(file) = file
        && !entry.files.iter().any(|f| f == file)
    {
        entry.files.push(file.to_owned());
    }
}

/// 显示用的名字：重启管理器给的名字里有路径（命令行窗口的标题常是程序的完整路径）时，改用程序的文件名。
fn display_name(user: &FileUser) -> String {
    let name = user.app_name.trim();
    if !name.is_empty() && !name.contains('\\') && !name.contains('/') {
        return name.to_owned();
    }
    user.program.clone().unwrap_or_else(|| format!("进程 {}", user.pid))
}

fn users(found: Vec<Found>) -> Vec<FileLockUser> {
    let me = std::process::id();
    let mut out: Vec<FileLockUser> = found
        .into_iter()
        .map(|f| {
            let more_files = f.files.len().saturating_sub(MAX_NAMES);
            let mut files = f.files;
            files.truncate(MAX_NAMES);
            FileLockUser {
                pid: f.user.pid,
                name: display_name(&f.user),
                program: f.user.program,
                kind: f.user.kind,
                service: f.user.service,
                files,
                more_files,
                is_self: f.user.pid == me,
                other_session: f.user.other_session,
            }
        })
        .collect();
    // 能自己关掉的排在前面：有窗口的程序、命令行、资源管理器、后台程序、服务、关键进程
    out.sort_by(|a, b| (a.kind, &a.name, a.pid).cmp(&(b.kind, &b.name, b.pid)));
    out
}

struct Listing {
    files: Vec<PathBuf>,
    truncated: bool,
    unreadable: usize,
}

/// 文件夹里的文件，最多 [`MAX_FOLDER_FILES`] 个。符号链接和目录联接不跟进去：它们指向文件夹外面，
/// 删文件夹时删掉的只是链接本身。
fn walk(root: &Path) -> Listing {
    let mut listing = Listing { files: Vec::new(), truncated: false, unreadable: 0 };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            listing.unreadable += 1;
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                listing.unreadable += 1;
                continue;
            };
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(entry.path());
            } else if listing.files.len() >= MAX_FOLDER_FILES {
                listing.truncated = true;
                return listing;
            } else {
                listing.files.push(entry.path());
            }
        }
    }
    listing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::FileUserKind;
    use crate::platform::mock::MockPlatform;

    fn user(pid: u32, name: &str, program: &str, kind: FileUserKind) -> FileUser {
        FileUser {
            pid,
            started: u64::from(pid) * 10,
            app_name: name.into(),
            program: Some(program.into()),
            service: None,
            kind,
            other_session: false,
        }
    }

    #[test]
    fn files_are_checked_one_by_one_and_merged_per_program() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("报告.docx");
        let b = dir.path().join("数据.xlsx");
        let c = dir.path().join("照片.jpg");
        for p in [&a, &b, &c] {
            fs::write(p, b"x").unwrap();
        }
        let gone = dir.path().join("已经删了.txt");
        let p = MockPlatform::new();
        let office = user(100, "Microsoft Office", "WINWORD.EXE", FileUserKind::Window);
        p.use_file(&a, office.clone());
        p.use_file(&b, office);
        p.use_file(&c, user(200, "Windows 资源管理器", "explorer.exe", FileUserKind::Explorer));
        p.use_file(&c, user(300, "Windows Defender Antivirus Service", "MsMpEng.exe", FileUserKind::Service));

        let r = find(&p, &LockTarget::Files(vec![a, b, c, gone])).unwrap();
        assert_eq!(r.mode, FileLockMode::Files);
        assert_eq!(r.targets, ["报告.docx", "数据.xlsx", "照片.jpg", "已经删了.txt"]);
        assert_eq!(r.checked, 3);
        assert_eq!(r.missing, ["已经删了.txt"]);
        assert!(r.failed.is_empty() && !r.truncated);
        let names: Vec<_> = r.users.iter().map(|u| u.name.as_str()).collect();
        // 有窗口的程序在前，服务在后
        assert_eq!(names, ["Microsoft Office", "Windows 资源管理器", "Windows Defender Antivirus Service"]);
        assert_eq!(r.users[0].files, ["报告.docx", "数据.xlsx"]);
        assert_eq!(r.users[1].files, ["照片.jpg"]);
        assert!(r.users.iter().all(|u| !u.is_self));
    }

    #[test]
    fn a_folder_is_checked_at_once_and_mapped_when_small() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("子文件夹");
        fs::create_dir(&sub).unwrap();
        let held = sub.join("在用.log");
        fs::write(&held, b"x").unwrap();
        fs::write(dir.path().join("没人用.txt"), b"x").unwrap();
        let p = MockPlatform::new();
        p.use_file(&held, user(100, "", "sync.exe", FileUserKind::Other));

        let r = find(&p, &LockTarget::Folder(dir.path().to_path_buf())).unwrap();
        assert_eq!(r.mode, FileLockMode::Folder);
        assert_eq!(r.checked, 2);
        assert_eq!(r.users.len(), 1);
        // 重启管理器没给名字时用程序的文件名；文件用相对这个文件夹的路径
        assert_eq!(r.users[0].name, "sync.exe");
        assert_eq!(r.users[0].files, [Path::new("子文件夹").join("在用.log").display().to_string()]);
        assert_eq!(r.targets, [name_of(dir.path())]);
    }

    #[test]
    fn a_big_folder_is_capped_and_not_mapped() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..=MAP_FOLDER_FILES {
            fs::write(dir.path().join(format!("{i}.txt")), b"x").unwrap();
        }
        let p = MockPlatform::new();
        p.use_file(&dir.path().join("7.txt"), user(100, "编辑器", "editor.exe", FileUserKind::Window));
        let r = find(&p, &LockTarget::Folder(dir.path().to_path_buf())).unwrap();
        assert_eq!(r.checked, MAP_FOLDER_FILES + 1);
        assert_eq!(r.users.len(), 1);
        assert!(r.users[0].files.is_empty(), "文件太多时说不出是哪几个");
        assert!(!r.truncated);
    }

    #[test]
    fn names_with_paths_and_long_file_lists_are_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let p = MockPlatform::new();
        let console = user(100, r"C:\Windows\system32\cmd.exe", "cmd.exe", FileUserKind::Console);
        let mut files = Vec::new();
        for i in 0..MAX_NAMES + 3 {
            let f = dir.path().join(format!("{i:02}.txt"));
            fs::write(&f, b"x").unwrap();
            p.use_file(&f, console.clone());
            files.push(f);
        }
        let r = find(&p, &LockTarget::Files(files)).unwrap();
        assert_eq!(r.users[0].name, "cmd.exe", "名字里有路径时用程序的文件名");
        assert_eq!(r.users[0].files.len(), MAX_NAMES);
        assert_eq!(r.users[0].more_files, 3);
    }

    #[test]
    fn a_file_that_cannot_be_checked_is_listed_and_the_rest_still_are() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("a.txt");
        let bad = dir.path().join("b.txt");
        fs::write(&ok, b"x").unwrap();
        fs::write(&bad, b"x").unwrap();
        let p = MockPlatform::new();
        p.use_file(&ok, user(100, "记事本", "notepad.exe", FileUserKind::Window));
        p.fail_file_users(&bad);
        let r = find(&p, &LockTarget::Files(vec![ok, bad.clone()])).unwrap();
        assert_eq!(r.failed, ["b.txt"]);
        assert_eq!(r.checked, 1);
        assert_eq!(r.users.len(), 1);
        // 全都查不了：报错
        assert!(find(&p, &LockTarget::Files(vec![bad])).is_err());
    }

    #[test]
    fn too_many_files_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<PathBuf> = (0..MAX_FILES + 5).map(|i| dir.path().join(format!("{i}.txt"))).collect();
        let r = find(&MockPlatform::new(), &LockTarget::Files(files)).unwrap();
        assert!(r.truncated);
        assert_eq!(r.targets.len(), MAX_FILES);
        assert_eq!(r.missing.len(), MAX_FILES);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_folders_are_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("外面的.txt"), b"x").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("链接")).unwrap();
        fs::write(dir.path().join("里面的.txt"), b"x").unwrap();
        let listing = walk(dir.path());
        assert_eq!(listing.files, [dir.path().join("里面的.txt")]);
    }
}
