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

/// `test:<名字>` 链接能用的设备测试，界面里都要有：按钮文字（labels.ts 的 DEVICE_TEST_LABELS）
/// 和跳过去的那一项（DeviceTests.vue 里 id 为 device-test-<名字> 的元素）。
#[test]
fn every_device_test_has_a_ui_card() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tests_vue = std::fs::read_to_string(root.join("app/src/components/DeviceTests.vue")).unwrap();
    let labels_ts = std::fs::read_to_string(root.join("app/src/labels.ts")).unwrap();
    let labels = labels_ts.split("DEVICE_TEST_LABELS").nth(1).expect("labels.ts 里有 DEVICE_TEST_LABELS");
    let labels = &labels[..labels.find('}').expect("DEVICE_TEST_LABELS 有结尾")];
    for name in medkit_core::tools::DEVICE_TESTS {
        assert!(tests_vue.contains(&format!("id=\"device-test-{name}\"")), "DeviceTests.vue 里没有 device-test-{name}");
        assert!(labels.contains(&format!("\n  {name}: '")), "DEVICE_TEST_LABELS 里没有 {name}");
    }
    assert_eq!(
        labels.matches(": '").count(),
        medkit_core::tools::DEVICE_TESTS.len(),
        "DEVICE_TEST_LABELS 和 DEVICE_TESTS 的名字要一样多"
    );
}

/// 和界面上的写法一样（app/src/utils/symptomMatch.ts 的 normalizeForSearch，少了全角转半角）：转小写，去掉空格和标点。
fn normalize_for_search(s: &str) -> String {
    const DROP: &str = "-_·•,，.。、!！?？:：;；'\"“”‘’「」『』()（）【】[]";
    s.to_lowercase().chars().filter(|c| !c.is_whitespace() && !DROP.contains(*c)).collect()
}

/// 「按症状修」粘贴报错截图时，认出来的字靠症状的名字、关键词对上症状（app/src/utils/symptomMatch.ts：一个关键词被同一个
/// 症状另一个对上的关键词包含时只算长的，对上的字加起来至少 4 个，按字数排）。常见报错的原话要对上该对的症状、而且排第一。
#[test]
fn common_error_messages_find_their_symptom() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (bundle, _) = Bundle::from_repo(&root);
    let bundle = bundle.expect("数据要能打包");
    let best = |text: &str| -> Option<(String, usize)> {
        let haystack = normalize_for_search(text);
        let mut scored: Vec<(String, usize)> = Vec::new();
        for s in &bundle.catalog.symptoms {
            let mut keys: Vec<String> = std::iter::once(s.title.get("zh-CN").to_owned())
                .chain(s.keywords.iter().cloned())
                .map(|k| normalize_for_search(&k))
                .filter(|k| k.chars().count() >= 2 && haystack.contains(k.as_str()))
                .collect();
            keys.sort();
            keys.dedup();
            let score: usize = keys
                .iter()
                .filter(|k| !keys.iter().any(|o| o != *k && o.contains(k.as_str())))
                .map(|k| k.chars().count())
                .sum();
            if score >= 4 {
                scored.push((s.id.clone(), score));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().next()
    };
    let cases = [
        ("由于找不到 MSVCP140.dll，无法继续执行代码。重新安装程序可能会解决此问题。", "app-missing-dll"),
        ("应用程序无法正常启动(0xc000007b)。请单击“确定”关闭应用程序。", "app-missing-dll"),
        ("Windows 无法访问 \\\\192.168.1.5 错误代码: 0x80070035 找不到网络路径。", "lan-share"),
        ("连接到打印机 操作无法完成(错误 0x0000011b)。", "printer-11b"),
        ("安装更新时出现一些问题，但我们稍后会重试。错误 0x800f0922", "update-failed"),
        ("无法访问此网站 连接已重置。 ERR_CONNECTION_RESET", "network"),
        ("你的设备遇到问题，需要重启。终止代码: CRITICAL_PROCESS_DIED", "bluescreen"),
        ("任务管理器已被系统管理员停用。", "tools-disabled"),
        ("该文件没有与之关联的应用来执行该操作。", "shortcuts-broken"),
        ("\"ping\" 不是内部或外部命令，也不是可运行的程序或批处理文件。", "command-not-found"),
        ("你需要使用新应用以打开此 ms-windows-store 链接", "store-broken"),
        ("USB 设备无法识别 你连接到此计算机的最后一个 USB 设备发生故障，Windows 无法识别它。", "usb-drive"),
        ("该设备无法启动。 (代码 10)", "device-error"),
        ("Windows 已停止此设备，因为它已报告了问题。 (代码 43)", "device-error"),
        ("C:\\ 上的回收站已损坏。是否清空该驱动器上的回收站?", "recycle-bin-corrupted"),
        (
            "The Recycle Bin on D:\\ is corrupted. Do you want to empty the Recycle Bin for this drive?",
            "recycle-bin-corrupted",
        ),
        ("文件正在使用 操作无法完成，因为文件已在 Microsoft Word 中打开。请关闭该文件并重试。", "file-in-use"),
        ("进程无法访问此文件，因为另一个程序正在使用此文件。", "file-in-use"),
        ("此应用无法在你的电脑上运行 若要找到适用于你的电脑的版本，请咨询软件发布者。 关闭", "app-cannot-run"),
        ("D:\\下载\\setup.exe 不是有效的 Win32 应用程序。", "app-cannot-run"),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|(text, want)| match best(text) {
            Some((id, _)) if id == *want => None,
            other => Some(format!("「{text}」应该对上 {want}，结果是 {other:?}")),
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
