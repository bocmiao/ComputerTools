//! 把 YAML 里的消息模板和脚本返回的事实拼成最终显示的文字。

use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::{Map, Value};

use crate::model::FactLabels;

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{([a-z][a-z0-9_]*)\}").unwrap());

/// 用事实替换 `{名字}`；找不到的占位符原样保留（冒烟测试会检查）。
pub fn render(template: &str, facts: &Map<String, Value>) -> String {
    PLACEHOLDER
        .replace_all(template, |caps: &Captures<'_>| {
            facts.get(&caps[1]).map_or_else(|| caps[0].to_owned(), format_fact)
        })
        .into_owned()
}

/// 模板里没被替换掉的占位符。
pub fn unresolved(text: &str) -> Vec<String> {
    PLACEHOLDER.captures_iter(text).map(|c| c[1].to_owned()).collect()
}

/// 按说明表查出事实值的说明（见 [`FactLabels`]）：字符串、数字查它自己，数组逐个查、按原顺序连起来，
/// 查不到的跳过；事实不存在或者一个都查不到时是空字符串。
pub fn label_fact(value: Option<&Value>, labels: &FactLabels, lang: &str) -> String {
    let one = |v: &Value| -> Option<&str> {
        let key = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return None,
        };
        labels.values.get(&key).map(|t| t.get(lang))
    };
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(one).collect::<Vec<_>>().concat(),
        Some(v) => one(v).unwrap_or_default().to_owned(),
        None => String::new(),
    }
}

pub fn format_fact(v: &Value) -> String {
    match v {
        Value::Null => "（无）".to_owned(),
        Value::Bool(b) => if *b { "是" } else { "否" }.to_owned(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                let r = (f * 10.0).round() / 10.0;
                if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}") }
            } else {
                n.to_string()
            }
        }
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(format_fact).collect::<Vec<_>>().join("、"),
        Value::Object(_) => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_numbers_and_missing() {
        let facts = json!({"free_gb": 12.345, "free_pct": 5.0, "n": 3, "ok": true, "names": ["a", "b"]});
        let facts = facts.as_object().unwrap();
        assert_eq!(render("剩 {free_gb} GB（{free_pct}%）", facts), "剩 12.3 GB（5%）");
        assert_eq!(render("{n} 次，{ok}，{names}", facts), "3 次，是，a、b");
        assert_eq!(render("{missing} 保留", facts), "{missing} 保留");
        assert_eq!(unresolved("{missing} 和 {free_gb}"), vec!["missing", "free_gb"]);
    }

    #[test]
    fn labels_values_numbers_and_arrays() {
        let labels = FactLabels {
            from: "codes".into(),
            values: [("10", "代码 10：启动不了。"), ("43", "代码 43：报告了问题。"), ("0x80070005", "拒绝访问。")]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), crate::model::Text::zh_only(v)))
                .collect(),
        };
        let v = json!([43, 7, 10]);
        assert_eq!(label_fact(Some(&v), &labels, "zh-CN"), "代码 43：报告了问题。代码 10：启动不了。");
        assert_eq!(label_fact(Some(&json!(10)), &labels, "zh-CN"), "代码 10：启动不了。");
        assert_eq!(label_fact(Some(&json!("0x80070005")), &labels, "zh-CN"), "拒绝访问。");
        assert_eq!(label_fact(Some(&json!("0x80070006")), &labels, "zh-CN"), "");
        assert_eq!(label_fact(Some(&json!([7])), &labels, "zh-CN"), "");
        assert_eq!(label_fact(Some(&json!({"a": 1})), &labels, "zh-CN"), "");
        assert_eq!(label_fact(None, &labels, "zh-CN"), "");
    }
}
