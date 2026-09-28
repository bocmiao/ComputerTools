//! 读取和校验 catalog/ 下的数据文件。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{
    Action, Check, Feature, Maturity, Profile, Recommend, RegistryAction, ResultSpec, Risk, Symptom, Target, Text,
    Tool, ToolGroup, Undo, UndoKeyword,
};
use crate::registry::{RegValue, SpecRoot, split_key};
use crate::yaml;

pub const SCHEMA_VERSION: u32 = 1;
pub const BUILTIN_PROBES: &[&str] =
    &["cpu-features", "clock", "keyboard-aids", "winsock", "display-resolution", "wifi-link"];

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
    #[serde(default)]
    pub tools: Vec<Tool>,
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
    load_kind(catalog_root, "tools", &mut data.tools, |t| &t.id, &mut data.sources, &mut problems);
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
    (r"HKLM\SYSTEM\CurrentControlSet\Services\mrxsmb10", "不开 SMB1（第五节第 22 条）"),
    // 更新：策略类的开关（禁用自动更新、断开更新服务、改更新源）一律不碰；plan 4.4 的「暂停更新」用系统自带的暂停方式
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate", "不通过策略禁用或限制更新（第五节第 5 条）"),
    (r"HKLM\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy", "不关防火墙（第五节第 22 条）"),
    (r"HKLM\SOFTWARE\Policies\Microsoft\WindowsFirewall", "不关防火墙（第五节第 22 条）"),
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows Defender Security Center", "不关安全中心（第五节第 4 条）"),
];

/// 不能往里写值、但可以删值的键：浏览器策略。写进去就是替用户锁主页、装扩展（第五节第 9 条）；
/// 删掉广告软件写进去的策略是 plan 4.3「浏览器被劫持」的修复，所以 `delete: true` 放行。
const WRITE_DENIED_KEYS: &[(&str, &str)] = &[
    (r"HKLM\SOFTWARE\Policies\Microsoft\Edge", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
    (r"HKCU\Software\Policies\Microsoft\Edge", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
    (r"HKLM\SOFTWARE\Policies\Google\Chrome", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
    (r"HKCU\Software\Policies\Google\Chrome", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
    (r"HKLM\SOFTWARE\Policies\Mozilla\Firefox", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
    (r"HKCU\Software\Policies\Mozilla\Firefox", "不改浏览器的主页、搜索和扩展（第五节第 9 条）"),
];
const DENIED_VALUES: &[(&str, &str, &str)] = &[
    (r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableLUA", "不关 UAC（第五节第 22 条）"),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
        "ConsentPromptBehaviorAdmin",
        "不关 UAC 提示（第五节第 22 条）",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
        "ConsentPromptBehaviorUser",
        "不关 UAC 提示（第五节第 22 条）",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
        "PromptOnSecureDesktop",
        "不关 UAC 提示（第五节第 22 条）",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer",
        "SmartScreenEnabled",
        "不关 SmartScreen（第五节第 22 条）",
    ),
    (r"HKCU\Software\Microsoft\Edge\SmartScreenEnabled", "", "不关 SmartScreen（第五节第 22 条）"),
    // 暂停更新只能用系统自带的暂停方式，不能直接写一个很远的到期时间
    (
        r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings",
        "PauseUpdatesExpiryTime",
        "不延长暂停更新（第五节第 26 条）",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings",
        "PauseFeatureUpdatesEndTime",
        "不延长暂停更新（第五节第 26 条）",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings",
        "PauseQualityUpdatesEndTime",
        "不延长暂停更新（第五节第 26 条）",
    ),
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU", "NoAutoUpdate", "不永久禁用更新（第五节第 5 条）"),
    (
        r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings",
        "FlightSettingsMaxPauseDays",
        "不延长暂停更新（第五节第 26 条）",
    ),
    (r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System", "EnableSmartScreen", "不关 SmartScreen（第五节第 22 条）"),
    // 开 SMB1 服务端是在 Parameters 键上写一个名叫 SMB1 的值，不是子键
    (r"HKLM\SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters", "SMB1", "不开 SMB1（第五节第 22 条）"),
];
const DENIED_SERVICES: &[(&str, &str)] = &[
    ("WinDefend", "不关 Defender（第五节第 4 条）"),
    ("wuauserv", "不禁用 Windows 更新（第五节第 5 条）"),
    ("UsoSvc", "不禁用 Windows 更新（第五节第 5 条）"),
    ("WaaSMedicSvc", "不禁用 Windows 更新（第五节第 5 条）"),
    ("mpssvc", "不关防火墙（第五节第 22 条）"),
    ("SecurityHealthService", "不关安全中心（第五节第 4 条）"),
    ("mrxsmb10", "不开 SMB1（第五节第 22 条）"),
];

/// 这些脚本类功能不写「制造故障」的脚本，通用的往返测试跳过它们，由 tests/windows.rs 里的专门测试来测：
/// - 要做的事正是第五节不许做的（禁用 Windows 更新服务），这种脚本不放进安装包；
/// - 没法安全地制造故障（重置 Winsock：得往测试机的 Winsock 里装一个 LSP）。
pub const BREAK_IN_TESTS: &[&str] = &["update.enable-services", "network.winsock-reset", "disk.remove-old-drivers"];

static CONTROL_SET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^hklm\\system\\controlset\d+\\").unwrap());

/// 黑名单比较用的键名：统一小写；`HKLM\SYSTEM\ControlSet001\…` 这类写法和
/// `CurrentControlSet` 指向同一处，统一成后者，免得换个写法就绕过去。
fn normalize_key(key: &str) -> String {
    CONTROL_SET_RE.replace(&key.to_ascii_lowercase(), r"hklm\system\currentcontrolset\").into_owned()
}

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
        let tool_ids = self.unique_ids("tools", data.tools.iter().map(|t| t.id.as_str()));
        let targets = LinkTargets { features: &feature_ids, symptoms: &symptom_ids, tools: &tool_ids };

        for c in &data.checks {
            self.check(c, &targets);
        }
        for t in &data.tools {
            self.tool(t, &targets);
        }
        for f in &data.features {
            self.feature(f, &check_ids);
        }
        for s in &data.symptoms {
            self.symptom(s, &check_ids, &feature_ids, &targets);
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

    fn check(&mut self, c: &Check, targets: &LinkTargets) {
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
        self.results(&file, &c.results, targets);
        for (name, labels) in &c.fact_labels {
            if !FACT_NAME_RE.is_match(name) || !FACT_NAME_RE.is_match(&labels.from) {
                self.err(&file, format!("fact_labels.{name} 的名字或 from 格式不对（只能用小写字母、数字和下划线）"));
            }
            if *name == labels.from {
                self.err(&file, format!("fact_labels.{name} 不能和它的 from 同名（会盖掉脚本返回的事实）"));
            }
            if labels.values.is_empty() {
                self.err(&file, format!("fact_labels.{name}.values 不能为空"));
            }
            for (value, text) in &labels.values {
                if value.trim().is_empty() {
                    self.err(&file, format!("fact_labels.{name}.values 里有空的值"));
                }
                self.text(&file, text, &format!("fact_labels.{name}.values.{value}"));
            }
        }
        if c.references.is_empty() {
            self.warn(&file, "最好写上 references（来源）".into());
        }
    }

    /// 检测和 info、action 小工具共用的 results 校验。
    fn results(&mut self, file: &str, results: &BTreeMap<String, ResultSpec>, targets: &LinkTargets) {
        if results.is_empty() {
            self.err(file, "results 不能为空".into());
        }
        for (code, spec) in results {
            if !ID_RE.is_match(code) {
                self.err(file, format!("结果代码格式不对：{code}"));
            }
            self.text(file, &spec.message, &format!("results.{code}.message"));
            self.placeholders(file, &spec.message, code);
            if let Some(next) = &spec.next {
                self.text(file, next, &format!("results.{code}.next"));
                self.placeholders(file, next, code);
            }
            for link in &spec.links {
                if !targets.contains(link) {
                    self.err(file, format!("results.{code}.links 里的链接无效：{link}"));
                }
            }
        }
    }

    fn tool(&mut self, t: &Tool, targets: &LinkTargets) {
        let file = self.file("tools", &t.id);
        self.common(&file, t.schema_version, &t.title, "title");
        self.text(&file, &t.description, "description");
        if t.category.trim().is_empty() {
            self.err(&file, "category 不能为空".into());
        }
        let labels_empty = t.labels.sections.is_empty() && t.labels.rows.is_empty() && t.labels.values.is_empty();
        match t.group {
            ToolGroup::Info | ToolGroup::Action => {
                match &t.run {
                    Some(run) => {
                        let run = run.script.clone();
                        self.script_ref(&file, &run, "run");
                    }
                    None => self.err(&file, "info、action 小工具必须写 run".into()),
                }
                if t.open.is_some() {
                    self.err(&file, "只有 open 小工具能写 open".into());
                }
                if t.timeout_sec == 0 || t.timeout_sec > 600 {
                    self.err(&file, "timeout_sec 要在 1 到 600 之间".into());
                }
                self.results(&file, &t.results, targets);
                if t.group == ToolGroup::Info {
                    if t.confirm.is_some() {
                        self.err(&file, "只有 action 小工具能写 confirm（info 只读，不用确认）".into());
                    }
                    for (kind, map) in
                        [("sections", &t.labels.sections), ("rows", &t.labels.rows), ("values", &t.labels.values)]
                    {
                        for (key, text) in map {
                            if !FACT_NAME_RE.is_match(key) && !ID_RE.is_match(key) {
                                self.err(&file, format!("labels.{kind} 的键格式不对：{key}"));
                            }
                            self.text(&file, text, &format!("labels.{kind}.{key}"));
                        }
                    }
                } else {
                    if !labels_empty {
                        self.err(&file, "只有 info 小工具能写 labels".into());
                    }
                    if let Some(c) = &t.confirm {
                        self.text(&file, c, "confirm");
                    }
                }
            }
            ToolGroup::Open => {
                if t.run.is_some() || !t.results.is_empty() || !labels_empty || t.confirm.is_some() {
                    self.err(&file, "open 小工具只写 open，不写 run、results、labels、confirm".into());
                }
                if t.requires_admin || t.user_hive {
                    self.err(&file, "open 小工具不跑脚本，不要写 requires_admin、user_hive".into());
                }
                match t.open.as_ref().map(|o| {
                    (o.program.as_deref(), o.settings.as_deref(), o.troubleshooter.as_deref(), o.website.as_deref())
                }) {
                    Some((Some(p), None, None, None)) => {
                        if crate::tools::program(p).is_none() {
                            let names: Vec<&str> = crate::tools::OPEN_PROGRAMS.iter().map(|(n, _)| *n).collect();
                            self.err(&file, format!("open.program 不在名单里：{p}（可用：{}）", names.join("、")));
                        }
                    }
                    Some((None, Some(page), None, None)) => {
                        if crate::tools::settings_page(page).is_none() {
                            self.err(
                                &file,
                                format!(
                                    "open.settings 不在名单里：{page}（可用：{}）",
                                    crate::tools::SETTINGS_PAGES.join("、")
                                ),
                            );
                        }
                    }
                    Some((None, None, Some(name), None)) => {
                        if crate::tools::troubleshooter(name).is_none() {
                            self.err(
                                &file,
                                format!(
                                    "open.troubleshooter 不在名单里：{name}（可用：{}）",
                                    crate::tools::TROUBLESHOOTERS.join("、")
                                ),
                            );
                        }
                    }
                    Some((None, None, None, Some(name))) => {
                        if crate::tools::website(name).is_none() {
                            self.err(
                                &file,
                                format!(
                                    "open.website 不在名单里：{name}（可用：{}）",
                                    crate::tools::WEBSITES.join("、")
                                ),
                            );
                        }
                    }
                    _ => self.err(&file, "open 必须且只能写 program、settings、troubleshooter、website 之一".into()),
                }
            }
        }
        if t.references.is_empty() {
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
                if f.prepare.is_some() {
                    self.err(&file, "原语类功能不要写 prepare".into());
                }
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
                match (&f.prepare, f.reversible()) {
                    (Some(p), true) => self.script_ref(&file, &p.script, "prepare"),
                    (None, true) => self.err(&file, "可撤销的脚本类功能必须写 prepare".into()),
                    (Some(_), false) => self.err(&file, "不能撤销的脚本类功能不要写 prepare".into()),
                    (None, false) => {}
                }
                // detect 用在两处：没有 verify 时判断状态，撤销前核对修改还在不在。有 verify、又撤销不了的用不着它
                match &f.detect {
                    Some(d) => {
                        let d = d.script.clone();
                        self.script_ref(&file, &d, "detect");
                    }
                    None if f.verify.is_some() && !f.reversible() => {}
                    None => self.err(&file, "脚本类功能必须写 detect（撤销不了、写了 verify 的除外）".into()),
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
                        if BREAK_IN_TESTS.contains(&f.id.as_str()) {
                            self.err(
                                &file,
                                "这一项的故障由测试代码制造，不要写 break 脚本（见 BREAK_IN_TESTS）".into(),
                            );
                        }
                        let b = b.script.clone();
                        self.script_ref(&file, &b, "break");
                    }
                    None if BREAK_IN_TESTS.contains(&f.id.as_str()) => {}
                    None => self.warn(&file, "最好写上 break（测试用的故障制造脚本）".into()),
                }
                if !f.break_actions.is_empty() || !f.windows_default.is_empty() {
                    self.err(&file, "脚本类功能不要写 break_actions / windows_default".into());
                }
            }
            (false, Some(_)) => self.err(&file, "actions 和 run 只能二选一".into()),
            (true, None) => self.err(&file, "必须写 actions 或 run".into()),
        }

        // 计划书 6.2：「只应用推荐项」会一次改好几项，所以推荐的只能是安全、能撤销的
        if f.recommend == Recommend::Recommended && (f.risk != Risk::Safe || !f.reversible()) {
            self.err(&file, "recommend 为 recommended 的功能必须是 risk: safe 而且能撤销".into());
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
                // HKU\.DEFAULT（登录界面用的那一份用户设置）不属于哪个用户，和 HKLM 一样算整台电脑的
                let (fits, want) = match target {
                    Target::CurrentUser => (root == SpecRoot::Hkcu, "HKCU\\"),
                    Target::Machine => (root != SpecRoot::Hkcu, "HKLM\\ 或 HKU\\.DEFAULT\\"),
                };
                if !fits {
                    self.err(file, format!("{at}：target 和注册表根对不上，这个功能只能写 {want}"));
                }
            }
            Err(e) => self.err(file, format!("{at}：{e}")),
        }
        let key_lower = normalize_key(&r.key);
        for (denied, why) in DENIED_KEYS {
            let d = normalize_key(denied);
            if key_lower == d || key_lower.starts_with(&format!("{d}\\")) {
                self.err(file, format!("{at}：不允许写 {}：{why}", r.key));
            }
        }
        for (k, n, why) in DENIED_VALUES {
            if key_lower == normalize_key(k) && r.name.eq_ignore_ascii_case(n) {
                self.err(file, format!("{at}：不允许写 {}\\{}：{why}", r.key, r.name));
            }
        }
        // 黑名单里的服务，直接写它的注册表（比如 Start = 4）和用 service 原语是一回事
        for (name, why) in DENIED_SERVICES {
            let d = normalize_key(&format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{name}"));
            if key_lower == d || key_lower.starts_with(&format!("{d}\\")) {
                self.err(file, format!("{at}：不允许改服务 {name} 的注册表：{why}"));
            }
        }
        if !r.delete {
            for (denied, why) in WRITE_DENIED_KEYS {
                let d = normalize_key(denied);
                if key_lower == d || key_lower.starts_with(&format!("{d}\\")) {
                    self.err(file, format!("{at}：不允许往 {} 里写值（只能删）：{why}", r.key));
                }
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

    fn symptom(&mut self, s: &Symptom, checks: &HashSet<&str>, features: &HashSet<&str>, targets: &LinkTargets) {
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
        if !s.links.is_empty() && s.guide.is_none() {
            self.err(&file, "写了 links（手动步骤下面的按钮）就要写 guide".into());
        }
        let mut seen = HashSet::new();
        for link in &s.links {
            if link.starts_with("feature:") {
                self.err(
                    &file,
                    format!("症状的 links 只能是 tool:、symptom: 或 test:，修复写在 steps 的 fixes 里：{link}"),
                );
            } else if !targets.contains(link) {
                self.err(&file, format!("links 里的链接无效：{link}"));
            }
            if *link == format!("symptom:{}", s.id) {
                self.err(&file, format!("links 不能指向自己：{link}"));
            }
            if !seen.insert(link.as_str()) {
                self.err(&file, format!("links 里有重复的链接：{link}"));
            }
        }
    }
}

/// 检测结果的 links 能指向的对象。
struct LinkTargets<'a> {
    features: &'a HashSet<&'a str>,
    symptoms: &'a HashSet<&'a str>,
    tools: &'a HashSet<&'a str>,
}

impl LinkTargets<'_> {
    fn contains(&self, link: &str) -> bool {
        match link.split_once(':') {
            Some(("symptom", id)) => self.symptoms.contains(id),
            Some(("feature", id)) => self.features.contains(id),
            Some(("tool", id)) => self.tools.contains(id),
            Some(("test", id)) => crate::tools::DEVICE_TESTS.contains(&id),
            _ => false,
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
    tools: HashMap<String, usize>,
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
            tools: index(&data.tools, |t| &t.id),
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

    pub fn tool(&self, id: &str) -> Option<&Tool> {
        self.tools.get(id).map(|&i| &self.data.tools[i])
    }
}
