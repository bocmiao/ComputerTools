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
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use winreg::RegKey;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY,
    KEY_WRITE, RegType as WinRegType,
};

use super::{OpenRequest, OsInfo, PResult, Platform, PlatformError, UserIdentity, edition_from_id};
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

pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        Self
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

fn console_explorer_pid() -> Option<u32> {
    // SAFETY: 无参数
    let session = unsafe { WTSGetActiveConsoleSessionId() };
    if session == u32::MAX {
        return None;
    }
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
        let pid = console_explorer_pid()?;
        // SAFETY: 只请求查询权限
        let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
        if process.0.is_null() {
            return None;
        }
        let token = process_token(process.0)?;
        token_user(token.0)
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

    fn open(&self, request: &OpenRequest) -> PResult<()> {
        match request {
            OpenRequest::Program { exe, args } => open_program(exe, args),
            OpenRequest::Settings(page) => open_settings(page),
        }
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
fn open_program(exe: &str, args: &[&str]) -> PResult<()> {
    use std::process::{Command, Stdio};
    let system32 = system_directory();
    let program = system32.join(exe);
    if !program.is_file() {
        return Err(PlatformError::NotFound(exe.to_owned()));
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
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let uri = wide(format!("ms-settings:{page}"));
    let verb = wide("open");
    // ShellExecute 可能通过 COM 找协议的处理程序，先在这个线程上初始化 COM（微软文档的要求）
    // SAFETY: 参数都是合法值；成功（含 S_FALSE）时要配对调用 CoUninitialize
    let com = unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
    // SAFETY: 字符串都以 NUL 结尾；不需要父窗口
    let r = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), uri.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    if com >= 0 {
        // SAFETY: 和上面成功的 CoInitializeEx 配对
        unsafe { CoUninitialize() };
    }
    // 返回值大于 32 表示成功
    if r as isize > 32 {
        Ok(())
    } else {
        Err(PlatformError::Other(format!(
            "系统没有打开「设置」（错误代码 {}）。可以点开始菜单里的齿轮图标，自己打开「设置」找这一项",
            r as isize
        )))
    }
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
