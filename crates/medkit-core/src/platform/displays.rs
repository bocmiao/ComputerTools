//! 读正在用的显示器（只读）：用微软公开的「连接和配置显示器」（CCD）接口。QueryDisplayConfig（只要活动的路径）给出
//! 每个显示器现在的桌面大小（源模式）和显示方向；DisplayConfigGetDeviceInfo 读显示器推荐的分辨率
//! （DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE，「设置 → 屏幕」里标着「推荐」的就是它）、型号名和接口类型
//! （GET_TARGET_NAME：monitorFriendlyDeviceName 是 EDID 里写的名字，不含序列号）。几条路径用同一个源，就是「复制」。
//! 做法照 MartinGC94/DisplayConfig（MIT）的 GetPreferredMode 和 DisplayInfo。交出去的只有型号名和分辨率。

use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_ROTATION_ROTATE90, DISPLAYCONFIG_ROTATION_ROTATE270, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_TARGET_PREFERRED_MODE, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};

use super::{Display, Displays, PResult, PlatformError};

pub fn displays() -> PResult<Displays> {
    // SAFETY: 没有参数
    let remote = unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0;
    let (paths, modes) = match query() {
        Ok(found) => found,
        // 远程桌面会话里可能根本读不了显示器配置；这时只说是远程桌面
        Err(_) if remote => return Ok(Displays { remote, list: Vec::new() }),
        Err(e) => return Err(e),
    };
    let mut list = Vec::new();
    for path in &paths {
        let target = &path.targetInfo;
        // SAFETY: 没带 QDC_VIRTUAL_MODE_AWARE 时，这个联合体里是 modeInfoIdx（无效时是 0xFFFFFFFF，下面取不到）
        let index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let Some(mode) = modes.get(index).filter(|m| m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE) else {
            continue;
        };
        // SAFETY: infoType 是 SOURCE，联合体里是源模式
        let source = unsafe { mode.Anonymous.sourceMode };
        let (mut width, mut height) = (source.width, source.height);
        if target.rotation == DISPLAYCONFIG_ROTATION_ROTATE90 || target.rotation == DISPLAYCONFIG_ROTATION_ROTATE270 {
            std::mem::swap(&mut width, &mut height);
        }
        let name = target_name(target.adapterId, target.id);
        let technology = name.as_ref().map_or(target.outputTechnology, |n| n.outputTechnology);
        list.push(Display {
            name: name.as_ref().and_then(|n| text(&n.monitorFriendlyDeviceName)),
            internal: is_internal(technology),
            width,
            height,
            preferred: preferred_mode(target.adapterId, target.id),
            cloned: paths.iter().filter(|p| same_source(p, path)).count() > 1,
        });
    }
    Ok(Displays { remote, list })
}

/// 每条活动路径的桌面 GDI 设备名（`\\.\DISPLAY1` 这种，EnumDisplayMonitors、GetMonitorInfo 给的就是它）和上面接着的
/// 显示器：型号名、是不是自带的屏幕。「复制」时一个设备名上接着好几个。调亮度时按设备名对上名字用。
pub(super) fn monitors_by_gdi_name() -> PResult<Vec<(String, Option<String>, bool)>> {
    let (paths, _) = query()?;
    Ok(paths
        .iter()
        .filter_map(|path| {
            let source = &path.sourceInfo;
            // SAFETY: DISPLAYCONFIG_SOURCE_DEVICE_NAME 以 header 开头，是 GET_SOURCE_NAME 要的结构体
            let gdi: DISPLAYCONFIG_SOURCE_DEVICE_NAME =
                unsafe { device_info(DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, source.adapterId, source.id)? };
            let target = &path.targetInfo;
            let name = target_name(target.adapterId, target.id);
            let technology = name.as_ref().map_or(target.outputTechnology, |n| n.outputTechnology);
            Some((
                text(&gdi.viewGdiDeviceName)?,
                name.as_ref().and_then(|n| text(&n.monitorFriendlyDeviceName)),
                is_internal(technology),
            ))
        })
        .collect())
}

/// 活动的显示路径和它们用到的模式。两次调用之间有显示器插拔时缓冲区会不够，重来（微软文档的做法）。
fn query() -> PResult<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>)> {
    let failed = |what: &str, code: u32| PlatformError::Other(format!("{what}失败（错误代码 {code}）"));
    for _ in 0..5 {
        let (mut path_count, mut mode_count) = (0u32, 0u32);
        // SAFETY: 两个输出位置有效
        let status = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) };
        if status != ERROR_SUCCESS {
            return Err(failed("读显示器配置的大小", status));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: 两个数组的长度如实传入；不要拓扑（只有带 QDC_DATABASE_CURRENT 时才能要）
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                null_mut(),
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Err(failed("读显示器配置", status));
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);
        return Ok((paths, modes));
    }
    Err(PlatformError::Other("读显示器配置时显示器一直在变，稍后再试".into()))
}

/// 向显示器要一项信息：请求以 header 开头，header 里写上要什么、整个请求有多大、是哪个显示器。
///
/// # Safety
/// `T` 必须是以 `DISPLAYCONFIG_DEVICE_INFO_HEADER` 开头的 `#[repr(C)]` 结构体，而且是 `kind` 这一项要的那种。
unsafe fn device_info<T: Default>(kind: DISPLAYCONFIG_DEVICE_INFO_TYPE, adapter: LUID, id: u32) -> Option<T> {
    let mut request = T::default();
    let header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: u32::try_from(size_of::<T>()).ok()?,
        adapterId: adapter,
        id,
    };
    let pointer = std::ptr::from_mut(&mut request).cast::<DISPLAYCONFIG_DEVICE_INFO_HEADER>();
    // SAFETY: 调用者保证 T 以 header 开头（见上）；系统最多写 header.size 个字节，也就是整个 T
    let status = unsafe {
        pointer.write(header);
        DisplayConfigGetDeviceInfo(pointer)
    };
    (status == 0).then_some(request)
}

fn target_name(adapter: LUID, id: u32) -> Option<DISPLAYCONFIG_TARGET_DEVICE_NAME> {
    // SAFETY: DISPLAYCONFIG_TARGET_DEVICE_NAME 以 header 开头，是 GET_TARGET_NAME 要的结构体
    unsafe { device_info(DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, adapter, id) }
}

fn preferred_mode(adapter: LUID, id: u32) -> Option<(u32, u32)> {
    // SAFETY: DISPLAYCONFIG_TARGET_PREFERRED_MODE 以 header 开头，是 GET_TARGET_PREFERRED_MODE 要的结构体
    let mode: DISPLAYCONFIG_TARGET_PREFERRED_MODE =
        unsafe { device_info(DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE, adapter, id)? };
    (mode.width > 0 && mode.height > 0).then_some((mode.width, mode.height))
}

/// 笔记本、一体机自带的屏幕接在内部接口上（微软文档里 DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY 的说明）。
fn is_internal(technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY) -> bool {
    matches!(
        technology,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
            | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS
            | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
            | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
    )
}

fn same_source(a: &DISPLAYCONFIG_PATH_INFO, b: &DISPLAYCONFIG_PATH_INFO) -> bool {
    let (a, b) = (&a.sourceInfo, &b.sourceInfo);
    a.adapterId.LowPart == b.adapterId.LowPart && a.adapterId.HighPart == b.adapterId.HighPart && a.id == b.id
}

/// 以 NUL 结尾的定长 UTF-16 字符串；空的是 `None`。
pub(super) fn text(buf: &[u16]) -> Option<String> {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]).trim().to_owned();
    (!s.is_empty()).then_some(s)
}
