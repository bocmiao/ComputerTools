//! 「此应用无法在你的电脑上运行」：看一个程序文件本身能不能在这台电脑上运行。照微软《PE Format》读文件开头的几个头
//! （只读，不运行它）：是不是程序、给哪种处理器的（x86、x64、ARM64……）、是不是没下载完整、是不是 16 位的老程序、
//! 是不是 DLL。结论和这台电脑的处理器、Windows 版本对一对（64 位 Windows 能运行 32 位程序，反过来不行；ARM 电脑上
//! Windows 11 才能运行 x64 程序）。
//!
//! 只看文件头能看出来的：版本太老、依赖缺了、被安全软件拦这些，看不出来，界面上照实说。

/// 程序文件的格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 0 字节
    Empty,
    /// 不是程序；猜一猜是什么
    NotExe(Guess),
    /// DOS 的程序（只有 MZ 头）或者 Windows 3.x 的 16 位程序（NE）、更老的格式（LE、LX）
    Old16,
    /// Windows 的程序（PE）
    Pe(Pe),
}

/// 不是程序时，文件开头看起来像什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guess {
    /// Windows Installer 安装包（.msi，OLE 复合文档）
    Msi,
    /// 压缩包（zip、rar、7z）
    Archive,
    /// 网页（下载时下成了一个网页）
    Html,
    Pdf,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pe {
    /// COFF 头里的 Machine（IMAGE_FILE_MACHINE_*）
    pub machine: u16,
    /// 是 DLL，不是能双击运行的程序
    pub dll: bool,
    /// 子系统：2 = 图形界面，3 = 命令行；别的（驱动、EFI、Xbox……）不是给桌面运行的
    pub subsystem: u16,
    /// .NET 程序（有 CLR 头）
    pub dotnet: bool,
    /// 文件比头里写的短：没下载完整，或者被截断了
    pub truncated: bool,
}

pub const MACHINE_I386: u16 = 0x014c;
pub const MACHINE_AMD64: u16 = 0x8664;
pub const MACHINE_ARM64: u16 = 0xaa64;
pub const MACHINE_ARMNT: u16 = 0x01c4;
pub const MACHINE_ARM: u16 = 0x01c0;
pub const MACHINE_THUMB: u16 = 0x01c2;
pub const MACHINE_IA64: u16 = 0x0200;

const SUBSYSTEM_WINDOWS_GUI: u16 = 2;
const SUBSYSTEM_WINDOWS_CUI: u16 = 3;
const IMAGE_FILE_DLL: u16 = 0x2000;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn guess(head: &[u8]) -> Guess {
    let lower: Vec<u8> = head.iter().take(512).map(u8::to_ascii_lowercase).collect();
    let text = String::from_utf8_lossy(&lower);
    if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        Guess::Msi
    } else if head.starts_with(b"PK\x03\x04") || head.starts_with(b"Rar!") || head.starts_with(b"7z\xBC\xAF\x27\x1C") {
        Guess::Archive
    } else if head.starts_with(b"%PDF") {
        Guess::Pdf
    } else if text.trim_start_matches('\u{feff}').trim_start().starts_with('<')
        && (text.contains("<html") || text.contains("<!doctype html"))
    {
        Guess::Html
    } else {
        Guess::Unknown
    }
}

/// 读文件开头的 `head`（至少头和节表：读 1 MB 足够），`len` 是整个文件的大小。
pub fn parse(head: &[u8], len: u64) -> Format {
    if len == 0 {
        return Format::Empty;
    }
    if !head.starts_with(b"MZ") {
        return Format::NotExe(guess(head));
    }
    // DOS 头的 e_lfanew：新格式的头在哪
    let Some(new) = u32_at(head, 0x3c).map(|v| v as usize) else {
        return Format::Pe(Pe { machine: 0, dll: false, subsystem: 0, dotnet: false, truncated: true });
    };
    let truncated_pe = Format::Pe(Pe { machine: 0, dll: false, subsystem: 0, dotnet: false, truncated: true });
    match new.checked_add(4).and_then(|end| head.get(new..end)) {
        Some(b"PE\0\0") => {}
        Some([b'N', b'E', ..]) | Some([b'L', b'E', ..]) | Some([b'L', b'X', ..]) => return Format::Old16,
        // 新头的位置在文件外面：多半是没下载完整；在文件里却什么都不是：只有 DOS 头的老程序
        _ if new as u64 + 4 > len => return truncated_pe,
        None => return truncated_pe,
        _ => return Format::Old16,
    }
    let coff = new + 4;
    let (Some(machine), Some(sections), Some(opt_size), Some(characteristics)) =
        (u16_at(head, coff), u16_at(head, coff + 2), u16_at(head, coff + 16), u16_at(head, coff + 18))
    else {
        return truncated_pe;
    };
    let opt = coff + 20;
    let magic = u16_at(head, opt).unwrap_or(0);
    let subsystem = u16_at(head, opt + 68).unwrap_or(0);
    // 数据目录：PE32 从可选头的 96 字节起，PE32+ 从 112 字节起，每项 8 字节（地址、大小）
    let (dirs, count_at) = if magic == 0x20b { (opt + 112, opt + 108) } else { (opt + 96, opt + 92) };
    let dir_count = u32_at(head, count_at).unwrap_or(0) as usize;
    let dir = |i: usize| -> Option<(u32, u32)> {
        if i >= dir_count {
            return None;
        }
        Some((u32_at(head, dirs + i * 8)?, u32_at(head, dirs + i * 8 + 4)?))
    };
    let dotnet = dir(14).is_some_and(|(rva, size)| rva != 0 && size != 0);

    // 文件至少要有多长：每一节的内容在文件里的结束位置，加上签名（证书表是按文件偏移写的）
    let table = opt + opt_size as usize;
    let mut needed: u64 = 0;
    let mut table_ok = true;
    for i in 0..sections as usize {
        let at = table + i * 40;
        match (u32_at(head, at + 16), u32_at(head, at + 20)) {
            (Some(size), Some(pointer)) if size > 0 => needed = needed.max(u64::from(pointer) + u64::from(size)),
            (Some(_), Some(_)) => {}
            _ => table_ok = false,
        }
    }
    if let Some((offset, size)) = dir(4)
        && offset != 0
        && size != 0
    {
        needed = needed.max(u64::from(offset) + u64::from(size));
    }
    let truncated = !table_ok && (table as u64 + sections as u64 * 40 > len) || needed > len;
    Format::Pe(Pe { machine, dll: characteristics & IMAGE_FILE_DLL != 0, subsystem, dotnet, truncated })
}

/// 处理器的种类，给界面用的名字。
pub fn machine_name(machine: u16) -> &'static str {
    match machine {
        MACHINE_I386 => "x86",
        MACHINE_AMD64 => "x64",
        MACHINE_ARM64 => "arm64",
        MACHINE_ARMNT | MACHINE_ARM | MACHINE_THUMB => "arm32",
        MACHINE_IA64 => "ia64",
        _ => "other",
    }
}

/// 结论（界面按它说话）：
/// - `empty`：0 字节；`not-exe`：不是程序（看 [`Guess`]）；`truncated`：没下载完整；`dll`：是 DLL；
/// - `old16`：DOS、Windows 3.x 的 16 位程序，64 位 Windows 不能运行；
/// - `wrong-machine`：给别的处理器的（ARM64 的程序在 x64 电脑上、x64 的程序在 32 位 Windows 或者 Windows 10 的 ARM 电脑上、
///   ARM 32 位、安腾的）；`not-desktop`：不是桌面程序（驱动、EFI 这些）；
/// - `ok`：从文件本身看能运行。
///
/// `native` 是这台电脑的处理器（IMAGE_FILE_MACHINE_*），`build` 是 Windows 的版本号。
pub fn verdict(format: &Format, native: u16, build: u32) -> &'static str {
    match format {
        Format::Empty => "empty",
        Format::NotExe(_) => "not-exe",
        Format::Old16 => "old16",
        Format::Pe(pe) if pe.truncated => "truncated",
        Format::Pe(pe) if pe.dll => "dll",
        Format::Pe(pe) if !runs_on(pe.machine, native, build) => "wrong-machine",
        Format::Pe(pe) if pe.subsystem != SUBSYSTEM_WINDOWS_GUI && pe.subsystem != SUBSYSTEM_WINDOWS_CUI => {
            "not-desktop"
        }
        Format::Pe(_) => "ok",
    }
}

/// 这种处理器的程序能不能在这台电脑上运行。Windows 11 的 ARM 电脑能模拟运行 x86 和 x64 的程序，Windows 10 的只能模拟 x86；
/// ARM 32 位的程序从 Windows 11 24H2（26100）起不再支持。
fn runs_on(machine: u16, native: u16, build: u32) -> bool {
    match (machine, native) {
        (MACHINE_I386, _) => true,
        (MACHINE_AMD64, MACHINE_AMD64) => true,
        (MACHINE_AMD64, MACHINE_ARM64) => build >= 22000,
        (MACHINE_ARM64, MACHINE_ARM64) => true,
        (MACHINE_ARMNT | MACHINE_ARM | MACHINE_THUMB, MACHINE_ARM64) => build < 26100,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 拼一个最小的 PE 文件头：DOS 头 + PE 头 + 可选头 + 一个节（内容在 0x400 起，长 `raw` 字节）。
    fn pe(machine: u16, plus: bool, subsystem: u16, dll: bool, raw: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0..2].copy_from_slice(b"MZ");
        let new = 0x80usize;
        b[0x3c..0x40].copy_from_slice(&(new as u32).to_le_bytes());
        b[new..new + 4].copy_from_slice(b"PE\0\0");
        let coff = new + 4;
        b[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
        b[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        let opt_size: u16 = if plus { 240 } else { 224 };
        b[coff + 16..coff + 18].copy_from_slice(&opt_size.to_le_bytes());
        let ch: u16 = 0x0102 | if dll { IMAGE_FILE_DLL } else { 0 };
        b[coff + 18..coff + 20].copy_from_slice(&ch.to_le_bytes());
        let opt = coff + 20;
        b[opt..opt + 2].copy_from_slice(&(if plus { 0x20bu16 } else { 0x10bu16 }).to_le_bytes());
        b[opt + 68..opt + 70].copy_from_slice(&subsystem.to_le_bytes());
        let count_at = if plus { opt + 108 } else { opt + 92 };
        b[count_at..count_at + 4].copy_from_slice(&16u32.to_le_bytes());
        let table = opt + opt_size as usize;
        b[table..table + 5].copy_from_slice(b".text");
        b[table + 16..table + 20].copy_from_slice(&raw.to_le_bytes());
        b[table + 20..table + 24].copy_from_slice(&0x400u32.to_le_bytes());
        b.resize(0x400 + raw as usize, 0xCC);
        b
    }

    fn check(bytes: &[u8]) -> Format {
        parse(&bytes[..bytes.len().min(64 * 1024)], bytes.len() as u64)
    }

    #[test]
    fn programs_are_told_apart_by_their_headers() {
        let x64 = check(&pe(MACHINE_AMD64, true, SUBSYSTEM_WINDOWS_GUI, false, 0x200));
        assert_eq!(
            x64,
            Format::Pe(Pe { machine: MACHINE_AMD64, dll: false, subsystem: 2, dotnet: false, truncated: false })
        );
        assert_eq!(verdict(&x64, MACHINE_AMD64, 26100), "ok");
        let x86 = check(&pe(MACHINE_I386, false, SUBSYSTEM_WINDOWS_CUI, false, 0x200));
        assert_eq!(verdict(&x86, MACHINE_AMD64, 19045), "ok");
        let dll = check(&pe(MACHINE_AMD64, true, SUBSYSTEM_WINDOWS_GUI, true, 0x200));
        assert_eq!(verdict(&dll, MACHINE_AMD64, 26100), "dll");
        let driver = check(&pe(MACHINE_AMD64, true, 1, false, 0x200));
        assert_eq!(verdict(&driver, MACHINE_AMD64, 26100), "not-desktop");
    }

    #[test]
    fn a_download_that_stopped_halfway_is_truncated() {
        let mut b = pe(MACHINE_AMD64, true, SUBSYSTEM_WINDOWS_GUI, false, 0x10000);
        b.truncate(0x5000);
        let f = check(&b);
        assert!(matches!(f, Format::Pe(Pe { truncated: true, .. })), "{f:?}");
        assert_eq!(verdict(&f, MACHINE_AMD64, 26100), "truncated");
        // 只剩 DOS 头
        assert_eq!(verdict(&check(&b[..0x60]), MACHINE_AMD64, 26100), "truncated");
        assert_eq!(check(&[]), Format::Empty);
    }

    #[test]
    fn programs_for_other_processors_do_not_run() {
        let arm64 = check(&pe(MACHINE_ARM64, true, SUBSYSTEM_WINDOWS_GUI, false, 0x200));
        assert_eq!(verdict(&arm64, MACHINE_AMD64, 26100), "wrong-machine");
        assert_eq!(verdict(&arm64, MACHINE_ARM64, 26100), "ok");
        let x64 = check(&pe(MACHINE_AMD64, true, SUBSYSTEM_WINDOWS_GUI, false, 0x200));
        assert_eq!(verdict(&x64, MACHINE_I386, 19045), "wrong-machine");
        assert_eq!(verdict(&x64, MACHINE_ARM64, 19041), "wrong-machine", "Windows 10 的 ARM 电脑不能运行 x64");
        assert_eq!(verdict(&x64, MACHINE_ARM64, 22631), "ok", "Windows 11 的 ARM 电脑能模拟运行 x64");
        let arm32 = check(&pe(MACHINE_ARMNT, false, SUBSYSTEM_WINDOWS_GUI, false, 0x200));
        assert_eq!(verdict(&arm32, MACHINE_ARM64, 22631), "ok");
        assert_eq!(verdict(&arm32, MACHINE_ARM64, 26100), "wrong-machine");
        let ia64 = check(&pe(MACHINE_IA64, true, SUBSYSTEM_WINDOWS_GUI, false, 0x200));
        assert_eq!(verdict(&ia64, MACHINE_AMD64, 26100), "wrong-machine");
        assert_eq!(machine_name(MACHINE_ARMNT), "arm32");
    }

    #[test]
    fn old_and_non_programs_are_recognized() {
        let mut ne = vec![0u8; 0x200];
        ne[0..2].copy_from_slice(b"MZ");
        ne[0x3c] = 0x80;
        ne[0x80..0x82].copy_from_slice(b"NE");
        assert_eq!(check(&ne), Format::Old16);
        // 只有 DOS 头、新头的位置在文件里却什么都不是：DOS 程序
        let mut dos = vec![0u8; 0x200];
        dos[0..2].copy_from_slice(b"MZ");
        dos[0x3c] = 0x40;
        assert_eq!(check(&dos), Format::Old16);
        assert_eq!(verdict(&Format::Old16, MACHINE_AMD64, 26100), "old16");

        assert_eq!(check(b"PK\x03\x04rest"), Format::NotExe(Guess::Archive));
        assert_eq!(check(b"\xEF\xBB\xBF<!DOCTYPE html><html>"), Format::NotExe(Guess::Html));
        assert_eq!(check(b"%PDF-1.7"), Format::NotExe(Guess::Pdf));
        assert_eq!(check(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0]), Format::NotExe(Guess::Msi));
        assert_eq!(check(b"hello"), Format::NotExe(Guess::Unknown));
    }
}
