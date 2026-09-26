//! 把 YAML 转成 `serde_json::Value`，再交给 serde 反序列化成强类型。
//!
//! 用 yaml-rust2（纯 Rust、在维护中）而不是已经归档的 serde_yaml。

use std::collections::HashSet;

use serde_json::{Map, Number, Value};
use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::Marker;
use yaml_rust2::{Yaml, YamlLoader};

pub fn parse(src: &str) -> Result<Value, String> {
    check_duplicate_keys(src)?;
    let docs = YamlLoader::load_from_str(src).map_err(|e| format!("YAML 语法错误：{e}"))?;
    match docs.as_slice() {
        [] => Err("文件是空的".into()),
        [doc] => to_json(doc),
        _ => Err("一个文件里只能有一个 YAML 文档".into()),
    }
}

/// yaml-rust2 的 YamlLoader 遇到重复的键会静默覆盖，所以先用事件流单独检查一遍。
fn check_duplicate_keys(src: &str) -> Result<(), String> {
    enum Frame {
        Seq,
        Map { seen: HashSet<String>, expect_key: bool },
    }
    #[derive(Default)]
    struct Recv {
        stack: Vec<Frame>,
        dups: Vec<(String, usize)>,
        alias_line: Option<usize>,
    }
    impl Recv {
        fn node(&mut self, scalar: Option<&str>, mark: Marker) {
            if let Some(Frame::Map { seen, expect_key }) = self.stack.last_mut() {
                if *expect_key {
                    if let Some(k) = scalar
                        && !seen.insert(k.to_owned())
                    {
                        self.dups.push((k.to_owned(), mark.line()));
                    }
                    *expect_key = false;
                } else {
                    *expect_key = true;
                }
            }
        }
    }
    impl MarkedEventReceiver for Recv {
        fn on_event(&mut self, ev: Event, mark: Marker) {
            match ev {
                Event::Scalar(v, ..) => self.node(Some(&v), mark),
                Event::Alias(_) => {
                    self.alias_line.get_or_insert(mark.line());
                    self.node(None, mark);
                }
                Event::SequenceStart(..) => {
                    self.node(None, mark);
                    self.stack.push(Frame::Seq);
                }
                Event::MappingStart(..) => {
                    self.node(None, mark);
                    self.stack.push(Frame::Map { seen: HashSet::new(), expect_key: true });
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    self.stack.pop();
                }
                _ => {}
            }
        }
    }
    let mut recv = Recv::default();
    Parser::new_from_str(src).load(&mut recv, true).map_err(|e| format!("YAML 语法错误：{e}"))?;
    if let Some(line) = recv.alias_line {
        return Err(format!("第 {line} 行：不支持 YAML 锚点和别名，请直接写出内容"));
    }
    match recv.dups.first() {
        Some((key, line)) => Err(format!("第 {line} 行：重复的键 {key}")),
        None => Ok(()),
    }
}

fn to_json(y: &Yaml) -> Result<Value, String> {
    Ok(match y {
        Yaml::Null => Value::Null,
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::Real(s) => {
            let f: f64 = s.parse().map_err(|_| format!("无法解析的数字：{s}"))?;
            Value::Number(Number::from_f64(f).ok_or_else(|| format!("不支持的数字：{s}"))?)
        }
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Array(items) => Value::Array(items.iter().map(to_json).collect::<Result<_, _>>()?),
        Yaml::Hash(h) => {
            let mut map = Map::new();
            for (k, v) in h {
                let key = match k {
                    Yaml::String(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Boolean(b) => b.to_string(),
                    other => return Err(format!("不支持的键：{other:?}")),
                };
                if map.insert(key.clone(), to_json(v)?).is_some() {
                    return Err(format!("重复的键：{key}"));
                }
            }
            Value::Object(map)
        }
        Yaml::Alias(_) => return Err("不支持 YAML 锚点和别名".into()),
        Yaml::BadValue => return Err("无法解析的值".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn converts_common_shapes() {
        let v = parse(
            r#"
id: x.y
title: { zh-CN: 标题 }
n: 0
f: 1.5
b: true
list: [a, "b"]
empty: ""
clsid: "{20D04FE0-3AEA-1069-A2D8-08002B30309D}"
nothing: ~
"#,
        )
        .unwrap();
        assert_eq!(
            v,
            json!({
                "id": "x.y", "title": {"zh-CN": "标题"}, "n": 0, "f": 1.5, "b": true,
                "list": ["a", "b"], "empty": "", "clsid": "{20D04FE0-3AEA-1069-A2D8-08002B30309D}",
                "nothing": null
            })
        );
    }

    #[test]
    fn unquoted_brace_becomes_a_map() {
        // 提醒贡献者：{…} 不加引号会被当成映射，后面的类型校验会报错。
        let v = parse("name: {20D04FE0}").unwrap();
        assert!(v["name"].is_object());
    }

    #[test]
    fn rejects_duplicates_and_aliases() {
        assert!(parse("a: 1\na: 2\n").is_err());
        assert!(parse("a: &x 1\nb: *x\n").is_err());
        assert!(parse("").is_err());
    }
}
