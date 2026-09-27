//! 图片批量处理的保存部分。图片在界面里用 WebView2 自带的解码器解码、缩放、重新编码（不加新的依赖），
//! 这里只负责把结果存进用户在系统对话框里选的文件夹：
//! - 界面只传文件名（不带路径）和内容；文件夹只能由后端的选择框取得；
//! - 只新建文件，不覆盖任何已有的文件（重名就在名字后面加「 (2)」「 (3)」……），所以原图不会被改动，
//!   选了原图所在的文件夹也一样；
//! - 只收 JPG、PNG、WebP，内容的开头要和扩展名对得上；
//! - 写到一半失败，就把这个没写完的文件删掉；
//! - 修改时间设成原图的，按时间排序时顺序不乱。
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 一张图片最多多少字节（界面最多处理 1 亿像素的图片，PNG 也在这个范围里）
pub const MAX_BYTES: usize = 300 * 1024 * 1024;
/// 重名时最多加到「 (999)」
const MAX_COPIES: u32 = 999;

#[derive(Debug, Default)]
pub struct ImageState {
    /// 用户选的保存位置
    pub folder: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Jpeg,
    Png,
    Webp,
}

impl Format {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
        }
    }

    /// 文件开头的标记（JPEG 的 SOI、PNG 的签名、WebP 的 RIFF 头）
    fn matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Jpeg => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
            Self::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            Self::Webp => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        }
    }
}

/// 界面传来的原图修改时间（1970 年以来的毫秒数）
pub fn modified_from_millis(text: &str) -> Option<SystemTime> {
    let ms: u64 = text.trim().parse().ok()?;
    UNIX_EPOCH.checked_add(Duration::from_millis(ms))
}

/// 解开界面用 encodeURIComponent 编码的文件名（请求头只能放 ASCII）。
pub fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 把一张处理好的图片存进 `folder`，只新建、不覆盖。返回实际用的文件名。
pub fn save(folder: &Path, name: &str, bytes: &[u8], modified: Option<SystemTime>) -> Result<String, String> {
    crate::rename::check_name(name)?;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => return Err(format!("「{name}」没有扩展名。")),
    };
    let format = Format::from_extension(ext).ok_or_else(|| format!("只能保存成 JPG、PNG 或 WebP：{name}"))?;
    if bytes.is_empty() {
        return Err(format!("「{name}」是空的，没有保存。"));
    }
    if bytes.len() > MAX_BYTES {
        return Err(format!("「{name}」太大了（超过 {} MB），没有保存。", MAX_BYTES / 1024 / 1024));
    }
    if !format.matches(bytes) {
        return Err(format!("「{name}」的内容不是 {} 图片，没有保存。", format.label()));
    }
    if !folder.is_dir() {
        return Err("保存的文件夹不见了，请重新选择。".into());
    }
    for n in 1..=MAX_COPIES {
        let candidate = if n == 1 { name.to_owned() } else { format!("{stem} ({n}).{ext}") };
        if n > 1 {
            crate::rename::check_name(&candidate)?;
        }
        let path = folder.join(&candidate);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            // Windows 上和已有的文件夹同名时报的是「拒绝访问」，不是「已存在」
            Err(e) if e.kind() == ErrorKind::AlreadyExists || path.symlink_metadata().is_ok() => continue,
            Err(e) => return Err(format!("保存「{candidate}」失败：{e}")),
        };
        if let Err(e) = file.write_all(bytes).and_then(|()| file.flush()) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(format!("保存「{candidate}」失败，没写完的文件已经删掉：{e}"));
        }
        if let Some(time) = modified {
            // 改不了修改时间不算失败：图片已经存好了
            let _ = file.set_modified(time);
        }
        return Ok(candidate);
    }
    Err(format!("文件夹里和「{name}」同名的文件太多了，换一个文件夹再试。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const WEBP: &[u8] = b"RIFF\x10\0\0\0WEBPVP8 ";

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-images-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn never_overwrites_and_numbers_the_copies() {
        let d = Dir::new("copies");
        fs::write(d.0.join("照片.jpg"), b"original").unwrap();
        fs::create_dir(d.0.join("照片 (2).jpg")).unwrap();
        assert_eq!(save(&d.0, "照片.jpg", JPEG, None).unwrap(), "照片 (3).jpg");
        assert_eq!(fs::read(d.0.join("照片.jpg")).unwrap(), b"original");
        assert_eq!(fs::read(d.0.join("照片 (3).jpg")).unwrap(), JPEG);
        assert_eq!(save(&d.0, "海边.PNG", PNG, None).unwrap(), "海边.PNG");
        assert_eq!(save(&d.0, "a.webp", WEBP, None).unwrap(), "a.webp");
        assert_eq!(save(&d.0, "a.webp", WEBP, None).unwrap(), "a (2).webp");
    }

    #[test]
    fn only_images_whose_content_matches_their_extension() {
        let d = Dir::new("formats");
        for (name, bytes) in [
            ("a.exe", JPEG),
            ("a.jpg.lnk", JPEG),
            ("a.png", JPEG),
            ("a.jpg", PNG),
            ("a.webp", b"RIFF\0\0\0\0WAVE".as_slice()),
            ("a.jpg", b"".as_slice()),
            ("jpg", JPEG),
            (".jpg", JPEG),
            ("..\\a.jpg", JPEG),
            ("sub/a.jpg", JPEG),
            ("CON.jpg", JPEG),
            ("a.jpg ", JPEG),
        ] {
            assert!(save(&d.0, name, bytes, None).is_err(), "{name} 应该被拒绝");
        }
        assert_eq!(fs::read_dir(&d.0).unwrap().count(), 0, "被拒绝的都不应该留下文件");
        assert!(save(&d.0.join("没有这个文件夹"), "a.jpg", JPEG, None).unwrap_err().contains("文件夹不见了"));
    }

    #[test]
    fn keeps_the_original_modified_time() {
        let d = Dir::new("mtime");
        let time = modified_from_millis("1700000000000").unwrap();
        let saved = save(&d.0, "a.jpg", JPEG, Some(time)).unwrap();
        assert_eq!(fs::metadata(d.0.join(saved)).unwrap().modified().unwrap(), time);
        assert_eq!(modified_from_millis("abc"), None);
    }

    #[test]
    fn names_from_the_interface_are_decoded() {
        assert_eq!(decode_component("%E7%85%A7%E7%89%87%20(1).jpg").as_deref(), Some("照片 (1).jpg"));
        assert_eq!(decode_component("plain.png").as_deref(), Some("plain.png"));
        assert_eq!(decode_component("%E7%85"), None, "不是完整的 UTF-8");
        assert_eq!(decode_component("%zz.jpg"), None);
        assert_eq!(decode_component("50%"), None);
        assert_eq!(decode_component("%+1.jpg"), None, "from_str_radix 认的正号不算");
    }
}
