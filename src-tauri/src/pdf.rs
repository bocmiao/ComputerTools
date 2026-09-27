//! 图片合成 PDF 的保存部分。PDF 在界面里拼好（每张图片用 WebView2 自带的编码器存成 JPEG，一页一张，
//! 不加新的依赖），这里只负责存盘：
//! - 存到哪里、叫什么，由用户在系统的「另存为」对话框里选，界面拿不到、也传不了路径；
//!   选了已经有的文件，对话框自己会先问要不要替换；
//! - 只收开头是 `%PDF-`、最后有 `%%EOF` 的内容，最大 [`MAX_BYTES`]；
//! - 选的名字不是 .pdf 结尾的，后面补上 .pdf；补出来的名字已经有文件了就不存（对话框没问过要不要替换它）；
//! - 先写进同一个文件夹里的临时文件，写完再换上正式的名字：写到一半失败时，原来的同名文件不受影响。
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// 一个 PDF 最多多少字节（界面最多 200 页；超过时让用户换小一点的清晰度）
pub const MAX_BYTES: usize = 512 * 1024 * 1024;
/// 界面没给、或者给的文件名不能用时，「另存为」对话框里默认的名字
pub const DEFAULT_NAME: &str = "图片合成.pdf";

#[derive(Debug, Default)]
pub struct PdfState {
    /// 最近一次存好的 PDF（「在文件夹里显示」用）
    pub saved: Option<PathBuf>,
}

/// 界面拼好的内容像不像一个完整的 PDF。
pub fn check(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "PDF 太大了（超过 {} MB），没有保存。换小一点的清晰度，或者分成几个 PDF。",
            MAX_BYTES / 1024 / 1024
        ));
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("内容不是 PDF，没有保存。".into());
    }
    let tail = &bytes[bytes.len().saturating_sub(32)..];
    if !tail.windows(5).any(|w| w == b"%%EOF") {
        return Err("PDF 不完整，没有保存。".into());
    }
    Ok(())
}

/// 对话框返回的路径：不是 .pdf 结尾的补上 .pdf。第二个值说明对话框有没有就这个名字问过要不要替换。
pub fn target(chosen: PathBuf) -> (PathBuf, bool) {
    if chosen.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return (chosen, true);
    }
    let mut name = chosen.into_os_string();
    name.push(".pdf");
    (PathBuf::from(name), false)
}

/// 把 PDF 存到 `path`：先写临时文件，写完再换上正式的名字。`may_replace` 为 false 时不替换已有的文件。
pub fn write(path: &Path, bytes: &[u8], may_replace: bool) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = path.parent().filter(|p| p.is_dir()).ok_or("保存的文件夹不见了，请重新选择。")?;
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => return Err(format!("「{name}」是一个文件夹，换个名字再保存。")),
        Ok(_) if !may_replace => return Err(format!("这里已经有一个叫「{name}」的文件了，换个名字再保存。")),
        _ => {}
    }
    let mut temp = None;
    for n in 1..=100u32 {
        let candidate = folder.join(format!("{name}.{}-{n}.medkit-tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => {
                temp = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("保存「{name}」失败：{e}")),
        }
    }
    let (temp_path, mut file) = temp.ok_or_else(|| format!("保存「{name}」失败：建不了临时文件。"))?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = written.and_then(|()| fs::rename(&temp_path, path)) {
        let _ = fs::remove_file(&temp_path);
        let kept = if path.exists() { "，原来的同名文件没动" } else { "" };
        return Err(format!("保存「{name}」失败{kept}：{e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDF: &[u8] = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n1 0 obj\n<< >>\nendobj\ntrailer\n<< >>\n%%EOF\n";

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("medkit-pdf-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> =
                fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_complete_pdfs_are_accepted() {
        assert!(check(PDF).is_ok());
        assert!(check(b"%PDF-1.4\n1 0 obj").unwrap_err().contains("不完整"));
        assert!(check(b"MZ\x90\0%%EOF").unwrap_err().contains("不是 PDF"));
        assert!(check(b"").is_err());
        let big = vec![b' '; MAX_BYTES + 1];
        assert!(check(&big).unwrap_err().contains("太大"));
    }

    #[test]
    fn a_missing_extension_is_added_and_then_nothing_is_replaced() {
        assert_eq!(target(PathBuf::from("材料.PDF")), (PathBuf::from("材料.PDF"), true));
        assert_eq!(target(PathBuf::from("材料")), (PathBuf::from("材料.pdf"), false));
        assert_eq!(target(PathBuf::from("材料.jpg")), (PathBuf::from("材料.jpg.pdf"), false));

        let d = Dir::new("extension");
        fs::write(d.0.join("材料.pdf"), b"old").unwrap();
        let e = write(&d.0.join("材料.pdf"), PDF, false).unwrap_err();
        assert!(e.contains("已经有一个叫「材料.pdf」"), "{e}");
        assert_eq!(fs::read(d.0.join("材料.pdf")).unwrap(), b"old");
        assert_eq!(d.names(), ["材料.pdf"], "临时文件都不留");
    }

    #[test]
    fn replaces_only_what_the_dialog_asked_about() {
        let d = Dir::new("replace");
        let path = d.0.join("报名材料.pdf");
        write(&path, PDF, false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), PDF);
        fs::write(&path, b"old").unwrap();
        write(&path, PDF, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), PDF);
        assert_eq!(d.names(), ["报名材料.pdf"], "临时文件换成了正式的名字");
    }

    #[test]
    fn folders_and_missing_folders_are_refused() {
        let d = Dir::new("folders");
        fs::create_dir(d.0.join("子文件夹.pdf")).unwrap();
        assert!(write(&d.0.join("子文件夹.pdf"), PDF, true).unwrap_err().contains("是一个文件夹"));
        assert!(write(&d.0.join("没有这个文件夹").join("a.pdf"), PDF, true).unwrap_err().contains("文件夹不见了"));
        assert_eq!(d.names(), ["子文件夹.pdf"]);
    }
}
