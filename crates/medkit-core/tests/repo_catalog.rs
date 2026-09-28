//! 仓库里的真实数据必须通过校验（和 `medkit-data check` 一样），并且能打包、解包。

use std::path::Path;

use medkit_core::bundle::Bundle;
use medkit_core::catalog::Severity;

#[test]
fn repository_catalog_is_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (bundle, problems) = Bundle::from_repo(&root);
    let errors: Vec<String> =
        problems.iter().filter(|p| p.severity == Severity::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "数据有错误：\n{}", errors.join("\n"));
    let bundle = bundle.expect("没有错误时一定能打包");

    let json = bundle.to_json();
    let back = Bundle::from_json(&json).unwrap();
    assert_eq!(back.hash, bundle.hash);

    let dir = tempfile::tempdir().unwrap();
    let manifest = back.extract(dir.path()).unwrap();
    for name in back.scripts.keys() {
        manifest.verify(name).unwrap();
    }
}

/// 「最近的更新」脚本认得的每个错误代码，YAML 里都要有「这个代码的意思是……」。
#[test]
fn every_update_error_code_has_a_meaning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (bundle, _) = Bundle::from_repo(&root);
    let bundle = bundle.expect("数据要能打包");
    let check = bundle.catalog.checks.iter().find(|c| c.id == "update.history").expect("有 update.history");
    let meanings = &check.fact_labels.get("error_meaning").expect("有 fact_labels.error_meaning").values;
    let script = &bundle.scripts["checks/update/history.ps1"].content;
    // 只看代码表（'0x…' = '结果代码'），不看「不算失败」的那几个
    let re = regex::Regex::new(r"'(0x[0-9A-F]{8})' = '").unwrap();
    let codes: Vec<&str> = re.captures_iter(script).map(|c| c.get(1).unwrap().as_str()).collect();
    assert!(codes.len() > 70, "脚本里的代码太少了：{}", codes.len());
    let missing: Vec<&&str> = codes.iter().filter(|c| !meanings.contains_key(**c)).collect();
    assert!(missing.is_empty(), "这些代码没有说明：{missing:?}");
}
