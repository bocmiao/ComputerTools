//! 启动前的准备：检查运行环境、准备只有管理员能写的数据目录、解压并校验脚本、组装引擎。
//!
//! 数据目录（Windows）：
//! - `%ProgramData%\Medkit\runtime\<数据哈希>\`：运行时的脚本，每次执行前校验哈希
//! - `%ProgramData%\Medkit\journal\journal.jsonl`：修改日志
//!
//! 在其他系统上（只用于开发界面）改用假的系统和脚本执行器，数据放在临时目录里。

use std::path::Path;
use std::sync::{Arc, Mutex};

use medkit_core::Engine;
use medkit_core::bundle::Bundle;
use medkit_core::catalog::Catalog;
use medkit_core::journal::Journal;

/// 构建时由 build.rs 打包好的数据和脚本。
const BUNDLE_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/bundle.json"));

pub struct AppState {
    /// 引擎没能启动时，每个命令都返回这个原因，界面会显示出来
    pub engine: Result<Arc<Engine>, String>,
    pub rename: Arc<Mutex<crate::rename::RenameState>>,
    pub images: Arc<Mutex<crate::images::ImageState>>,
    /// 「图片合成 PDF」最近一次存好的文件
    pub pdf: Arc<Mutex<crate::pdf::PdfState>>,
    /// 「U 盘里的文件不见了」选的文件夹、最近一次结果和能撤销的改动
    pub hidden: Arc<Mutex<crate::hidden::HiddenState>>,
    pub awake: Arc<Mutex<crate::awake::AwakeState>>,
    /// 小药箱安排的定时关机、定时重启
    pub shutdown: Arc<Mutex<crate::shutdown::ShutdownState>>,
    /// 「文件删不掉：是谁占着」最近一次选的文件或文件夹（「再查一次」用）
    pub lockers: Arc<Mutex<Option<medkit_core::lockers::LockTarget>>>,
    /// 「找大文件和重复文件」选的文件夹和最近一次结果里的文件
    pub space: Arc<Mutex<crate::space::SpaceState>>,
}

pub fn init() -> AppState {
    AppState {
        engine: build_engine().map(Arc::new),
        rename: Arc::new(Mutex::new(Default::default())),
        images: Arc::new(Mutex::new(Default::default())),
        pdf: Arc::new(Mutex::new(Default::default())),
        hidden: Arc::new(Mutex::new(Default::default())),
        awake: Arc::new(Mutex::new(Default::default())),
        shutdown: Arc::new(Mutex::new(Default::default())),
        lockers: Arc::new(Mutex::new(None)),
        space: Arc::new(Mutex::new(Default::default())),
    }
}

fn build_engine() -> Result<Engine, String> {
    let bundle = Bundle::from_json(BUNDLE_JSON)?;
    let root = data_root();
    let runtime = root.join("runtime");
    let scripts_dir = runtime.join(&bundle.hash);
    let journal_dir = root.join("journal");
    for dir in [&root, &runtime, &scripts_dir, &journal_dir] {
        secure_dir(dir).map_err(|e| format!("准备数据目录 {} 失败：{e}", dir.display()))?;
    }
    remove_old_runtimes(&runtime, &bundle.hash);
    let manifest = bundle.extract(&scripts_dir).map_err(|e| format!("解压脚本失败：{e}"))?;
    let journal = Journal::open(journal_dir.join("journal.jsonl")).map_err(|e| e.to_string())?;
    let (platform, runner) = backend(scripts_dir, manifest);
    Ok(Engine::new(Catalog::new(bundle.catalog), bundle.hash, env!("CARGO_PKG_VERSION"), platform, runner, journal))
}

/// 数据版本变了以后，旧的脚本目录就用不上了。
fn remove_old_runtimes(runtime: &Path, current: &str) {
    let Ok(entries) = std::fs::read_dir(runtime) else { return };
    for entry in entries.flatten() {
        if entry.file_name() != current && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// ───────────── Windows ─────────────

#[cfg(windows)]
mod os {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use medkit_core::platform::Platform;
    use medkit_core::platform::windows::{WindowsPlatform, ensure_secure_dir, program_data};
    use medkit_core::script::{HostConfig, PowerShellHost, ScriptManifest, ScriptRunner};
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, SetDefaultDllDirectories};
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    /// WebView2 运行时在注册表里的登记位置（微软文档《Distribute your app and the WebView2 Runtime》）。
    const WEBVIEW2_CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    pub fn data_root() -> PathBuf {
        program_data().join("Medkit")
    }

    pub fn secure_dir(dir: &Path) -> std::io::Result<()> {
        ensure_secure_dir(dir).map(|_| ())
    }

    fn webview2_installed() -> bool {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};
        let version = |root, flags| {
            RegKey::predef(root)
                .open_subkey_with_flags(format!(r"SOFTWARE\{WEBVIEW2_CLIENT}"), KEY_READ | flags)
                .and_then(|k| k.get_value::<String, _>("pv"))
                .ok()
        };
        [version(HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY), version(HKEY_CURRENT_USER, 0)]
            .into_iter()
            .flatten()
            .any(|v| !v.is_empty() && v != "0.0.0.0")
    }

    /// 启动最开头调用：清掉能改变 WebView2 行为的环境变量。
    ///
    /// 程序以管理员身份运行，但它的环境块是从启动它的那个（普通权限）进程继承来的。
    /// 一个普通权限的程序如果先设好这些变量再启动小药箱，就能让提权后的 WebView2 去加载
    /// 别处的 Edge 或者带上额外的浏览器参数。这里在初始化 WebView2 之前把它们清掉。
    pub fn harden_environment() {
        const VARS: &[&str] = &[
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            "WEBVIEW2_USER_DATA_FOLDER",
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
        ];
        for name in VARS {
            // SAFETY: 在 main 最开头、还是单线程时调用，没有并发读写环境。
            unsafe { std::env::remove_var(name) };
        }
        // 之后按名字加载的 DLL 只从程序目录和 System32 找，不再找当前目录和 PATH
        // SAFETY: 只设置本进程的 DLL 搜索路径
        unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) };
    }

    /// 只允许同时开一个小药箱。两个实例各有各的锁，可能交错修改同一个位置、交错写同一份修改日志，
    /// 撤销时就会恢复到错误的值。用全局命名互斥体（跨登录会话）拦住第二个。
    /// 返回 false 表示已经有一个在运行。互斥体要一直留到进程退出，所以故意不关句柄。
    pub fn claim_single_instance() -> bool {
        let name: Vec<u16> = "Global\\club.miao.medkit.instance".encode_utf16().chain(Some(0)).collect();
        // SAFETY: name 以 NUL 结尾；安全属性用默认值
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: 紧接在 CreateMutexW 之后读取
        let err = unsafe { GetLastError() };
        !handle.is_null() && err != ERROR_ALREADY_EXISTS
    }

    pub fn preflight() -> Result<(), String> {
        // 清单里要求了管理员权限，但用 __COMPAT_LAYER=RunAsInvoker 之类的方法仍然可以绕过
        if !WindowsPlatform::new().is_admin() {
            return Err("电脑小药箱需要以管理员身份运行。请右键点击程序，选择「以管理员身份运行」。".to_owned());
        }
        Ok(())
    }

    /// 注册表里查不到 WebView2 时的提示。只在界面真的起不来时显示：注册表查不到不一定就是没装。
    pub fn webview2_hint() -> Option<String> {
        (!webview2_installed()).then(|| {
            "这台电脑可能缺少 Microsoft Edge WebView2 运行时，小药箱的界面需要它。\n\n\
             请用安装包重新安装（安装包会自动装上它），\
             或者在微软官网搜索「WebView2 运行时」下载安装后，再打开小药箱。"
                .to_owned()
        })
    }

    /// 用 System32 下的 powershell.exe 的绝对路径，不依赖 PATH。
    fn powershell() -> PathBuf {
        let mut buf = [0u16; 260];
        // SAFETY: 缓冲区长度正确
        let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let system32 = if n > 0 && n < buf.len() {
            PathBuf::from(String::from_utf16_lossy(&buf[..n]))
        } else {
            PathBuf::from(r"C:\Windows\System32")
        };
        system32.join(r"WindowsPowerShell\v1.0\powershell.exe")
    }

    pub fn backend(scripts: PathBuf, manifest: ScriptManifest) -> (Arc<dyn Platform>, Arc<dyn ScriptRunner>) {
        let host =
            PowerShellHost::new(HostConfig { program: powershell(), scripts_root: scripts, manifest: Some(manifest) });
        (Arc::new(WindowsPlatform::new()), Arc::new(host))
    }

    pub fn fatal(message: &str) {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (text, caption) = (wide(message), wide("电脑小药箱"));
        // SAFETY: 两个字符串都以 NUL 结尾
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR) };
    }
}

// ───────────── 其他系统（只用于开发界面） ─────────────

#[cfg(not(windows))]
mod os {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use medkit_core::platform::Platform;
    use medkit_core::platform::mock::MockPlatform;
    use medkit_core::script::{MockRunner, ScriptManifest, ScriptRunner};

    pub fn data_root() -> PathBuf {
        std::env::temp_dir().join("medkit-dev")
    }

    pub fn secure_dir(dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)
    }

    pub fn harden_environment() {}

    pub fn claim_single_instance() -> bool {
        true
    }

    pub fn preflight() -> Result<(), String> {
        Ok(())
    }

    pub fn webview2_hint() -> Option<String> {
        None
    }

    pub fn backend(_scripts: PathBuf, _manifest: ScriptManifest) -> (Arc<dyn Platform>, Arc<dyn ScriptRunner>) {
        let runner = MockRunner::new();
        // 假系统上没有开机启动项，右键菜单和「新建」菜单里也没有软件加的项目
        runner.returns(medkit_core::startup::LIST_SCRIPT, serde_json::json!({ "result": "ok", "items": [] }));
        runner.returns(medkit_core::context_menu::LIST_SCRIPT, serde_json::json!({ "result": "ok", "items": [] }));
        runner.returns(medkit_core::new_menu::LIST_SCRIPT, serde_json::json!({ "result": "ok", "items": [] }));
        (Arc::new(MockPlatform::new()), Arc::new(runner))
    }

    pub fn fatal(message: &str) {
        eprintln!("{message}");
    }
}

use os::{backend, data_root, secure_dir};
pub use os::{claim_single_instance, fatal, harden_environment, preflight, webview2_hint};

/// 测试用：用假系统和真实的内嵌数据造一个 AppState，不碰真实系统、不落盘到固定位置。
#[cfg(any(test, feature = "test-helpers"))]
pub fn test_state() -> AppState {
    AppState {
        engine: build_engine().map(Arc::new),
        rename: Arc::new(Mutex::new(Default::default())),
        images: Arc::new(Mutex::new(Default::default())),
        pdf: Arc::new(Mutex::new(Default::default())),
        hidden: Arc::new(Mutex::new(Default::default())),
        awake: Arc::new(Mutex::new(Default::default())),
        shutdown: Arc::new(Mutex::new(Default::default())),
        lockers: Arc::new(Mutex::new(None)),
        space: Arc::new(Mutex::new(Default::default())),
    }
}
