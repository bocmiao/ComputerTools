//! 回收站坏了：列出各个盘的回收站里有多少东西，清空并重建其中一个（办法在 medkit_core::recycle_bin，照微软
//! 《The Recycle Bin is corrupted》）。
//! - 只列固定的和可移动的盘；界面只传盘符，而且必须是现在还在的盘；
//! - 结果里只有盘符、卷标、个数和大小，没有文件名（回收站里按账户分文件夹，文件夹名是 SID，也不给）。
use std::path::PathBuf;
use std::time::{Duration, Instant};

use medkit_core::recycle_bin::{self, Removed};
use serde::Serialize;

/// 数所有盘最多花多久（移动硬盘、U 盘慢）
const MEASURE_TOTAL: Duration = Duration::from_secs(8);
/// 一个盘最多数多少个文件
const MEASURE_FILES: u64 = 200_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleDriveView {
    /// 盘符，比如「C」
    pub letter: String,
    pub label: String,
    pub removable: bool,
    /// Windows 装在这个盘上
    pub system: bool,
    /// 这个盘上有回收站文件夹
    pub exists: bool,
    pub files: u64,
    pub bytes: u64,
    /// 数完了（没数完的是「至少这么多」）
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleRepairView {
    pub letter: String,
    /// absent：本来就没有回收站文件夹；done：删干净了；partly：有的文件删不掉
    pub outcome: &'static str,
    /// 删不掉、还剩下的文件个数
    pub left: u64,
}

struct DriveInfo {
    letter: char,
    label: String,
    removable: bool,
    system: bool,
}

#[cfg(windows)]
fn drive_list() -> Vec<DriveInfo> {
    let system = std::env::var("SystemDrive").unwrap_or_default().to_ascii_uppercase();
    medkit_core::platform::windows::drives()
        .into_iter()
        .map(|d| DriveInfo {
            letter: d.letter,
            label: d.label,
            removable: d.removable,
            system: system.starts_with(d.letter),
        })
        .collect()
}

#[cfg(not(windows))]
fn drive_list() -> Vec<DriveInfo> {
    Vec::new()
}

fn root(letter: char) -> PathBuf {
    PathBuf::from(format!("{letter}:\\"))
}

/// 各个盘的回收站里有多少东西（一共最多数 8 秒，数不完的盘标「至少」）。
pub fn drives() -> Vec<RecycleDriveView> {
    let start = Instant::now();
    drive_list()
        .into_iter()
        .map(|d| {
            let left = MEASURE_TOTAL.saturating_sub(start.elapsed());
            let contents = recycle_bin::measure(&root(d.letter), left, MEASURE_FILES);
            RecycleDriveView {
                letter: d.letter.to_string(),
                label: d.label,
                removable: d.removable,
                system: d.system,
                exists: contents.is_some(),
                files: contents.map_or(0, |c| c.files),
                bytes: contents.map_or(0, |c| c.bytes),
                complete: contents.is_none_or(|c| c.complete),
            }
        })
        .collect()
}

/// 盘符只能是一个字母，而且是现在还在的盘。
pub fn check_letter(letter: &str, present: &[char]) -> Result<char, String> {
    let letter = letter.trim().trim_end_matches([':', '\\']).to_ascii_uppercase();
    let mut chars = letter.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return Err("盘符不对。".into()) };
    if !c.is_ascii_uppercase() {
        return Err("盘符不对。".into());
    }
    if !present.contains(&c) {
        return Err("这个盘现在不在了，请刷新一下列表。".into());
    }
    Ok(c)
}

/// 清空并重建一个盘的回收站：删掉 `<盘>:\$Recycle.Bin`，重启以后 Windows 重新建一个。
pub fn repair(letter: &str) -> Result<RecycleRepairView, String> {
    let present: Vec<char> = drive_list().iter().map(|d| d.letter).collect();
    let c = check_letter(letter, &present)?;
    let removed = recycle_bin::remove(&root(c)).map_err(|e| format!("没能删掉 {c} 盘的回收站：{e}"))?;
    let (outcome, left) = match removed {
        Removed::Absent => ("absent", 0),
        Removed::Done => ("done", 0),
        Removed::Partly { left } => ("partly", left),
    };
    Ok(RecycleRepairView { letter: c.to_string(), outcome, left })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_must_be_one_of_the_present_drives() {
        let present = ['C', 'D'];
        assert_eq!(check_letter("d", &present), Ok('D'));
        assert_eq!(check_letter("C:\\", &present), Ok('C'));
        assert!(check_letter("E", &present).unwrap_err().contains("不在了"));
        assert!(check_letter("CD", &present).is_err());
        assert!(check_letter("", &present).is_err());
        assert!(check_letter("1", &present).is_err());
        assert!(check_letter("..", &present).is_err());
    }
}
