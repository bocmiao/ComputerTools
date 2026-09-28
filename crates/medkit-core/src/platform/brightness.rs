//! 显示器的亮度（工具箱「显示器亮度」）：外接显示器用 DDC/CI 读写。做法照 emoacht/Monitorian（MIT）的
//! MonitorConfiguration：EnumDisplayMonitors 列出每块桌面（HMONITOR），GetPhysicalMonitorsFromHMONITOR 拿到上面的物理
//! 显示器；先用显示器支持 MCCS 时的高级接口 GetMonitorBrightness / SetMonitorBrightness，不行再用 VCP 代码 0x10（亮度）的
//! GetVCPFeatureAndVCPFeatureReply / SetVCPFeature，有的显示器第一次读会失败，再读一次。显示器报的原始值不一定是 0–100，
//! 按它报的最小、最大值换算（[`super::brightness_percent`]）。SetMonitorBrightness 有时报成功其实没设上（Monitorian 的
//! 注释），设完读回来。句柄用完就还（DestroyPhysicalMonitors），不留状态；DDC/CI 同一时间只让一个请求用。型号名按 GDI
//! 设备名从 CCD 对上（[`super::displays::monitors_by_gdi_name`]）。笔记本自带的屏幕一般不支持 DDC/CI，读不到就是调不了。

use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, PHYSICAL_MONITOR, SetMonitorBrightness,
    SetVCPFeature,
};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows_sys::core::BOOL;

use super::{MonitorBrightness, PResult, PlatformError, brightness_percent, brightness_raw};

/// VESA MCCS 的 VCP 代码 0x10：亮度
const VCP_BRIGHTNESS: u8 = 0x10;

/// DDC/CI 走的是显示器线里一条很慢的总线：同一时间只让一个请求用
static BUS: Mutex<()> = Mutex::new(());

pub fn list() -> PResult<Vec<MonitorBrightness>> {
    let _bus = BUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let names = super::displays::monitors_by_gdi_name().unwrap_or_default();
    let mut found = Vec::new();
    for (hmonitor, device) in desktops() {
        let physical = Physical::of(hmonitor);
        let named: Vec<_> = names.iter().filter(|(gdi, ..)| gdi.eq_ignore_ascii_case(&device)).collect();
        // 一块桌面上只接着一个显示器时才对得上是哪个（「复制」时对不上）
        let (name, internal) = match named.as_slice() {
            [(_, name, internal)] => (name.clone(), *internal),
            _ => (None, false),
        };
        for index in 0..physical.len() {
            let percent =
                physical.handle(index).and_then(read).and_then(|r| brightness_percent(r.min, r.current, r.max));
            found.push(MonitorBrightness { id: format!("{device}#{index}"), name: name.clone(), internal, percent });
        }
    }
    Ok(found)
}

pub fn set(id: &str, percent: u8) -> PResult<u8> {
    let _bus = BUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let gone = || PlatformError::NotFound("显示器".into());
    let (device, index) =
        id.rsplit_once('#').and_then(|(d, i)| Some((d, i.parse::<usize>().ok()?))).ok_or_else(gone)?;
    let hmonitor =
        desktops().into_iter().find(|(_, d)| d.eq_ignore_ascii_case(device)).map(|(h, _)| h).ok_or_else(gone)?;
    let physical = Physical::of(hmonitor);
    let handle = physical.handle(index).ok_or_else(gone)?;
    let reading = read(handle).ok_or_else(|| PlatformError::Unsupported("用电脑调这台显示器的亮度".into()))?;
    let raw = brightness_raw(reading.min, reading.max, percent);
    // SAFETY: handle 是 physical 里还没还给系统的物理显示器句柄
    let accepted = unsafe {
        if reading.low_level { SetVCPFeature(handle, VCP_BRIGHTNESS, raw) } else { SetMonitorBrightness(handle, raw) }
    } != 0;
    if !accepted {
        // SAFETY: 没有参数
        let code = unsafe { GetLastError() };
        return Err(PlatformError::Other(format!("显示器没有接受新的亮度（错误代码 0x{code:08X}）")));
    }
    // 有的显示器报成功其实没设上：读回来；读不回来就当设上了
    Ok(read(handle).and_then(|r| brightness_percent(r.min, r.current, r.max)).unwrap_or(percent))
}

/// 读到的原始亮度；`low_level` 是用 VCP 代码读的（写也要用它）。
struct Reading {
    min: u32,
    current: u32,
    max: u32,
    low_level: bool,
}

fn read(handle: HANDLE) -> Option<Reading> {
    let (mut min, mut current, mut max) = (0u32, 0u32, 0u32);
    // SAFETY: 三个输出位置有效
    if unsafe { GetMonitorBrightness(handle, &mut min, &mut current, &mut max) } != 0 && max > min {
        return Some(Reading { min, current, max, low_level: false });
    }
    for _ in 0..2 {
        let mut kind = 0;
        // SAFETY: 输出位置有效
        if unsafe { GetVCPFeatureAndVCPFeatureReply(handle, VCP_BRIGHTNESS, &mut kind, &mut current, &mut max) } != 0
            && max > 0
        {
            return Some(Reading { min: 0, current, max, low_level: true });
        }
    }
    None
}

/// 每块桌面（HMONITOR）和它的 GDI 设备名。
fn desktops() -> Vec<(HMONITOR, String)> {
    unsafe extern "system" fn collect(hmonitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
        // SAFETY: list 是下面那个 Vec 的地址，EnumDisplayMonitors 返回之前一直有效，只有这里写它
        unsafe { (*(list as *mut Vec<HMONITOR>)).push(hmonitor) };
        1
    }
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: 回调只往 handles 里放句柄；不限定设备上下文和区域
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect), &mut handles as *mut Vec<HMONITOR> as LPARAM) };
    handles
        .into_iter()
        .filter_map(|hmonitor| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).ok()?;
            // SAFETY: info 是完整的 MONITORINFOEXW，cbSize 如实填了
            let ok = unsafe { GetMonitorInfoW(hmonitor, std::ptr::from_mut(&mut info).cast::<MONITORINFO>()) } != 0;
            if !ok {
                return None;
            }
            super::displays::text(&info.szDevice).map(|device| (hmonitor, device))
        })
        .collect()
}

/// 一块桌面上的物理显示器；用完还给系统。
struct Physical(Vec<PHYSICAL_MONITOR>);

impl Physical {
    fn of(hmonitor: HMONITOR) -> Self {
        let mut count = 0u32;
        // SAFETY: 输出位置有效
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count) } == 0 || count == 0 {
            return Self(Vec::new());
        }
        let mut list = vec![PHYSICAL_MONITOR::default(); count as usize];
        // SAFETY: list 有 count 个元素
        if unsafe { GetPhysicalMonitorsFromHMONITOR(hmonitor, count, list.as_mut_ptr()) } == 0 {
            return Self(Vec::new());
        }
        Self(list)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    /// 结构体是按 1 字节对齐的：句柄按值取出来，不借引用。
    fn handle(&self, index: usize) -> Option<HANDLE> {
        self.0.get(index).map(|m| m.hPhysicalMonitor)
    }
}

impl Drop for Physical {
    fn drop(&mut self) {
        if let Ok(count) = u32::try_from(self.0.len())
            && count > 0
        {
            // SAFETY: 这些句柄是 GetPhysicalMonitorsFromHMONITOR 给的，还没还过
            unsafe { DestroyPhysicalMonitors(count, self.0.as_ptr()) };
        }
    }
}
