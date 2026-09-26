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
