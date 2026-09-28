//! Windows 上的真实实现。
//!
//! - 注册表：winreg；`HKU\<SID>` 通过 HKEY_USERS 下的完整路径访问，不单独打开 SID 根键
//! - 服务：直接调用 SCM 接口，只改启动类型（含「延迟启动」）
//! - 登录用户：当前控制台会话里 explorer.exe 的令牌用户
//! - 数据目录：只有 SYSTEM 和 Administrators 能写；发现被预先放置或是联接点时挪开重建

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SUCCESS, GetLastError, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, GetTokenInformation,
    LookupAccountSidW, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SID_NAME_USE, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW,
    QUERY_SERVICE_CONFIGW, QueryServiceConfig2W, QueryServiceConfigW, SC_HANDLE, SC_MANAGER_CONNECT,
    SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_DELAYED_AUTO_START_INFO,
    SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use winreg::RegKey;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY,
    KEY_WRITE, RegType as WinRegType,
};

use super::{KeyboardAids, OpenRequest, OsInfo, PResult, Platform, PlatformError, UserIdentity, edition_from_id};
use crate::model::StartType;
use crate::registry::{RegRoot, RegValue};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
/// 所有者 Administrators；SYSTEM、Administrators 完全控制；Users 只读。受保护（不继承上级）。
///
/// 所有者要明确设成 Administrators 组：提权后的管理员新建的目录，所有者可能是这个管理员账户本身
/// （取决于组策略），下次启动时就会被当成「不可信」挪走，修改日志也就跟着没了。
const DATA_DIR_SDDL: &str = "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";
const SID_ADMINISTRATORS: &str = "S-1-5-32-544";
const SID_SYSTEM: &str = "S-1-5-18";

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// 读一个以 NUL 结尾的 UTF-16 字符串。
///
/// # Safety
/// `p` 必须指向以 NUL 结尾的有效 UTF-16 字符串，或者为空指针。
unsafe fn from_wide_ptr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: 调用方保证 p 以 NUL 结尾
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: 上面已经确认 [p, p+len) 可读
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn map_io(context: &str, e: io::Error) -> PlatformError {
    if e.kind() == io::ErrorKind::PermissionDenied {
        PlatformError::AccessDenied(context.to_owned())
    } else {
        PlatformError::Other(format!("{context}：{e}"))
    }
}

pub struct WindowsPlatform {
    /// 上一次认出来的登录用户。资源管理器刚好在重启（比如用了「重启资源管理器」小工具）时，
    /// 暂时找不到它，就用这个，免得把登录用户的设置改到管理员账户身上。
    last_user: std::sync::Mutex<Option<UserIdentity>>,
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self { last_user: std::sync::Mutex::new(None) }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────── 注册表 ─────────────

fn base(root: &RegRoot, key: &str) -> (RegKey, String) {
    match root {
        RegRoot::LocalMachine => (RegKey::predef(HKEY_LOCAL_MACHINE), key.to_owned()),
        RegRoot::CurrentUser => (RegKey::predef(HKEY_CURRENT_USER), key.to_owned()),
        RegRoot::User(sid) => (RegKey::predef(HKEY_USERS), format!("{sid}\\{key}")),
        RegRoot::DefaultUser => (RegKey::predef(HKEY_USERS), format!(".DEFAULT\\{key}")),
    }
}

fn utf16_bytes_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

fn from_winreg(v: &winreg::RegValue) -> PResult<RegValue> {
    let b = v.bytes.as_ref();
    Ok(match v.vtype {
        WinRegType::REG_DWORD if b.len() == 4 => RegValue::Dword(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        WinRegType::REG_QWORD if b.len() == 8 => {
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            RegValue::Qword(u64::from_le_bytes(a))
        }
        WinRegType::REG_SZ => RegValue::String(utf16_bytes_to_string(b)),
        WinRegType::REG_EXPAND_SZ => RegValue::ExpandString(utf16_bytes_to_string(b)),
        WinRegType::REG_MULTI_SZ => {
            let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let items = units.split(|&u| u == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
            RegValue::MultiString(items)
        }
        WinRegType::REG_BINARY => RegValue::Binary(b.to_vec()),
        ref other => {
            // 不认识的类型原样拒绝：记不下原值，就不能保证撤销时还原成一模一样
            return Err(PlatformError::Unsupported(format!("注册表值类型 {other:?}")));
        }
    })
}

fn to_utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

fn to_winreg(v: &RegValue) -> winreg::RegValue<'static> {
    let (bytes, vtype) = match v {
        RegValue::Dword(n) => (n.to_le_bytes().to_vec(), WinRegType::REG_DWORD),
        RegValue::Qword(n) => (n.to_le_bytes().to_vec(), WinRegType::REG_QWORD),
        RegValue::String(s) => (to_utf16z(s), WinRegType::REG_SZ),
        RegValue::ExpandString(s) => (to_utf16z(s), WinRegType::REG_EXPAND_SZ),
        RegValue::MultiString(items) => {
            let mut out = Vec::new();
            for s in items {
                out.extend(to_utf16z(s));
            }
            out.extend([0, 0]);
            (out, WinRegType::REG_MULTI_SZ)
        }
        RegValue::Binary(b) => (b.clone(), WinRegType::REG_BINARY),
    };
    winreg::RegValue { bytes: bytes.into(), vtype }
}

// ───────────── 服务 ─────────────

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: 句柄由 OpenSCManagerW / OpenServiceW 返回且只关闭一次
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn open_service(name: &str, access: u32) -> PResult<Option<ScHandle>> {
    // SAFETY: 参数都是有效指针或空指针
    let scm = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    if scm.is_null() {
        return Err(map_io("打开服务管理器", last_error()));
    }
    let scm = ScHandle(scm);
    let wname = wide(name);
    // SAFETY: scm 有效，wname 以 NUL 结尾
    let svc = unsafe { OpenServiceW(scm.0, wname.as_ptr(), access) };
    if svc.is_null() {
        // SAFETY: 紧接在失败的系统调用之后读取
        if unsafe { GetLastError() } == ERROR_SERVICE_DOES_NOT_EXIST {
            return Ok(None);
        }
        return Err(map_io(&format!("打开服务 {name}"), last_error()));
    }
    Ok(Some(ScHandle(svc)))
}

/// 服务配置里我们关心的两项：启动类型、是否属于加载顺序组（属于的话不能设为延迟启动）。
fn query_config(svc: &ScHandle, name: &str) -> PResult<(u32, bool)> {
    let mut needed = 0u32;
    // SAFETY: 第一次调用只为取需要的大小
    unsafe { QueryServiceConfigW(svc.0, null_mut(), 0, &mut needed) };
    // 用 u64 缓冲区保证 8 字节对齐（结构体里有指针）
    let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: 缓冲区大小不小于 needed
    if unsafe { QueryServiceConfigW(svc.0, buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(), needed, &mut needed) } == 0
    {
        return Err(map_io(&format!("读取服务 {name} 的配置"), last_error()));
    }
    // SAFETY: 调用成功，缓冲区开头是 QUERY_SERVICE_CONFIGW，里面的字符串指针指向同一个缓冲区
    let (start, group) = unsafe {
        let cfg = &*buf.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        (cfg.dwStartType, from_wide_ptr(cfg.lpLoadOrderGroup))
    };
    Ok((start, !group.is_empty()))
}

fn query_start_type(svc: &ScHandle, name: &str) -> PResult<StartType> {
    let (start, _) = query_config(svc, name)?;
    Ok(match start {
        SERVICE_AUTO_START => {
            let mut info = SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: 0 };
            let size = u32::try_from(std::mem::size_of::<SERVICE_DELAYED_AUTO_START_INFO>()).unwrap_or(4);
            let mut needed = 0u32;
            // SAFETY: info 是大小合适的结构体
            let ok = unsafe {
                QueryServiceConfig2W(
                    svc.0,
                    SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                    (&raw mut info).cast::<u8>(),
                    size,
                    &mut needed,
                )
            };
            if ok != 0 && info.fDelayedAutostart != 0 { StartType::DelayedAuto } else { StartType::Auto }
        }
        SERVICE_DEMAND_START => StartType::Manual,
        SERVICE_DISABLED => StartType::Disabled,
        other => return Err(PlatformError::Unsupported(format!("服务 {name} 的启动类型 {other}（驱动类服务）"))),
    })
}

// ───────────── 用户与令牌 ─────────────

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: 句柄有效且只关闭一次
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn token_info(token: HANDLE, class: i32) -> Option<Vec<u64>> {
    let mut len = 0u32;
    // SAFETY: 第一次调用只为取大小
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: 缓冲区大小不小于 len
    let ok = unsafe { GetTokenInformation(token, class, buf.as_mut_ptr().cast(), len, &mut len) };
    (ok != 0).then_some(buf)
}

fn sid_string(sid: PSID) -> Option<String> {
    let mut s: *mut u16 = null_mut();
    // SAFETY: sid 来自有效的令牌信息
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 {
        return None;
    }
    // SAFETY: 成功时 s 是以 NUL 结尾的字符串，需要 LocalFree 释放
    let out = unsafe { from_wide_ptr(s) };
    unsafe { LocalFree(s as HLOCAL) };
    Some(out)
}

fn account_name(sid: PSID) -> Option<String> {
    let (mut name_len, mut domain_len, mut kind): (u32, u32, SID_NAME_USE) = (0, 0, 0);
    // SAFETY: 第一次调用只为取大小
    unsafe { LookupAccountSidW(null(), sid, null_mut(), &mut name_len, null_mut(), &mut domain_len, &mut kind) };
    if name_len == 0 {
        return None;
    }
    let mut name = vec![0u16; name_len as usize];
    let mut domain = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: 缓冲区大小来自上一次调用
    let ok = unsafe {
        LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut kind,
        )
    };
    if ok == 0 {
        return None;
    }
    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    Some(if domain.is_empty() { name } else { format!("{domain}\\{name}") })
}

fn token_user(token: HANDLE) -> Option<UserIdentity> {
    let buf = token_info(token, TokenUser)?;
    // SAFETY: 缓冲区开头是 TOKEN_USER，且在函数内有效
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let sid_str = sid_string(sid)?;
    let name = account_name(sid).unwrap_or_else(|| sid_str.clone());
    Some(UserIdentity { sid: sid_str, name })
}

fn process_token(process: HANDLE) -> Option<Handle> {
    let mut token: HANDLE = null_mut();
    // SAFETY: process 是有效句柄
    (unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } != 0).then(|| Handle(token))
}

/// 登录用户的资源管理器：先找小药箱自己所在的会话（远程桌面、快速切换用户时，控制台上可能是别人），
/// 找不到再找控制台会话。
fn user_explorer_pid() -> Option<u32> {
    let mut own = u32::MAX;
    // SAFETY: 输出参数是有效指针
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut own) } != 0;
    if ok && let Some(pid) = explorer_pid_in(own) {
        return Some(pid);
    }
    // SAFETY: 无参数
    let console = unsafe { WTSGetActiveConsoleSessionId() };
    if console == u32::MAX || (ok && console == own) {
        return None;
    }
    explorer_pid_in(console)
}

fn explorer_pid_in(session: u32) -> Option<u32> {
    // SAFETY: 标准的进程快照调用
    let snap = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) });
    if snap.0 == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry =
        PROCESSENTRY32W { dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).ok()?, ..Default::default() };
    // SAFETY: entry.dwSize 已正确设置
    let mut more = unsafe { Process32FirstW(snap.0, &mut entry) } != 0;
    while more {
        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let exe = String::from_utf16_lossy(&entry.szExeFile[..end]);
        if exe.eq_ignore_ascii_case("explorer.exe") {
            let mut sid = 0u32;
            // SAFETY: 输出参数是有效指针
            if unsafe { ProcessIdToSessionId(entry.th32ProcessID, &mut sid) } != 0 && sid == session {
                return Some(entry.th32ProcessID);
            }
        }
        // SAFETY: 同上
        more = unsafe { Process32NextW(snap.0, &mut entry) } != 0;
    }
    None
}

// ───────────── 系统信息 ─────────────

fn edition_label(id: &str) -> &'static str {
    match id {
        "CoreCountrySpecific" => "家庭中文版",
        "CoreSingleLanguage" => "家庭单语言版",
        "Core" | "CoreN" => "家庭版",
        "Professional" | "ProfessionalN" => "专业版",
        "ProfessionalWorkstation" => "专业工作站版",
        "ProfessionalEducation" => "专业教育版",
        "Enterprise" | "EnterpriseN" => "企业版",
        "EnterpriseS" | "EnterpriseSN" => "企业版 LTSC",
        "IoTEnterpriseS" => "IoT 企业版 LTSC",
        "IoTEnterprise" => "IoT 企业版",
        "Education" | "EducationN" => "教育版",
        _ => "",
    }
}

impl Platform for WindowsPlatform {
    fn reg_get(&self, root: &RegRoot, key: &str, name: &str) -> PResult<Option<RegValue>> {
        let (base, path) = base(root, key);
        let k = match base.open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY) {
            Ok(k) => k,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(map_io(&format!("{root}\\{key}"), e)),
        };
        match k.get_raw_value(name) {
            Ok(v) => from_winreg(&v).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(map_io(&format!("{root}\\{key}\\{name}"), e)),
        }
    }

    fn reg_key_exists(&self, root: &RegRoot, key: &str) -> PResult<bool> {
        let (base, path) = base(root, key);
        match base.open_subkey_with_flags(&path, KEY_QUERY_VALUE | KEY_WOW64_64KEY) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(map_io(&format!("{root}\\{key}"), e)),
        }
    }

    fn reg_set(&self, root: &RegRoot, key: &str, name: &str, value: &RegValue) -> PResult<()> {
        let (base, path) = base(root, key);
        let (k, _) = base
            .create_subkey_with_flags(&path, KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY)
            .map_err(|e| map_io(&format!("{root}\\{key}"), e))?;
        k.set_raw_value(name, &to_winreg(value)).map_err(|e| map_io(&format!("{root}\\{key}\\{name}"), e))
    }

    fn reg_delete_value(&self, root: &RegRoot, key: &str, name: &str) -> PResult<()> {
        let (base, path) = base(root, key);
        let k = match base.open_subkey_with_flags(&path, KEY_SET_VALUE | KEY_WOW64_64KEY) {
            Ok(k) => k,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(map_io(&format!("{root}\\{key}"), e)),
        };
        match k.delete_value(name) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(map_io(&format!("{root}\\{key}\\{name}"), e)),
        }
    }

    fn reg_delete_key_if_empty(&self, root: &RegRoot, key: &str) -> PResult<bool> {
        let (base, path) = base(root, key);
        let info = match base.open_subkey_with_flags(&path, KEY_READ | KEY_WOW64_64KEY) {
            Ok(k) => k.query_info().map_err(|e| map_io(&format!("{root}\\{key}"), e))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(map_io(&format!("{root}\\{key}"), e)),
        };
        if info.sub_keys > 0 || info.values > 0 {
            return Ok(false);
        }
        let (parent, leaf) = match path.rsplit_once('\\') {
            Some((p, l)) => (base.open_subkey_with_flags(p, KEY_WRITE | KEY_WOW64_64KEY), l),
            None => (Ok(base), path.as_str()),
        };
        let parent = parent.map_err(|e| map_io(&format!("{root}\\{key}"), e))?;
        parent
            .delete_subkey_with_flags(leaf, KEY_WOW64_64KEY)
            .map_err(|e| map_io(&format!("删除 {root}\\{key}"), e))?;
        Ok(true)
    }

    fn user_hive_loaded(&self, sid: &str) -> bool {
        // 已登录用户的配置单元挂在 HKEY_USERS\<SID> 下；注销后就打不开了
        RegKey::predef(HKEY_USERS).open_subkey_with_flags(sid, KEY_READ).is_ok()
    }

    fn service_get(&self, name: &str) -> PResult<Option<StartType>> {
        match open_service(name, SERVICE_QUERY_CONFIG)? {
            Some(svc) => query_start_type(&svc, name).map(Some),
            None => Ok(None),
        }
    }

    fn service_set(&self, name: &str, start_type: StartType) -> PResult<()> {
        let svc = open_service(name, SERVICE_QUERY_CONFIG | SERVICE_CHANGE_CONFIG)?
            .ok_or_else(|| PlatformError::NotFound(format!("服务 {name}")))?;
        let (original, grouped) = query_config(&svc, name)?;
        // 先检查再动手：属于加载顺序组的服务，Windows 不允许设为延迟启动（会报「参数错误」）
        if start_type == StartType::DelayedAuto && grouped {
            return Err(PlatformError::Unsupported(format!(
                "服务 {name} 属于加载顺序组，Windows 不允许把它设为延迟启动"
            )));
        }
        let start = match start_type {
            StartType::Auto | StartType::DelayedAuto => SERVICE_AUTO_START,
            StartType::Manual => SERVICE_DEMAND_START,
            StartType::Disabled => SERVICE_DISABLED,
        };
        let set_start = |value: u32| {
            // SAFETY: 其余参数用 SERVICE_NO_CHANGE / 空指针表示不改
            unsafe {
                ChangeServiceConfigW(
                    svc.0,
                    SERVICE_NO_CHANGE,
                    value,
                    SERVICE_NO_CHANGE,
                    null(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                )
            }
        };
        if set_start(start) == 0 {
            return Err(map_io(&format!("修改服务 {name}"), last_error()));
        }
        if start == SERVICE_AUTO_START {
            let info =
                SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: i32::from(start_type == StartType::DelayedAuto) };
            // SAFETY: info 在调用期间有效
            if unsafe { ChangeServiceConfig2W(svc.0, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, (&raw const info).cast()) }
                == 0
            {
                let err = map_io(&format!("设置服务 {name} 的延迟启动"), last_error());
                // 第一步已经改了启动类型，退回去，不留半截改动
                set_start(original);
                return Err(err);
            }
        }
        Ok(())
    }

    fn interactive_user(&self) -> Option<UserIdentity> {
        let found = user_explorer_pid().and_then(|pid| {
            // SAFETY: 只请求查询权限
            let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
            if process.0.is_null() {
                return None;
            }
            let token = process_token(process.0)?;
            token_user(token.0)
        });
        let mut last = self.last_user.lock().unwrap();
        match found {
            Some(u) => {
                *last = Some(u.clone());
                Some(u)
            }
            None => last.clone(),
        }
    }

    fn process_user(&self) -> Option<UserIdentity> {
        // SAFETY: 伪句柄，不需要关闭
        let token = process_token(unsafe { GetCurrentProcess() })?;
        token_user(token.0)
    }

    fn is_admin(&self) -> bool {
        // SAFETY: 同上
        let Some(token) = process_token(unsafe { GetCurrentProcess() }) else { return false };
        token_info(token.0, TokenElevation)
            // SAFETY: 缓冲区开头是 TOKEN_ELEVATION
            .is_some_and(|buf| unsafe { (*buf.as_ptr().cast::<TOKEN_ELEVATION>()).TokenIsElevated } != 0)
    }

    fn os_info(&self) -> OsInfo {
        let cv = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ | KEY_WOW64_64KEY);
        let get = |name: &str| -> String {
            cv.as_ref().ok().and_then(|k| k.get_value::<String, _>(name).ok()).unwrap_or_default()
        };
        let build: u32 = get("CurrentBuild").trim().parse().unwrap_or(0);
        let edition_id = get("EditionID");
        let major = if build >= 22000 { "Windows 11" } else { "Windows 10" };
        let label = edition_label(&edition_id);
        let caption = if label.is_empty() {
            // 不认识的版本：用注册表里的英文名，但修正 Win11 仍写着 Windows 10 的问题
            get("ProductName").replacen("Windows 10", major, 1)
        } else {
            format!("{major} {label}")
        };
        OsInfo {
            caption,
            build,
            display_version: get("DisplayVersion"),
            edition: edition_from_id(&edition_id),
            edition_id,
            computer_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        }
    }

    fn keyboard_aids(&self) -> PResult<KeyboardAids> {
        use windows_sys::Win32::UI::Accessibility::{FILTERKEYS, MOUSEKEYS, SKF_STICKYKEYSON, STICKYKEYS};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FKF_FILTERKEYSON, MKF_MOUSEKEYSON, SPI_GETFILTERKEYS, SPI_GETMOUSEKEYS, SPI_GETSTICKYKEYS,
            SystemParametersInfoW,
        };

        /// 读一个以 cbSize 开头的结构体。
        fn get<T>(action: u32, value: &mut T, what: &str) -> PResult<()> {
            let size = u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX);
            // SAFETY: value 是一个完整的 T（cbSize 已经填好），系统最多写 size 个字节
            if unsafe { SystemParametersInfoW(action, size, std::ptr::from_mut(value).cast(), 0) } == 0 {
                return Err(map_io(what, last_error()));
            }
            Ok(())
        }
        let size = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);

        let mut filter = FILTERKEYS {
            cbSize: size(std::mem::size_of::<FILTERKEYS>()),
            dwFlags: 0,
            iWaitMSec: 0,
            iDelayMSec: 0,
            iRepeatMSec: 0,
            iBounceMSec: 0,
        };
        get(SPI_GETFILTERKEYS, &mut filter, "读筛选键的状态")?;
        let mut sticky = STICKYKEYS { cbSize: size(std::mem::size_of::<STICKYKEYS>()), dwFlags: 0 };
        get(SPI_GETSTICKYKEYS, &mut sticky, "读粘滞键的状态")?;
        let mut mouse = MOUSEKEYS {
            cbSize: size(std::mem::size_of::<MOUSEKEYS>()),
            dwFlags: 0,
            iMaxSpeed: 0,
            iTimeToMaxSpeed: 0,
            iCtrlSpeed: 0,
            dwReserved1: 0,
            dwReserved2: 0,
        };
        get(SPI_GETMOUSEKEYS, &mut mouse, "读鼠标键的状态")?;
        Ok(KeyboardAids {
            filter_keys: filter.dwFlags & FKF_FILTERKEYSON != 0,
            sticky_keys: sticky.dwFlags & SKF_STICKYKEYSON != 0,
            mouse_keys: mouse.dwFlags & MKF_MOUSEKEYSON != 0,
        })
    }

    fn open(&self, request: &OpenRequest) -> PResult<()> {
        match request {
            OpenRequest::Program { exe, args, console } => open_program(exe, args, console),
            OpenRequest::Settings(page) => open_settings(page),
            OpenRequest::GetHelp(name) => open_get_help(name),
            OpenRequest::Web(url) => open_web(url),
        }
    }

    /// SHLoadIndirectString：`@文件,-编号` 按资源文件读（LoadLibraryEx 的资源模式，不运行里面的代码），
    /// `@{包全名?ms-resource://…}` 从应用包的资源里读。
    fn indirect_string(&self, source: &str) -> Option<String> {
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        };
        use windows_sys::Win32::UI::Shell::SHLoadIndirectString;

        if !source.starts_with('@') {
            return None;
        }
        let src = wide(source);
        let mut buf = vec![0u16; 1024];
        // 应用包的资源要用 COM；成功（含 S_FALSE）时要配对调用 CoUninitialize
        // SAFETY: 参数都是合法值
        let com = unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
        // SAFETY: src 以 NUL 结尾，buf 的长度如实传入，保留参数传空指针
        let hr = unsafe { SHLoadIndirectString(src.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, null()) };
        if com >= 0 {
            // SAFETY: 和上面成功的 CoInitializeEx 配对
            unsafe { CoUninitialize() };
        }
        if hr < 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let text = String::from_utf16_lossy(&buf[..len]).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    fn file_users(&self, files: &[PathBuf]) -> PResult<Vec<super::FileUser>> {
        super::restart_manager::file_users(files)
    }

    fn pointed_window(&self) -> PResult<Option<super::PointedWindow>> {
        super::window_info::pointed_window()
    }

    fn file_strings(&self, path: &Path) -> super::FileStrings {
        super::window_info::file_strings(path)
    }

    fn installed_programs(&self) -> PResult<Vec<super::InstalledProgram>> {
        let sid = self.interactive_user().map(|u| u.sid).filter(|s| crate::registry::is_sid(s));
        Ok(super::window_info::installed_programs(sid.as_deref()))
    }

    fn winsock_catalog(&self) -> PResult<Vec<super::WinsockEntry>> {
        super::winsock::catalog()
    }

    fn displays(&self) -> PResult<super::Displays> {
        super::displays::displays()
    }

    fn monitor_brightness(&self) -> PResult<Vec<super::MonitorBrightness>> {
        super::brightness::list()
    }

    fn set_monitor_brightness(&self, id: &str, percent: u8) -> PResult<u8> {
        super::brightness::set(id, percent)
    }

    fn wifi_status(&self) -> PResult<super::WifiStatus> {
        super::wifi::status()
    }
}

// ───────────── 打开系统工具 ─────────────

/// System32 的绝对路径（不依赖 PATH 和当前目录）。
fn system_directory() -> PathBuf {
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY: 缓冲区长度正确
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n > 0 && n < buf.len() {
        PathBuf::from(String::from_utf16_lossy(&buf[..n]))
    } else {
        PathBuf::from(r"C:\Windows\System32")
    }
}

/// 按绝对路径启动 System32 下的程序。以小药箱的权限（管理员）运行，所以不会再弹 UAC。
fn open_program(exe: &str, args: &[&str], console: &[&str]) -> PResult<()> {
    use std::process::{Command, Stdio};
    let system32 = system_directory();
    let program = system32.join(exe);
    if !program.is_file() {
        return Err(PlatformError::NotFound(exe.to_owned()));
    }
    if !console.is_empty() {
        // 命令行窗口：交给 ShellExecute 开一个新的控制台窗口（这里的输入输出不能接到空设备上，不然窗口里什么都
        // 看不到）；当前文件夹是 System32，程序都写绝对路径
        let params = crate::tools::console_command_line(&system32, console).map_err(PlatformError::NotFound)?;
        return shell_execute(program.as_os_str(), None, Some(&params), Some(&system32))
            .map_err(|code| PlatformError::Other(format!("启动 {exe} 失败（错误代码 {code}）")));
    }
    let mut cmd = Command::new(&program);
    for arg in args {
        if arg.ends_with(".msc") {
            let snap_in = system32.join(arg);
            if !snap_in.is_file() {
                return Err(PlatformError::NotFound((*arg).to_owned()));
            }
            cmd.arg(snap_in);
        } else {
            cmd.arg(arg);
        }
    }
    // 当前目录设成 System32：便携版可能放在「下载」里，别让子进程从那里找 DLL
    cmd.current_dir(&system32).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| PlatformError::Other(format!("启动 {exe} 失败：{e}")))
}

/// 打开「设置」里的一页（ms-settings:<page>）。「设置」是系统应用，由系统按登录用户打开。
fn open_settings(page: &str) -> PResult<()> {
    shell_open(OsStr::new(&format!("ms-settings:{page}")), None).map_err(|code| {
        PlatformError::Other(format!(
            "系统没有响应（错误代码 {code}）。可以点开始菜单里的齿轮图标，自己打开「设置」找这一项"
        ))
    })
}

/// 「获取帮助」里微软的疑难解答。精简过的系统、服务器版常常没有「获取帮助」应用，系统找不到处理这种链接的应用。
fn open_get_help(name: &str) -> PResult<()> {
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    const SE_ERR_NOASSOC: u32 = 31;
    const ERROR_NO_ASSOCIATION: u32 = 1155;
    // 没有能打开这种链接的应用就不去打开：系统会弹「需要使用新应用以打开此链接」，有的系统上 ShellExecuteExW
    // 还会一直等着这个没人点的对话框
    if protocol_handler("ms-contact-support").is_none() {
        return Err(PlatformError::NotFound("获取帮助".into()));
    }
    shell_open(OsStr::new(&format!("ms-contact-support://smc-to-emerald/{name}")), None).map_err(|code| match code {
        ERROR_FILE_NOT_FOUND | SE_ERR_NOASSOC | ERROR_NO_ASSOCIATION => PlatformError::NotFound("获取帮助".into()),
        SHELL_TIMED_OUT => PlatformError::Other(format!(
            "等了 {} 秒「获取帮助」还没有打开，可能还在启动：稍等一会儿；一直没出来的，到「设置」的「疑难解答」页里找",
            SHELL_TIMEOUT.as_secs()
        )),
        code => PlatformError::Other(format!("系统没有响应（错误代码 {code}）")),
    })
}

/// 网页：请资源管理器用登录用户的普通权限打开（见 explorer_exec），浏览器不会跟着小药箱以管理员身份运行。
fn open_web(url: &str) -> PResult<()> {
    use super::explorer_exec::{self, Error};
    if !url.starts_with("https://") {
        return Err(PlatformError::Other(format!("只打开 https 网址：{url}")));
    }
    explorer_exec::shell_execute(url, None).map_err(|e| match e {
        Error::NoDesktop => PlatformError::NotFound("资源管理器".into()),
        Error::TimedOut => {
            PlatformError::Other(format!("等了 {} 秒资源管理器还没有响应", explorer_exec::TIMEOUT.as_secs()))
        }
        Error::Failed(hr) => PlatformError::Other(format!("资源管理器没能打开（错误代码 0x{:08X}）", hr as u32)),
    })
}

/// 这台电脑上打开 `scheme:` 这种链接的应用：AssocQueryStringW 查到的应用（AppUserModelID），桌面程序查不到
/// AppUserModelID 时给打开它的命令行。都查不到就是没有能打开这种链接的应用。只查不打开。
pub fn protocol_handler(scheme: &str) -> Option<String> {
    use windows_sys::Win32::UI::Shell::{
        ASSOCF_IS_PROTOCOL, ASSOCF_NOTRUNCATE, ASSOCSTR_APPID, ASSOCSTR_COMMAND, AssocQueryStringW,
    };
    let scheme = wide(scheme);
    [ASSOCSTR_APPID, ASSOCSTR_COMMAND].into_iter().find_map(|kind| {
        let mut buf = vec![0u16; 2048];
        let mut len = buf.len() as u32;
        // SAFETY: scheme 以 NUL 结尾；buf 有 len 个字符；pszExtra 为空表示默认的动作
        let hr = unsafe {
            AssocQueryStringW(
                ASSOCF_IS_PROTOCOL | ASSOCF_NOTRUNCATE,
                kind,
                scheme.as_ptr(),
                null(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        if hr != 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let found = String::from_utf16_lossy(&buf[..end]);
        (!found.trim().is_empty()).then_some(found)
    })
}

/// 在资源管理器里打开一个文件夹（交给系统的外壳，由登录用户的资源管理器打开）。图片批量处理用它打开用户自己
/// 选的保存位置。按「文件夹」类型打开（SEE_MASK_CLASSNAME）：检查之后这个位置就算被换成了同名的程序，
/// 也只会被当成文件夹去打开，不会以管理员身份运行它。
pub fn open_folder(path: &Path) -> PResult<()> {
    if !path.is_dir() {
        return Err(PlatformError::NotFound(path.display().to_string()));
    }
    shell_open(path.as_os_str(), Some("folder"))
        .map_err(|code| PlatformError::Other(format!("资源管理器没有响应（错误代码 {code}），请自己打开这个文件夹")))
}

/// 在资源管理器里打开文件所在的文件夹，并选中这个文件（找大文件、重复文件时用）。只是显示，不打开文件本身，
/// 由登录用户的资源管理器打开。
pub fn reveal_file(path: &Path) -> PResult<()> {
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

    if !path.is_file() {
        return Err(PlatformError::NotFound(path.display().to_string()));
    }
    let wide_path = wide(path.as_os_str());
    // SAFETY: 参数都是合法值；成功（含 S_FALSE）时要配对调用 CoUninitialize
    let com = unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
    // SAFETY: wide_path 以 NUL 结尾；返回的 ITEMIDLIST 用完要 ILFree
    let pidl = unsafe { ILCreateFromPathW(wide_path.as_ptr()) };
    let hr = if pidl.is_null() {
        -1
    } else {
        // cidl 为 0：pidl 就是要选中的那一项，打开它所在的文件夹并选中它
        // SAFETY: pidl 有效；不传子项
        let hr = unsafe { SHOpenFolderAndSelectItems(pidl, 0, null(), 0) };
        // SAFETY: pidl 来自 ILCreateFromPathW，只释放一次
        unsafe { ILFree(pidl) };
        hr
    };
    if com >= 0 {
        // SAFETY: 和上面成功的 CoInitializeEx 配对
        unsafe { CoUninitialize() };
    }
    if hr < 0 {
        return Err(PlatformError::Other(format!("资源管理器没有响应（错误代码 {hr:#010x}），请自己打开这个文件夹")));
    }
    Ok(())
}

/// 本机的一个盘（硬盘分区或者 U 盘）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// 盘符，比如 'C'
    pub letter: char,
    /// U 盘、读卡器这类可移动的盘
    pub removable: bool,
    /// 卷标（「本地磁盘」这类名字是资源管理器自己起的，卷标常常是空的）
    pub label: String,
    /// NTFS、exFAT、FAT32……
    pub file_system: String,
    pub total: u64,
    /// 这个进程能用的剩余空间
    pub free: u64,
}

/// 本机的硬盘分区和 U 盘（固定的、可移动的有盘的；光驱、网络驱动器、没插卡的读卡器不列）。「硬盘测速」用它。
/// 查的时候关掉「驱动器中没有磁盘」这类系统对话框（SEM_FAILCRITICALERRORS），空的读卡器只是不列出来。
/// 这台电脑的处理器（IMAGE_FILE_MACHINE_*：x64 是 0x8664，ARM64 是 0xAA64）。IsWow64Process2 说的是系统本身的，
/// 不是小药箱这个进程的（ARM 电脑上模拟运行时，进程是 x64，系统是 ARM64）。读不到时返回 `None`。
pub fn native_machine() -> Option<u16> {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY: GetCurrentProcess 是伪句柄，不用关；两个输出都是有效的 u16
    let ok = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } != 0;
    (ok && native != 0).then_some(native)
}

pub fn drives() -> Vec<Drive> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    let mut old_mode = 0;
    // SAFETY: 只改这个线程的错误模式，结束时改回去
    let changed = unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, &mut old_mode) } != 0;
    // SAFETY: 没有参数
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for (i, letter) in ('A'..='Z').enumerate() {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = wide(format!("{letter}:\\"));
        // SAFETY: root 以 NUL 结尾
        let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
        if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
            continue;
        }
        let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
        // SAFETY: root 以 NUL 结尾，三个输出都是有效的 u64
        if unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, &mut total_free) } == 0 {
            continue;
        }
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        // SAFETY: 缓冲区长度和给的一样；不要的输出传空指针
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                null_mut(),
                null_mut(),
                null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        } != 0;
        let text = |buf: &[u16]| {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..end]).trim().to_owned()
        };
        out.push(Drive {
            letter,
            removable: kind == DRIVE_REMOVABLE,
            label: if ok { text(&label) } else { String::new() },
            file_system: if ok { text(&fs) } else { String::new() },
            total,
            free,
        });
    }
    if changed {
        // SAFETY: 改回原来的错误模式
        unsafe { SetThreadErrorMode(old_mode, null_mut()) };
    }
    out
}

/// 文件或文件夹的属性（GetFileAttributesW：只读 0x1、隐藏 0x2、系统 0x4、文件夹 0x10……）；读不到返回 None。
/// 不跟着符号链接走：链接自己的属性里有 0x400（重解析点）。「U 盘里的文件不见了」用它找被藏起来的文件。
pub fn file_attributes(path: &Path) -> Option<u32> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, INVALID_FILE_ATTRIBUTES};
    let wide_path = wide(path.as_os_str());
    // SAFETY: wide_path 以 NUL 结尾
    let value = unsafe { GetFileAttributesW(wide_path.as_ptr()) };
    (value != INVALID_FILE_ATTRIBUTES).then_some(value)
}

/// 设文件或文件夹的属性（SetFileAttributesW）。只传它能设的位；什么都不留时传 FILE_ATTRIBUTE_NORMAL（0x80）。
pub fn set_file_attributes(path: &Path, value: u32) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::SetFileAttributesW;
    let wide_path = wide(path.as_os_str());
    // SAFETY: wide_path 以 NUL 结尾
    if unsafe { SetFileAttributesW(wide_path.as_ptr(), value) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// ShellExecuteExW 最多等多久。个别系统上它会一直不返回（比如在等一个没人点的「需要使用新应用以打开此链接」），
/// 小药箱的按钮不能跟着一直转圈。
const SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// 等超时了，shell_execute 返回的错误代码（就是系统的 WAIT_TIMEOUT）。
const SHELL_TIMED_OUT: u32 = 258;

/// ShellExecuteExW 的 open；`class` 给了就按这个文件类型打开，不看目标本身是什么。失败时返回 GetLastError 的代码。
fn shell_open(target: &OsStr, class: Option<&str>) -> Result<(), u32> {
    shell_execute(target, class, None, None)
}

/// ShellExecuteExW（open），在单独的线程上调用，最多等 SHELL_TIMEOUT：超时返回 SHELL_TIMED_OUT，那个线程留着
/// 等系统返回，返回以后自己结束。`params`、`dir`：程序的参数和当前文件夹。
fn shell_execute(target: &OsStr, class: Option<&str>, params: Option<&str>, dir: Option<&Path>) -> Result<(), u32> {
    const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
    let target = target.to_owned();
    let class = class.map(str::to_owned);
    let params = params.map(str::to_owned);
    let dir = dir.map(Path::to_path_buf);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("shell-execute".into())
        .spawn(move || {
            let _ = tx.send(shell_execute_now(&target, class.as_deref(), params.as_deref(), dir.as_deref()));
        })
        .map_err(|_| ERROR_NOT_ENOUGH_MEMORY)?;
    rx.recv_timeout(SHELL_TIMEOUT).unwrap_or(Err(SHELL_TIMED_OUT))
}

/// 在当前线程上调用 ShellExecuteExW（open），等它返回。
fn shell_execute_now(target: &OsStr, class: Option<&str>, params: Option<&str>, dir: Option<&Path>) -> Result<(), u32> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_CLASSNAME, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let uri = wide(target);
    let verb = wide("open");
    let class = class.map(wide);
    let params = params.map(wide);
    let dir = dir.map(|d| wide(d.as_os_str()));
    // ShellExecute 可能通过 COM 找协议的处理程序，先在这个线程上初始化 COM（微软文档的要求）
    // SAFETY: 参数都是合法值；成功（含 S_FALSE）时要配对调用 CoUninitialize
    let com = unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
    // SAFETY: 全零是这个结构体的合法初始值
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    // NOASYNC：等交接完成再返回（随后就要释放 COM）；FLAG_NO_UI：失败时不再弹系统的错误框，由小药箱自己说明
    info.fMask = SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    info.lpVerb = verb.as_ptr();
    info.lpFile = uri.as_ptr();
    if let Some(class) = &class {
        info.fMask |= SEE_MASK_CLASSNAME;
        info.lpClass = class.as_ptr();
    }
    if let Some(params) = &params {
        info.lpParameters = params.as_ptr();
    }
    if let Some(dir) = &dir {
        info.lpDirectory = dir.as_ptr();
    }
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: info 已按文档初始化，字符串以 NUL 结尾且在调用期间有效
    let ok = unsafe { ShellExecuteExW(&mut info) } != 0;
    // SAFETY: 紧接在 ShellExecuteExW 之后读取
    let code = if ok { 0 } else { unsafe { GetLastError() } };
    if com >= 0 {
        // SAFETY: 和上面成功的 CoInitializeEx 配对
        unsafe { CoUninitialize() };
    }
    if ok { Ok(()) } else { Err(code) }
}

// ───────────── 数据目录 ─────────────

/// `%ProgramData%`
pub fn program_data() -> PathBuf {
    std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from)
}

fn owner_sid(path: &Path) -> io::Result<String> {
    let wpath = wide(path);
    let mut owner: PSID = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: 输出参数是有效指针；sd 需要 LocalFree
    let err = unsafe {
        GetNamedSecurityInfoW(
            wpath.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        )
    };
    if err != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(err as i32));
    }
    let sid = sid_string(owner).unwrap_or_default();
    // SAFETY: sd 由 GetNamedSecurityInfoW 分配
    unsafe { LocalFree(sd as HLOCAL) };
    Ok(sid)
}

fn apply_protected_dacl(path: &Path) -> io::Result<()> {
    let sddl = wide(DATA_DIR_SDDL);
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: sddl 以 NUL 结尾
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut())
    } == 0
    {
        return Err(last_error());
    }
    let (mut present, mut dacl_defaulted, mut owner_defaulted) = (0, 0, 0);
    let mut dacl: *mut ACL = null_mut();
    let mut owner: PSID = null_mut();
    // SAFETY: sd 有效；输出指针指向 sd 内部
    let ok = unsafe {
        GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut dacl_defaulted) != 0
            && GetSecurityDescriptorOwner(sd, &mut owner, &mut owner_defaulted) != 0
    };
    let result = if !ok || present == 0 || owner.is_null() {
        Err(last_error())
    } else {
        let wpath = wide(path);
        // SAFETY: owner 和 dacl 指向 sd 内部，在 LocalFree 之前有效
        let err = unsafe {
            SetNamedSecurityInfoW(
                wpath.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                owner,
                null_mut(),
                dacl,
                null(),
            )
        };
        if err == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(err as i32)) }
    };
    // SAFETY: sd 由 ConvertStringSecurityDescriptorToSecurityDescriptorW 分配
    unsafe { LocalFree(sd as HLOCAL) };
    result
}

/// 准备一个只有管理员能写的目录（需要以管理员身份运行）。
///
/// 普通用户可以在 ProgramData 下建目录，所以如果目录已经存在，先确认它不是联接点、
/// 所有者是 SYSTEM 或 Administrators；不是就挪开，重新建一个干净的。
/// 返回被挪开的旧目录（如果有）。
pub fn ensure_secure_dir(path: &Path) -> io::Result<Option<PathBuf>> {
    let mut moved = None;
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        let is_reparse = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        let trusted = !is_reparse && matches!(owner_sid(path).as_deref(), Ok(SID_ADMINISTRATORS | SID_SYSTEM));
        if !trusted {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let aside = path.with_extension(format!("untrusted-{stamp}"));
            std::fs::rename(path, &aside)?;
            moved = Some(aside);
        }
    }
    std::fs::create_dir_all(path)?;
    apply_protected_dacl(path)?;
    Ok(moved)
}

/// 目录的所有者 SID（测试和诊断用）。
pub fn dir_owner_sid(path: &Path) -> io::Result<String> {
    owner_sid(path)
}
