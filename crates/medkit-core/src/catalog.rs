//! 读取和校验 catalog/ 下的数据文件。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{
    Action, Check, Feature, Maturity, Profile, RegistryAction, Symptom, Target, Text, Undo, UndoKeyword,
};
use crate::registry::{RegValue, SpecRoot, split_key};
use crate::yaml;

pub const SCHEMA_VERSION: u32 = 1;
pub const BUILTIN_PROBES: &[&str] = &["cpu-features"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// 校验发现的一个问题。
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub severity: Severity,
    pub file: String,
    pub message: String,
}

impl Problem {
    pub fn error(file: &str, message: String) -> Self {
        Self { severity: Severity::Error, file: file.to_owned(), message }
    }

    pub fn warning(file: &str, message: String) -> Self {
        Self { severity: Severity::Warning, file: file.to_owned(), message }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = match self.severity {
            Severity::Error => "错误",
            Severity::Warning => "警告",
        };
        write!(f, "[{tag}] {}：{}", self.file, self.message)
    }
}

pub fn has_errors(problems: &[Problem]) -> bool {
    problems.iter().any(|p| p.severity == Severity::Error)
}

/// 全部数据（可以序列化进 bundle）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogData {
    pub checks: Vec<Check>,
    pub features: Vec<Feature>,
    pub symptoms: Vec<Symptom>,
    pub profiles: Vec<Profile>,
    /// ID → 来源文件（只用于报错）
    #[serde(skip)]
    pub sources: HashMap<String, String>,
}

/// 从 `catalog/` 目录读取所有数据文件。读不了、解析不了的文件记为问题，不中断。
pub fn load_dir(catalog_root: &Path) -> (CatalogData, Vec<Problem>) {
    let mut data = CatalogData::default();
    let mut problems = Vec::new();
    load_kind(catalog_root, "checks", &mut data.checks, |c| &c.id, &mut data.sources, &mut problems);
    load_kind(catalog_root, "features", &mut data.features, |f| &f.id, &mut data.sources, &mut problems);
    load_kind(catalog_root, "symptoms", &mut data.symptoms, |s| &s.id, &mut data.sources, &mut problems);
    load_kind(catalog_root, "profiles", &mut data.profiles, |p| &p.id, &mut data.sources, &mut problems);
    (data, problems)
}

fn load_kind<T: DeserializeOwned>(
    root: &Path,
    dir: &str,
    out: &mut Vec<T>,
    id_of: impl Fn(&T) -> &String,
    sources: &mut HashMap<String, String>,
    problems: &mut Vec<Problem>,
) {
    let mut files = Vec::new();
    collect_files(&root.join(dir), &["yaml", "yml"], &mut files);
    files.sort();
    for path in files {
        let rel = relative_display(root, &path);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                problems.push(Problem::error(&rel, format!("读取失败：{e}")));
                continue;
            }
        };
        match parse_typed::<T>(&text) {
            Ok(item) => {
                sources.insert(format!("{dir}:{}", id_of(&item)), rel);
                out.push(item);
            }
            Err(msg) => problems.push(Problem::error(&rel, msg)),
        }
    }
}

/// 解析一个 YAML 文档为强类型，报错里带上出错字段的路径。
pub fn parse_typed<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let value = yaml::parse(text)?;
    serde_path_to_error::deserialize(value).map_err(|e| {
        let path = e.path().to_string();
        if path == "." { e.inner().to_string() } else { format!("{path}：{}", e.inner()) }
    })
}

/// 递归收集某些扩展名的文件。目录不存在时什么也不做。
pub fn collect_files(dir: &Path, exts: &[&str], out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, exts, out);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e)) {
            out.push(path);
        }
    }
}

fn relative_display(root: &Path, path: &Path) -> String {
    let base = root.parent().unwrap_or(root);
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+([.-][a-z0-9]+)*$").unwrap());
static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{([^{}]*)\}").unwrap());
static FACT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").unwrap());

/// 这些位置对应计划书第五节「不做」的事，数据里不允许写。键名比较不区分大小写。
const DENIED_KEYS: &[(&str, &str)] = &[
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows Defender", "不关 Defender（第五节第 4 条）"),
    (r"HKLM\SOFTWARE\Microsoft\Windows Defender", "不关 Defender（第五节第 4 条）"),
    (r"HKLM\SYSTEM\CurrentControlSet\Services\WinDefend", "不关 Defender（第五节第 4 条）"),
    (r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options", "不碰 IFEO"),
    (r"HKLM\SYSTEM\CurrentControlSet\Control\SafeBoot", "不改安全模式配置"),
    (r"HKLM\SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters\SMB1", "不开 SMB1（第五节第 22 条）"),
    (r"HKLM\SYSTEM\CurrentControlSet\Services\mrxsmb10", "不开 SMB1（第五节第 22 条）"),
];
const DENIED_VALUES: &[(&str, &str, &str)] = &[
    (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableLUA", "不关 UAC（第五节第 22 条）"),
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU", "NoAutoUpdate", "不永久禁用更新（第五节第 5 条）"),
    (
        r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings",
        "FlightSettingsMaxPauseDays",
        "不延长暂停更新（第五节第 26 条）",
    ),
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System", "EnableSmartScreen", "不关 SmartScreen（第五节第 22 条）"),
];
const DENIED_SERVICES: &[(&str, &str)] = &[
    ("WinDefend", "不关 Defender（第五节第 4 条）"),
    ("wuauserv", "不禁用 Windows 更新（第五节第 5 条）"),
    ("UsoSvc", "不禁用 Windows 更新（第五节第 5 条）"),
    ("WaaSMedicSvc", "不禁用 Windows 更新（第五节第 5 条）"),
    ("mpssvc", "不关防火墙（第五节第 22 条）"),
    ("SecurityHealthService", "不关安全中心（第五节第 4 条）"),
];

/// 校验全部数据。`scripts` 是 scripts/ 下所有脚本的相对路径。
pub fn validate(data: &CatalogData, scripts: &BTreeSet<String>) -> Vec<Problem> {
    let mut v = Validator { data, scripts, problems: Vec::new() };
    v.run();
    v.problems
}

struct Validator<'a> {
    data: &'a CatalogData,
    scripts: &'a BTreeSet<String>,
    problems: Vec<Problem>,
}

impl Validator<'_> {
    fn file(&self, kind: &str, id: &str) -> String {
        self.data.sources.get(&format!("{kind}:{id}")).cloned().unwrap_or_else(|| format!("{kind}/{id}"))
    }

    fn err(&mut self, file: &str, msg: String) {
        self.problems.push(Problem::error(file, msg));
    }

    fn warn(&mut self, file: &str, msg: String) {
        self.problems.push(Problem::warning(file, msg));
    }

    fn run(&mut self) {
        // 把引用拷出来，遍历数据时就不会和 &mut self 冲突。
        let data: &CatalogData = self.data;
        let check_ids = self.unique_ids("checks", data.checks.iter().map(|c| c.id.as_str()));
        let feature_ids = self.unique_ids("features", data.features.iter().map(|f| f.id.as_str()));
        let symptom_ids = self.unique_ids("symptoms", data.symptoms.iter().map(|s| s.id.as_str()));
        self.unique_ids("profiles", data.profiles.iter().map(|p| p.id.as_str()));

        for c in &data.checks {
            self.check(c, &feature_ids, &symptom_ids);
        }
        for f in &data.features {
            self.feature(f, &check_ids);
        }
        for s in &data.symptoms {
            self.symptom(s, &check_ids, &feature_ids);
        }
        for p in &data.profiles {
            let file = self.file("profiles", &p.id);
            self.common(&file, p.schema_version, &p.title, "title");
            if p.checks.is_empty() {
                self.err(&file, "checks 不能为空".into());
            }
            let mut seen = HashSet::new();
            for c in &p.checks {
                if !check_ids.contains(c.as_str()) {
                    self.err(&file, format!("引用了不存在的检测：{c}"));
                }
                if !seen.insert(c) {
                    self.err(&file, format!("检测重复出现：{c}"));
                }
            }
        }
    }

    fn unique_ids<'b>(&mut self, kind: &str, ids: impl Iterator<Item = &'b str>) -> HashSet<&'b str> {
        let mut set = HashSet::new();
        for id in ids {
            let file = self.file(kind, id);
            if !ID_RE.is_match(id) {
                self.err(&file, format!("ID 格式不对（只能用小写字母、数字、点和短横线）：{id}"));
            }
            if !set.insert(id) {
                self.err(&file, format!("ID 重复：{id}"));
            }
        }
        set
    }

    fn common(&mut self, file: &str, schema_version: u32, title: &Text, field: &str) {
        if schema_version != SCHEMA_VERSION {
            self.err(file, format!("schema_version 必须是 {SCHEMA_VERSION}"));
        }
        self.text(file, title, field);
    }

    fn text(&mut self, file: &str, t: &Text, field: &str) {
        match t.0.get(Text::DEFAULT_LANG) {
            Some(s) if !s.trim().is_empty() => {}
            _ => self.err(file, format!("{field} 必须有非空的 zh-CN 文本")),
        }
    }

    fn script_ref(&mut self, file: &str, script: &str, field: &str) {
        if crate::script::validate_script_path(script).is_err() {
            self.err(file, format!("{field} 的脚本路径不合法：{script}"));
        } else if !self.scripts.contains(script) {
            self.err(file, format!("{field} 引用的脚本不存在：scripts/{script}"));
        }
    }

    fn check(&mut self, c: &Check, features: &HashSet<&str>, symptoms: &HashSet<&str>) {
        let file = self.file("checks", &c.id);
        self.common(&file, c.schema_version, &c.title, "title");
        if let Some(d) = &c.description {
            self.text(&file, d, "description");
        }
        match (&c.probe.script, &c.probe.builtin) {
            (Some(s), None) => self.script_ref(&file, s, "probe.script"),
            (None, Some(b)) => {
                if !BUILTIN_PROBES.contains(&b.as_str()) {
                    self.err(&file, format!("不认识的内置检测：{b}（可用：{}）", BUILTIN_PROBES.join("、")));
                }
            }
            _ => self.err(&file, "probe 必须且只能写 script 或 builtin 之一".into()),
        }
        if c.timeout_sec == 0 || c.timeout_sec > 600 {
            self.err(&file, "timeout_sec 要在 1 到 600 之间".into());
        }
        if c.results.is_empty() {
            self.err(&file, "results 不能为空".into());
        }
        for (code, spec) in &c.results {
            if !ID_RE.is_match(code) {
                self.err(&file, format!("结果代码格式不对：{code}"));
            }
            self.text(&file, &spec.message, &format!("results.{code}.message"));
            self.placeholders(&file, &spec.message, code);
            if let Some(next) = &spec.next {
                self.text(&file, next, &format!("results.{code}.next"));
                self.placeholders(&file, next, code);
            }
            for link in &spec.links {
                let ok = match link.split_once(':') {
                    Some(("symptom", id)) => symptoms.contains(id),
                    Some(("feature", id)) => features.contains(id),
                    _ => false,
                };
                if !ok {
                    self.err(&file, format!("results.{code}.links 里的链接无效：{link}"));
                }
            }
        }
        if c.references.is_empty() {
            self.warn(&file, "最好写上 references（来源）".into());
        }
    }

    fn placeholders(&mut self, file: &str, t: &Text, code: &str) {
        for text in t.0.values() {
            for cap in PLACEHOLDER_RE.captures_iter(text) {
                let name = &cap[1];
                if !FACT_NAME_RE.is_match(name) {
                    self.err(
                        file,
                        format!("results.{code} 里的占位符格式不对：{{{name}}}（只能用小写字母、数字和下划线）"),
                    );
                }
            }
        }
    }

    fn feature(&mut self, f: &Feature, checks: &HashSet<&str>) {
        let file = self.file("features", &f.id);
        self.common(&file, f.schema_version, &f.title, "title");
        self.text(&file, &f.description, "description");

        match (f.actions.is_empty(), &f.run) {
            (false, None) => {
                if !matches!(f.undo, Undo::Keyword(UndoKeyword::Auto)) {
                    self.err(&file, "原语类功能的 undo 必须是 auto".into());
                }
                if f.detect.is_some() {
                    self.err(&file, "原语类功能由引擎自动检测，不要写 detect".into());
                }
                if f.break_script.is_some() {
                    self.err(&file, "原语类功能用 break_actions 制造故障，不要写 break 脚本".into());
                }
                if f.windows_default.is_empty() {
                    self.warn(&file, "最好写上 windows_default（系统默认状态）".into());
                }
            }
            (true, Some(run)) => {
                let run = run.script.clone();
                self.script_ref(&file, &run, "run");
                match &f.detect {
                    Some(d) => {
                        let d = d.script.clone();
                        self.script_ref(&file, &d, "detect");
                    }
                    None => self.err(&file, "脚本类功能必须写 detect".into()),
                }
                match &f.undo {
                    Undo::Script(s) => {
                        let s = s.script.clone();
                        self.script_ref(&file, &s, "undo");
                    }
                    Undo::Keyword(UndoKeyword::None) => {}
                    Undo::Keyword(UndoKeyword::Auto) => {
                        self.err(&file, "脚本类功能的 undo 不能是 auto：写撤销脚本，或者 none 加原因".into())
                    }
                }
                match &f.break_script {
                    Some(b) => {
                        let b = b.script.clone();
                        self.script_ref(&file, &b, "break");
                    }
                    None => self.warn(&file, "最好写上 break（测试用的故障制造脚本）".into()),
                }
                if !f.break_actions.is_empty() || !f.windows_default.is_empty() {
                    self.err(&file, "脚本类功能不要写 break_actions / windows_default".into());
                }
            }
            (false, Some(_)) => self.err(&file, "actions 和 run 只能二选一".into()),
            (true, None) => self.err(&file, "必须写 actions 或 run".into()),
        }

        if !f.reversible() {
            match &f.irreversible_reason {
                Some(t) => self.text(&file, t, "irreversible_reason"),
                None => self.err(&file, "undo 为 none 时必须写 irreversible_reason".into()),
            }
        } else if f.irreversible_reason.is_some() {
            self.err(&file, "能撤销的功能不要写 irreversible_reason".into());
        }

        if let (Some(min), Some(max)) = (f.applies_to.min_build, f.applies_to.max_build)
            && min > max
        {
            self.err(&file, "applies_to.min_build 不能大于 max_build".into());
        }
        if let Some(v) = &f.verify
            && !checks.contains(v.as_str())
        {
            self.err(&file, format!("verify 引用了不存在的检测：{v}"));
        }

        for (list, name) in
            [(&f.actions, "actions"), (&f.windows_default, "windows_default"), (&f.break_actions, "break_actions")]
        {
            for (i, a) in list.iter().enumerate() {
                self.action(&file, f.target, a, &format!("{name}[{i}]"));
            }
        }
        if f.references.is_empty() {
            self.warn(&file, "最好写上 references（来源）".into());
        }
    }

    fn action(&mut self, file: &str, target: Target, a: &Action, at: &str) {
        match a {
            Action::Registry(r) => self.registry_action(file, target, r, at),
            Action::Service(s) => {
                if target != Target::Machine {
                    self.err(file, format!("{at}：改服务的功能 target 必须是 machine"));
                }
                if s.name.trim().is_empty() || s.name.contains(['\\', '/']) {
                    self.err(file, format!("{at}：服务名不合法"));
                }
                if let Some((_, why)) = DENIED_SERVICES.iter().find(|(n, _)| n.eq_ignore_ascii_case(&s.name)) {
                    self.err(file, format!("{at}：不允许修改服务 {}：{why}", s.name));
                }
            }
        }
    }

    fn registry_action(&mut self, file: &str, target: Target, r: &RegistryAction, at: &str) {
        match split_key(&r.key) {
            Ok((root, _)) => {
                let expected = match target {
                    Target::CurrentUser => SpecRoot::Hkcu,
                    Target::Machine => SpecRoot::Hklm,
                };
                if root != expected {
                    let want = if expected == SpecRoot::Hkcu { "HKCU\\" } else { "HKLM\\" };
                    self.err(file, format!("{at}：target 和注册表根对不上，这个功能只能写 {want}"));
                }
            }
            Err(e) => self.err(file, format!("{at}：{e}")),
        }
        let key_lower = r.key.to_ascii_lowercase();
        for (denied, why) in DENIED_KEYS {
            let d = denied.to_ascii_lowercase();
            if key_lower == d || key_lower.starts_with(&format!("{d}\\")) {
                self.err(file, format!("{at}：不允许写 {}：{why}", r.key));
            }
        }
        for (k, n, why) in DENIED_VALUES {
            if key_lower == k.to_ascii_lowercase() && r.name.eq_ignore_ascii_case(n) {
                self.err(file, format!("{at}：不允许写 {}\\{}：{why}", r.key, r.name));
            }
        }
        match (r.delete, r.value_type, &r.value) {
            (true, None, None) => {}
            (true, _, _) => self.err(file, format!("{at}：delete 为 true 时不要写 type 和 value")),
            (false, Some(t), Some(v)) => {
                if let Err(e) = RegValue::from_spec(t, v) {
                    self.err(file, format!("{at}：{e}"));
                }
            }
            (false, _, _) => self.err(file, format!("{at}：要么写 type 和 value，要么写 delete: true")),
        }
    }

    fn symptom(&mut self, s: &Symptom, checks: &HashSet<&str>, features: &HashSet<&str>) {
        let file = self.file("symptoms", &s.id);
        self.common(&file, s.schema_version, &s.title, "title");
        if let Some(t) = &s.summary {
            self.text(&file, t, "summary");
        }
        for (i, c) in s.causes.iter().enumerate() {
            self.text(&file, c, &format!("causes[{i}]"));
        }
        if s.keywords.is_empty() {
            self.warn(&file, "最好写上 keywords（用户的日常说法），搜索靠它".into());
        }
        match (&s.guide, s.maturity) {
            (None, Maturity::Guide) => self.err(&file, "maturity 为 guide 时必须写 guide".into()),
            (Some(g), _) => self.text(&file, g, "guide"),
            _ => {}
        }
        if s.steps.is_empty() && s.maturity != Maturity::Guide {
            self.err(&file, "没有 steps 的症状，maturity 只能是 guide".into());
        }
        for (i, step) in s.steps.iter().enumerate() {
            if !checks.contains(step.check.as_str()) {
                self.err(&file, format!("steps[{i}] 引用了不存在的检测：{}", step.check));
            }
            for fix in &step.fixes {
                if !features.contains(fix.as_str()) {
                    self.err(&file, format!("steps[{i}] 引用了不存在的功能：{fix}"));
                }
            }
        }
    }
}

/// 建好索引、可以查询的 catalog。
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub data: CatalogData,
    checks: HashMap<String, usize>,
    features: HashMap<String, usize>,
    symptoms: HashMap<String, usize>,
    profiles: HashMap<String, usize>,
}

impl Catalog {
    pub fn new(data: CatalogData) -> Self {
        fn index<T>(items: &[T], id: impl Fn(&T) -> &str) -> HashMap<String, usize> {
            items.iter().enumerate().map(|(i, t)| (id(t).to_owned(), i)).collect()
        }
        Self {
            checks: index(&data.checks, |c| &c.id),
            features: index(&data.features, |f| &f.id),
            symptoms: index(&data.symptoms, |s| &s.id),
            profiles: index(&data.profiles, |p| &p.id),
            data,
        }
    }

    pub fn check(&self, id: &str) -> Option<&Check> {
        self.checks.get(id).map(|&i| &self.data.checks[i])
    }

    pub fn feature(&self, id: &str) -> Option<&Feature> {
        self.features.get(id).map(|&i| &self.data.features[i])
    }

    pub fn symptom(&self, id: &str) -> Option<&Symptom> {
        self.symptoms.get(id).map(|&i| &self.data.symptoms[i])
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id).map(|&i| &self.data.profiles[i])
    }
}
