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
