//! 批量重命名：只处理用户在系统对话框中选定目录的直属普通文件。
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// Windows MoveFileW refuses an existing destination, unlike rename's replacement behavior.
#[cfg(windows)]
fn move_new(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileW;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn move_new(source: &Path, target: &Path) -> std::io::Result<()> {
    if target.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "target exists"));
    }
    fs::rename(source, target)
}

#[derive(Default)]
pub struct RenameState {
    pub folder: Option<PathBuf>,
    pub preview: Option<Snapshot>,
}

#[derive(Clone, PartialEq)]
pub struct Snapshot {
    prefix: String,
    files: Vec<FileSnapshot>,
}

#[derive(Clone, PartialEq)]
struct FileSnapshot {
    source: String,
    target: String,
    size: u64,
    modified: Option<u128>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntry {
    pub source: String,
    pub target: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreview {
    pub folder: String,
    pub entries: Vec<RenameEntry>,
}

fn validate_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() || prefix.chars().count() > 80 {
        return Err("前缀需要 1 到 80 个字符。".into());
    }
    if prefix.ends_with(' ') || prefix.ends_with('.')
        || prefix.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err("前缀包含 Windows 文件名不允许的字符，或以空格、句点结尾。".into());
    }
    Ok(())
}

fn scan(folder: &Path, prefix: &str) -> Result<Snapshot, String> {
    validate_prefix(prefix)?;
    let meta = fs::symlink_metadata(folder).map_err(|e| format!("无法打开所选文件夹：{e}"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("所选位置已不是普通文件夹，请重新选择。".into());
    }
    let mut files = Vec::new();
    for item in fs::read_dir(folder).map_err(|e| format!("无法读取文件夹：{e}"))? {
        let item = item.map_err(|e| format!("读取文件列表失败：{e}"))?;
        let meta = item.metadata().map_err(|e| format!("读取文件属性失败：{e}"))?;
        if !meta.is_file() || item.file_type().is_ok_and(|t| t.is_symlink()) {
            continue;
        }
        let source = item.file_name().into_string().map_err(|_| "文件夹里有无法显示名称的文件。".to_string())?;
        files.push((source, meta));
        if files.len() > 500 {
            return Err("一次最多处理 500 个文件，请先缩小文件夹范围。".into());
        }
    }
    if files.is_empty() {
        return Err("文件夹里没有可重命名的普通文件。".into());
    }
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0)));
    let width = files.len().to_string().len().max(2);
    let mut targets = HashSet::new();
    let mut snapshot = Vec::with_capacity(files.len());
    for (index, (source, meta)) in files.into_iter().enumerate() {
        let extension = Path::new(&source).extension().and_then(|s| s.to_str()).map_or(String::new(), |s| format!(".{s}"));
        let target = format!("{prefix}{:0width$}{extension}", index + 1);
        if target.encode_utf16().count() > 255 {
            return Err("生成的文件名过长，请缩短前缀。".into());
        }
        if !targets.insert(target.to_lowercase()) || folder.join(&target).exists() {
            return Err(format!("目标文件 {target} 已存在。请换一个前缀。"));
        }
        snapshot.push(FileSnapshot {
            source,
            target,
            size: meta.len(),
            modified: meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()),
        });
    }
    Ok(Snapshot { prefix: prefix.to_string(), files: snapshot })
}

pub fn preview(state: &mut RenameState, prefix: &str) -> Result<RenamePreview, String> {
    state.preview = None;
    let folder = state.folder.as_ref().ok_or("请先选择文件夹。")?;
    let snapshot = scan(folder, prefix)?;
    let result = RenamePreview {
        folder: folder.display().to_string(),
        entries: snapshot.files.iter().map(|f| RenameEntry { source: f.source.clone(), target: f.target.clone() }).collect(),
    };
    state.preview = Some(snapshot);
    Ok(result)
}

pub fn apply(state: &mut RenameState) -> Result<usize, String> {
    let expected = state.preview.take().ok_or("请先预览重命名结果。")?;
    let folder = state.folder.as_ref().ok_or("请重新选择文件夹。")?;
    if scan(folder, &expected.prefix)? != expected {
        return Err("预览后文件夹内容发生了变化，请重新预览。".into());
    }
    let mut done = Vec::new();
    for file in &expected.files {
        if let Err(error) = move_new(&folder.join(&file.source), &folder.join(&file.target)) {
            let mut rollback_failed = 0;
            for previous in done.iter().rev() {
                if move_new(&folder.join(&previous.target), &folder.join(&previous.source)).is_err() {
                    rollback_failed += 1;
                }
            }
            return Err(format!("重命名 {} 失败：{error}。{}", file.source, if rollback_failed == 0 { "已恢复先前改动。".to_string() } else { format!("有 {rollback_failed} 个文件未能恢复，请检查文件夹。") }));
        }
        done.push(file);
    }
    Ok(done.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_apply_and_conflict() {
        let root = std::env::temp_dir().join(format!("medkit-rename-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("b.txt"), b"b").unwrap();
        fs::write(root.join("a.jpg"), b"a").unwrap();
        let mut state = RenameState { folder: Some(root.clone()), preview: None };
        let plan = preview(&mut state, "照片_").unwrap();
        assert_eq!(plan.entries[0].target, "照片_01.jpg");
        assert_eq!(plan.entries[1].target, "照片_02.txt");
        assert_eq!(apply(&mut state).unwrap(), 2);
        assert_eq!(fs::read(root.join("照片_01.jpg")).unwrap(), b"a");
        assert!(preview(&mut state, "照片_").is_err());
        let plan = preview(&mut state, "归档_").unwrap();
        assert_eq!(plan.entries.len(), 2);
        fs::write(root.join("new.txt"), b"new").unwrap();
        assert!(apply(&mut state).unwrap_err().contains("发生了变化"));
        assert!(root.join("照片_01.jpg").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
