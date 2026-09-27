//! 硬盘测速的命令部分：列出能测的盘，测一个盘。测的办法在 medkit_core::disk_speed。
//! - 只列固定的和可移动的盘（光驱、网络驱动器不列），剩余空间不到 2 GB 的列出来但不能测；
//! - 界面只传盘符，而且必须是现在列出来、能测的盘；同一时间只测一个。
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveView {
    /// 盘符，比如「C」
    pub letter: String,
    pub label: String,
    pub file_system: String,
    pub removable: bool,
    /// Windows 装在这个盘上
    pub system: bool,
    pub total: u64,
    pub free: u64,
    /// 剩余空间够不够测
    pub can_test: bool,
}

#[cfg(windows)]
pub fn drives() -> Vec<DriveView> {
    let system = std::env::var("SystemDrive").unwrap_or_default().to_ascii_uppercase();
    medkit_core::platform::windows::drives()
        .into_iter()
        .map(|d| DriveView {
            letter: d.letter.to_string(),
            label: d.label,
            file_system: d.file_system,
            removable: d.removable,
            system: system.starts_with(d.letter),
            total: d.total,
            free: d.free,
            can_test: d.free >= medkit_core::disk_speed::MIN_FREE,
        })
        .collect()
}

#[cfg(not(windows))]
pub fn drives() -> Vec<DriveView> {
    Vec::new()
}

/// 盘符只能是一个字母，而且是现在列出来、能测的盘
pub fn check_letter(letter: &str, list: &[DriveView]) -> Result<String, String> {
    let letter = letter.trim().trim_end_matches([':', '\\']).to_ascii_uppercase();
    if letter.len() != 1 || !letter.chars().all(|c| c.is_ascii_uppercase()) {
        return Err("盘符不对。".into());
    }
    let drive = list.iter().find(|d| d.letter == letter).ok_or("这个盘现在不在了，请刷新一下列表。")?;
    if !drive.can_test {
        return Err("这个盘剩余空间不到 2 GB，测速要写一个临时文件，先腾出点地方再测。".into());
    }
    Ok(letter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(letter: &str, can_test: bool) -> DriveView {
        DriveView {
            letter: letter.into(),
            label: String::new(),
            file_system: "NTFS".into(),
            removable: false,
            system: false,
            total: 1,
            free: 1,
            can_test,
        }
    }

    #[test]
    fn only_listed_drives_with_room_can_be_tested() {
        let list = [drive("C", true), drive("E", false)];
        assert_eq!(check_letter("c", &list).unwrap(), "C");
        assert_eq!(check_letter("C:\\", &list).unwrap(), "C");
        assert!(check_letter("D", &list).unwrap_err().contains("不在了"));
        assert!(check_letter("E", &list).unwrap_err().contains("2 GB"));
        for bad in ["", "CD", "1", "..", "C:\\Windows"] {
            assert!(check_letter(bad, &list).is_err(), "{bad}");
        }
    }
}
