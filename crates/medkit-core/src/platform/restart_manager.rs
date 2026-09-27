//! 重启管理器（Restart Manager）：哪些进程在用这些文件。
//!
//! 安装程序更新文件之前就是用它找「谁在用这些文件」的：把文件登记进一个会话，它列出打开着这些文件、
//! 或者把它们当作程序模块（exe、dll）加载了的进程。只查不关：不调用 RmShutdown，会话用完就结束。
//! 文档：<https://learn.microsoft.com/en-us/windows/win32/rstmgr/using-restart-manager-with-a-secondary-installer>
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, ERROR_SUCCESS, FILETIME, GetLastError,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmConsole, RmCritical, RmEndSession, RmExplorer, RmGetList, RmMainWindow,
    RmOtherWindow, RmRegisterResources, RmService, RmStartSession,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use super::{FileUser, FileUserKind, PResult, PlatformError};

/// 一次登记多少个文件（文件夹里的文件可能有几千个）
const CHUNK: usize = 256;
/// RmGetList 说位置不够时重试几次（两次调用之间可能又有程序打开了文件）
const ATTEMPTS: usize = 5;

/// 会话句柄；离开作用域时结束会话。
struct Session(u32);

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: 句柄来自 RmStartSession，这里只结束一次
        unsafe { RmEndSession(self.0) };
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// 定长数组里以 NUL 结尾的字符串，去掉控制字符。
fn from_wide_z(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).chars().filter(|c| !c.is_control()).collect::<String>().trim().to_owned()
}

fn filetime(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

fn fail(what: &str, code: u32) -> PlatformError {
    PlatformError::Other(format!("{what}失败（错误代码 {code}）"))
}

pub fn file_users(files: &[PathBuf]) -> PResult<Vec<FileUser>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut handle = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: key 的长度是文档要求的 CCH_RM_SESSION_KEY + 1；dwSessionFlags 必须是 0
    let rc = unsafe { RmStartSession(&mut handle, 0, key.as_mut_ptr()) };
    if rc != ERROR_SUCCESS {
        return Err(fail("开始查询", rc));
    }
    let session = Session(handle);

    let names: Vec<Vec<u16>> = files.iter().map(|p| wide(p.as_os_str())).collect();
    for chunk in names.chunks(CHUNK) {
        let ptrs: Vec<*const u16> = chunk.iter().map(|w| w.as_ptr()).collect();
        // SAFETY: ptrs 里每一项都是以 NUL 结尾、在调用期间有效的字符串；不登记程序和服务
        let rc = unsafe { RmRegisterResources(session.0, ptrs.len() as u32, ptrs.as_ptr(), 0, null(), 0, null()) };
        if rc != ERROR_SUCCESS {
            return Err(fail("登记文件", rc));
        }
    }

    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    for _ in 0..ATTEMPTS {
        let mut needed = 0u32;
        let mut count = infos.len() as u32;
        let mut reasons = 0u32;
        let ptr = if infos.is_empty() { null_mut() } else { infos.as_mut_ptr() };
        // SAFETY: ptr 指向 count 个元素，或者 count 为 0、ptr 为空（只问要多少个位置）
        let rc = unsafe { RmGetList(session.0, &mut needed, &mut count, ptr, &mut reasons) };
        if rc == ERROR_SUCCESS {
            infos.truncate(count as usize);
            let own_session = session_of(std::process::id());
            return Ok(infos.iter().filter_map(|info| to_user(info, own_session)).collect());
        }
        if rc != ERROR_MORE_DATA {
            return Err(fail("列出在用这些文件的程序", rc));
        }
        // 多留几个位置：下一次调用之前可能又有程序打开了这些文件
        infos = vec![RM_PROCESS_INFO::default(); needed as usize + 8];
    }
    Err(PlatformError::Other("在用这些文件的程序一直在变，请过一会儿再查".into()))
}

fn session_of(pid: u32) -> Option<u32> {
    let mut id = 0u32;
    // SAFETY: id 是有效的输出位置
    (unsafe { ProcessIdToSessionId(pid, &mut id) } != 0).then_some(id)
}

/// 重启管理器的一项 → [`FileUser`]。进程已经退出（或者进程号被别的进程用上了）时返回 `None`。
fn to_user(info: &RM_PROCESS_INFO, own_session: Option<u32>) -> Option<FileUser> {
    let pid = info.Process.dwProcessId;
    let started = filetime(info.Process.ProcessStartTime);
    let program = match program_name(pid, started) {
        Program::Gone => return None,
        Program::Name(name) => Some(name),
        Program::Unknown => None,
    };
    let kind = match info.ApplicationType {
        t if t == RmMainWindow || t == RmOtherWindow => FileUserKind::Window,
        t if t == RmConsole => FileUserKind::Console,
        t if t == RmExplorer => FileUserKind::Explorer,
        t if t == RmService => FileUserKind::Service,
        t if t == RmCritical => FileUserKind::Critical,
        _ => FileUserKind::Other,
    };
    let service = Some(from_wide_z(&info.strServiceShortName)).filter(|s| !s.is_empty());
    // 服务都在会话 0 里，不算「另一个用户」
    let other_session = kind != FileUserKind::Service
        && info.TSSessionId != u32::MAX
        && own_session.is_some_and(|own| own != info.TSSessionId);
    Some(FileUser { pid, started, app_name: from_wide_z(&info.strAppName), program, service, kind, other_session })
}

enum Program {
    /// 程序的文件名
    Name(String),
    /// 进程已经退出，或者进程号换了主人
    Gone,
    /// 进程还在，但读不到它的路径
    Unknown,
}

/// 进程的程序文件名（只要文件名：完整路径里可能有用户名）。启动时间对不上说明原来的进程已经退出。
fn program_name(pid: u32, started: u64) -> Program {
    // SAFETY: 只查询信息；打开失败返回空句柄
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        // 进程号不存在是「参数错误」；别的错误（比如没有权限）当作进程还在
        // SAFETY: 紧接在 OpenProcess 之后读取
        let code = unsafe { GetLastError() };
        return if code == ERROR_INVALID_PARAMETER { Program::Gone } else { Program::Unknown };
    }
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: handle 有效，四个输出位置都有效
    let timed = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } != 0;
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    // SAFETY: buf 有 len 个元素
    let named = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) } != 0;
    // SAFETY: handle 来自 OpenProcess，只关一次
    unsafe { CloseHandle(handle) };
    if timed && filetime(created) != started {
        return Program::Gone;
    }
    if !named {
        return Program::Unknown;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    match Path::new(&path).file_name() {
        Some(name) => Program::Name(name.to_string_lossy().into_owned()),
        None => Program::Unknown,
    }
}
