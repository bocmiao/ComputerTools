//! 长图拼接的保存部分。长图在界面里拼好（画布，用 WebView2 自带的编码器存成 JPG 或 PNG，不加新的依赖），
//! 这里只负责存盘，办法和图片合成 PDF 一样（[`crate::pdf::target_with`]、[`crate::pdf::write`]）：
//! - 存到哪里、叫什么，由用户在系统的「另存为」对话框里选；
//! - 只收完整的 JPG（开头 FF D8 FF、最后 FF D9）和 PNG（签名开头、IEND 结尾），最大 [`MAX_BYTES`]；
//!   对话框只列这一种图片，建议的名字也换成这一种的扩展名；
//! - 选的名字不是这种图片的扩展名的，后面补上；先写临时文件，写完再换上正式的名字。
use std::path::{Path, PathBuf};

/// 一张长图最多多少字节（画布最长 32767 像素，PNG 也在这个范围里）
pub const MAX_BYTES: usize = 300 * 1024 * 1024;
/// 界面没给、或者给的文件名不能用时，「另存为」对话框里默认的名字（不带扩展名）
pub const DEFAULT_STEM: &str = "长图";

#[derive(Debug, Default)]
pub struct LongImageState {
    /// 最近一次存好的长图（「在文件夹里显示」用）
    pub saved: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
}

impl Kind {
    /// 认可的扩展名，第一个是补扩展名时用的
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Png => &["png"],
        }
    }

    /// 「另存为」对话框里文件类型的名字
    pub fn filter_name(self) -> &'static str {
        match self {
            Self::Jpeg => "JPG 图片",
            Self::Png => "PNG 图片",
        }
    }
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// PNG 最后一块：长度 0 的 IEND 和它的 CRC
const PNG_END: &[u8] = &[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

/// 界面拼好的内容是不是一张完整的 JPG 或 PNG。
pub fn check(bytes: &[u8]) -> Result<Kind, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!("长图太大了（超过 {} MB），没有保存。少拼几张，或者存成 JPG。", MAX_BYTES / 1024 / 1024));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return if bytes.ends_with(&[0xFF, 0xD9]) {
            Ok(Kind::Jpeg)
        } else {
            Err("图片不完整，没有保存。".into())
        };
    }
    if bytes.starts_with(PNG_SIGNATURE) {
        return if bytes.ends_with(PNG_END) { Ok(Kind::Png) } else { Err("图片不完整，没有保存。".into()) };
    }
    Err("内容不是 JPG 或 PNG 图片，没有保存。".into())
}

/// 「另存为」对话框里建议的名字：界面给的名字（已经检查过能用）换成这种图片的扩展名。
pub fn suggested_name(name: Option<&str>, kind: Kind) -> String {
    let stem = name
        .map(|n| Path::new(n).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_STEM.to_owned());
    format!("{stem}.{}", kind.extensions()[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut v = PNG_SIGNATURE.to_vec();
        v.extend_from_slice(b"\0\0\0\x0dIHDR........\0\0\0\0");
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(PNG_END);
        v
    }

    #[test]
    fn only_complete_jpegs_and_pngs_are_accepted() {
        assert_eq!(check(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, 0xFF, 0xD9]), Ok(Kind::Jpeg));
        assert_eq!(check(&png()), Ok(Kind::Png));
        assert!(check(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]).unwrap_err().contains("不完整"));
        let mut cut = png();
        cut.truncate(cut.len() - 4);
        assert!(check(&cut).unwrap_err().contains("不完整"));
        assert!(check(b"%PDF-1.4\n%%EOF").unwrap_err().contains("不是 JPG 或 PNG"));
        assert!(check(b"RIFF\0\0\0\0WEBPVP8 ").unwrap_err().contains("不是 JPG 或 PNG"), "WebP 不收");
        assert!(check(b"").is_err());
        let mut big = vec![0xFF, 0xD8, 0xFF];
        big.resize(MAX_BYTES + 1, 0);
        assert!(check(&big).unwrap_err().contains("太大"));
    }

    #[test]
    fn the_suggested_name_gets_the_right_extension() {
        assert_eq!(suggested_name(Some("聊天记录 2026-09-27.jpg"), Kind::Jpeg), "聊天记录 2026-09-27.jpg");
        assert_eq!(suggested_name(Some("聊天记录 2026-09-27.jpg"), Kind::Png), "聊天记录 2026-09-27.png");
        assert_eq!(suggested_name(Some("长图"), Kind::Png), "长图.png");
        assert_eq!(suggested_name(Some("  .jpg"), Kind::Jpeg), "长图.jpg", "只有扩展名的");
        assert_eq!(suggested_name(None, Kind::Jpeg), "长图.jpg");
        assert_eq!(Kind::Jpeg.extensions(), ["jpg", "jpeg"]);
        assert_eq!(Kind::Png.filter_name(), "PNG 图片");
    }
}
