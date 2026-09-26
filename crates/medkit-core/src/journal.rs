//! 修改日志：一行一条 JSON，只追加，不修改（见 docs/architecture.md 第 8 节）。
//!
//! 每个原语写两条记录：改之前写 `apply`（带修改前的快照），改完写 `commit`（带结果）。
//! 这样即使程序在两步之间崩溃，日志里也有原值，照样可以恢复。

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{Error, Result};
use crate::model::StartType;
use crate::registry::{RegRoot, RegValue};

pub const RECORD_VERSION: u32 = 1;

/// 改的是什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetRef {
    Registry {
        root: RegRoot,
        key: String,
        name: String,
    },
    Service {
        name: String,
    },
    Script {
        feature: String,
        /// 执行时传给脚本的 `-UserHive`（只对 target: current-user 的功能有）。
        /// 撤销时原样传回，不能按撤销那一刻的登录用户重新解析，否则会改到别人身上。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hive: Option<String>,
    },
}

/// 某个时刻的状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Registry {
        /// `None` 表示这个值不存在
        value: Option<RegValue>,
        /// 为了写这个值而新建的键（从浅到深）
        #[serde(default)]
        created_keys: Vec<String>,
    },
    Service {
        start_type: Option<StartType>,
    },
    Script {
        data: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Record {
    Apply(ApplyRecord),
    Commit(CommitRecord),
    Undo(UndoRecord),
}

/// 改之前写：打算改什么、改之前是什么样。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyRecord {
    pub v: u32,
    pub id: String,
    pub session: String,
    pub time: String,
    pub feature: String,
    /// 功能里第几个原语（脚本类功能为 0）
    pub action: usize,
    pub target: TargetRef,
    pub before: State,
}

/// 改完写：结果如何。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    pub v: u32,
    #[serde(rename = "ref")]
    pub reference: String,
    pub time: String,
    pub ok: bool,
    #[serde(default)]
    pub after: Option<State>,
    #[serde(default)]
    pub error: Option<String>,
    /// 没改成功，而且自动退回以后读回来还不是原样：系统上可能留着改了一半的东西，要让用户能手动恢复
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub left_changes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UndoReason {
    /// 用户点了「恢复原状」
    User,
    /// 同一功能里后面的原语失败了，自动退回
    Rollback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoRecord {
    pub v: u32,
    pub id: String,
    pub session: String,
    pub time: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub reason: UndoReason,
    pub forced: bool,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// 一条修改合并后的状态。
#[derive(Debug, Clone)]
pub struct Entry {
    pub apply: ApplyRecord,
    pub commit: Option<CommitRecord>,
    /// 成功的撤销（最多一条）
    pub undo: Option<UndoRecord>,
}

impl Entry {
    /// 改动是否（可能）已经落到系统上：成功提交，或者没有提交记录（崩溃在中途）。
    pub fn maybe_applied(&self) -> bool {
        self.commit.as_ref().is_none_or(|c| c.ok || c.left_changes)
    }

    pub fn is_pending(&self) -> bool {
        self.commit.is_none()
    }
}

pub struct Journal {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Journal {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Journal(format!("建不了目录 {}：{e}", dir.display())))?;
        }
        Ok(Self { path, lock: Mutex::new(()) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, record: &Record) -> Result<()> {
        let _g = self.lock.lock().unwrap();
        let mut line = serde_json::to_string(record).map_err(|e| Error::Journal(e.to_string()))?;
        line.push('\n');
        // 上次写到一半断电的话，文件末尾是没有换行的半行。先补一个换行，
        // 否则这条新记录会和半行粘成一行，读的时候整行被当成坏行跳过，这次修改就没了记录。
        if !ends_with_newline(&self.path)? {
            line.insert(0, '\n');
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| Error::Journal(format!("打不开 {}：{e}", self.path.display())))?;
        f.write_all(line.as_bytes()).and_then(|()| f.sync_data()).map_err(|e| Error::Journal(format!("写入失败：{e}")))
    }

    /// 读全部记录。坏掉的行（例如写到一半断电）跳过，返回跳过的行数。
    pub fn read_all(&self) -> Result<(Vec<Record>, usize)> {
        let _g = self.lock.lock().unwrap();
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(e) => return Err(Error::Journal(format!("打不开 {}：{e}", self.path.display()))),
        };
        let mut records = Vec::new();
        let mut bad = 0;
        // 按字节切行、每行单独解码：断电可能截断在一个中文字符的中间，
        // 那一行不是合法的 UTF-8，只能跳过它，不能让整份日志都读不出来。
        for raw in bytes.split(|&b| b == b'\n') {
            let Ok(line) = std::str::from_utf8(raw) else {
                bad += 1;
                continue;
            };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Record>(line) {
                Ok(r) => records.push(r),
                Err(_) => bad += 1,
            }
        }
        Ok((records, bad))
    }

    /// 合并成每条修改的状态，按写入顺序排列。
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let (records, _) = self.read_all()?;
        Ok(merge(records))
    }
}

/// 文件为空、不存在，或者最后一个字节是换行时返回 true。
fn ends_with_newline(path: &Path) -> Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(Error::Journal(format!("打不开 {}：{e}", path.display()))),
    };
    let len = f.metadata().map_err(|e| Error::Journal(e.to_string()))?.len();
    if len == 0 {
        return Ok(true);
    }
    let mut last = [0u8; 1];
    f.seek(SeekFrom::Start(len - 1))
        .and_then(|_| f.read_exact(&mut last))
        .map_err(|e| Error::Journal(format!("读不了 {}：{e}", path.display())))?;
    Ok(last[0] == b'\n')
}

pub fn merge(records: Vec<Record>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in records {
        match r {
            Record::Apply(a) => {
                index.insert(a.id.clone(), entries.len());
                entries.push(Entry { apply: a, commit: None, undo: None });
            }
            Record::Commit(c) => {
                if let Some(&i) = index.get(&c.reference) {
                    entries[i].commit = Some(c);
                }
            }
            Record::Undo(u) => {
                if let Some(&i) = index.get(&u.reference)
                    && u.ok
                    && entries[i].undo.is_none()
                {
                    entries[i].undo = Some(u);
                }
            }
        }
    }
    entries
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(id: &str) -> Record {
        Record::Apply(ApplyRecord {
            v: 1,
            id: id.into(),
            session: "s".into(),
            time: now_rfc3339(),
            feature: "f".into(),
            action: 0,
            target: TargetRef::Service { name: "x".into() },
            before: State::Service { start_type: Some(StartType::Manual) },
        })
    }

    #[test]
    fn append_read_merge_and_skip_bad_lines() {
        let dir = tempfile::tempdir().unwrap();
        let j = Journal::open(dir.path().join("sub/journal.jsonl")).unwrap();
        j.append(&apply("a")).unwrap();
        j.append(&Record::Commit(CommitRecord {
            v: 1,
            reference: "a".into(),
            time: now_rfc3339(),
            ok: true,
            after: Some(State::Service { start_type: Some(StartType::Disabled) }),
            error: None,
            left_changes: false,
        }))
        .unwrap();
        j.append(&apply("b")).unwrap();
        // 模拟写到一半断电
        std::fs::OpenOptions::new().append(true).open(j.path()).unwrap().write_all(b"{\"kind\":\"ap").unwrap();
        let (records, bad) = j.read_all().unwrap();
        assert_eq!((records.len(), bad), (3, 1));
        let entries = merge(records);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].commit.as_ref().unwrap().ok && !entries[0].is_pending());
        assert!(entries[1].is_pending() && entries[1].maybe_applied());
    }

    #[test]
    fn record_json_shape() {
        let json = serde_json::to_value(apply("a")).unwrap();
        assert_eq!(json["kind"], "apply");
        assert_eq!(json["target"]["service"]["name"], "x");
        assert_eq!(json["before"]["service"]["start_type"], "manual");
    }
}
