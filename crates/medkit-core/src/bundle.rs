//! 把 catalog 和脚本打包成一个 JSON（构建时内嵌进 exe），运行时解压脚本并校验。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::{self, CatalogData, Problem};
use crate::lint;
use crate::script::{HOST_SCRIPT, ScriptManifest};

pub const BUNDLE_FORMAT: u32 = 1;

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub format: u32,
    /// 数据和脚本内容的哈希，同时用作运行目录名
    pub hash: String,
    pub catalog: CatalogData,
    pub scripts: BTreeMap<String, BundledScript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledScript {
    pub sha256: String,
    pub content: String,
}

impl Bundle {
    /// 从仓库根目录（含 catalog/ 和 scripts/）构建。有错误时返回 `None`，问题列表里有详情。
    pub fn from_repo(root: &Path) -> (Option<Self>, Vec<Problem>) {
        let (catalog, mut problems) = catalog::load_dir(&root.join("catalog"));
        let scripts = load_scripts(&root.join("scripts"), &mut problems);
        let names: BTreeSet<String> = scripts.keys().cloned().collect();
        if !names.contains(HOST_SCRIPT) {
            problems.push(Problem::error("scripts", format!("缺少宿主脚本 scripts/{HOST_SCRIPT}")));
        }
        problems.extend(catalog::validate(&catalog, &names));
        if catalog::has_errors(&problems) {
            return (None, problems);
        }
        let hash = content_hash(&catalog, &scripts);
        (Some(Self { format: BUNDLE_FORMAT, hash, catalog, scripts }), problems)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("bundle 可以序列化")
    }

    /// 读取内嵌的 bundle，并核对每个脚本的哈希。
    pub fn from_json(s: &str) -> Result<Self, String> {
        let b: Self = serde_json::from_str(s).map_err(|e| format!("内嵌数据损坏：{e}"))?;
        if b.format != BUNDLE_FORMAT {
            return Err(format!("内嵌数据格式 {} 不受支持", b.format));
        }
        for (name, script) in &b.scripts {
            if sha256_hex(script.content.as_bytes()) != script.sha256 {
                return Err(format!("内嵌脚本 {name} 的哈希对不上"));
            }
        }
        Ok(b)
    }

    /// 把脚本写到 `dir` 下（已存在且内容相同的跳过），返回执行前校验用的清单。
    pub fn extract(&self, dir: &Path) -> std::io::Result<ScriptManifest> {
        let mut hashes = BTreeMap::new();
        for (rel, script) in &self.scripts {
            let path = dir.join(rel);
            let unchanged = std::fs::read(&path).is_ok_and(|old| sha256_hex(&old) == script.sha256);
            if !unchanged {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, script.content.as_bytes())?;
            }
            hashes.insert(rel.clone(), script.sha256.clone());
        }
        Ok(ScriptManifest::new(dir, hashes))
    }
}

fn load_scripts(dir: &Path, problems: &mut Vec<Problem>) -> BTreeMap<String, BundledScript> {
    let mut files = Vec::new();
    catalog::collect_files(dir, &["ps1"], &mut files);
    let mut out = BTreeMap::new();
    for path in files {
        let rel = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                problems.push(Problem::error(
                    &format!("scripts/{rel}"),
                    format!("读取失败（必须是 UTF-8/ASCII 文本）：{e}"),
                ));
                continue;
            }
        };
        if let Err(e) = crate::script::validate_script_path(&rel) {
            problems.push(Problem::error(&format!("scripts/{rel}"), e.to_string()));
            continue;
        }
        problems.extend(lint::lint_script(&rel, &content));
        let sha256 = sha256_hex(content.as_bytes());
        out.insert(rel, BundledScript { sha256, content });
    }
    problems.extend(lint::lint_shared_blocks(out.iter().map(|(rel, s)| (rel.as_str(), s.content.as_str()))));
    out
}

fn content_hash(catalog: &CatalogData, scripts: &BTreeMap<String, BundledScript>) -> String {
    let bytes = serde_json::to_vec(&(catalog, scripts)).expect("可以序列化");
    sha256_hex(&bytes)[..16].to_owned()
}
