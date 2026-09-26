//! 黑名单要真的拦得住：计划书第五节「不做」的事，数据里换个写法也不能混进来。

use std::collections::BTreeSet;

use medkit_core::catalog::{self, CatalogData, Severity};
use medkit_core::model::Feature;

fn errors_for(key: &str, name: &str) -> Vec<String> {
    let yaml = format!(
        r#"
id: test.denied
schema_version: 1
title: {{ zh-CN: 测试 }}
description: {{ zh-CN: 测试 }}
category: test
risk: safe
level: light
recommend: optional
target: machine
actions:
  - registry: {{ key: '{key}', name: {name}, type: dword, value: 1 }}
windows_default:
  - registry: {{ key: '{key}', name: {name}, delete: true }}
undo: auto
references: [ "https://example.com" ]
"#
    );
    let f: Feature = catalog::parse_typed(&yaml).unwrap();
    let data = CatalogData { features: vec![f], ..Default::default() };
    catalog::validate(&data, &BTreeSet::new())
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect()
}

#[test]
fn enabling_smb1_is_refused() {
    let e = errors_for(r"HKLM\SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters", "SMB1");
    assert!(e.iter().any(|m| m.contains("SMB1")), "{e:?}");
}

#[test]
fn control_set_aliases_do_not_bypass_the_deny_list() {
    for key in [
        r"HKLM\SYSTEM\ControlSet001\Services\LanmanServer\Parameters",
        r"HKLM\SYSTEM\ControlSet002\Services\LanmanServer\Parameters",
    ] {
        let e = errors_for(key, "SMB1");
        assert!(e.iter().any(|m| m.contains("SMB1")), "{key}：{e:?}");
    }
    let e = errors_for(r"HKLM\SYSTEM\ControlSet001\Control\SafeBoot\Minimal", "X");
    assert!(e.iter().any(|m| m.contains("安全模式")), "{e:?}");
    let e = errors_for(r"HKLM\SYSTEM\ControlSet001\Services\WinDefend", "Start");
    assert!(e.iter().any(|m| m.contains("Defender")), "{e:?}");
}

#[test]
fn ordinary_keys_are_still_allowed() {
    let e = errors_for(r"HKLM\SOFTWARE\MedkitTest", "Flag");
    assert!(e.is_empty(), "{e:?}");
}

/// 审查时实际绕过去的几种写法：直接写服务的注册表、UAC 提示、防火墙策略、更新策略、
/// 暂停更新的到期时间、SmartScreen。
#[test]
fn registry_writes_that_disable_protections_are_refused() {
    for (key, name, why) in [
        (r"HKLM\SYSTEM\CurrentControlSet\Services\wuauserv", "Start", "更新"),
        (r"HKLM\SYSTEM\ControlSet001\Services\mpssvc", "Start", "防火墙"),
        (r"HKLM\SYSTEM\CurrentControlSet\Services\WinDefend", "Start", "Defender"),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "ConsentPromptBehaviorAdmin", "UAC"),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "PromptOnSecureDesktop", "UAC"),
        (
            r"HKLM\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\StandardProfile",
            "EnableFirewall",
            "防火墙",
        ),
        (r"HKLM\SOFTWARE\Policies\Microsoft\WindowsFirewall\DomainProfile", "EnableFirewall", "防火墙"),
        (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "DisableWindowsUpdateAccess", "更新"),
        (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU", "NoAutoUpdate", "更新"),
        (r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings", "PauseUpdatesExpiryTime", "暂停更新"),
        (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer", "SmartScreenEnabled", "SmartScreen"),
    ] {
        let e = errors_for(key, name);
        assert!(e.iter().any(|m| m.contains(why)), "{key}\\{name} 应该被拦住（{why}）：{e:?}");
    }
}

fn browser_policy_errors(delete: bool) -> Vec<String> {
    let action = if delete {
        r"{ key: 'HKLM\SOFTWARE\Policies\Microsoft\Edge', name: HomepageLocation, delete: true }"
    } else {
        r"{ key: 'HKLM\SOFTWARE\Policies\Microsoft\Edge', name: HomepageLocation, type: string, value: 'http://x' }"
    };
    let yaml = format!(
        r#"
id: test.browser
schema_version: 1
title: {{ zh-CN: 测试 }}
description: {{ zh-CN: 测试 }}
category: test
risk: safe
level: light
recommend: optional
target: machine
actions:
  - registry: {action}
undo: auto
references: [ "https://example.com" ]
"#
    );
    let f: Feature = catalog::parse_typed(&yaml).unwrap();
    let data = CatalogData { features: vec![f], ..Default::default() };
    catalog::validate(&data, &BTreeSet::new())
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect()
}

/// 浏览器策略：写进去就是替用户锁主页（第五节第 9 条），拦住；删掉广告软件写的策略是修复，放行。
#[test]
fn browser_policies_can_be_removed_but_not_written() {
    let e = browser_policy_errors(false);
    assert!(e.iter().any(|m| m.contains("浏览器")), "{e:?}");
    assert!(browser_policy_errors(true).is_empty());
}

/// 「只应用推荐项」会一次改好几项，推荐的只能是安全、能撤销的。
#[test]
fn recommended_features_must_be_safe_and_reversible() {
    let yaml = r#"
id: test.risky
schema_version: 1
title: { zh-CN: 测试 }
description: { zh-CN: 测试 }
category: test
risk: caution
level: light
recommend: recommended
target: machine
actions:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Flag, type: dword, value: 1 }
windows_default:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Flag, delete: true }
undo: auto
references: [ "https://example.com" ]
"#;
    let f: Feature = catalog::parse_typed(yaml).unwrap();
    let data = CatalogData { features: vec![f], ..Default::default() };
    let e: Vec<String> = catalog::validate(&data, &BTreeSet::new()).into_iter().map(|p| p.message).collect();
    assert!(e.iter().any(|m| m.contains("recommended")), "{e:?}");
}
