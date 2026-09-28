//! 图片转文字在假系统上的行为：把图片路径交给认字的脚本，结果整理成一行一行的文字。

use std::path::Path;
use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::journal::Journal;
use medkit_core::ocr;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::script::MockRunner;
use medkit_core::views::OcrStatus;
use serde_json::json;

fn engine(runner: Arc<MockRunner>, dir: &Path) -> Engine {
    Engine::new(
        Catalog::new(CatalogData::default()),
        "test",
        "0.0.0-test",
        Arc::new(MockPlatform::new()),
        runner,
        Journal::open(dir.join("journal.jsonl")).unwrap(),
    )
}

#[test]
fn the_picture_goes_to_the_script_and_the_words_come_back_as_lines() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(MockRunner::new());
    runner.returns(
        ocr::SCRIPT,
        json!({ "result": "ok", "language": "zh-Hans-CN", "languages": ["zh-Hans-CN", "en-US"],
                "lines": [ { "words": ["发", "票", "号", "码", "：", "12345678"] }, { "words": ["Total:", "¥", "88.00"] } ],
                "pieces": 2, "truncated": false }),
    );
    let e = engine(runner.clone(), dir.path());
    let picture = dir.path().join("ocr").join("picture.png");
    let r = e.ocr_recognize(&picture).unwrap();
    assert_eq!(r.status, OcrStatus::Ok);
    assert_eq!(r.text, "发票号码：12345678\nTotal: ¥ 88.00");
    assert_eq!(r.language.as_deref(), Some("中文（简体）"));
    assert!(r.chinese);
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, ocr::SCRIPT);
    assert_eq!(calls[0].1["Path"], picture.to_string_lossy().as_ref());
    assert!(e.journal_list().unwrap().is_empty(), "只读，不记修改日志");
}

#[test]
fn a_computer_without_chinese_recognition_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(MockRunner::new());
    runner.returns(ocr::SCRIPT, json!({ "result": "no-language", "languages": [] }));
    let r = engine(runner, dir.path()).ocr_recognize(Path::new("x.png")).unwrap();
    assert_eq!(r.status, OcrStatus::NoLanguage);
    assert!(!r.chinese && r.text.is_empty());
}

#[test]
fn script_errors_become_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(MockRunner::new());
    runner.returns(ocr::SCRIPT, json!({ "result": "surprise" }));
    let err = engine(runner, dir.path()).ocr_recognize(Path::new("x.png")).unwrap_err().to_string();
    assert!(err.contains("看不懂"), "{err}");
}
