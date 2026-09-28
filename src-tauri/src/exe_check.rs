//! 「此应用无法在你的电脑上运行」：选一个程序文件，读文件开头（最多 1 MB）看它本身能不能在这台电脑上运行（判断在
//! medkit_core::exe_info）。只读，不运行它；结果里只有文件名，没有文件夹。
use std::io::Read;
use std::path::Path;

use medkit_core::exe_info::{self, Format, Guess};
use serde::Serialize;

/// 读文件开头这么多：头和节表都在里面
const HEAD: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExeCheckView {
    /// 文件名（不带文件夹）
    pub name: String,
    pub size: u64,
    /// empty / not-exe / truncated / dll / old16 / wrong-machine / not-desktop / ok（见 medkit_core::exe_info::verdict）
    pub verdict: &'static str,
    /// 不是程序时像什么：msi / archive / html / pdf / unknown
    pub guess: Option<&'static str>,
    /// 程序是给哪种处理器的：x86 / x64 / arm64 / arm32 / ia64 / other
    pub machine: Option<&'static str>,
    /// 这台电脑的处理器：x86 / x64 / arm64 / other
    pub pc: &'static str,
    /// 这台电脑是不是 Windows 11
    pub windows11: bool,
    /// 命令行程序（双击以后黑框一闪就没了）
    pub console: bool,
    /// .NET 程序
    pub dotnet: bool,
}

fn guess_name(g: Guess) -> &'static str {
    match g {
        Guess::Msi => "msi",
        Guess::Archive => "archive",
        Guess::Html => "html",
        Guess::Pdf => "pdf",
        Guess::Unknown => "unknown",
    }
}

/// 看 `path` 这个文件。`native` 是这台电脑的处理器（IMAGE_FILE_MACHINE_*），`build` 是 Windows 的版本号。
pub fn check(path: &Path, native: u16, build: u32) -> Result<ExeCheckView, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打不开这个文件：{e}"))?;
    let size = file.metadata().map_err(|e| format!("读不了这个文件：{e}"))?.len();
    let mut head = Vec::new();
    file.take(HEAD).read_to_end(&mut head).map_err(|e| format!("读不了这个文件：{e}"))?;
    let format = exe_info::parse(&head, size);
    let (guess, machine, console, dotnet) = match format {
        Format::NotExe(g) => (Some(guess_name(g)), None, false, false),
        Format::Pe(pe) if !pe.truncated => {
            (None, Some(exe_info::machine_name(pe.machine)), pe.subsystem == 3, pe.dotnet)
        }
        _ => (None, None, false, false),
    };
    Ok(ExeCheckView {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size,
        verdict: exe_info::verdict(&format, native, build),
        guess,
        machine,
        pc: exe_info::machine_name(native),
        windows11: build >= 22000,
        console,
        dotnet,
    })
}

/// 这台电脑的处理器和 Windows 版本号。
#[cfg(windows)]
pub fn this_pc() -> (u16, u32) {
    use medkit_core::platform::Platform;
    let native = medkit_core::platform::windows::native_machine().unwrap_or(exe_info::MACHINE_AMD64);
    (native, medkit_core::platform::windows::WindowsPlatform::new().os_info().build)
}

#[cfg(not(windows))]
pub fn this_pc() -> (u16, u32) {
    (exe_info::MACHINE_AMD64, 26100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_file_name_is_reported() {
        let dir = std::env::temp_dir().join(format!("medkit-exe-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("安装包.exe");
        std::fs::write(&path, b"<!DOCTYPE html><html><body>download page</body></html>").unwrap();
        let v = check(&path, exe_info::MACHINE_AMD64, 26100).unwrap();
        assert_eq!((v.name.as_str(), v.verdict, v.guess, v.machine), ("安装包.exe", "not-exe", Some("html"), None));
        let json = serde_json::to_string(&v).unwrap();
        std::fs::write(&path, b"").unwrap();
        let empty = check(&path, exe_info::MACHINE_AMD64, 26100).unwrap().verdict;
        let missing = check(&dir.join("没有这个.exe"), exe_info::MACHINE_AMD64, 26100);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!json.contains(&dir.display().to_string()), "{json}");
        assert_eq!(empty, "empty");
        assert!(missing.is_err());
    }
}
