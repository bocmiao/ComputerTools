//! 请资源管理器替小药箱打开网址（或程序）。
//!
//! 小药箱以管理员身份运行。直接用 ShellExecute 打开网址，浏览器也会以管理员身份运行：从网页下载的安装包打开时
//! 不再弹 UAC；已经开着的浏览器（普通权限）也接不上这个窗口。所以网页交给资源管理器去打开：资源管理器以登录用户
//! 的普通权限运行，它打开的浏览器也是。
//!
//! 做法照微软的 ExecInExplorer 示例（microsoft/Windows-classic-samples，MIT）和 Raymond Chen《How can I launch
//! an unelevated process from my elevated process and vice versa?》：在 ShellWindows 里找到桌面窗口，从桌面的
//! 外壳视图拿到 Shell.Application（IShellDispatch2），请它 ShellExecute。Firefox 的 ShellExecuteByExplorer
//! （widget/windows/ShellHeaderOnlyUtils.h）也这样做，并且先调用 CoAllowSetForegroundWindow，让打开的窗口能到
//! 最前面，这里照做。
//!
//! windows-sys 只有函数和常量，没有 COM 接口：下面只写出用到的方法，前面的方法用 usize 占住位置。方法的顺序照
//! Windows SDK 的接口定义（和 windows crate 生成的虚函数表逐个核对过）。

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::mpsc;
use std::time::Duration;

use windows_sys::Win32::Foundation::{E_OUTOFMEMORY, E_POINTER, S_FALSE, SysAllocString, SysFreeString};
use windows_sys::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoAllowSetForegroundWindow,
    CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::Variant::{VARIANT, VT_BSTR};
use windows_sys::Win32::UI::Shell::{
    IUnknown_QueryService, SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows_sys::core::{BSTR, GUID, HRESULT};

const IID_ISHELL_WINDOWS: GUID = GUID::from_u128(0x85cb6900_4d95_11cf_960c_0080c7f4ee85);
const IID_ISHELL_BROWSER: GUID = GUID::from_u128(0x000214e2_0000_0000_c000_000000000046);
const IID_IDISPATCH: GUID = GUID::from_u128(0x00020400_0000_0000_c000_000000000046);
const IID_ISHELL_FOLDER_VIEW_DUAL: GUID = GUID::from_u128(0xe7a1af80_4d96_11cf_960c_0080c7f4ee85);
const IID_ISHELL_DISPATCH2: GUID = GUID::from_u128(0xa4c6892c_3ba9_11d2_9dea_00c04fb16162);

/// 最多等多久：资源管理器没有响应时，调用它的方法会一直等，小药箱的按钮不能跟着一直转圈。
pub const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 找不到桌面：资源管理器没在运行（被结束了，或者换成了别的桌面程序），或者小药箱是用另一个账户提升的
    NoDesktop,
    /// 等了 [`TIMEOUT`] 资源管理器还没有答复。那个线程留着等它，答复以后自己结束
    TimedOut,
    /// 某一步失败了（HRESULT）
    Failed(HRESULT),
}

type Raw = *mut c_void;

#[repr(C)]
struct IUnknownVtbl {
    query_interface: unsafe extern "system" fn(Raw, *const GUID, *mut Raw) -> HRESULT,
    _add_ref: usize,
    release: unsafe extern "system" fn(Raw) -> u32,
}

/// IShellWindows（exdisp.h）：IDispatch 的 4 个方法之后依次是 Count、Item、_NewEnum、Register、RegisterPending、
/// Revoke、OnNavigate、OnActivated、FindWindowSW
#[repr(C)]
struct IShellWindowsVtbl {
    unknown: IUnknownVtbl,
    _dispatch: [usize; 4],
    _before: [usize; 8],
    find_window_sw:
        unsafe extern "system" fn(Raw, *const VARIANT, *const VARIANT, i32, *mut i32, i32, *mut Raw) -> HRESULT,
}

/// IShellBrowser（shobjidl_core.h）：IOleWindow 的 2 个方法之后依次是 InsertMenusSB、SetMenuSB、RemoveMenusSB、
/// SetStatusTextSB、EnableModelessSB、TranslateAcceleratorSB、BrowseObject、GetViewStateStream、GetControlWindow、
/// SendControlMsg、QueryActiveShellView
#[repr(C)]
struct IShellBrowserVtbl {
    unknown: IUnknownVtbl,
    _ole_window: [usize; 2],
    _before: [usize; 10],
    query_active_shell_view: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT,
}

/// IShellView（shobjidl_core.h）：IOleWindow 的 2 个方法之后依次是 TranslateAccelerator、EnableModeless、
/// UIActivate、Refresh、CreateViewWindow、DestroyViewWindow、GetCurrentInfo、AddPropertySheetPages、
/// SaveViewState、SelectItem、GetItemObject
#[repr(C)]
struct IShellViewVtbl {
    unknown: IUnknownVtbl,
    _ole_window: [usize; 2],
    _before: [usize; 10],
    get_item_object: unsafe extern "system" fn(Raw, u32, *const GUID, *mut Raw) -> HRESULT,
}

/// IShellFolderViewDual（shldisp.h）：IDispatch 之后第一个是 get_Application
#[repr(C)]
struct IShellFolderViewDualVtbl {
    unknown: IUnknownVtbl,
    _dispatch: [usize; 4],
    get_application: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT,
}

/// IShellDispatch2（shldisp.h）：IDispatch 之后是 IShellDispatch 的 23 个方法（Application 到 ControlPanelItem），
/// 再往后是 IsRestricted、ShellExecute
#[repr(C)]
struct IShellDispatch2Vtbl {
    unknown: IUnknownVtbl,
    _dispatch: [usize; 4],
    _shell_dispatch: [usize; 23],
    _is_restricted: usize,
    shell_execute: unsafe extern "system" fn(Raw, BSTR, VARIANT, VARIANT, VARIANT, VARIANT) -> HRESULT,
}

/// 一个 COM 接口指针（引用归我们）；离开作用域时 Release。
struct Com(Raw);

impl Com {
    /// 刚拿到的接口指针；方法说成功了却给了空指针，算失败。
    fn take(hr: HRESULT, raw: Raw) -> Result<Com, Error> {
        if hr < 0 {
            Err(Error::Failed(hr))
        } else if raw.is_null() {
            Err(Error::Failed(E_POINTER))
        } else {
            Ok(Com(raw))
        }
    }

    /// 虚函数表。
    ///
    /// # Safety
    /// `T` 必须是这个接口（或者它开头一段）的虚函数表布局。
    unsafe fn vtbl<T>(&self) -> &T {
        // SAFETY: COM 接口指针指向的第一个字段就是虚函数表的指针；布局由调用者保证
        unsafe { &**(self.0 as *const *const T) }
    }

    fn query(&self, iid: &GUID) -> Result<Com, Error> {
        let mut raw = null_mut();
        // SAFETY: self.0 是有效的接口指针，所有接口的虚函数表都以 IUnknown 开头
        let hr = unsafe { (self.vtbl::<IUnknownVtbl>().query_interface)(self.0, iid, &mut raw) };
        Com::take(hr, raw)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: 引用是我们的，只释放这一次
        unsafe { (self.vtbl::<IUnknownVtbl>().release)(self.0) };
    }
}

/// 一个 BSTR；离开作用域时释放。
struct Bstr(BSTR);

impl Bstr {
    fn new(s: &str) -> Result<Bstr, Error> {
        let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
        // SAFETY: wide 以 NUL 结尾
        let b = unsafe { SysAllocString(wide.as_ptr()) };
        if b.is_null() { Err(Error::Failed(E_OUTOFMEMORY)) } else { Ok(Bstr(b)) }
    }

    /// 装着这个字符串的 VARIANT（VT_BSTR）。字符串还归 `self`，VARIANT 不要 VariantClear。
    fn variant(&self) -> VARIANT {
        let mut v = empty_variant();
        v.Anonymous.Anonymous.vt = VT_BSTR;
        v.Anonymous.Anonymous.Anonymous.bstrVal = self.0;
        v
    }
}

impl Drop for Bstr {
    fn drop(&mut self) {
        // SAFETY: SysAllocString 分配的，只释放这一次
        unsafe { SysFreeString(self.0) };
    }
}

/// VT_EMPTY
fn empty_variant() -> VARIANT {
    // SAFETY: 全零就是 VT_EMPTY
    unsafe { std::mem::zeroed() }
}

/// 请资源管理器打开 `file`（网址、程序），`args` 是程序的参数。在单独的线程上调用（要自己的 COM 套间），
/// 最多等 [`TIMEOUT`]。资源管理器接下来是异步打开的：返回成功只说明它接下了。
pub fn shell_execute(file: &str, args: Option<&str>) -> Result<(), Error> {
    let file = file.to_owned();
    let args = args.map(str::to_owned);
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("explorer-exec".into())
        .spawn(move || {
            // SAFETY: 这个线程自己的 COM 初始化；成功（含 S_FALSE）时要配对调用 CoUninitialize
            let com = unsafe { CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
            let result = if com < 0 { Err(Error::Failed(com)) } else { shell_execute_now(&file, args.as_deref()) };
            if com >= 0 {
                // SAFETY: 和上面成功的 CoInitializeEx 配对；接口指针都已经在 shell_execute_now 里释放了
                unsafe { CoUninitialize() };
            }
            let _ = tx.send(result);
        })
        .map_err(|_| Error::Failed(E_OUTOFMEMORY))?;
    rx.recv_timeout(TIMEOUT).unwrap_or(Err(Error::TimedOut))
}

/// 在当前线程（已经初始化成单线程套间）上走一遍：ShellWindows → 桌面 → 桌面的外壳视图 → Shell.Application →
/// ShellExecute。
fn shell_execute_now(file: &str, args: Option<&str>) -> Result<(), Error> {
    // 资源管理器提供的 ShellWindows：它登记着所有资源管理器窗口，包括桌面
    let mut raw = null_mut();
    // SAFETY: 参数都是合法值
    let hr = unsafe { CoCreateInstance(&ShellWindows, null_mut(), CLSCTX_LOCAL_SERVER, &IID_ISHELL_WINDOWS, &mut raw) };
    let windows = Com::take(hr, raw)?;

    let empty = empty_variant();
    let mut hwnd = 0i32;
    let mut raw = null_mut();
    // SAFETY: windows 是 IShellWindows；两个位置参数都是 VT_EMPTY
    let hr = unsafe {
        (windows.vtbl::<IShellWindowsVtbl>().find_window_sw)(
            windows.0,
            &empty,
            &empty,
            SWC_DESKTOP,
            &mut hwnd,
            SWFO_NEEDDISPATCH,
            &mut raw,
        )
    };
    if hr == S_FALSE {
        // 调用成功了，但是没有桌面窗口
        if !raw.is_null() {
            drop(Com(raw));
        }
        return Err(Error::NoDesktop);
    }
    let desktop = Com::take(hr, raw)?;

    let mut raw = null_mut();
    // SAFETY: desktop 是有效的接口指针
    let hr = unsafe { IUnknown_QueryService(desktop.0, &SID_STopLevelBrowser, &IID_ISHELL_BROWSER, &mut raw) };
    let browser = Com::take(hr, raw)?;

    let mut raw = null_mut();
    // SAFETY: browser 是 IShellBrowser
    let hr = unsafe { (browser.vtbl::<IShellBrowserVtbl>().query_active_shell_view)(browser.0, &mut raw) };
    let view = Com::take(hr, raw)?;

    let mut raw = null_mut();
    // SAFETY: view 是 IShellView
    let hr = unsafe {
        (view.vtbl::<IShellViewVtbl>().get_item_object)(view.0, SVGIO_BACKGROUND as u32, &IID_IDISPATCH, &mut raw)
    };
    let background = Com::take(hr, raw)?;
    let folder_view = background.query(&IID_ISHELL_FOLDER_VIEW_DUAL)?;

    let mut raw = null_mut();
    // SAFETY: folder_view 是 IShellFolderViewDual
    let hr = unsafe { (folder_view.vtbl::<IShellFolderViewDualVtbl>().get_application)(folder_view.0, &mut raw) };
    let application = Com::take(hr, raw)?;
    let shell = application.query(&IID_ISHELL_DISPATCH2)?;

    // 把「把窗口放到最前面」的许可交给资源管理器，不然浏览器可能开在小药箱后面。小药箱不在前台时这一步会失败
    // （E_ACCESSDENIED），不影响打开
    // SAFETY: shell 是有效的接口指针，保留参数传空
    unsafe { CoAllowSetForegroundWindow(shell.0, null()) };

    let file = Bstr::new(file)?;
    let args = args.map(Bstr::new).transpose()?;
    let args = args.as_ref().map_or_else(empty_variant, Bstr::variant);
    // 当前文件夹、动词（默认的「打开」）、窗口怎么显示都用默认的
    // SAFETY: shell 是 IShellDispatch2；BSTR 在调用期间有效；VARIANT 按值传入，调用方仍拥有其中的字符串
    let hr = unsafe {
        (shell.vtbl::<IShellDispatch2Vtbl>().shell_execute)(
            shell.0,
            file.0,
            args,
            empty_variant(),
            empty_variant(),
            empty_variant(),
        )
    };
    if hr < 0 { Err(Error::Failed(hr)) } else { Ok(()) }
}
