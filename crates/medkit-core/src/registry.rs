//! 注册表的值和路径。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::RegType;

/// 注册表根。`HKCU\…` 会在运行时解析成登录用户的 `HKU\<SID>`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegRoot {
    LocalMachine,
    /// 当前进程的 HKCU。只在拿不到登录用户时作为兜底，界面会提示。
    CurrentUser,
    /// `HKU\<SID>`
    User(String),
}

impl RegRoot {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "HKLM" => Some(Self::LocalMachine),
            "HKCU" => Some(Self::CurrentUser),
            _ => {
                let sid = s.strip_prefix("HKU\\")?;
                is_sid(sid).then(|| Self::User(sid.to_owned()))
            }
        }
    }
}

impl fmt::Display for RegRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalMachine => f.write_str("HKLM"),
            Self::CurrentUser => f.write_str("HKCU"),
            Self::User(sid) => write!(f, "HKU\\{sid}"),
        }
    }
}

impl Serialize for RegRoot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RegRoot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("无效的注册表根：{s}")))
    }
}

/// `S-1-5-21-…` 这样的 SID。只允许数字和短横线，防止被拼进路径。
pub fn is_sid(s: &str) -> bool {
    s.starts_with("S-1-")
        && s.len() <= 184
        && s[4..].split('-').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// 数据文件里写的根：只允许 HKCU 和 HKLM。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecRoot {
    Hkcu,
    Hklm,
}

/// 把 `HKCU\Software\…` 拆成根和子键。子键不能为空，不能有空段，也不能以 `\` 结尾。
pub fn split_key(key: &str) -> Result<(SpecRoot, &str), String> {
    let (root, rest) = if let Some(rest) = key.strip_prefix("HKCU\\") {
        (SpecRoot::Hkcu, rest)
    } else if let Some(rest) = key.strip_prefix("HKLM\\") {
        (SpecRoot::Hklm, rest)
    } else {
        return Err(format!("注册表路径必须以 HKCU\\ 或 HKLM\\ 开头：{key}"));
    };
    if rest.is_empty() || rest.split('\\').any(str::is_empty) {
        return Err(format!("注册表路径格式不对：{key}"));
    }
    if rest.contains('/') {
        return Err(format!("注册表路径要用反斜杠：{key}"));
    }
    Ok((root, rest))
}

/// 从浅到深列出一个子键路径上的每一级：`A\B\C` → `A`、`A\B`、`A\B\C`。
pub fn key_ancestors(subkey: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for part in subkey.split('\\') {
        if !acc.is_empty() {
            acc.push('\\');
        }
        acc.push_str(part);
        out.push(acc.clone());
    }
    out
}

/// 注册表里的一个值。JSON 形如 `{"type":"dword","data":0}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "kebab-case")]
pub enum RegValue {
    Dword(u32),
    Qword(u64),
    String(String),
    ExpandString(String),
    MultiString(Vec<String>),
    Binary(#[serde(with = "hex_bytes")] Vec<u8>),
}

impl RegValue {
    /// 按数据文件里的 `type` 和 `value` 构造。
    pub fn from_spec(value_type: RegType, value: &serde_json::Value) -> Result<Self, String> {
        use serde_json::Value as J;
        Ok(match (value_type, value) {
            (RegType::Dword, J::Number(n)) => {
                let n = n.as_u64().ok_or("dword 必须是 0 到 4294967295 之间的整数")?;
                Self::Dword(u32::try_from(n).map_err(|_| "dword 超出范围")?)
            }
            (RegType::Qword, J::Number(n)) => Self::Qword(n.as_u64().ok_or("qword 必须是非负整数")?),
            (RegType::String, J::String(s)) => Self::String(s.clone()),
            (RegType::ExpandString, J::String(s)) => Self::ExpandString(s.clone()),
            (RegType::MultiString, J::Array(items)) => Self::MultiString(
                items
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).ok_or("multi-string 的每一项都必须是字符串"))
                    .collect::<Result<_, _>>()?,
            ),
            (RegType::Binary, J::String(s)) => {
                Self::Binary(hex::decode(s).map_err(|e| format!("binary 必须是十六进制字符串：{e}"))?)
            }
            (t, v) => return Err(format!("值 {v} 和类型 {t:?} 对不上")),
        })
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Dword(_) => "dword",
            Self::Qword(_) => "qword",
            Self::String(_) => "string",
            Self::ExpandString(_) => "expand-string",
            Self::MultiString(_) => "multi-string",
            Self::Binary(_) => "binary",
        }
    }
}

impl fmt::Display for RegValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dword(n) => write!(f, "{n}（dword）"),
            Self::Qword(n) => write!(f, "{n}（qword）"),
            Self::String(s) | Self::ExpandString(s) if s.is_empty() => f.write_str("空字符串"),
            Self::String(s) | Self::ExpandString(s) => write!(f, "\"{s}\""),
            Self::MultiString(items) => write!(f, "[{}]", items.join(", ")),
            Self::Binary(bytes) => {
                let shown: Vec<String> = bytes.iter().take(16).map(|b| format!("{b:02X}")).collect();
                let more = if bytes.len() > 16 { " …" } else { "" };
                write!(f, "{}{more}（binary）", shown.join(" "))
            }
        }
    }
}

/// 可选的值：`None` 表示「不存在」。
pub fn display_opt(v: Option<&RegValue>) -> String {
    v.map_or_else(|| "（不存在）".to_owned(), ToString::to_string)
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_and_ancestors() {
        let (root, sub) = split_key(r"HKCU\Software\A\B").unwrap();
        assert_eq!(root, SpecRoot::Hkcu);
        assert_eq!(sub, r"Software\A\B");
        assert_eq!(key_ancestors(sub), vec!["Software", r"Software\A", r"Software\A\B"]);
        assert!(split_key(r"HKEY_CURRENT_USER\Software").is_err());
        assert!(split_key(r"HKLM\").is_err());
        assert!(split_key(r"HKLM\A\\B").is_err());
        assert!(split_key(r"HKLM\A\").is_err());
    }

    #[test]
    fn roots_round_trip() {
        for s in ["HKLM", "HKCU", r"HKU\S-1-5-21-111-222-333-1001"] {
            assert_eq!(RegRoot::parse(s).unwrap().to_string(), s);
        }
        assert!(RegRoot::parse(r"HKU\S-1-5-21-1\..\x").is_none());
        assert!(RegRoot::parse(r"HKU\.DEFAULT").is_none());
    }

    #[test]
    fn values_from_spec_and_json() {
        assert_eq!(RegValue::from_spec(RegType::Dword, &json!(0)).unwrap(), RegValue::Dword(0));
        assert_eq!(RegValue::from_spec(RegType::Dword, &json!(4294967295u64)).unwrap(), RegValue::Dword(u32::MAX));
        assert!(RegValue::from_spec(RegType::Dword, &json!(-1)).is_err());
        assert!(RegValue::from_spec(RegType::Dword, &json!("0")).is_err());
        let bin = RegValue::from_spec(RegType::Binary, &json!("0200ff")).unwrap();
        assert_eq!(bin, RegValue::Binary(vec![2, 0, 255]));
        let j = serde_json::to_value(&bin).unwrap();
        assert_eq!(j, json!({"type": "binary", "data": "0200ff"}));
        assert_eq!(serde_json::from_value::<RegValue>(j).unwrap(), bin);
        assert_eq!(serde_json::to_value(RegValue::Dword(1)).unwrap(), json!({"type": "dword", "data": 1}));
    }
}
