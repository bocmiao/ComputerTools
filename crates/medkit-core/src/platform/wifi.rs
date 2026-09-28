//! 现在连着的 WiFi（内置检测「WiFi 连接情况」）。做法照 emoacht/ManagedNativeWifi（MIT）的 NativeWifi：WlanOpenHandle →
//! WlanEnumInterfaces 找状态是「已连接」的无线网卡 → WlanQueryInterface（wlan_intf_opcode_current_connection）读信号质量、
//! 物理层类型、收发速率、身份验证和加密 → WlanQueryInterface（wlan_intf_opcode_channel_number）读信道 → WlanGetNetworkBssList
//! 按 BSSID 找到连着的接入点，读中心频率（分得清 2.4、5、6 GHz）和信号强度（dBm）。WiFi 名称和 BSSID 只在这里用来查，不往外给。
//! wlanapi.dll 按需从 System32 加载（LoadLibraryExW + GetProcAddress），不放进导入表：没有 WiFi 组件的系统（服务器版、精简过的
//! 系统）上程序照样能打开，这一项说读不了。返回的内存用 WlanFreeMemory 还，句柄用 WlanCloseHandle 关。

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::NetworkManagement::WiFi::{
    DOT11_BSS_TYPE, DOT11_SSID, WLAN_API_VERSION_2_0, WLAN_BSS_ENTRY, WLAN_BSS_LIST, WLAN_CONNECTION_ATTRIBUTES,
    WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST, WLAN_INTF_OPCODE, WLAN_OPCODE_VALUE_TYPE,
    dot11_BSS_type_infrastructure, wlan_interface_state_connected, wlan_intf_opcode_channel_number,
    wlan_intf_opcode_current_connection,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows_sys::core::{BOOL, GUID};

use super::{PResult, PlatformError, WifiLink, WifiStatus};

type OpenHandle = unsafe extern "system" fn(u32, *const c_void, *mut u32, *mut HANDLE) -> u32;
type CloseHandle = unsafe extern "system" fn(HANDLE, *const c_void) -> u32;
type EnumInterfaces = unsafe extern "system" fn(HANDLE, *const c_void, *mut *mut WLAN_INTERFACE_INFO_LIST) -> u32;
type QueryInterface = unsafe extern "system" fn(
    HANDLE,
    *const GUID,
    WLAN_INTF_OPCODE,
    *const c_void,
    *mut u32,
    *mut *mut c_void,
    *mut WLAN_OPCODE_VALUE_TYPE,
) -> u32;
type GetNetworkBssList = unsafe extern "system" fn(
    HANDLE,
    *const GUID,
    *const DOT11_SSID,
    DOT11_BSS_TYPE,
    BOOL,
    *const c_void,
    *mut *mut WLAN_BSS_LIST,
) -> u32;
type FreeMemory = unsafe extern "system" fn(*const c_void);

/// wlanapi.dll 里用到的几个函数。
struct Api {
    open: OpenHandle,
    close: CloseHandle,
    enum_interfaces: EnumInterfaces,
    query: QueryInterface,
    bss_list: GetNetworkBssList,
    free: FreeMemory,
}

/// 第一次用时加载；系统里没有 wlanapi.dll、或者少了哪个函数时是 `None`。
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(load).as_ref()
}

fn load() -> Option<Api> {
    let file: Vec<u16> = "wlanapi.dll".encode_utf16().chain([0]).collect();
    // SAFETY: file 以 NUL 结尾；只在 System32 里找。模块一直不卸载：下面的函数指针在程序退出前都要能用
    let module = unsafe { LoadLibraryExW(file.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return None;
    }
    macro_rules! function {
        ($name:literal, $ty:ty) => {{
            // SAFETY: 名字以 NUL 结尾，module 是上面加载的
            let p = unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) }?;
            // SAFETY: 签名照微软 wlanapi.h，和 windows-sys 生成的声明一样
            unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(p) }
        }};
    }
    Some(Api {
        open: function!("WlanOpenHandle", OpenHandle),
        close: function!("WlanCloseHandle", CloseHandle),
        enum_interfaces: function!("WlanEnumInterfaces", EnumInterfaces),
        query: function!("WlanQueryInterface", QueryInterface),
        bss_list: function!("WlanGetNetworkBssList", GetNetworkBssList),
        free: function!("WlanFreeMemory", FreeMemory),
    })
}

/// 打开的 WLAN 客户端句柄；用完关掉。
struct Client<'a> {
    api: &'a Api,
    handle: HANDLE,
}

impl Drop for Client<'_> {
    fn drop(&mut self) {
        // SAFETY: handle 是 WlanOpenHandle 给的，还没关过
        unsafe { (self.api.close)(self.handle, null()) };
    }
}

/// wlanapi 分配的内存；用完还给它。
struct Owned<'a, T> {
    api: &'a Api,
    ptr: *mut T,
}

impl<T> Drop for Owned<'_, T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: ptr 是 wlanapi 分配给调用方的，还没还过
            unsafe { (self.api.free)(self.ptr.cast()) };
        }
    }
}

pub fn status() -> PResult<WifiStatus> {
    let no_service = WifiStatus { service: false, adapters: 0, link: None };
    let Some(api) = api() else {
        return Ok(no_service);
    };
    let mut version = 0u32;
    let mut handle: HANDLE = null_mut();
    // SAFETY: 两个输出位置有效
    let code = unsafe { (api.open)(WLAN_API_VERSION_2_0, null(), &mut version, &mut handle) };
    if code != ERROR_SUCCESS {
        // 「WLAN AutoConfig」服务没在运行（台式机、服务器版常这样）、被禁用
        return Ok(no_service);
    }
    let client = Client { api, handle };
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
    // SAFETY: 输出位置有效
    let code = unsafe { (api.enum_interfaces)(client.handle, null(), &mut list) };
    let list = Owned { api, ptr: list };
    if code != ERROR_SUCCESS || list.ptr.is_null() {
        return Err(PlatformError::Other(format!("列出无线网卡失败（错误代码 {code}）")));
    }
    // SAFETY: 列表由 wlanapi 分配，dwNumberOfItems 个元素紧接着排在 InterfaceInfo 后面
    let interfaces: &[WLAN_INTERFACE_INFO] = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!((*list.ptr).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>(),
            (*list.ptr).dwNumberOfItems as usize,
        )
    };
    let link = interfaces
        .iter()
        .filter(|i| i.isState == wlan_interface_state_connected)
        .find_map(|i| connection(&client, &i.InterfaceGuid));
    Ok(WifiStatus { service: true, adapters: interfaces.len(), link })
}

/// 一块连着 WiFi 的无线网卡的连接。
fn connection(client: &Client<'_>, guid: &GUID) -> Option<WifiLink> {
    let api = client.api;
    let current: Owned<'_, WLAN_CONNECTION_ATTRIBUTES> = query(client, guid, wlan_intf_opcode_current_connection)?;
    // SAFETY: query 核对过返回的大小不小于结构体
    let c = unsafe { &*current.ptr };
    if c.isState != wlan_interface_state_connected {
        return None;
    }
    let a = &c.wlanAssociationAttributes;
    let s = &c.wlanSecurityAttributes;
    let channel: Option<Owned<'_, u32>> = query(client, guid, wlan_intf_opcode_channel_number);
    // SAFETY: 同上
    let channel = channel.map(|p| unsafe { *p.ptr }).filter(|&n| n > 0);
    // 连着的接入点：扫描结果里 BSSID 一样的那一个
    let mut bss: *mut WLAN_BSS_LIST = null_mut();
    // SAFETY: SSID 和输出位置有效；按连着的网络的类型、加不加密查
    let code = unsafe {
        (api.bss_list)(
            client.handle,
            guid,
            &a.dot11Ssid,
            dot11_BSS_type_infrastructure,
            s.bSecurityEnabled,
            null(),
            &mut bss,
        )
    };
    let bss = Owned { api, ptr: bss };
    let entry = if code == ERROR_SUCCESS && !bss.ptr.is_null() {
        // SAFETY: 列表由 wlanapi 分配，dwNumberOfItems 个元素紧接着排在 wlanBssEntries 后面
        let entries: &[WLAN_BSS_ENTRY] = unsafe {
            std::slice::from_raw_parts(
                std::ptr::addr_of!((*bss.ptr).wlanBssEntries).cast::<WLAN_BSS_ENTRY>(),
                (*bss.ptr).dwNumberOfItems as usize,
            )
        };
        entries.iter().find(|e| e.dot11Bssid == a.dot11Bssid).map(|e| (e.ulChCenterFrequency, e.lRssi))
    } else {
        None
    };
    Some(WifiLink {
        signal: u8::try_from(a.wlanSignalQuality.min(100)).unwrap_or(100),
        rssi: entry.map(|(_, rssi)| rssi).filter(|&r| r < 0),
        frequency_mhz: entry.map(|(khz, _)| khz / 1000).filter(|&m| m > 0),
        channel,
        phy: a.dot11PhyType,
        rx_kbps: a.ulRxRate,
        tx_kbps: a.ulTxRate,
        auth: s.dot11AuthAlgorithm,
        cipher: s.dot11CipherAlgorithm,
    })
}

/// WlanQueryInterface 读一项；返回的大小比 `T` 小时当作没读到。
fn query<'a, T>(client: &Client<'a>, guid: &GUID, opcode: WLAN_INTF_OPCODE) -> Option<Owned<'a, T>> {
    let mut size = 0u32;
    let mut data: *mut c_void = null_mut();
    // SAFETY: 输出位置有效；不要值的类型
    let code = unsafe { (client.api.query)(client.handle, guid, opcode, null(), &mut size, &mut data, null_mut()) };
    let owned = Owned { api: client.api, ptr: data.cast::<T>() };
    (code == ERROR_SUCCESS && !owned.ptr.is_null() && size as usize >= size_of::<T>()).then_some(owned)
}
