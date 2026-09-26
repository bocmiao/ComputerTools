//! 把 YAML 里的消息模板和脚本返回的事实拼成最终显示的文字。

use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::{Map, Value};

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
}
