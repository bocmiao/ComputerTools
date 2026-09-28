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
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use medkit_core::Engine;
use medkit_core::bundle::Bundle;
use medkit_core::catalog::{BREAK_IN_TESTS, Catalog};
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

/// 会禁用、重启 Windows 更新服务的测试，和检测、小工具的冒烟测试不能同时跑：
/// 服务被禁用的那一刻，更新相关的检测会查到一个测试造出来的故障。
static UPDATE_SERVICES: Mutex<()> = Mutex::new(());

fn update_lock() -> MutexGuard<'static, ()> {
    UPDATE_SERVICES.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 往返测试会改所有修复动到的地方，有的修复另有专门的测试改同样的地方（network.hosts-cleanup 和
/// hosts_cleanup_removes_only_flagged_lines 都改 hosts 文件）：错开，免得一个测试改的被另一个退回去
static ROUND_TRIP: Mutex<()> = Mutex::new(());

fn round_trip_lock() -> MutexGuard<'static, ()> {
    ROUND_TRIP.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn every_check_runs_cleanly_on_windows_powershell() {
    let _update = update_lock();
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
    let report = engine.report_generate(None).unwrap();
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
    let _round_trip = round_trip_lock();
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, platform) = real_engine(dir.path());
    let mut failures = Vec::new();
    for f in &bundle.catalog.features {
        if BREAK_IN_TESTS.contains(&f.id.as_str()) {
            eprintln!("跳过 {}：另有专门的测试", f.id);
            continue;
        }
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
    let _update = update_lock();
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
                // 命令行窗口里依次运行的程序（DISM 还会再起一个 DismHost）：打开以后一起关掉
                let mut children: Vec<&str> = p.console.iter().map(|c| c.split(' ').next().unwrap_or(c)).collect();
                let present = system32.join(p.exe).is_file()
                    && p.args.iter().filter(|a| a.ends_with(".msc")).all(|a| system32.join(a).is_file())
                    && children.iter().all(|c| system32.join(c).is_file());
                if !children.is_empty() {
                    children.push("DismHost.exe");
                }
                let before = pids_of(p.exe);
                let before_children: Vec<(&str, Vec<u32>)> = children.iter().map(|c| (*c, pids_of(c))).collect();
                let r = engine.tool_open(&t.id);
                match (&r, present) {
                    (Ok(()), true) => eprintln!("打开了 {:<28} {}", t.id, p.exe),
                    (Err(e), false) if e.to_string().contains("这台电脑上没有") => {
                        println!("::notice title={}::这台 CI 机器上没有 {}：{e}", t.id, p.exe);
                    }
                    _ => failures.push(format!("{}：{} 在不在：{present}，结果：{r:?}", t.id, p.exe)),
                }
                std::thread::sleep(Duration::from_secs(2));
                // 命令行窗口：窗口里的程序真的起来了（DISM 在跑，或者它很快出错退出、接着跑起了 sfc），才说明命令行拼对了
                if r.is_ok() && !p.console.is_empty() {
                    let started = (0..12).any(|_| {
                        let seen = before_children.iter().any(|(c, b)| pids_of(c).iter().any(|pid| !b.contains(pid)));
                        if !seen {
                            std::thread::sleep(Duration::from_millis(250));
                        }
                        seen
                    });
                    if started {
                        eprintln!("  命令行窗口里的程序跑起来了：{}", p.console.join(" & "));
                    } else {
                        failures.push(format!("{}：命令行窗口里的程序没有跑起来，命令行可能拼错了", t.id));
                    }
                }
                close_new(p.exe, &before);
                for (c, b) in &before_children {
                    close_new(c, b);
                }
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

/// 右键菜单：在当前用户下临时登记一个命令和一个外壳扩展（程序指向这个测试程序本身，不会被加载），
/// 列表脚本要在真的 Windows PowerShell 5.1 上认出它们（Windows 自带的一项都不能列）；拿掉、撤销都要写对位置。
/// 结束时（包括断言失败时）删掉登记的键和 Blocked 里的值。
#[test]
#[ignore = "会临时在右键菜单里登记测试项目（结束时删掉）"]
fn context_menu_entries_are_listed_hidden_and_restored() {
    use medkit_core::views::ContextMenuKind;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE};

    const BLOCKED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";
    let id = new_id();
    let verb = format!("MedkitTest-{id}");
    let clsid = format!("{{{}}}", new_id().to_uppercase());
    let verb_key = format!(r"Software\Classes\Directory\Background\shell\{verb}");
    let handler_key = format!(r"Software\Classes\Directory\shellex\ContextMenuHandlers\{verb}");
    let class_key = format!(r"Software\Classes\CLSID\{clsid}");
    struct Cleanup(Vec<String>, String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
            for key in &self.0 {
                let _ = hkcu.delete_subkey_all(key);
            }
            for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
                if let Ok(k) = winreg::RegKey::predef(root).open_subkey_with_flags(BLOCKED, KEY_SET_VALUE) {
                    let _ = k.delete_value(&self.1);
                }
            }
        }
    }
    let _cleanup = Cleanup(vec![verb_key.clone(), handler_key.clone(), class_key.clone()], clsid.clone());
    let exe = std::env::current_exe().unwrap().display().to_string();
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let (k, _) = hkcu.create_subkey(&verb_key).unwrap();
    k.set_value("MUIVerb", &"小药箱测试命令").unwrap();
    let (c, _) = hkcu.create_subkey(format!(r"{verb_key}\command")).unwrap();
    c.set_value("", &format!("\"{exe}\" \"%V\"")).unwrap();
    let (h, _) = hkcu.create_subkey(&handler_key).unwrap();
    h.set_value("", &clsid).unwrap();
    let (cls, _) = hkcu.create_subkey(&class_key).unwrap();
    cls.set_value("", &"Medkit test extension").unwrap();
    let (server, _) = hkcu.create_subkey(format!(r"{class_key}\InprocServer32")).unwrap();
    server.set_value("", &exe).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, platform) = real_engine(dir.path());
    let started = std::time::Instant::now();
    let items = engine.context_menu_list().unwrap();
    eprintln!("列出 {} 项，用时 {} ms", items.len(), started.elapsed().as_millis());
    for i in &items {
        eprintln!(
            "{:<10} {:<40} {} {:<24} {:?} {}",
            format!("{:?}", i.kind),
            i.title,
            if i.visible { "显示" } else { "不显示" },
            i.scopes.join("、"),
            i.signature,
            i.publisher.as_deref().unwrap_or("-")
        );
    }
    // Windows 自带的一项都不能列：程序不在 Windows 目录下，命令和扩展不是微软签名的，也不会只有个文件名
    let windir = std::env::var("SystemRoot").unwrap().to_lowercase();
    for i in &items {
        assert!(!i.path.to_lowercase().starts_with(&windir), "Windows 自带的不应该列出来：{i:?}");
        if i.kind != ContextMenuKind::App {
            assert!(!i.publisher.as_deref().is_some_and(|p| p.starts_with("Microsoft")), "微软的不应该列出来：{i:?}");
            assert!(i.path.is_empty() || i.path.contains('\\'), "只有文件名、找不到在哪的不应该列出来：{i:?}");
        }
    }
    let command = items.iter().find(|i| i.title == "小药箱测试命令").expect("测试用的命令没有列出来");
    assert_eq!(command.kind, ContextMenuKind::Command);
    assert_eq!(
        (command.location.as_str(), command.scopes.as_slice()),
        ("当前用户", ["文件夹空白处".to_owned()].as_slice())
    );
    assert!(command.visible && command.exists, "{command:?}");
    let extension =
        items.iter().find(|i| i.kind == ContextMenuKind::Extension && i.path == exe).expect("测试用的扩展没有列出来");
    assert_eq!(extension.title, "Medkit test extension");

    let root =
        platform.interactive_user().filter(|u| is_sid(&u.sid)).map_or(RegRoot::CurrentUser, |u| RegRoot::User(u.sid));
    let r = engine.context_menu_set(&command.id, false).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(
        platform.reg_get(&root, &verb_key, "ProgrammaticAccessOnly").unwrap(),
        Some(RegValue::String(String::new()))
    );
    let r2 = engine.context_menu_set(&extension.id, false).unwrap();
    assert!(r2.ok, "{r2:?}");
    assert_eq!(
        platform.reg_get(&RegRoot::LocalMachine, BLOCKED, &clsid).unwrap(),
        Some(RegValue::String(String::new()))
    );
    let again = engine.context_menu_list().unwrap();
    assert!(again.iter().any(|i| i.id == command.id && !i.visible), "拿掉以后要显示成不显示");
    assert!(again.iter().any(|i| i.id == extension.id && !i.visible));

    for id in r.entry_ids.iter().chain(&r2.entry_ids) {
        let u = engine.journal_undo(id, false).unwrap();
        assert!(u.ok, "{u:?}");
    }
    assert_eq!(platform.reg_get(&root, &verb_key, "ProgrammaticAccessOnly").unwrap(), None);
    assert_eq!(platform.reg_get(&RegRoot::LocalMachine, BLOCKED, &clsid).unwrap(), None);
}

/// 键盘的辅助功能：临时打开粘滞键（只改这次登录，不写注册表），内置检测要读得出来；关掉以后也要读得出来。
/// 结束时（包括断言失败时）恢复原来的设置。
#[test]
#[ignore = "会临时打开粘滞键（结束时恢复）"]
fn keyboard_aids_follow_the_live_state() {
    use windows_sys::Win32::UI::Accessibility::{SKF_STICKYKEYSON, STICKYKEYS};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETSTICKYKEYS, SPI_SETSTICKYKEYS, SystemParametersInfoW};

    fn call(action: u32, value: &mut STICKYKEYS) -> bool {
        let size = u32::try_from(std::mem::size_of::<STICKYKEYS>()).unwrap();
        // SAFETY: value 是完整的 STICKYKEYS，cbSize 已经填好；fWinIni 为 0，不写注册表
        unsafe { SystemParametersInfoW(action, size, std::ptr::from_mut(value).cast(), 0) != 0 }
    }
    struct Restore(STICKYKEYS);
    impl Drop for Restore {
        fn drop(&mut self) {
            let mut v = self.0;
            call(SPI_SETSTICKYKEYS, &mut v);
        }
    }

    let mut original = STICKYKEYS { cbSize: u32::try_from(std::mem::size_of::<STICKYKEYS>()).unwrap(), dwFlags: 0 };
    assert!(call(SPI_GETSTICKYKEYS, &mut original), "读不了粘滞键：{}", std::io::Error::last_os_error());
    let _restore = Restore(original);
    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, _platform) = real_engine(dir.path());

    for on in [true, false] {
        let mut v = original;
        v.dwFlags = if on { v.dwFlags | SKF_STICKYKEYSON } else { v.dwFlags & !SKF_STICKYKEYSON };
        if !call(SPI_SETSTICKYKEYS, &mut v) {
            println!("::notice title=keyboard-aids::这台 CI 机器上改不了粘滞键：{}", std::io::Error::last_os_error());
            return;
        }
        let r = engine.run_check("system.keyboard-aids").unwrap();
        assert!(r.error.is_none(), "{r:?}");
        assert_eq!(r.facts["sticky_keys"], on, "{r:?}");
        eprintln!("粘滞键 {}：{}", if on { "开" } else { "关" }, r.message);
    }
}

/// 恢复 Windows 更新服务。制造故障就是禁用 Windows 更新服务，这种脚本不放进安装包（catalog.rs 的
/// BREAK_IN_TESTS），所以在这里直接改服务：把 Windows Update 和 BITS 设成禁用，执行修复，核对它们改回了
/// 「手动」；再撤销，核对它们回到「禁用」。结束时（包括断言失败时）恢复原来的启动方式。
#[test]
#[ignore = "会临时禁用 Windows 更新服务（结束时恢复）；需要管理员权限"]
fn update_services_are_restored_and_undone() {
    const SERVICES: [&str; 2] = ["wuauserv", "BITS"];
    struct Restore(Vec<(&'static str, StartType)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let p = WindowsPlatform::new();
            for (name, start) in &self.0 {
                if let Err(e) = p.service_set(name, *start) {
                    eprintln!("恢复 {name} 的启动方式失败：{e}");
                }
            }
        }
    }

    let _update = update_lock();
    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, platform) = real_engine(dir.path());
    let original: Vec<_> =
        SERVICES.iter().map(|&n| (n, platform.service_get(n).unwrap().expect("有这个服务"))).collect();
    let _restore = Restore(original.clone());
    eprintln!("原来的启动方式：{original:?}");

    for name in SERVICES {
        platform.service_set(name, StartType::Disabled).unwrap();
    }
    let broken = engine.feature_detect("update.enable-services").unwrap();
    assert_eq!(broken.state, FeatureStateKind::NotApplied, "{broken:?}");
    // 看被禁用的服务列表：CI 机器上可能本来就有别的更新服务被禁用
    let disabled_count =
        |r: &medkit_core::views::CheckResult| r.facts["disabled_services"].as_array().map_or(0, Vec::len);
    let check = engine.run_check("update.blocked").unwrap();
    assert!(check.error.is_none(), "{check:?}");
    assert!(disabled_count(&check) >= SERVICES.len(), "{check:?}");
    eprintln!("禁用以后：{}", check.message);

    let r = engine.feature_apply("update.enable-services").unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    for name in SERVICES {
        assert_eq!(platform.service_get(name).unwrap(), Some(StartType::Manual), "{name} 应该改回「手动」");
    }
    let fixed = engine.run_check("update.blocked").unwrap();
    assert!(fixed.error.is_none(), "{fixed:?}");
    assert_eq!(disabled_count(&fixed), 0, "{fixed:?}");
    eprintln!("修复以后：{}", fixed.message);

    for id in r.entry_ids.iter().rev() {
        let u = engine.journal_undo(id, false).unwrap();
        assert!(u.ok, "{u:?}");
    }
    for name in SERVICES {
        assert_eq!(platform.service_get(name).unwrap(), Some(StartType::Disabled), "撤销以后 {name} 应该回到「禁用」");
    }
}

/// 文件删不掉：是谁占着。另开一个 powershell.exe 独占打开一个文件（不许别人读、写、删），重启管理器要查出
/// 正是这个进程、正是这个文件；查文件夹也要查到；进程结束以后再查，就没有它了。
#[test]
fn file_lockers_find_the_process_holding_a_file() {
    use medkit_core::lockers::{self, LockTarget};

    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let held = dir.path().join("占着的文件.txt");
    std::fs::write(&held, b"medkit").unwrap();
    std::fs::write(dir.path().join("没人用的.txt"), b"x").unwrap();
    // 路径放在环境变量里传，不拼进命令行
    let mut child = Kill(
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$f = [System.IO.File]::Open($env:MEDKIT_HELD, 'Open', 'Read', 'None'); Start-Sleep -Seconds 120",
            ])
            .env("MEDKIT_HELD", &held)
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id();
    let platform = WindowsPlatform::new();

    // 等它打开文件（PowerShell 启动要一两秒）
    let mut report = None;
    for _ in 0..60 {
        let r = lockers::find(&platform, &LockTarget::Files(vec![held.clone()])).unwrap();
        if r.users.iter().any(|u| u.pid == pid) {
            report = Some(r);
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let report = report.expect("15 秒内应该查到占着文件的 powershell.exe");
    eprintln!("查文件：{report:?}");
    let user = report.users.iter().find(|u| u.pid == pid).unwrap();
    assert_eq!(user.files, ["占着的文件.txt"]);
    assert!(user.program.as_deref().is_some_and(|p| p.eq_ignore_ascii_case("powershell.exe")), "{user:?}");
    assert!(!user.is_self && !user.other_session, "{user:?}");
    assert_eq!(report.checked, 1);
    assert!(report.missing.is_empty() && report.failed.is_empty());

    let folder = lockers::find(&platform, &LockTarget::Folder(dir.path().to_path_buf())).unwrap();
    eprintln!("查文件夹：{folder:?}");
    assert_eq!(folder.checked, 2);
    let user = folder.users.iter().find(|u| u.pid == pid).expect("查文件夹也应该查到");
    assert_eq!(user.files, ["占着的文件.txt"]);

    let _ = child.0.kill();
    let _ = child.0.wait();
    let after = lockers::find(&platform, &LockTarget::Files(vec![held])).unwrap();
    assert!(after.users.iter().all(|u| u.pid != pid), "进程结束以后不该再查到它：{after:?}");
}

/// 删掉 hosts 里有问题的记录：真的往 hosts 文件末尾加一行屏蔽常用网站的记录，检测要认出来；执行修复，
/// 核对只删了这一行、文件其余部分一个字节都没变；撤销以后这一行回来。结束时（包括断言失败时）把 hosts
/// 连同只读属性恢复原样。（往返测试只看检测结果，这里还逐字节核对文件。）
#[test]
#[ignore = "会临时改动 hosts 文件（结束时恢复）；需要管理员权限"]
fn hosts_cleanup_removes_only_flagged_lines() {
    let _round_trip = round_trip_lock();
    let hosts = PathBuf::from(std::env::var("SystemRoot").unwrap()).join(r"System32\drivers\etc\hosts");
    let original = std::fs::read(&hosts).unwrap_or_default();
    // 只读的 hosts 先去掉只读，结束时连同只读一起恢复
    let permissions = std::fs::metadata(&hosts).map(|m| m.permissions()).ok();
    struct Restore(PathBuf, Vec<u8>, Option<std::fs::Permissions>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, &self.1);
            if let Some(p) = &self.2 {
                let _ = std::fs::set_permissions(&self.0, p.clone());
            }
        }
    }
    let _restore = Restore(hosts.clone(), original.clone(), permissions.clone());
    if let Some(p) = permissions.filter(|p| p.readonly()) {
        let mut writable = p;
        // Windows 上这只是去掉「只读」属性（这个测试只在 Windows 上跑）
        #[allow(clippy::permissions_set_readonly_false)]
        writable.set_readonly(false);
        std::fs::set_permissions(&hosts, writable).unwrap();
    }
    eprintln!("CI 机器上的 hosts 文件（{} 字节）：\n{}", original.len(), String::from_utf8_lossy(&original));

    let line = b"0.0.0.0 medkit-separate-test.baidu.com\r\n";
    let mut broken = original.clone();
    if !broken.is_empty() && !broken.ends_with(b"\n") {
        broken.extend_from_slice(b"\r\n");
    }
    broken.extend_from_slice(line);
    std::fs::write(&hosts, &broken).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, _platform) = real_engine(dir.path());
    let before = engine.run_check("network.hosts").unwrap();
    eprintln!("加上以后：{}", before.message);
    assert_eq!(before.result_code.as_deref(), Some("blocks-common"), "{before:?}");
    let r = engine.feature_apply("network.hosts-cleanup").unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    let cleaned = std::fs::read(&hosts).unwrap();
    let mut expected = original.clone();
    if !expected.is_empty() && !expected.ends_with(b"\n") {
        expected.extend_from_slice(b"\r\n");
    }
    assert_eq!(cleaned, expected, "只删那一行，别的一个字节都不变");
    for id in r.entry_ids.iter().rev() {
        let u = engine.journal_undo(id, false).unwrap();
        assert!(u.ok, "{u:?}");
    }
    assert_eq!(std::fs::read(&hosts).unwrap(), broken, "撤销以后那一行回到原来的位置");
}

/// 定时关机：安排 10 小时以后关机，再安排一次报「已经安排了」，取消掉，再取消一次报「本来就没有」。
/// 定 10 小时：万一取消失败，CI 机器早就用完收回了，不会真的关机。结束时（包括断言失败时）再取消一次。
#[test]
#[ignore = "会安排一次关机再马上取消；需要关机权限"]
fn shutdown_can_be_scheduled_and_cancelled() {
    use medkit_core::platform::shutdown::{self, ShutdownRequest};
    struct Abort;
    impl Drop for Abort {
        fn drop(&mut self) {
            let _ = shutdown::abort();
        }
    }
    let _abort = Abort;
    let message = "电脑小药箱的测试，马上会取消";
    // CI 机器上本来安排了关机的话（一般没有），先取消
    eprintln!("开始前取消：{:?}", shutdown::abort());
    assert_eq!(shutdown::schedule(10 * 3600, false, message).unwrap(), ShutdownRequest::Scheduled);
    assert_eq!(shutdown::schedule(10 * 3600, true, message).unwrap(), ShutdownRequest::AlreadyScheduled);
    assert!(shutdown::abort().unwrap(), "安排了就能取消");
    assert!(!shutdown::abort().unwrap(), "取消以后就没有安排了");
    // 取消以后能重新安排（界面上「改成这个时间」就是先取消再安排）
    assert_eq!(shutdown::schedule(10 * 3600, true, message).unwrap(), ShutdownRequest::Scheduled);
    assert!(shutdown::abort().unwrap());
}

/// 硬盘测速：列出的盘里有系统盘（固定的、有剩余空间）；在临时文件夹里用不经过缓存的方式真测一次（小一点、快一点），
/// 速度都大于 0，测完临时文件不留。
#[test]
fn disk_speed_lists_drives_and_measures_without_leftovers() {
    use medkit_core::disk_speed::{self, Limits};
    use medkit_core::platform::windows::drives;
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let list = drives();
    eprintln!("{list:#?}");
    let c = list.iter().find(|d| system.starts_with(d.letter)).expect("系统盘在列表里");
    assert!(!c.removable && c.total > 0 && c.free > 0 && !c.file_system.is_empty(), "{c:?}");

    let dir = tempfile::tempdir().unwrap();
    let limits =
        Limits { write: Duration::from_secs(2), read: Duration::from_secs(2), random: Duration::from_millis(500) };
    let r = disk_speed::run(dir.path(), 64 * 1024 * 1024, limits).unwrap();
    eprintln!("{r:?}");
    assert!(r.seq_write > 0.0 && r.seq_read > 0.0 && r.random_iops > 0.0, "{r:?}");
    assert!(r.tested_bytes >= 8 * 1024 * 1024);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "关掉就删了");
}

/// 「U 盘里的文件不见了」要用的文件属性：设上隐藏、系统（病毒就是这么藏的），读回来一样；去掉以后文件夹只剩
/// 「文件夹」属性、只传 NORMAL 的文件读回来是 NORMAL；链接自己带「重解析点」属性（不跟着走）。在临时文件夹里做，不碰别的。
#[test]
fn file_attributes_can_be_hidden_and_shown_again() {
    use medkit_core::platform::windows::{file_attributes, set_file_attributes};
    const READONLY: u32 = 0x1;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const DIRECTORY: u32 = 0x10;
    const ARCHIVE: u32 = 0x20;
    const NORMAL: u32 = 0x80;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("作业");
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("第一章.docx");
    std::fs::write(&file, b"x").unwrap();
    let mask = READONLY | HIDDEN | SYSTEM | DIRECTORY | ARCHIVE | NORMAL;

    set_file_attributes(&folder, HIDDEN | SYSTEM).unwrap();
    set_file_attributes(&file, HIDDEN | SYSTEM | READONLY | ARCHIVE).unwrap();
    assert_eq!(file_attributes(&folder).unwrap() & mask, DIRECTORY | HIDDEN | SYSTEM);
    assert_eq!(file_attributes(&file).unwrap() & mask, HIDDEN | SYSTEM | READONLY | ARCHIVE);
    // 隐藏、系统的文件夹照样能列出里面的东西
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);

    set_file_attributes(&folder, NORMAL).unwrap();
    set_file_attributes(&file, ARCHIVE).unwrap();
    assert_eq!(file_attributes(&folder).unwrap() & mask, DIRECTORY, "文件夹去掉以后只剩「文件夹」");
    assert_eq!(file_attributes(&file).unwrap() & mask, ARCHIVE);
    set_file_attributes(&file, NORMAL).unwrap();
    assert_eq!(file_attributes(&file).unwrap() & mask, NORMAL);
    assert!(file_attributes(&dir.path().join("没有这个文件")).is_none());
    assert!(set_file_attributes(&dir.path().join("没有这个文件"), NORMAL).is_err());

    // 目录联接（不用管理员权限就能建）：自己的属性里有重解析点
    let link = dir.path().join("联接");
    let status = Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&link).arg(&folder).output().unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    assert_ne!(file_attributes(&link).unwrap() & 0x400, 0, "联接带「重解析点」属性");
}

/// 为「找回 Windows 照片查看器」「管理新建菜单」收集这台机器上的实际情况（照片查看器的 ProgID、系统自带的图片
/// 文件类型写了什么、PhotoViewer.dll 里的文字，每一个「新建」菜单项和资源管理器的缓存），打印出来。只读。
#[test]
#[ignore = "只读，打印这台机器上照片查看器和「新建」菜单的注册表内容"]
fn photo_viewer_and_new_menu_facts() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("facts.ps1");
    std::fs::write(&script, include_str!("facts/photo-viewer-and-new-menu.ps1")).unwrap();
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .output()
        .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "脚本出错：{:?}", out.status);
}

/// 「新建」菜单：在当前用户的注册表里造两个测试用的项（ProgID 下的写法，和直接写在扩展名下、数据是二进制的），
/// 用真的脚本列出来；关掉、恢复、在修改日志里撤销，逐个核对注册表里的值；再把 Windows 自带的「位图图像」（HKLM）
/// 关掉再恢复。结束时（包括断言失败时）删掉测试用的键。
#[test]
#[ignore = "会临时改动「新建」菜单（结束时恢复）；需要管理员权限"]
fn new_menu_entries_are_listed_hidden_and_restored() {
    let hkcu = RegRoot::CurrentUser;
    let first = r"Software\Classes\.medkittest";
    let second = r"Software\Classes\.medkittest2";
    let progid = r"Software\Classes\MedkitTest.Document";
    struct Cleanup(Vec<&'static str>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for key in &self.0 {
                let _ = Command::new("reg.exe").args(["delete", &format!(r"HKCU\{key}"), "/f"]).output();
            }
        }
    }
    let _cleanup = Cleanup(vec![first, second, progid]);
    let platform = WindowsPlatform::new();
    let s = |v: &str| RegValue::String(v.to_owned());
    platform.reg_set(&hkcu, first, "", &s("MedkitTest.Document")).unwrap();
    platform.reg_set(&hkcu, progid, "FriendlyTypeName", &s("小药箱测试文档")).unwrap();
    let first_new = format!(r"{first}\MedkitTest.Document\ShellNew");
    platform.reg_set(&hkcu, &first_new, "NullFile", &s("")).unwrap();
    let second_new = format!(r"{second}\ShellNew");
    let data = RegValue::Binary(vec![0x50, 0x4b, 0x05, 0x06, 0x00]);
    platform.reg_set(&hkcu, &second_new, "Data", &data).unwrap();
    platform.reg_set(&hkcu, &second_new, "MenuText", &s("小药箱测试二(&T)")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, platform) = real_engine(dir.path());
    let started = std::time::Instant::now();
    let items = engine.new_menu_list().unwrap();
    eprintln!("列「新建」菜单用了 {} 毫秒：", started.elapsed().as_millis());
    for i in &items {
        eprintln!(
            "  {:<16} {:<24} {} {} {}",
            i.ext,
            i.title,
            i.location,
            if i.visible { "显示" } else { "不显示" },
            i.windows_own
        );
    }
    let find = |items: &[medkit_core::views::NewMenuItem], ext: &str| {
        items.iter().find(|i| i.ext == ext).cloned().unwrap_or_else(|| panic!("没列出 {ext}"))
    };
    let test1 = find(&items, ".medkittest");
    assert_eq!((test1.title.as_str(), test1.location.as_str(), test1.visible), ("小药箱测试文档", "当前用户", true));
    assert_eq!(find(&items, ".medkittest2").title, "小药箱测试二");
    let txt = find(&items, ".txt");
    assert!(txt.windows_own && txt.visible && !txt.title.starts_with('@'), "{txt:?}");
    assert!(items.iter().all(|i| i.ext != ".lnk"), "快捷方式不列");

    let r = engine.new_menu_set(".medkittest", false).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(platform.reg_get(&hkcu, &first_new, "NullFile").unwrap(), None);
    assert_eq!(platform.reg_get(&hkcu, &first_new, "MedkitHidden.NullFile").unwrap(), Some(s("")));
    assert!(!find(&engine.new_menu_list().unwrap(), ".medkittest").visible);
    let r = engine.new_menu_set(".medkittest", true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(platform.reg_get(&hkcu, &first_new, "NullFile").unwrap(), Some(s("")));
    assert_eq!(platform.reg_get(&hkcu, &first_new, "MedkitHidden.NullFile").unwrap(), None);

    let r = engine.new_menu_set(".medkittest2", false).unwrap();
    assert_eq!(platform.reg_get(&hkcu, &second_new, "MedkitHidden.Data").unwrap(), Some(data.clone()));
    for id in r.entry_ids.iter().rev() {
        assert!(engine.journal_undo(id, false).unwrap().ok);
    }
    assert_eq!(platform.reg_get(&hkcu, &second_new, "Data").unwrap(), Some(data));
    assert_eq!(platform.reg_get(&hkcu, &second_new, "MedkitHidden.Data").unwrap(), None);

    // Windows 自带的、在 HKLM 里的
    let bmp = r"SOFTWARE\Classes\.bmp\ShellNew";
    let hklm = RegRoot::LocalMachine;
    if find(&items, ".bmp").visible {
        let before = platform.reg_get(&hklm, bmp, "NullFile").unwrap();
        let r = engine.new_menu_set(".bmp", false).unwrap();
        assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
        assert_eq!(platform.reg_get(&hklm, bmp, "NullFile").unwrap(), None);
        let r = engine.new_menu_set(".bmp", true).unwrap();
        assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
        assert_eq!(platform.reg_get(&hklm, bmp, "NullFile").unwrap(), before);
        assert_eq!(platform.reg_get(&hklm, bmp, "MedkitHidden.NullFile").unwrap(), None);
    }
}

/// 资源管理器里多出来的图标：照网盘的做法（微软《Integrate a Cloud Storage Provider》）登记三个测试用的图标，都指向
/// 一个临时文件夹——导航栏里一个登记在当前用户的注册表里（像 OneDrive）、一个的 CLSID 只在所有用户的注册表里（像
/// 图库），「此电脑」里一个（像 WPS 云文档）。用真的脚本列出来，隐藏、恢复、在修改日志里撤销，逐个核对注册表；
/// 再另起进程用 Windows 自己的外壳（Shell.Application）看「此电脑」里还有没有（NonEnum）、导航栏的开关外壳读出来
/// 是多少。结束时（包括断言失败时）删掉测试用的键。
#[test]
#[ignore = "会临时往资源管理器里加三个测试用的图标（结束时删掉）；需要管理员权限"]
fn shell_places_are_listed_hidden_and_restored() {
    use medkit_core::views::{ShellPlace, ShellPlaceItem};
    use serde_json::Value;

    const PINNED: &str = "System.IsPinnedToNameSpaceTree";
    const EXPLORER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer";
    let clsid = || format!("{{{}}}", new_id().to_uppercase());
    let (cloud, machine, pc) = (clsid(), clsid(), clsid());
    let non_enum = r"Software\Microsoft\Windows\CurrentVersion\Policies\NonEnum";
    struct Cleanup(Vec<String>, String, String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for key in &self.0 {
                let _ = Command::new("reg.exe").args(["delete", key, "/f"]).output();
            }
            let _ = Command::new("reg.exe").args(["delete", &self.1, "/v", &self.2, "/f"]).output();
        }
    }
    let _cleanup = Cleanup(
        vec![
            format!(r"HKCU\Software\Classes\CLSID\{cloud}"),
            format!(r"HKCU\Software\Classes\WOW6432Node\CLSID\{cloud}"),
            format!(r"HKCU\Software\Classes\CLSID\{machine}"),
            format!(r"HKLM\SOFTWARE\Classes\CLSID\{machine}"),
            format!(r"HKCU\Software\Classes\CLSID\{pc}"),
            format!(r"HKCU\{EXPLORER}\Desktop\NameSpace\{cloud}"),
            format!(r"HKCU\{EXPLORER}\Desktop\NameSpace\{machine}"),
            format!(r"HKCU\{EXPLORER}\MyComputer\NameSpace\{pc}"),
        ],
        format!(r"HKCU\{non_enum}"),
        pc.clone(),
    );

    let platform = WindowsPlatform::new();
    let (hkcu, hklm) = (RegRoot::CurrentUser, RegRoot::LocalMachine);
    let s = |v: &str| RegValue::String(v.to_owned());
    let x = |v: &str| RegValue::ExpandString(v.to_owned());
    let target = tempfile::tempdir().unwrap();
    // 网盘的登记方式：shell32 里的文件系统文件夹，指向一个文件夹
    let register = |root: &RegRoot, classes: &str, id: &str, title: &str, pinned: Option<u32>| {
        let class = format!(r"{classes}\CLSID\{id}");
        platform.reg_set(root, &class, "", &s(title)).unwrap();
        if let Some(p) = pinned {
            platform.reg_set(root, &class, PINNED, &RegValue::Dword(p)).unwrap();
        }
        platform.reg_set(root, &class, "SortOrderIndex", &RegValue::Dword(0x42)).unwrap();
        platform
            .reg_set(root, &format!(r"{class}\InProcServer32"), "", &x(r"%SystemRoot%\system32\shell32.dll"))
            .unwrap();
        platform
            .reg_set(root, &format!(r"{class}\Instance"), "CLSID", &s("{0E5AAE11-A475-4c5b-AB00-C66DE400274E}"))
            .unwrap();
        let bag = format!(r"{class}\Instance\InitPropertyBag");
        platform.reg_set(root, &bag, "Attributes", &RegValue::Dword(0x11)).unwrap();
        platform.reg_set(root, &bag, "TargetFolderPath", &x(&target.path().display().to_string())).unwrap();
        let folder = format!(r"{class}\ShellFolder");
        platform.reg_set(root, &folder, "FolderValueFlags", &RegValue::Dword(0x28)).unwrap();
        platform.reg_set(root, &folder, "Attributes", &RegValue::Dword(0xF080_004D)).unwrap();
    };
    register(&hkcu, r"Software\Classes", &cloud, "MedkitTest Cloud", Some(1));
    platform
        .reg_set(&hkcu, &format!(r"Software\Classes\WOW6432Node\CLSID\{cloud}"), PINNED, &RegValue::Dword(1))
        .unwrap();
    platform.reg_set(&hkcu, &format!(r"{EXPLORER}\Desktop\NameSpace\{cloud}"), "", &s("MedkitTest Cloud")).unwrap();
    register(&hklm, r"SOFTWARE\Classes", &machine, "MedkitTest Machine", Some(1));
    platform.reg_set(&hkcu, &format!(r"{EXPLORER}\Desktop\NameSpace\{machine}"), "", &s("MedkitTest Machine")).unwrap();
    register(&hkcu, r"Software\Classes", &pc, "MedkitTest PC", None);
    platform.reg_set(&hkcu, &format!(r"{EXPLORER}\MyComputer\NameSpace\{pc}"), "", &s("MedkitTest PC")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let probe_script = dir.path().join("probe.ps1");
    std::fs::write(&probe_script, include_str!("facts/shell-places-probe.ps1")).unwrap();
    let nav_ids = format!("{cloud},{machine}");
    let probe = || -> Value {
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&probe_script)
            .args(["-Nav", &nav_ids])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        assert!(out.status.success(), "外壳探测脚本出错：{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("探测结果不是 JSON（{e}）：{text}"))
    };
    let in_pc = |v: &Value, key: &str| v[key].as_array().is_some_and(|a| a.iter().any(|n| n == "MedkitTest PC"));
    let on = |v: &Value| match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(t) => t == "1" || t.eq_ignore_ascii_case("true"),
        _ => false,
    };
    let report = |when: &str, v: &Value| {
        eprintln!(
            "{when}：「此电脑」里{}测试图标（算上隐藏的{}），外壳读到的导航栏开关 {} / {}，HKCR 里的名字 {} / {}",
            if in_pc(v, "pc") { "有" } else { "没有" },
            if in_pc(v, "pc_all") { "有" } else { "也没有" },
            v["pinned"][cloud.as_str()],
            v["pinned"][machine.as_str()],
            v["merged"][cloud.as_str()],
            v["merged"][machine.as_str()],
        );
    };

    let (engine, _bundle, platform) = real_engine(dir.path());
    let root =
        platform.interactive_user().filter(|u| is_sid(&u.sid)).map_or(RegRoot::CurrentUser, |u| RegRoot::User(u.sid));
    let started = std::time::Instant::now();
    let items = engine.shell_places_list().unwrap();
    eprintln!("列资源管理器里的图标用了 {} 毫秒：", started.elapsed().as_millis());
    for i in &items {
        eprintln!(
            "  {:<12} {:<28} {} {}",
            format!("{:?}", i.places),
            i.title,
            if i.visible { "显示" } else { "不显示" },
            if i.windows_own { "Windows 自带" } else { "" }
        );
    }
    let find = |items: &[ShellPlaceItem], title: &str| {
        items.iter().find(|i| i.title == title).cloned().unwrap_or_else(|| panic!("没列出 {title}：{items:#?}"))
    };
    let ids: Vec<String> = ["MedkitTest Cloud", "MedkitTest Machine", "MedkitTest PC"]
        .into_iter()
        .map(|title| {
            let i = find(&items, title);
            assert!(i.visible && !i.windows_own && i.note.is_empty(), "{i:?}");
            i.id
        })
        .collect();
    assert_eq!(find(&items, "MedkitTest PC").places, [ShellPlace::Pc]);
    assert_eq!(find(&items, "MedkitTest Machine").places, [ShellPlace::Nav]);
    assert_eq!(ids[2], pc);
    // Windows 自己的基本位置一个都不能列：此电脑、网络、回收站、库、主文件夹
    for own in [
        "{20D04FE0-3AEA-1069-A2D8-08002B30309D}",
        "{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}",
        "{645FF040-5081-101B-9F08-00AA002F954E}",
        "{031E4825-7B94-4DC3-B131-E946B44C8DD5}",
        "{F874310E-B6B7-47DC-BC84-B9E6B38F5903}",
    ] {
        assert!(items.iter().all(|i| !i.id.ends_with(own)), "Windows 自己的不应该列出来：{own}");
    }

    let before = probe();
    report("隐藏前", &before);
    for id in &ids {
        let r = engine.shell_places_set(id, false).unwrap();
        assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    }
    let class = |id: &str| format!(r"Software\Classes\CLSID\{id}");
    let wow = format!(r"Software\Classes\WOW6432Node\CLSID\{cloud}");
    let machine_class = format!(r"SOFTWARE\Classes\CLSID\{machine}");
    assert_eq!(platform.reg_get(&root, &class(&cloud), PINNED).unwrap(), Some(RegValue::Dword(0)));
    assert_eq!(platform.reg_get(&root, &wow, PINNED).unwrap(), Some(RegValue::Dword(0)), "32 位程序看的那一份也改");
    assert_eq!(platform.reg_get(&root, &class(&machine), PINNED).unwrap(), Some(RegValue::Dword(0)));
    assert_eq!(
        platform.reg_get(&hklm, &machine_class, PINNED).unwrap(),
        Some(RegValue::Dword(1)),
        "所有用户的那份不动，只在当前用户这里隐藏"
    );
    assert_eq!(platform.reg_get(&root, non_enum, &pc).unwrap(), Some(RegValue::Dword(1)));
    assert_eq!(platform.reg_get(&root, non_enum, &cloud).unwrap(), None, "只在导航栏里的用导航栏的开关");
    let hidden = probe();
    report("隐藏后", &hidden);
    if in_pc(&before, "pc") {
        assert!(!in_pc(&hidden, "pc"), "外壳列「此电脑」时还有测试图标：NonEnum 没起作用");
    } else {
        eprintln!("注意：隐藏前外壳列「此电脑」时就没有测试图标，核对不了「此电脑」里的显示");
    }
    for id in [&cloud, &machine] {
        if on(&before["pinned"][id.as_str()]) {
            assert!(!on(&hidden["pinned"][id.as_str()]), "外壳读到的导航栏开关还开着：{id}");
        }
    }
    let items = engine.shell_places_list().unwrap();
    for title in ["MedkitTest Cloud", "MedkitTest Machine", "MedkitTest PC"] {
        assert!(!find(&items, title).visible, "{title} 隐藏以后要显示成不显示");
    }

    for id in &ids {
        let r = engine.shell_places_set(id, true).unwrap();
        assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    }
    assert_eq!(platform.reg_get(&root, &class(&cloud), PINNED).unwrap(), Some(RegValue::Dword(1)));
    assert_eq!(platform.reg_get(&root, &wow, PINNED).unwrap(), Some(RegValue::Dword(1)));
    assert!(!platform.reg_key_exists(&root, &class(&machine)).unwrap(), "隐藏时新建的键删掉了，回到登记时的样子");
    assert_eq!(platform.reg_get(&root, non_enum, &pc).unwrap(), None);
    let shown = probe();
    report("恢复后", &shown);
    if in_pc(&before, "pc") {
        assert!(in_pc(&shown, "pc"), "恢复以后外壳列「此电脑」时没有测试图标");
    }
    for id in [&cloud, &machine] {
        if on(&before["pinned"][id.as_str()]) {
            assert!(on(&shown["pinned"][id.as_str()]), "恢复以后外壳读到的导航栏开关还是关着的：{id}");
        }
    }

    // 在修改日志里撤销
    engine.shell_places_list().unwrap();
    let r = engine.shell_places_set(&ids[1], false).unwrap();
    for id in r.entry_ids.iter().rev() {
        let u = engine.journal_undo(id, false).unwrap();
        assert!(u.ok, "{u:?}");
    }
    assert!(!platform.reg_key_exists(&root, &class(&machine)).unwrap(), "撤销时新建的键跟着删掉");
    assert_eq!(platform.reg_get(&hklm, &machine_class, PINNED).unwrap(), Some(RegValue::Dword(1)));
}

/// 清空打印队列：在打印文件夹里放一个测试用的任务（一对 .SHD、.SPL，打印服务运行时不会去读新放进来的），用真的
/// 脚本清空，核对这两个文件没了、别的文件还在、打印服务又在运行；再清一次是「本来就没有」。和小工具的冒烟测试错开
/// （它也会跑这个小工具）。结束时（包括断言失败时）删掉测试文件、把打印服务启动起来。
#[test]
#[ignore = "会停一下打印服务、在打印文件夹里放测试文件（结束时删掉）；需要管理员权限"]
fn print_queue_is_cleared_and_the_spooler_comes_back() {
    let _update = update_lock();
    let folder = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join(r"System32\spool\PRINTERS");
    let id = new_id();
    let job = [folder.join(format!("MEDKIT{id}.SHD")), folder.join(format!("MEDKIT{id}.SPL"))];
    let keep = folder.join(format!("medkit-{id}.txt"));
    struct Cleanup(Vec<PathBuf>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for f in &self.0 {
                let _ = std::fs::remove_file(f);
            }
            let _ = Command::new("sc.exe").args(["start", "Spooler"]).output();
        }
    }
    let _cleanup = Cleanup(job.iter().cloned().chain([keep.clone()]).collect());
    let spooler = || {
        let out = Command::new("sc.exe").args(["query", "Spooler"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains("RUNNING")
    };
    if !spooler() || !folder.is_dir() {
        println!("::notice title=print queue::这台 CI 机器上打印服务没在运行，或者没有打印文件夹，跳过");
        return;
    }
    let others_before = std::fs::read_dir(&folder).map(|d| d.count()).unwrap_or(0);
    eprintln!("打印文件夹里原来有 {others_before} 个文件");
    for f in &job {
        std::fs::write(f, b"medkit test job").unwrap();
    }
    std::fs::write(&keep, b"not a print job").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, _platform) = real_engine(dir.path());
    let r = engine.tool_run("printer.clear-queue").unwrap();
    eprintln!("{:?} {}", r.status, r.message);
    assert!(r.error.is_none(), "{r:?}");
    assert!(job.iter().all(|f| !f.exists()), "测试用的任务没删掉：{r:?}");
    assert!(keep.exists(), "只删 .SHD、.SPL");
    assert!(r.message.contains("个卡住的打印任务"), "{r:?}");
    assert!(spooler(), "打印服务没有重新启动起来");
    std::fs::remove_file(&keep).unwrap();
    let again = engine.tool_run("printer.clear-queue").unwrap();
    // 打印文件夹里原来就有任务（CI 机器上一般没有）时，第二次是「清掉了」
    if others_before == 0 {
        assert!(again.message.contains("本来就没有"), "{again:?}");
    }
}

/// 微软 VC++ 运行库：小工具真的从微软官网下载（核对签名）、安装，或者确认已经装着最新版；之后检测要说「装好了」，
/// 下载用的文件夹要删掉。CI 机器上本来就装着：镜像里的不比微软官网的旧时是「已经装着最新的」，旧的话是「装好了」。
/// 和小工具的冒烟测试错开（它也会跑这个小工具，同时装的话第二个会说「有别的安装程序正在运行」）。
#[test]
#[ignore = "会从微软官网下载 VC++ 运行库并安装；需要管理员权限"]
fn vc_runtime_is_installed_from_microsoft_and_then_checks_ok() {
    use medkit_core::model::Status;

    let _update = update_lock();
    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, _platform) = real_engine(dir.path());
    let before = engine.run_check("system.vc-runtime").unwrap();
    eprintln!("装之前：{:?} {}", before.status, before.message);
    let r = engine.tool_run("system.install-vc-runtime").unwrap();
    eprintln!("{:?} {:>6} ms  {}", r.status, r.duration_ms, r.message);
    assert!(r.error.is_none(), "{r:?}");
    assert!(["装好了", "已经装着最新的"].iter().any(|m| r.message.contains(m)), "{r:?}");
    let after = engine.run_check("system.vc-runtime").unwrap();
    eprintln!("装之后：{:?} {}", after.status, after.message);
    assert_eq!(after.status, Status::Ok, "{after:?}");
    let temp = PathBuf::from(std::env::var("SystemRoot").unwrap()).join("Temp");
    let left: Vec<String> = std::fs::read_dir(&temp)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("medkit-vc-"))
                .collect()
        })
        .unwrap_or_default();
    assert!(left.is_empty(), "下载用的文件夹没删掉：{left:?}");
}

/// U 盘：在这台机器上一个个真设上「自动装载关掉」「U 盘写保护」「U 盘驱动禁用」（用各自修复的 break_actions），
/// 检测要查出来，点修复以后要查不出来；结束时（包括断言失败时）三处都恢复原样。CI 机器上没插 U 盘：别的都正常时是
/// 「没看到插着的 U 盘」。和往返测试错开（它也会改这几处）。
#[test]
#[ignore = "会临时改 U 盘相关的设置（结束时恢复）；需要管理员权限"]
fn usb_settings_are_detected_and_fixed() {
    struct Restore(Arc<WindowsPlatform>, Vec<Saved>);
    impl Drop for Restore {
        fn drop(&mut self) {
            restore(&self.0, &self.1);
        }
    }

    let _round_trip = round_trip_lock();
    let dir = tempfile::tempdir().unwrap();
    let (engine, bundle, platform) = real_engine(dir.path());
    let features = ["hardware.usb-automount-on", "hardware.usb-write-protect-off", "hardware.enable-usb-storage"];
    let saved: Vec<Saved> = features
        .iter()
        .flat_map(|id| snapshot(&platform, bundle.catalog.features.iter().find(|f| f.id == *id).unwrap()))
        .collect();
    let _restore = Restore(Arc::clone(&platform), saved);

    let code = |check: &str| {
        let r = engine.run_check(check).unwrap();
        eprintln!("{check:<24} {:?} {:?} {}", r.status, r.result_code, r.message);
        r.result_code.unwrap_or_default()
    };
    let before = code("hardware.usb-storage");
    assert!(["none", "ok", "settings-ok"].contains(&before.as_str()), "CI 机器上不该有这些设置：{before}");
    for (feature, check, broken) in [
        ("hardware.usb-automount-on", "hardware.usb-storage", "no-automount"),
        ("hardware.usb-write-protect-off", "hardware.usb-storage", "write-protect"),
        ("hardware.enable-usb-storage", "hardware.usb-driver", "disabled"),
    ] {
        if code(check) == "missing" {
            println!("::notice title=usb::这台 CI 机器上没有 U 盘驱动（USBSTOR），跳过 {feature}");
            continue;
        }
        engine.break_feature(feature).unwrap();
        assert_eq!(code(check), broken, "{feature}：设上以后检测没查出来");
        let r = engine.feature_apply(feature).unwrap();
        assert!(r.ok, "{feature}：{r:?}");
        assert_ne!(code(check), broken, "{feature}：修复以后检测还是这样");
    }
}

/// 在这台机器上运行一段 PowerShell，返回标准输出（去掉首尾空白）。
fn powershell(script: &str) -> String {
    let out =
        Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// 打印机脱机：在一台虚拟打印机（Microsoft Print to PDF 这类，CI 机器上没有真的打印机）上勾上「脱机使用打印机」、
/// 再暂停它，检测要查出来；小工具让它恢复工作以后，检测要查不出来。结束时（包括断言失败时）改回原样。
/// 一台打印机都没有时跳过。和小工具的冒烟测试错开（它也会跑这个小工具）。
#[test]
#[ignore = "会临时改一台虚拟打印机的「脱机使用」「暂停」（结束时恢复）；需要管理员权限"]
fn offline_and_paused_printers_are_put_back_to_work() {
    const WMI: &str = "$l = New-Object -ComObject WbemScripting.SWbemLocator; $w = $l.ConnectServer('.', 'root\\cimv2'); \
                       $null = $w.Security_.Privileges.AddAsString('SeLoadDriverPrivilege', $true);";
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            powershell(&format!(
                "{WMI} foreach ($p in @($w.ExecQuery(\"SELECT * FROM Win32_Printer WHERE Name = '{}'\"))) {{ \
                 $p.Properties_.Item('WorkOffline').Value = $false; $null = $p.Put_(); $null = $p.ExecMethod_('Resume') }}",
                self.0
            ));
        }
    }

    let _update = update_lock();
    let name = powershell(
        "@(Get-CimInstance Win32_Printer | Where-Object { $_.PortName -eq 'PORTPROMPT:' -and -not $_.WorkOffline -and $_.ExtendedPrinterStatus -ne 8 })[0].Name",
    );
    if name.is_empty() || name.contains(['\'', '\\', '"']) {
        println!("::notice title=printer offline::这台 CI 机器上没有能用来测试的虚拟打印机，跳过");
        return;
    }
    eprintln!("用来测试的打印机：{name}");
    let _restore = Restore(name.clone());
    let set = powershell(&format!(
        "{WMI} $p = @($w.ExecQuery(\"SELECT * FROM Win32_Printer WHERE Name = '{name}'\"))[0]; \
         $p.Properties_.Item('WorkOffline').Value = $true; $null = $p.Put_(); \
         $r = $p.ExecMethod_('Pause'); $r.Properties_.Item('ReturnValue').Value"
    ));
    eprintln!("暂停的返回值：{set}");

    let dir = tempfile::tempdir().unwrap();
    let (engine, _bundle, _platform) = real_engine(dir.path());
    let code = || {
        let r = engine.run_check("printer.offline").unwrap();
        eprintln!("{:?} {:?} {}", r.status, r.result_code, r.message);
        r.result_code.unwrap_or_default()
    };
    assert_eq!(code(), "work-offline", "勾上「脱机使用打印机」以后检测没查出来");
    let r = engine.tool_run("printer.resume").unwrap();
    eprintln!("{:?} {}", r.status, r.message);
    assert!(r.error.is_none(), "{r:?}");
    assert_eq!(r.result_code.as_deref(), Some("done"), "{r:?}");
    let after = code();
    assert!(!["work-offline", "paused"].contains(&after.as_str()), "恢复以后还是：{after}");
}

#[test]
fn window_owner_names_the_program_under_the_mouse() {
    use medkit_core::views::WindowOwnerKind;
    use medkit_core::window_owner::{self, is_generic, owning_program, program_folders};
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetCursorPos, GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SetCursorPos, SetWindowPos,
    };

    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let p = WindowsPlatform::new();

    // 版本信息：资源管理器是微软的
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let strings = p.file_strings(&Path::new(&windir).join("explorer.exe"));
    eprintln!("explorer.exe：{strings:?}");
    assert!(strings.company.as_deref().is_some_and(|c| c.contains("Microsoft")), "{strings:?}");
    assert!(strings.description.is_some(), "{strings:?}");
    assert_eq!(p.file_strings(Path::new(r"C:\medkit-no-such-file.exe")), Default::default());

    // 「应用和功能」里的程序：装在某个文件夹里的程序能对上它自己
    let programs = p.installed_programs().unwrap();
    eprintln!("「应用和功能」里有 {} 个程序", programs.len());
    assert!(!programs.is_empty());
    let mut matched = 0;
    for program in &programs {
        let Some(folder) = program_folders(program).into_iter().find(|f| !is_generic(f)) else { continue };
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        let Some(exe) =
            entries.flatten().map(|e| e.path()).find(|x| x.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        else {
            continue;
        };
        let owner = owning_program(&exe.to_string_lossy(), &programs);
        eprintln!("{} → {:?}", exe.display(), owner.map(|o| &o.name));
        assert!(owner.is_some(), "{} 在 {} 的文件夹里，却没对上", exe.display(), program.name);
        matched += 1;
        if matched >= 5 {
            break;
        }
    }
    if matched == 0 {
        println!("::notice title=window owner::这台 CI 机器上没有能用来核对的程序文件夹");
    }

    // 鼠标指着的窗口：随便指着什么都不能出错
    let (report, _) = window_owner::find(&p).unwrap();
    eprintln!("现在鼠标指着：{report:?}");

    // 打开一个记事本，把鼠标移到它上面，应该认出是 Windows 自带的记事本
    let child = Kill(Command::new(Path::new(&windir).join(r"System32\notepad.exe")).spawn().unwrap());
    let class: Vec<u16> = "Notepad".encode_utf16().chain(Some(0)).collect();
    let mut window = std::ptr::null_mut();
    for _ in 0..100 {
        // SAFETY: class 以 NUL 结尾，窗口标题不限
        let found = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        let mut pid = 0u32;
        // SAFETY: pid 是有效的输出位置
        if !found.is_null() && unsafe { GetWindowThreadProcessId(found, &mut pid) } != 0 && pid == child.0.id() {
            window = found;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if window.is_null() {
        println!("::notice title=window owner::这台 CI 机器上没等到记事本的窗口，跳过鼠标那一段");
        return;
    }
    // 别的测试同时会打开任务管理器这些窗口：把记事本放到最上面，免得被盖住
    // SAFETY: window 是记事本的窗口；只改前后顺序，不移动、不改大小
    unsafe { SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW) };
    let mut rect = RECT::default();
    // SAFETY: rect 是有效的输出位置
    assert_ne!(unsafe { GetWindowRect(window, &mut rect) }, 0);
    let (x, y) = ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
    let mut at = POINT::default();
    // SAFETY: 只移动鼠标指针；at 是有效的输出位置
    let moved = unsafe { SetCursorPos(x, y) } != 0 && unsafe { GetCursorPos(&mut at) } != 0 && (at.x, at.y) == (x, y);
    if !moved {
        println!("::notice title=window owner::这台 CI 机器上移不动鼠标指针，跳过鼠标那一段");
        return;
    }
    let mut last = None;
    for _ in 0..50 {
        let (r, path) = window_owner::find(&p).unwrap();
        if r.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case("notepad.exe")) {
            eprintln!("记事本：{r:?}");
            assert_eq!(r.kind, WindowOwnerKind::System, "{r:?}");
            assert!(r.description.is_some(), "{r:?}");
            assert!(r.width > 0 && r.height > 0 && r.position.is_some(), "{r:?}");
            assert!(path.is_some_and(|x| x.is_file()));
            return;
        }
        last = Some(r);
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("鼠标在记事本的窗口上，认出来的却是：{last:?}");
}
