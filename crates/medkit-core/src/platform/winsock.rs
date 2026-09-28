//! 读 Winsock 目录（只读）。用微软公开的服务提供程序接口：WSCEnumProtocols 列出每一项（包括隐藏的项和分层协议
//! 本身），WSCGetProviderPath 读 DLL 的路径（里面的 %SystemRoot% 这类环境变量没展开）；64 位 Windows 上 32 位程序
//! 用的那一份目录用 WSCEnumProtocols32、WSCGetProviderPath32 读。路径只在这里用来看文件在不在、读版本信息，
//! 交出去的只有文件名。

use std::mem::size_of;
use std::path::Path;

use windows_sys::Win32::Networking::WinSock::{
    SOCKET_ERROR, WSACleanup, WSADATA, WSAENOBUFS, WSAPROTOCOL_INFOW, WSAStartup, WSCEnumProtocols, WSCGetProviderPath,
};
#[cfg(target_pointer_width = "64")]
use windows_sys::Win32::Networking::WinSock::{WSCEnumProtocols32, WSCGetProviderPath32};
use windows_sys::core::GUID;

use super::{PResult, PlatformError, WinsockEntry};
use crate::winsock::{in_windows_folder, resolve_provider_path};

type EnumFn = unsafe extern "system" fn(*const i32, *mut WSAPROTOCOL_INFOW, *mut u32, *mut i32) -> i32;
type PathFn = unsafe extern "system" fn(*const GUID, *mut u16, *mut i32, *mut i32) -> i32;

/// 一份目录里的每一项。
fn enumerate(list: EnumFn) -> Result<Vec<WSAPROTOCOL_INFOW>, i32> {
    // 一般二三十项；不够时函数会说要多大，再来一次
    let mut count = 64usize;
    for _ in 0..3 {
        let mut buffer = vec![WSAPROTOCOL_INFOW::default(); count];
        let mut bytes = u32::try_from(count * size_of::<WSAPROTOCOL_INFOW>()).unwrap_or(u32::MAX);
        let mut errno = 0;
        // SAFETY: buffer 有 bytes 个字节，bytes 和 errno 是有效的输出位置；不按协议筛选
        let n = unsafe { list(std::ptr::null(), buffer.as_mut_ptr(), &mut bytes, &mut errno) };
        if n != SOCKET_ERROR {
            buffer.truncate(usize::try_from(n).unwrap_or(0));
            return Ok(buffer);
        }
        if errno != WSAENOBUFS {
            return Err(errno);
        }
        count = (bytes as usize).div_ceil(size_of::<WSAPROTOCOL_INFOW>()).max(count + 1);
    }
    Err(WSAENOBUFS)
}

/// 提供程序 DLL 的路径（原样，环境变量没展开）。
fn provider_path(get: PathFn, id: &GUID) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut len = i32::try_from(buffer.len()).unwrap_or(i32::MAX);
    let mut errno = 0;
    // SAFETY: id 指向有效的 GUID，buffer 有 len 个字符
    if unsafe { get(id, buffer.as_mut_ptr(), &mut len, &mut errno) } != 0 {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

fn read(list: EnumFn, get: PathFn, wow64: bool, windows: &str, out: &mut Vec<WinsockEntry>) -> Result<(), i32> {
    for info in enumerate(list)? {
        let raw = provider_path(get, &info.ProviderId).unwrap_or_default();
        let path = resolve_provider_path(&raw, wow64, |name| std::env::var(name).ok());
        let file = path.rsplit(['\\', '/']).next().unwrap_or_default().to_owned();
        let exists = !raw.trim().is_empty() && Path::new(&path).is_file();
        let strings = if exists { super::window_info::file_strings(Path::new(&path)) } else { Default::default() };
        let end = info.szProtocol.iter().position(|&c| c == 0).unwrap_or(info.szProtocol.len());
        out.push(WinsockEntry {
            protocol: String::from_utf16_lossy(&info.szProtocol[..end]),
            file,
            in_windows: in_windows_folder(&path, windows),
            exists,
            company: strings.company,
            product: strings.product,
            chain_len: info.ProtocolChain.ChainLen,
            family: info.iAddressFamily,
            socket_type: info.iSocketType,
            wow64,
        });
    }
    Ok(())
}

pub fn catalog() -> PResult<Vec<WinsockEntry>> {
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let mut data = WSADATA::default();
    // SAFETY: data 是有效的输出位置。微软的示例在列目录之前先初始化 Winsock；失败了也照样列
    let started = unsafe { WSAStartup(0x0202, &mut data) } == 0;
    let mut out = Vec::new();
    let native = read(WSCEnumProtocols, WSCGetProviderPath, false, &windows, &mut out);
    #[cfg(target_pointer_width = "64")]
    {
        // 32 位的那一份读不到时（精简过的系统）只看 64 位的
        let _ = read(WSCEnumProtocols32, WSCGetProviderPath32, true, &windows, &mut out);
    }
    if started {
        // SAFETY: 和上面成功的 WSAStartup 配对
        unsafe { WSACleanup() };
    }
    native.map_err(|errno| PlatformError::Other(format!("读不了 Winsock 目录（错误 {errno}）")))?;
    Ok(out)
}
