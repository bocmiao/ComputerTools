//! 构建时：把 catalog/ 和 scripts/ 校验后打包进 exe；生成 Windows 资源（图标、要求管理员的清单）和命令权限。

use std::path::Path;

/// 界面能调用的全部命令。每个命令会生成 `allow-<命令>` 权限，在 capabilities/main.json 里逐个放行。
const COMMANDS: &[&str] = &[
    "system_info",
    "catalog_summary",
    "symptom_detail",
    "run_profile",
    "run_check",
    "feature_detect",
    "feature_preview",
    "feature_apply",
    "journal_list",
    "journal_undo",
    "journal_undo_session",
    "report_generate",
];

fn main() {
    embed_bundle();
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows/app.manifest"));
    let attrs = tauri_build::Attributes::new()
        .windows_attributes(windows)
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    println!("cargo:rerun-if-changed=windows/app.manifest");
    tauri_build::try_build(attrs).expect("tauri-build 失败");
}

fn embed_bundle() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for dir in ["catalog", "scripts"] {
        println!("cargo:rerun-if-changed={}", repo.join(dir).display());
    }
    let (bundle, problems) = medkit_core::bundle::Bundle::from_repo(&repo);
    let Some(bundle) = bundle else {
        for p in problems.iter().filter(|p| p.severity == medkit_core::catalog::Severity::Error) {
            eprintln!("{p}");
        }
        panic!("catalog/ 或 scripts/ 没通过校验，详情运行：cargo run -p medkit-data -- check");
    };
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("bundle.json");
    std::fs::write(out, bundle.to_json()).expect("写 bundle.json");
}
