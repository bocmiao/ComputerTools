//! 只在 Windows 上运行：真实的注册表、服务、数据目录，以及用 Windows PowerShell 5.1 跑仓库里的检测和修复。
//!
//! 标了 `#[ignore]` 的测试会临时改动本机设置（结束时恢复），需要管理员权限：
//!
//! ```text
//! cargo test -p medkit-core --test windows -- --include-ignored
//! ```
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use medkit_core::Engine;
use medkit_core::bundle::Bundle;
use medkit_core::catalog::Catalog;
use medkit_core::journal::{Journal, new_id};
use medkit_core::model::{Action, Feature, StartType, ToolGroup};
use medkit_core::platform::Platform;
use medkit_core::platform::windows::{WindowsPlatform, dir_owner_sid, ensure_secure_dir};
use medkit_core::registry::{RegRoot, RegValue, SpecRoot, is_sid, split_key};
use medkit_core::render::unresolved;
use medkit_core::script::{HostConfig, PowerShellHost};
use medkit_core::tools;
use medkit_core::views::FeatureStateKind;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 在 HKCU 下建一个测试专用的键，测试结束时删掉。
struct TestKey {
    sub: String,
}

impl TestKey {
    fn new() -> Self {
        Self { sub: format!(r"Software\MedkitTest-{}", new_id()) }
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(&self.sub);
    }
}

#[test]
fn registry_values_round_trip() {
    let p = WindowsPlatform::new();
    let k = TestKey::new();
    let key = format!(r"{}\Deep\Er", k.sub);
    let root = RegRoot::CurrentUser;
    assert!(!p.reg_key_exists(&root, &k.sub).unwrap());
    assert_eq!(p.reg_get(&root, &key, "missing").unwrap(), None);

    let values = [
        ("dword", RegValue::Dword(0xDEAD_BEEF)),
        ("qword", RegValue::Qword(u64::MAX)),
        ("string", RegValue::String("中文 and spaces \\ \"quotes\"".into())),
        ("empty", RegValue::String(String::new())),
        ("expand", RegValue::ExpandString(r"%SystemRoot%\System32".into())),
        ("multi", RegValue::MultiString(vec!["one".into(), "二".into()])),
        ("binary", RegValue::Binary(vec![0, 1, 2, 0xFF])),
        ("", RegValue::String("default value".into())),
    ];
    for (name, v) in &values {
        p.reg_set(&root, &key, name, v).unwrap();
        assert_eq!(p.reg_get(&root, &key, name).unwrap().as_ref(), Some(v), "{name}");
    }
    assert!(p.reg_key_exists(&root, &key).unwrap());

    // 有值的键不能删；值删光以后可以
    assert!(!p.reg_delete_key_if_empty(&root, &key).unwrap());
    for (name, _) in &values {
        p.reg_delete_value(&root, &key, name).unwrap();
        assert_eq!(p.reg_get(&root, &key, name).unwrap(), None);
    }
    p.reg_delete_value(&root, &key, "never-existed").unwrap();
    assert!(p.reg_delete_key_if_empty(&root, &key).unwrap());
    assert!(!p.reg_key_exists(&root, &key).unwrap());
    // 有子键的键也不能删
    p.reg_set(&root, &format!(r"{}\Deep\Other", k.sub), "x", &RegValue::Dword(1)).unwrap();
    assert!(!p.reg_delete_key_if_empty(&root, &format!(r"{}\Deep", k.sub)).unwrap());
    assert!(!p.reg_delete_key_if_empty(&root, r"Software\MedkitTest-does-not-exist").unwrap());
}

#[test]
fn hku_path_reaches_the_same_hive_as_hkcu() {
    let p = WindowsPlatform::new();
    let me = p.process_user().expect("能拿到当前进程的用户");
    assert!(is_sid(&me.sid), "{}", me.sid);
    let k = TestKey::new();
    p.reg_set(&RegRoot::CurrentUser, &k.sub, "via", &RegValue::Dword(7)).unwrap();
    assert_eq!(p.reg_get(&RegRoot::User(me.sid.clone()), &k.sub, "via").unwrap(), Some(RegValue::Dword(7)));
}

#[test]
fn identity_and_os_info() {
    let p = WindowsPlatform::new();
    let os = p.os_info();
    eprintln!("{os:?}");
    eprintln!("登录用户：{:?}；进程用户：{:?}；管理员：{}", p.interactive_user(), p.process_user(), p.is_admin());
    assert!(os.build >= 17763, "{os:?}");
    assert!(!os.caption.is_empty() && !os.edition_id.is_empty(), "{os:?}");
    let me = p.process_user().unwrap();
    assert!(me.name.contains('\\'), "应该是「域\\用户名」：{}", me.name);
}

#[test]
fn services_can_be_read() {
    let p = WindowsPlatform::new();
    assert!(p.service_get("EventLog").unwrap().is_some());
    assert_eq!(p.service_get("MedkitNoSuchService").unwrap(), None);
}

/// 把一个服务的启动类型依次改成 `types`，每次读回核对，最后恢复原样。
fn cycle_start_types(p: &WindowsPlatform, name: &str, types: &[StartType]) {
    let original = p.service_get(name).unwrap().unwrap_or_else(|| panic!("没有服务 {name}"));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for &st in types {
            p.service_set(name, st).unwrap_or_else(|e| panic!("{name} → {st:?}：{e}"));
            assert_eq!(p.service_get(name).unwrap(), Some(st), "{name} → {st:?}");
        }
    }));
    p.service_set(name, original).unwrap();
    assert_eq!(p.service_get(name).unwrap(), Some(original));
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
#[ignore = "会临时改动服务的启动类型；需要管理员权限"]
fn service_start_type_round_trip() {
    let p = WindowsPlatform::new();
    cycle_start_types(&p, "Spooler", &[StartType::Manual, StartType::Auto, StartType::Disabled]);
    // 延迟启动要用不属于加载顺序组的服务
    cycle_start_types(&p, "W32Time", &[StartType::DelayedAuto, StartType::Auto, StartType::Manual]);
    assert!(p.service_set("MedkitNoSuchService", StartType::Manual).is_err());
}

#[test]
#[ignore = "需要管理员权限"]
fn delayed_start_is_refused_for_grouped_services_without_changing_anything() {
    let p = WindowsPlatform::new();
    // 打印服务属于 SpoolerGroup
    let before = p.service_get("Spooler").unwrap().expect("有打印服务");
    let outcome = p.service_set("Spooler", StartType::DelayedAuto);
    let after = p.service_get("Spooler").unwrap();
    p.service_set("Spooler", before).unwrap();
    let e = outcome.expect_err("属于加载顺序组的服务不能设为延迟启动");
    assert!(e.to_string().contains("加载顺序组"), "{e}");
    assert_eq!(after, Some(before), "拒绝时不能留下改动");
}

#[test]
#[ignore = "需要管理员权限"]
fn data_dir_is_secured_and_planted_dirs_are_moved_aside() {
    let base = tempfile::tempdir().unwrap();

    // 全新目录：建好，所有者是 Administrators
    let fresh = base.path().join("fresh");
    assert_eq!(ensure_secure_dir(&fresh).unwrap(), None);
    assert_eq!(dir_owner_sid(&fresh).unwrap(), "S-1-5-32-544");
    // 再调一次：自己建的目录不能被当成不可信
    assert_eq!(ensure_secure_dir(&fresh).unwrap(), None);
    std::fs::write(fresh.join("journal.jsonl"), b"{}\n").unwrap();
    assert_eq!(ensure_secure_dir(&fresh).unwrap(), None);
    assert!(fresh.join("journal.jsonl").exists());

    // 预先放好的符号链接：挪开，重建真目录，链接指向的地方不受影响
    let target = base.path().join("elsewhere");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"x").unwrap();
    let planted = base.path().join("planted");
    std::os::windows::fs::symlink_dir(&target, &planted).unwrap();
    let moved = ensure_secure_dir(&planted).unwrap().expect("应该挪开");
    assert!(std::fs::symlink_metadata(&moved).unwrap().file_type().is_symlink());
    assert!(!std::fs::symlink_metadata(&planted).unwrap().file_type().is_symlink());
    assert!(planted.is_dir());
    assert!(target.join("keep.txt").exists());
}

/// 用仓库里的数据和脚本、真实系统和 powershell.exe 组装一个引擎。
fn real_engine(dir: &Path) -> (Engine, Bundle, Arc<WindowsPlatform>) {
    let (bundle, problems) = Bundle::from_repo(&repo_root());
    let bundle = bundle.unwrap_or_else(|| panic!("仓库数据没通过校验：{problems:#?}"));
    let manifest = bundle.extract(&dir.join("runtime")).unwrap();
    let host = PowerShellHost::new(HostConfig {
        program: PathBuf::from("powershell.exe"),
        scripts_root: dir.join("runtime"),
        manifest: Some(manifest),
    });
    let platform = Arc::new(WindowsPlatform::new());
    let engine = Engine::new(
        Catalog::new(bundle.catalog.clone()),
        bundle.hash.clone(),
        "test",
        platform.clone(),
        Arc::new(host),
        Journal::open(dir.join("journal").join("journal.jsonl")).unwrap(),
    );
    (engine, bundle, platform)
}

/// CI 的 Windows 运行环境是服务器版虚拟机，有些检测在那里本来就查不了（例如只针对桌面版的检测）。
/// 这些检测的 ID 写在环境变量 `MEDKIT_SMOKE_EXPECTED_FAILURES` 里（逗号分隔），它们报的脚本错误只提示、不算失败。
/// 超时、宿主出错、脚本校验失败、没定义的结果代码，不管在不在名单里都算失败。
fn expected_failures() -> Vec<String> {
    std::env::var("MEDKIT_SMOKE_EXPECTED_FAILURES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 不是「这台机器查不了」，而是我们自己的问题。
fn is_contract_error(e: &str) -> bool {
    ["运行超时", "脚本文件校验失败", "脚本宿主出错", "脚本返回了没有定义的结果"].iter().any(|p| e.starts_with(p))
}

#[test]
fn every_check_runs_cleanly_on_windows_powershell() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, platform) = real_engine(dir.path());
    let admin = platform.is_admin();
    let expected = expected_failures();
    let mut failures = Vec::new();
    for id in &expected {
        if bundle.catalog.checks.iter().all(|c| &c.id != id) {
            failures.push(format!("MEDKIT_SMOKE_EXPECTED_FAILURES 里的 {id} 不是一个检测"));
        }
    }
    for c in &bundle.catalog.checks {
        if c.requires_admin && !admin {
            eprintln!("跳过（需要管理员）：{}", c.id);
            continue;
        }
        let r = engine.run_check(&c.id).unwrap();
        eprintln!("{:<40} {:<8} {:>6} ms  {}", c.id, format!("{:?}", r.status), r.duration_ms, r.message);
        if !r.facts.is_empty() {
            eprintln!("{:<40} facts: {}", "", serde_json::Value::Object(r.facts.clone()));
        }
        match &r.error {
            Some(e) if expected.contains(&c.id) && !is_contract_error(e) => {
                println!("::warning title={}::在这台 CI 机器上预期查不了：{e}", c.id);
            }
            Some(e) => failures.push(format!("{}：{e}", c.id)),
            None if expected.contains(&c.id) => {
                println!("::notice title={}::已经能跑通，可以从 MEDKIT_SMOKE_EXPECTED_FAILURES 里去掉", c.id);
            }
            None => {}
        }
        for text in std::iter::once(&r.message).chain(r.next.as_ref()) {
            let left = unresolved(text);
            if !left.is_empty() {
                failures.push(format!("{}：文字里有没替换掉的占位符 {left:?}：{text}", c.id));
            }
        }
    }
    let profiles: Vec<_> = bundle.catalog.profiles.iter().map(|p| p.id.clone()).collect();
    for id in profiles {
        engine.run_profile(&id).unwrap();
    }
    let report = engine.report_generate().unwrap();
    if let Some(me) = platform.process_user()
        && let Some((_, short)) = me.name.rsplit_once('\\')
        && short.chars().count() >= 2
    {
        assert!(!report.to_lowercase().contains(&short.to_lowercase()), "报告里出现了用户名：\n{report}");
    }
    assert!(failures.is_empty(), "有检测没跑通：\n{}", failures.join("\n"));
}

/// 功能涉及的所有注册表值和服务，在测试前后的原样。
enum Saved {
    Reg(RegRoot, String, String, Option<RegValue>),
    Service(String, Option<StartType>),
}

fn snapshot(p: &WindowsPlatform, f: &Feature) -> Vec<Saved> {
    let user_root = match p.interactive_user() {
        Some(u) if is_sid(&u.sid) => RegRoot::User(u.sid),
        _ => RegRoot::CurrentUser,
    };
    let mut out = Vec::new();
    for a in f.actions.iter().chain(&f.windows_default).chain(&f.break_actions) {
        match a {
            Action::Registry(r) => {
                let (spec, sub) = split_key(&r.key).unwrap();
                let root = if spec == SpecRoot::Hkcu { user_root.clone() } else { RegRoot::LocalMachine };
                let v = p.reg_get(&root, sub, &r.name).unwrap();
                out.push(Saved::Reg(root, sub.to_owned(), r.name.clone(), v));
            }
            Action::Service(s) => out.push(Saved::Service(s.name.clone(), p.service_get(&s.name).unwrap())),
        }
    }
    out
}

fn restore(p: &WindowsPlatform, saved: &[Saved]) {
    for s in saved.iter().rev() {
        let _ = match s {
            Saved::Reg(root, key, name, Some(v)) => p.reg_set(root, key, name, v),
            Saved::Reg(root, key, name, None) => p.reg_delete_value(root, key, name),
            Saved::Service(name, Some(st)) => p.service_set(name, *st),
            Saved::Service(_, None) => Ok(()),
        };
    }
}

#[test]
#[ignore = "会临时改动本机设置（结束时恢复）；需要管理员权限"]
fn every_feature_breaks_fixes_and_undoes() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, platform) = real_engine(dir.path());
    let mut failures = Vec::new();
    for f in &bundle.catalog.features {
        let preview = engine.feature_preview(&f.id).unwrap();
        if let Some(why) = preview.notes.iter().find(|n| n.starts_with("不能执行")) {
            eprintln!("跳过 {}：{why}", f.id);
            continue;
        }
        let saved = snapshot(&platform, f);
        let original = engine.feature_detect(&f.id).map_or(FeatureStateKind::Unknown, |d| d.state);
        let outcome = (|| -> Result<(), String> {
            engine.break_feature(&f.id).map_err(|e| format!("制造故障失败：{e}"))?;
            let broken = engine.feature_detect(&f.id).map_err(|e| format!("检测出错：{e}"))?;
            if broken.state == FeatureStateKind::Applied || broken.error.is_some() {
                return Err(format!("制造故障后检测结果不对：{broken:?}"));
            }
            let r = engine.feature_apply(&f.id).map_err(|e| format!("修复出错：{e}"))?;
            if !r.ok || r.verified != FeatureStateKind::Applied {
                return Err(format!("修复没有生效：{r:?}"));
            }
            for id in r.entry_ids.iter().rev() {
                let u = engine.journal_undo(id, false).map_err(|e| format!("撤销出错：{e}"))?;
                if !u.ok {
                    return Err(format!("撤销失败：{u:?}"));
                }
            }
            if f.reversible() {
                let after = engine.feature_detect(&f.id).map_err(|e| format!("检测出错：{e}"))?;
                if after.state != broken.state {
                    return Err(format!("撤销后没有回到修复前的状态：{:?} → {:?}", broken.state, after.state));
                }
            }
            Ok(())
        })();
        // 恢复测试前的样子
        if f.is_primitive() {
            restore(&platform, &saved);
        } else if original == FeatureStateKind::Applied {
            let _ = engine.feature_apply(&f.id);
        }
        match outcome {
            Ok(()) => eprintln!("通过 {}", f.id),
            Err(e) => failures.push(format!("{}：{e}", f.id)),
        }
    }
    assert!(failures.is_empty(), "有修复没通过往返测试：\n{}", failures.join("\n"));
}

// ───────────── 小工具 ─────────────

/// 所有 info、action 小工具都用 Windows PowerShell 5.1 真跑一遍，包括重启资源管理器
/// （CI 机器上有桌面，Winlogon 会把它拉起来）。表格里不能有没定义的文字，也不能出现电脑名、用户名。
/// 遮住的值（WiFi 密码）不打印。
#[test]
#[ignore = "会重启资源管理器、刷新 DNS 缓存"]
fn every_tool_runs_cleanly_on_windows_powershell() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, platform) = real_engine(dir.path());
    let computer = std::env::var("COMPUTERNAME").unwrap_or_default().to_lowercase();
    let user = platform
        .process_user()
        .and_then(|u| u.name.rsplit_once('\\').map(|(_, n)| n.to_lowercase()))
        .unwrap_or_default();
    let mut failures = Vec::new();
    for t in bundle.catalog.tools.iter().filter(|t| t.group != ToolGroup::Open) {
        let r = engine.tool_run(&t.id).unwrap();
        eprintln!("{:<32} {:<8} {:>6} ms  {}", t.id, format!("{:?}", r.status), r.duration_ms, r.message);
        let mut shown = Vec::new();
        for s in &r.sections {
            eprintln!("    [{}]", s.title);
            shown.push(s.title.to_lowercase());
            for row in &s.rows {
                let value = if row.secret { "（已遮住）" } else { row.value.as_str() };
                eprintln!("      {}：{value}", row.label);
                if !row.secret {
                    shown.push(row.value.to_lowercase());
                }
            }
        }
        if let Some(e) = &r.error {
            failures.push(format!("{}：{e}", t.id));
        }
        for text in std::iter::once(&r.message).chain(r.next.as_ref()) {
            let left = unresolved(text);
            if !left.is_empty() {
                failures.push(format!("{}：文字里有没替换掉的占位符 {left:?}：{text}", t.id));
            }
        }
        for (what, name) in [("电脑名", &computer), ("用户名", &user)] {
            if name.chars().count() >= 3 && shown.iter().any(|v| v.contains(name.as_str())) {
                failures.push(format!("{}：表格里出现了{what}", t.id));
            }
        }
    }
    assert!(failures.is_empty(), "有小工具没跑通：\n{}", failures.join("\n"));
}

/// 这个程序现在正在运行的进程号（用 tasklist，不用额外的依赖）。
fn pids_of(exe: &str) -> Vec<u32> {
    let Ok(out) = Command::new("tasklist").args(["/FI", &format!("IMAGENAME eq {exe}"), "/FO", "CSV", "/NH"]).output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split("\",\"").nth(1).and_then(|pid| pid.trim_matches('"').parse().ok()))
        .collect()
}

/// 关掉测试打开的窗口：只结束测试之前还没有的进程。
fn close_new(exe: &str, before: &[u32]) {
    for pid in pids_of(exe) {
        if !before.contains(&pid) {
            let _ = Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).status();
        }
    }
}

/// 每个打开类小工具真打开一次：程序在的要能打开；不在的（服务器版可能没装）要如实说「这台电脑上没有」。
/// 打开的窗口随后关掉。「设置」页面在服务器版上可能打不开，只提示、不算失败。
#[test]
#[ignore = "会打开再关掉系统工具的窗口"]
fn open_tools_launch_or_explain_why_not() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, _) = real_engine(dir.path());
    let system32 = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32");
    let mut failures = Vec::new();
    for t in bundle.catalog.tools.iter().filter(|t| t.group == ToolGroup::Open) {
        let open = t.open.as_ref().expect("open 小工具有 open");
        match (&open.program, &open.settings) {
            (Some(name), None) => {
                let p = tools::program(name).expect("名单里有");
                let present = system32.join(p.exe).is_file()
                    && p.args.iter().filter(|a| a.ends_with(".msc")).all(|a| system32.join(a).is_file());
                let before = pids_of(p.exe);
                let r = engine.tool_open(&t.id);
                match (&r, present) {
                    (Ok(()), true) => eprintln!("打开了 {:<28} {}", t.id, p.exe),
                    (Err(e), false) if e.to_string().contains("这台电脑上没有") => {
                        println!("::notice title={}::这台 CI 机器上没有 {}：{e}", t.id, p.exe);
                    }
                    _ => failures.push(format!("{}：{} 在不在：{present}，结果：{r:?}", t.id, p.exe)),
                }
                std::thread::sleep(Duration::from_secs(2));
                close_new(p.exe, &before);
            }
            (None, Some(page)) => {
                let before = pids_of("SystemSettings.exe");
                match engine.tool_open(&t.id) {
                    Ok(()) => eprintln!("打开了 {:<28} ms-settings:{page}", t.id),
                    Err(e) => println!("::warning title={}::ms-settings:{page} 在这台 CI 机器上打不开：{e}", t.id),
                }
                std::thread::sleep(Duration::from_secs(2));
                close_new("SystemSettings.exe", &before);
            }
            _ => failures.push(format!("{}：open 写得不对", t.id)),
        }
    }
    assert!(failures.is_empty(), "有打开类小工具不对：\n{}", failures.join("\n"));
}

/// 开机启动项：在 HKCU 的 Run 里放一个测试用的启动项（指向记事本），列出来、停用（写和任务管理器同一个开关，
/// 不删 Run 值）、再撤销，最后删掉。顺便把这台机器上的启动项都打印出来，看看列表和建议对不对。
#[test]
#[ignore]
fn startup_items_are_listed_disabled_and_restored() {
    use medkit_core::startup::Source;
    use medkit_core::views::StartupSignature;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
            for key in [RUN, APPROVED] {
                if let Ok(k) = hkcu.open_subkey_with_flags(key, KEY_SET_VALUE) {
                    let _ = k.delete_value(&self.0);
                }
            }
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, platform) = real_engine(dir.path());
    let name = format!("MedkitTest-{}", new_id());
    let _cleanup = Cleanup(name.clone());
    let (run, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN).unwrap();
    let windir = std::env::var("SystemRoot").unwrap();
    let command = format!("\"{windir}\\System32\\notepad.exe\" /medkit-test");
    run.set_value(&name, &command).unwrap();

    let items = engine.startup_list().unwrap();
    for i in &items {
        eprintln!(
            "{:<14} {:<44} {} {:?} {:?} {}",
            format!("{:?}", i.source),
            i.title,
            if i.enabled { "开" } else { "关" },
            i.advice,
            i.signature,
            i.publisher.as_deref().unwrap_or("-")
        );
    }
    let item = items.iter().find(|i| i.name == name).expect("测试用的启动项没有列出来");
    assert_eq!(item.source, Source::UserRun);
    assert!(item.exists && item.program.eq_ignore_ascii_case("notepad.exe"), "{item:?}");
    assert!(item.enabled);
    // 记事本是微软签名的。CI 从 PowerShell 7 里跑测试，模块路径没清理的话这里是 Unknown
    assert_eq!(item.signature, StartupSignature::Valid, "{item:?}");
    assert!(item.publisher.as_deref().is_some_and(|p| p.contains("Microsoft")), "{item:?}");

    let r = engine.startup_set(&item.id, false).unwrap();
    assert!(r.ok, "{r:?}");
    let root =
        platform.interactive_user().filter(|u| is_sid(&u.sid)).map_or(RegRoot::CurrentUser, |u| RegRoot::User(u.sid));
    match platform.reg_get(&root, APPROVED, &name).unwrap() {
        Some(RegValue::Binary(b)) => assert_eq!((b.len(), b[0]), (12, 3), "{b:?}"),
        other => panic!("开关没写对：{other:?}"),
    }
    assert_eq!(run.get_value::<String, _>(&name).unwrap(), command, "只停用，不能删 Run 值");
    assert!(!engine.startup_list().unwrap().iter().any(|i| i.name == name && i.enabled), "停用后列表里要显示成关");

    let u = engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(platform.reg_get(&root, APPROVED, &name).unwrap(), None, "撤销后开关要恢复成原来的「没有这个值」");
}
