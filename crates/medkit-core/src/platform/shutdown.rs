//! 定时关机、定时重启：InitiateSystemShutdownExW 安排，AbortSystemShutdownW 取消（和 `shutdown /s /t 秒数`、
//! `shutdown /a` 用的是同一套）。安排好以后系统会弹出通知；到时间强制关掉所有程序（和 `shutdown /t` 一样：
//! 等的时间大于 0 时默认强制），不会因为某个程序没保存就停在「这些应用阻止关机」那里。
//! 这件事记在系统里，安排它的程序退出了照样会关机。两个函数都要先打开本程序的关机权限（SeShutdownPrivilege）。

use std::io;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NO_SHUTDOWN_IN_PROGRESS, ERROR_NOT_ALL_ASSIGNED,
    ERROR_SHUTDOWN_IN_PROGRESS, ERROR_SHUTDOWN_IS_SCHEDULED, GetLastError, HANDLE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Shutdown::{
    AbortSystemShutdownW, InitiateSystemShutdownExW, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
    SHTDN_REASON_MINOR_OTHER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::{PResult, PlatformError};

/// 安排的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownRequest {
    /// 安排好了
    Scheduled,
    /// 已经安排了一次（不管是谁安排的），或者正在关机：这次没有安排
    AlreadyScheduled,
}

struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: 句柄来自 OpenProcessToken，只关闭一次
        unsafe { CloseHandle(self.0) };
    }
}

/// 打开本程序的关机权限（令牌里有，但默认没打开）。
fn enable_shutdown_privilege() -> PResult<()> {
    let mut token: HANDLE = null_mut();
    // SAFETY: GetCurrentProcess 是伪句柄；token 是有效的输出位置
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) } == 0 {
        return Err(PlatformError::Other(format!("打不开本程序的权限令牌：{}", io::Error::last_os_error())));
    }
    let token = Token(token);
    let mut luid = LUID { LowPart: 0, HighPart: 0 };
    // SAFETY: 本机（系统名为空）；SE_SHUTDOWN_NAME 以 NUL 结尾
    if unsafe { LookupPrivilegeValueW(null(), SE_SHUTDOWN_NAME, &mut luid) } == 0 {
        return Err(PlatformError::Other(format!("找不到关机权限：{}", io::Error::last_os_error())));
    }
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
    };
    // SAFETY: token 有效；不要旧的状态
    let ok = unsafe { AdjustTokenPrivileges(token.0, 0, &privileges, 0, null_mut(), null_mut()) };
    // 成功时也要看错误代码：ERROR_NOT_ALL_ASSIGNED 表示令牌里本来就没有这个权限
    // SAFETY: 紧跟在上面的调用之后
    let code = unsafe { GetLastError() };
    if ok == 0 {
        return Err(PlatformError::Other(format!("没能打开关机权限：{}", io::Error::from_raw_os_error(code as i32))));
    }
    if code == ERROR_NOT_ALL_ASSIGNED {
        return Err(PlatformError::AccessDenied("这个账户没有关机的权限".to_owned()));
    }
    Ok(())
}

/// `seconds` 秒以后关机（`restart` 时重启），`message` 显示在系统的通知里。
pub fn schedule(seconds: u32, restart: bool, message: &str) -> PResult<ShutdownRequest> {
    enable_shutdown_privilege()?;
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    // SAFETY: 本机（机器名为空）；message 以 NUL 结尾；到时间强制关掉程序
    let ok = unsafe {
        InitiateSystemShutdownExW(
            null(),
            message.as_ptr(),
            seconds,
            1,
            i32::from(restart),
            SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER | SHTDN_REASON_FLAG_PLANNED,
        )
    };
    if ok != 0 {
        return Ok(ShutdownRequest::Scheduled);
    }
    // SAFETY: 紧跟在失败的调用之后
    match unsafe { GetLastError() } {
        ERROR_SHUTDOWN_IS_SCHEDULED | ERROR_SHUTDOWN_IN_PROGRESS => Ok(ShutdownRequest::AlreadyScheduled),
        ERROR_ACCESS_DENIED => Err(PlatformError::AccessDenied("这个账户不能安排关机".to_owned())),
        code => Err(PlatformError::Other(io::Error::from_raw_os_error(code as i32).to_string())),
    }
}

/// 取消已经安排的关机或重启（不管是谁安排的）。取消了返回 true，本来就没有安排返回 false。
pub fn abort() -> PResult<bool> {
    enable_shutdown_privilege()?;
    // SAFETY: 本机（机器名为空）
    if unsafe { AbortSystemShutdownW(null()) } != 0 {
        return Ok(true);
    }
    // SAFETY: 紧跟在失败的调用之后
    match unsafe { GetLastError() } {
        ERROR_NO_SHUTDOWN_IN_PROGRESS => Ok(false),
        ERROR_ACCESS_DENIED => Err(PlatformError::AccessDenied("这个账户不能取消关机".to_owned())),
        code => Err(PlatformError::Other(io::Error::from_raw_os_error(code as i32).to_string())),
    }
}
