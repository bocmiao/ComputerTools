//! 检测「显卡：集成显卡和独立显卡」（scripts/checks/display/gpus.ps1）的判断：CI 的 Windows 机器是虚拟机，只有 Hyper-V
//! 的虚拟显卡，真跑只能走到「没找到显卡」。这里用 tests/pwsh/gpus-scenarios.ps1 冒充 WMI（同名函数盖过 Get-CimInstance），
//! 把常见显卡型号名和几种机器（游戏本、台式机插错线、独显被禁用、没驱动、一体机、虚拟机……）都跑一遍。
//! Windows 上用 powershell.exe（5.1），其他系统上用 pwsh（没装就跳过）。

use std::path::{Path, PathBuf};
use std::process::Command;

fn program() -> Option<PathBuf> {
    if cfg!(windows) {
        return Some(PathBuf::from("powershell.exe"));
    }
    std::env::var_os("PATH")
        .and_then(|paths| std::env::split_paths(&paths).map(|p| p.join("pwsh")).find(|p| p.is_file()))
}

#[test]
fn gpu_check_tells_integrated_and_discrete_apart() {
    let Some(program) = program() else {
        eprintln!("没有找到 PowerShell，跳过");
        return;
    };
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let harness = here.join("tests").join("pwsh").join("gpus-scenarios.ps1");
    let root = here.parent().and_then(Path::parent).expect("仓库根目录");
    let script = root.join("scripts").join("checks").join("display").join("gpus.ps1");
    let out = Command::new(program)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&harness)
        .arg("-Script")
        .arg(&script)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("{stdout}");
    assert!(out.status.success(), "场景脚本没跑完：\n{stdout}\n{stderr}");
    assert!(stdout.lines().any(|l| l.trim() == "bad=0"), "有场景判错了（BAD 那几行）：\n{stdout}\n{stderr}");
}
