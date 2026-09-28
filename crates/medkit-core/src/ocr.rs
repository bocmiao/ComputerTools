//! 图片转文字：用 Windows 自带的文字识别（Windows.Media.Ocr）。脚本 `scripts/ocr/recognize.ps1` 认字，这里把它的结果
//! 整理成界面要的样子：一行一行的文字（中文的字之间不加空格），用的是哪种语言，装没装中文的识别。
//!
//! 图片由界面交给后端存成小药箱数据文件夹里的临时文件，认完就删；文字只在这台电脑上处理，不上传。

use std::time::Duration;

use serde_json::Value;

use crate::views::{OcrStatus, OcrView};

/// 认字的脚本（不在数据文件里：由引擎直接调用）。
pub const SCRIPT: &str = "ocr/recognize.ps1";
/// 长截图要分成好几块认，老电脑上一块要几秒。
pub const TIMEOUT: Duration = Duration::from_secs(180);
/// 装中文文字识别的小工具（界面上没有中文识别时给这个按钮）。
pub const INSTALL_TOOL: &str = "system.install-ocr-chinese";

/// 前后不加空格的字：汉字、日文假名、注音，以及中文标点和全角字符。英文、数字、韩文之间照常加空格。
fn no_space(c: char) -> bool {
    matches!(u32::from(c),
        0x2E80..=0x2FDF      // 部首
        | 0x3000..=0x303F    // 中文标点
        | 0x3040..=0x30FF    // 假名
        | 0x3100..=0x312F    // 注音
        | 0x31A0..=0x31FF    // 注音扩展、片假名扩展
        | 0x3400..=0x4DBF    // 扩展 A
        | 0x4E00..=0x9FFF    // 常用汉字
        | 0xF900..=0xFAFF    // 兼容汉字
        | 0xFE30..=0xFE4F    // 竖排标点
        | 0xFF00..=0xFFEF    // 全角字符
        | 0x20000..=0x3134F) // 扩展 B 以后
}

/// 把一行里的词接起来：Windows 认中文时把每个字当成一个词，词之间用空格隔开（「你 好 世 界」），
/// 这里只在两边都不是中文的时候才加空格（「Windows 11」照旧）。
pub fn join_words<S: AsRef<str>>(words: &[S]) -> String {
    let mut out = String::new();
    let mut last: Option<char> = None;
    for word in words {
        let word = word.as_ref().trim();
        let Some(first) = word.chars().next() else { continue };
        if let Some(prev) = last
            && !no_space(prev)
            && !no_space(first)
        {
            out.push(' ');
        }
        out.push_str(word);
        last = word.chars().last();
    }
    out
}

/// 识别语言说成人话：「中文（简体）」「英语」；不认得的照原样写语言代码。
pub fn language_name(tag: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let primary = lower.split('-').next().unwrap_or_default();
    if primary == "zh" {
        let traditional = lower.split('-').any(|p| matches!(p, "hant" | "tw" | "hk" | "mo"));
        return if traditional { "中文（繁体）" } else { "中文（简体）" }.to_owned();
    }
    let name = match primary {
        "en" => "英语",
        "ja" => "日语",
        "ko" => "韩语",
        "fr" => "法语",
        "de" => "德语",
        "es" => "西班牙语",
        "it" => "意大利语",
        "pt" => "葡萄牙语",
        "ru" => "俄语",
        "ar" => "阿拉伯语",
        _ => return tag.to_owned(),
    };
    name.to_owned()
}

/// 数组或单个字符串（PowerShell 有时把只有一项的数组写成字符串）。
fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// 解析脚本的结果。
pub fn parse(v: &Value) -> Result<OcrView, String> {
    let status = match v.get("result").and_then(Value::as_str) {
        Some("ok") => OcrStatus::Ok,
        Some("no-language") => OcrStatus::NoLanguage,
        Some("unsupported") => OcrStatus::Unsupported,
        Some("bad-image") => OcrStatus::BadImage,
        other => return Err(format!("认字的脚本返回了看不懂的结果：{other:?}")),
    };
    let tags = strings(v.get("languages"));
    let mut languages: Vec<String> = Vec::new();
    for name in tags.iter().map(|t| language_name(t)) {
        if !languages.contains(&name) {
            languages.push(name);
        }
    }
    let chinese = tags.iter().any(|t| t.to_ascii_lowercase().starts_with("zh"));
    let mut lines = Vec::new();
    if status == OcrStatus::Ok {
        for line in v.get("lines").and_then(Value::as_array).into_iter().flatten() {
            let words = match line {
                Value::Object(o) => strings(o.get("words")),
                other => strings(Some(other)),
            };
            let text = join_words(&words);
            if !text.is_empty() {
                lines.push(text);
            }
        }
    }
    let language = v.get("language").and_then(Value::as_str).filter(|s| !s.is_empty()).map(language_name);
    Ok(OcrView {
        status,
        lines: lines.len(),
        text: lines.join("\n"),
        language,
        languages,
        chinese,
        truncated: v.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        detail: v.get("detail").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn chinese_characters_are_joined_without_spaces() {
        assert_eq!(join_words(&["你", "好", "，", "世", "界"]), "你好，世界");
        assert_eq!(join_words(&["Windows", "11", "家庭版"]), "Windows 11家庭版");
        assert_eq!(join_words(&["共", "3", "个", "文件"]), "共3个文件");
        assert_eq!(join_words(&["Hello,", "world!"]), "Hello, world!");
        assert_eq!(join_words(&["안녕하세요", "세계"]), "안녕하세요 세계", "韩文词之间有空格");
        assert_eq!(join_words(&["（", "注", "）", "A"]), "（注）A");
        assert_eq!(join_words::<&str>(&[]), "");
        assert_eq!(join_words(&["", " ", "字"]), "字");
    }

    #[test]
    fn language_names() {
        assert_eq!(language_name("zh-Hans-CN"), "中文（简体）");
        assert_eq!(language_name("zh-CN"), "中文（简体）");
        assert_eq!(language_name("zh-Hant-TW"), "中文（繁体）");
        assert_eq!(language_name("zh-HK"), "中文（繁体）");
        assert_eq!(language_name("en-US"), "英语");
        assert_eq!(language_name("sr-Latn-RS"), "sr-Latn-RS");
    }

    #[test]
    fn script_results_are_parsed() {
        let v = json!({
            "result": "ok", "language": "zh-Hans-CN", "languages": ["zh-Hans-CN", "en-US", "en-GB"],
            "lines": [ { "words": ["电", "脑", "小", "药", "箱"] }, { "words": "v1.2" }, { "words": [] } ],
            "pieces": 1, "truncated": false
        });
        let r = parse(&v).unwrap();
        assert_eq!(r.status, OcrStatus::Ok);
        assert_eq!(r.text, "电脑小药箱\nv1.2");
        assert_eq!(r.lines, 2);
        assert_eq!(r.language.as_deref(), Some("中文（简体）"));
        assert_eq!(r.languages, ["中文（简体）", "英语"], "同一种语言只写一次");
        assert!(r.chinese && !r.truncated);

        let r = parse(&json!({ "result": "bad-image", "languages": ["en-US"], "detail": " 找不到组件。 " })).unwrap();
        assert_eq!(r.status, OcrStatus::BadImage);
        assert_eq!(r.detail.as_deref(), Some("找不到组件。"));
        let r = parse(&json!({ "result": "no-language", "languages": [] })).unwrap();
        assert_eq!(r.status, OcrStatus::NoLanguage);
        assert!(!r.chinese && r.text.is_empty() && r.language.is_none());
        // 只装了英语：认得出来，但不是中文
        let r = parse(&json!({ "result": "ok", "language": "en-US", "languages": "en-US", "lines": [] })).unwrap();
        assert!(!r.chinese && r.languages == ["英语"]);
        assert!(parse(&json!({ "result": "what" })).is_err());
    }
}
