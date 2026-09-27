//! 引擎：检测、功能的检测 / 预览 / 执行、修改日志、撤销、报告、小工具。
//!
//! 关键不变量：
//! - 每个原语改动前先把原值写进修改日志（apply），改完再写结果（commit）；
//! - 一个功能里的原语是一个整体，中途失败就把已改的按倒序退回；
//! - 撤销前先核对当前值是否还是当初写进去的值，不是就提示「被改过」，由用户决定。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value};

use crate::builtin;
use crate::catalog::Catalog;
use crate::error::{Error, Result};
use crate::journal::{
    ApplyRecord, CommitRecord, Entry, Journal, RECORD_VERSION, Record, State, TargetRef, UndoReason, UndoRecord,
    new_id, now_rfc3339,
};
use crate::model::{
    Action, Check, Feature, RegType, RegistryAction, Risk, StartType, Status, Symptom, Target, Tool, ToolGroup, Undo,
};
use crate::platform::{OpenRequest, Platform, PlatformError};
use crate::registry::{RegRoot, RegValue, SpecRoot, display_opt, is_sid, key_ancestors, split_key};
use crate::render::render;
use crate::report::redact;
use crate::script::ScriptRunner;
use crate::startup;
use crate::tools;
use crate::views::{
    ApplyResult, CatalogSummary, CheckResult, FeatureState, FeatureStateKind, FeatureSummary, JournalEntryView,
    JournalSession, Preview, PreviewChange, ProfileSummary, StartupItem, SymptomDetail, SymptomStep, SymptomSummary,
    SystemInfo, ToolOpens, ToolResult, ToolSummary, UndoResult,
};

const SCRIPT_FEATURE_TIMEOUT: Duration = Duration::from_secs(300);
/// Windows 11 的第一个版本号
const WIN11_BUILD: u32 = 22000;
const RESTORE_POINT_TIMEOUT: Duration = Duration::from_secs(300);
pub const RESTORE_POINT_SCRIPT: &str = "host/restore-point.ps1";
/// 旧的做法（删掉 Run 值来停用启动项）留在修改日志里的记录，仍然按原样显示、可以撤销
const LEGACY_STARTUP_FEATURE: &str = "boot.startup-disable";
/// 列启动项要查每个程序的签名，大文件慢；脚本自己有时间上限，这里再留些余量
const STARTUP_LIST_TIMEOUT: Duration = Duration::from_secs(90);

/// 一个原语没改成功。
struct StepError {
    /// 已经写进修改日志的 apply 记录（没走到那一步时为空）
    entry: Option<String>,
    error: Error,
    /// 退回以后读回来一看，系统还没回到原样
    left_changes: bool,
}

impl StepError {
    fn early(error: Error) -> Self {
        Self { entry: None, error, left_changes: false }
    }
}

/// 原语的目标状态。
enum Desired {
    Value(RegValue),
    Absent,
    Start(StartType),
}

impl Desired {
    fn of(a: &Action) -> Result<Self> {
        Ok(match a {
            Action::Registry(r) if r.delete => Self::Absent,
            Action::Registry(r) => {
                let (Some(t), Some(v)) = (r.value_type, r.value.as_ref()) else {
                    return Err(Error::Catalog(format!("{} 缺少 type 或 value", r.key)));
                };
                Self::Value(RegValue::from_spec(t, v).map_err(Error::Catalog)?)
            }
            Action::Service(s) => Self::Start(s.start_type),
        })
    }

    fn display(&self) -> String {
        match self {
            Self::Value(v) => v.to_string(),
            Self::Absent => "（删除这个值）".to_owned(),
            Self::Start(st) => st.label().to_owned(),
        }
    }
}

pub struct Engine {
    catalog: Catalog,
    catalog_version: String,
    app_version: String,
    platform: Arc<dyn Platform>,
    runner: Arc<dyn ScriptRunner>,
    journal: Journal,
    session: String,
    lang: String,
    /// 同一时间只执行一个修改
    apply_lock: Mutex<()>,
    /// 最近一次跑检测清单的结果，写报告用
    last_results: Mutex<Vec<CheckResult>>,
    /// 最近一次跑检测清单的时间
    last_profile_time: Mutex<Option<String>>,
    /// 不在体检里、单独查过的检测（例如「按症状修」里查的），报告里单列
    other_results: Mutex<Vec<CheckResult>>,
    /// 本机时区。启动时读一次：之后多线程时，有的系统上读不到
    local_offset: Option<time::UtcOffset>,
    /// 最近一次列出的开机启动项：改开关时只认这里面有的
    startup_items: Mutex<Vec<startup::RawItem>>,
}

impl Engine {
    pub fn new(
        catalog: Catalog,
        catalog_version: impl Into<String>,
        app_version: impl Into<String>,
        platform: Arc<dyn Platform>,
        runner: Arc<dyn ScriptRunner>,
        journal: Journal,
    ) -> Self {
        Self {
            catalog,
            catalog_version: catalog_version.into(),
            app_version: app_version.into(),
            platform,
            runner,
            journal,
            session: new_id(),
            lang: "zh-CN".to_owned(),
            apply_lock: Mutex::new(()),
            last_results: Mutex::new(Vec::new()),
            last_profile_time: Mutex::new(None),
            other_results: Mutex::new(Vec::new()),
            local_offset: time::UtcOffset::current_local_offset().ok(),
            startup_items: Mutex::new(Vec::new()),
        }
    }

    /// 报告里显示的时间：换成本机时间（读不到时区时注明 UTC）。
    fn local_time(&self, rfc3339: &str) -> String {
        let Ok(t) = time::OffsetDateTime::parse(rfc3339, &time::format_description::well_known::Rfc3339) else {
            return rfc3339.to_owned();
        };
        let fmt = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]");
        match self.local_offset {
            Some(o) => t.to_offset(o).format(&fmt).unwrap_or_else(|_| rfc3339.to_owned()),
            None => format!("{}（UTC）", t.to_offset(time::UtcOffset::UTC).format(&fmt).unwrap_or_default()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    // ───────────── 系统信息与目录 ─────────────

    pub fn system_info(&self) -> SystemInfo {
        let os = self.platform.os_info();
        let interactive = self.platform.interactive_user();
        let process = self.platform.process_user();
        let mismatch = matches!((&interactive, &process), (Some(a), Some(b)) if a.sid != b.sid);
        SystemInfo {
            os_caption: os.caption,
            build: os.build,
            edition: os.edition_id,
            is_admin: self.platform.is_admin(),
            interactive_user: interactive.map(|u| u.name),
            elevated_user_mismatch: mismatch,
            app_version: self.app_version.clone(),
            catalog_version: self.catalog_version.clone(),
        }
    }

    pub fn catalog_summary(&self) -> CatalogSummary {
        let d = &self.catalog.data;
        CatalogSummary {
            profiles: d
                .profiles
                .iter()
                .map(|p| ProfileSummary {
                    id: p.id.clone(),
                    title: p.title.get(&self.lang).to_owned(),
                    check_count: p.checks.len(),
                })
                .collect(),
            symptoms: d.symptoms.iter().map(|s| self.symptom_summary(s)).collect(),
            features: d.features.iter().map(|f| self.feature_summary(f)).collect(),
            tools: d.tools.iter().map(|t| self.tool_summary(t)).collect(),
        }
    }

    fn symptom_summary(&self, s: &Symptom) -> SymptomSummary {
        SymptomSummary {
            id: s.id.clone(),
            title: s.title.get(&self.lang).to_owned(),
            summary: s.summary.as_ref().map(|t| t.get(&self.lang).to_owned()),
            keywords: s.keywords.clone(),
            maturity: s.maturity,
        }
    }

    fn feature_summary(&self, f: &Feature) -> FeatureSummary {
        let reason = self.applicability(f);
        FeatureSummary {
            id: f.id.clone(),
            title: f.title.get(&self.lang).to_owned(),
            description: f.description.get(&self.lang).to_owned(),
            category: f.category.clone(),
            risk: f.risk,
            level: f.level,
            recommend: f.recommend,
            subjective: f.subjective,
            reboot: f.reboot,
            reversible: f.reversible(),
            irreversible_reason: f.irreversible_reason.as_ref().map(|t| t.get(&self.lang).to_owned()),
            applicable: reason.is_none(),
            not_applicable_reason: reason,
        }
    }

    pub fn symptom_detail(&self, id: &str) -> Result<SymptomDetail> {
        let s = self.catalog.symptom(id).ok_or_else(|| Error::not_found("症状", id))?;
        let steps = s
            .steps
            .iter()
            .map(|step| SymptomStep {
                check: step.check.clone(),
                check_title: self
                    .catalog
                    .check(&step.check)
                    .map_or_else(|| step.check.clone(), |c| c.title.get(&self.lang).to_owned()),
                stop_on: step.stop_on.clone(),
                fixes: step
                    .fixes
                    .iter()
                    .filter_map(|f| self.catalog.feature(f))
                    .map(|f| self.feature_summary(f))
                    .collect(),
            })
            .collect();
        Ok(SymptomDetail {
            summary: self.symptom_summary(s),
            causes: s.causes.iter().map(|t| t.get(&self.lang).to_owned()).collect(),
            guide: s.guide.as_ref().map(|t| t.get(&self.lang).to_owned()),
            steps,
        })
    }

    // ───────────── 检测 ─────────────

    /// 登录用户的注册表根，传给脚本的 `-UserHive`。
    fn user_hive(&self) -> String {
        match self.platform.interactive_user() {
            Some(u) if is_sid(&u.sid) => format!("Registry::HKEY_USERS\\{}", u.sid),
            _ => "HKCU:".to_owned(),
        }
    }

    pub fn run_check(&self, id: &str) -> Result<CheckResult> {
        let check = self.catalog.check(id).ok_or_else(|| Error::not_found("检测", id))?;
        let r = self.run_check_inner(check);
        // 修完、撤销完再查一次时，报告里的体检结果也要跟着更新；不在体检里的单独记下
        if let Some(slot) = self.last_results.lock().unwrap().iter_mut().find(|x| x.id == r.id) {
            *slot = r.clone();
        } else {
            let mut others = self.other_results.lock().unwrap();
            others.retain(|x| x.id != r.id);
            others.push(r.clone());
        }
        Ok(r)
    }

    fn run_check_inner(&self, check: &Check) -> CheckResult {
        let start = Instant::now();
        let outcome: std::result::Result<Value, String> = if check.requires_admin && !self.platform.is_admin() {
            Err("这一项需要管理员权限".to_owned())
        } else if let Some(script) = &check.probe.script {
            let mut args = Map::new();
            if check.user_hive {
                args.insert("UserHive".into(), Value::String(self.user_hive()));
            }
            self.runner.run(script, &args, Duration::from_secs(check.timeout_sec.into())).map_err(|e| e.to_string())
        } else if let Some(name) = &check.probe.builtin {
            let now = time::OffsetDateTime::now_utc().to_offset(self.local_offset.unwrap_or(time::UtcOffset::UTC));
            builtin::run(name, &builtin::Env { os: &self.platform.os_info(), now })
        } else {
            Err("没有检测方式".to_owned())
        };
        let elapsed = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        self.interpret(check, outcome, elapsed)
    }

    fn interpret(&self, check: &Check, outcome: std::result::Result<Value, String>, ms: u64) -> CheckResult {
        let mut result = CheckResult {
            id: check.id.clone(),
            title: check.title.get(&self.lang).to_owned(),
            category: check.category.clone(),
            status: Status::Unknown,
            result_code: None,
            message: "这一项没查出来。".to_owned(),
            fixer: None,
            next: None,
            links: Vec::new(),
            facts: Map::new(),
            error: None,
            duration_ms: ms,
        };
        match outcome {
            Err(e) => result.error = Some(e),
            Ok(v) => {
                let code = v.get("result").and_then(Value::as_str).map(str::to_owned);
                result.facts = v.get("facts").and_then(Value::as_object).cloned().unwrap_or_default();
                match code.as_deref().and_then(|c| check.results.get(c)) {
                    Some(spec) => {
                        result.status = spec.status;
                        result.message = render(spec.message.get(&self.lang), &result.facts);
                        result.fixer = spec.fixer;
                        result.next = spec.next.as_ref().map(|t| render(t.get(&self.lang), &result.facts));
                        result.links = spec.links.clone();
                    }
                    None => {
                        result.error =
                            Some(format!("脚本返回了没有定义的结果：{}", code.as_deref().unwrap_or("（空）")));
                    }
                }
                result.result_code = code;
            }
        }
        result
    }

    pub fn run_profile(&self, id: &str) -> Result<Vec<CheckResult>> {
        let profile = self.catalog.profile(id).ok_or_else(|| Error::not_found("检测清单", id))?;
        let results: Vec<CheckResult> =
            profile.checks.iter().filter_map(|c| self.catalog.check(c)).map(|c| self.run_check_inner(c)).collect();
        self.other_results.lock().unwrap().retain(|x| results.iter().all(|r| r.id != x.id));
        *self.last_results.lock().unwrap() = results.clone();
        *self.last_profile_time.lock().unwrap() = Some(now_rfc3339());
        Ok(results)
    }

    // ───────────── 小工具 ─────────────
    //
    // 小工具不改设置（会改设置的一律做成功能），所以不写修改日志、没有撤销。

    fn tool(&self, id: &str) -> Result<&Tool> {
        self.catalog.tool(id).ok_or_else(|| Error::not_found("小工具", id))
    }

    fn tool_summary(&self, t: &Tool) -> ToolSummary {
        ToolSummary {
            id: t.id.clone(),
            title: t.title.get(&self.lang).to_owned(),
            description: t.description.get(&self.lang).to_owned(),
            category: t.category.clone(),
            group: t.group,
            opens: t.open.as_ref().map(|o| if o.program.is_some() { ToolOpens::Program } else { ToolOpens::Settings }),
            audience: t.audience,
            confirm: t.confirm.as_ref().map(|c| c.get(&self.lang).to_owned()),
        }
    }

    /// 运行一个 info 或 action 小工具。
    pub fn tool_run(&self, id: &str) -> Result<ToolResult> {
        let tool = self.tool(id)?;
        let Some(run) = &tool.run else {
            return Err(Error::Invalid(format!("「{}」不用运行，直接打开就行", tool.title.get(&self.lang))));
        };
        let start = Instant::now();
        let outcome = if tool.requires_admin && !self.platform.is_admin() {
            Err("这一项需要管理员权限".to_owned())
        } else {
            let mut args = Map::new();
            if tool.user_hive {
                args.insert("UserHive".into(), Value::String(self.user_hive()));
            }
            self.runner.run(&run.script, &args, Duration::from_secs(tool.timeout_sec.into())).map_err(|e| e.to_string())
        };
        let elapsed = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        Ok(self.interpret_tool(tool, outcome, elapsed))
    }

    fn interpret_tool(&self, tool: &Tool, outcome: std::result::Result<Value, String>, ms: u64) -> ToolResult {
        let failed = if tool.group == ToolGroup::Info { "没能读出来。" } else { "没能完成。" };
        let mut result = ToolResult {
            id: tool.id.clone(),
            title: tool.title.get(&self.lang).to_owned(),
            status: Status::Unknown,
            result_code: None,
            message: failed.to_owned(),
            next: None,
            links: Vec::new(),
            sections: Vec::new(),
            error: None,
            duration_ms: ms,
        };
        match outcome {
            Err(e) => result.error = Some(e),
            Ok(v) => {
                let code = v.get("result").and_then(Value::as_str).map(str::to_owned);
                let facts = v.get("facts").and_then(Value::as_object).cloned().unwrap_or_default();
                match code.as_deref().and_then(|c| tool.results.get(c)) {
                    Some(spec) => {
                        result.status = spec.status;
                        result.message = render(spec.message.get(&self.lang), &facts);
                        result.next = spec.next.as_ref().map(|t| render(t.get(&self.lang), &facts));
                        result.links = spec.links.clone();
                    }
                    None => {
                        result.error =
                            Some(format!("脚本返回了没有定义的结果：{}", code.as_deref().unwrap_or("（空）")));
                    }
                }
                if tool.group == ToolGroup::Info {
                    let mut missing = Vec::new();
                    result.sections = tools::render_sections(v.get("sections"), &tool.labels, &self.lang, &mut missing);
                    if !missing.is_empty() && result.error.is_none() {
                        result.error =
                            Some(format!("表格里有对不上的地方（脚本或数据文件要改）：{}", missing.join("、")));
                    }
                }
                result.result_code = code;
            }
        }
        result
    }

    /// 打开一个 open 小工具：系统自带的工具，或者「设置」里的一页。
    pub fn tool_open(&self, id: &str) -> Result<()> {
        let tool = self.tool(id)?;
        let title = tool.title.get(&self.lang);
        let request = match tool.open.as_ref().map(|o| (o.program.as_deref(), o.settings.as_deref())) {
            Some((Some(name), None)) => {
                let p = tools::program(name)
                    .ok_or_else(|| Error::Catalog(format!("{id} 的 open.program 不在名单里：{name}")))?;
                OpenRequest::Program { exe: p.exe, args: p.args }
            }
            Some((None, Some(page))) => OpenRequest::Settings(
                tools::settings_page(page)
                    .ok_or_else(|| Error::Catalog(format!("{id} 的 open.settings 不在名单里：{page}")))?,
            ),
            _ => return Err(Error::Invalid(format!("「{title}」不是用来打开的工具"))),
        };
        self.platform.open(&request).map_err(|e| match e {
            PlatformError::NotFound(file) => {
                Error::Invalid(format!("这台电脑上没有「{title}」（找不到 {file}），可能被精简系统删掉了。"))
            }
            PlatformError::Other(msg) => Error::Invalid(format!("没能打开「{title}」：{msg}")),
            other => Error::Invalid(format!("没能打开「{title}」：{other}")),
        })
    }

    // ───────────── 功能：检测、预览 ─────────────

    fn feature(&self, id: &str) -> Result<&Feature> {
        self.catalog.feature(id).ok_or_else(|| Error::not_found("功能", id))
    }

    /// 不适用于这台电脑的原因。
    fn applicability(&self, f: &Feature) -> Option<String> {
        let os = self.platform.os_info();
        // 说人话：最常见的是「只有 Win11 有」「只有 Win10 有」，其余的才报版本号
        if let Some(min) = f.applies_to.min_build
            && os.build < min
        {
            return Some(if min == WIN11_BUILD && os.build < WIN11_BUILD {
                "只适用于 Windows 11，这台电脑装的是 Windows 10".to_owned()
            } else {
                format!("要先把系统更新到版本号 {min} 或更新（这台是 {}）", os.build)
            });
        }
        if let Some(max) = f.applies_to.max_build
            && os.build > max
        {
            return Some(if max < WIN11_BUILD && os.build >= WIN11_BUILD {
                "只适用于 Windows 10，这台电脑装的是 Windows 11".to_owned()
            } else {
                format!("只适用于版本号 {max} 及以前的系统（这台是 {}）", os.build)
            });
        }
        if !f.applies_to.editions.is_empty() && !os.edition.is_some_and(|e| f.applies_to.editions.contains(&e)) {
            return Some(format!("不适用于这个 Windows 版本（{}）", os.edition_id));
        }
        None
    }

    fn resolve_registry(&self, r: &RegistryAction) -> Result<(RegRoot, String)> {
        let (spec_root, sub) = split_key(&r.key).map_err(Error::Catalog)?;
        let root = match spec_root {
            SpecRoot::Hklm => RegRoot::LocalMachine,
            SpecRoot::Hkcu => match self.platform.interactive_user() {
                Some(u) if is_sid(&u.sid) => RegRoot::User(u.sid),
                _ => RegRoot::CurrentUser,
            },
        };
        Ok((root, sub.to_owned()))
    }

    fn target_label(target: &TargetRef) -> String {
        match target {
            TargetRef::Registry { root, key, name } => {
                let root = match root {
                    RegRoot::LocalMachine => "HKLM".to_owned(),
                    RegRoot::CurrentUser => "HKCU（当前账户）".to_owned(),
                    RegRoot::User(_) => "HKCU（登录用户）".to_owned(),
                };
                let name = if name.is_empty() { "（默认值）" } else { name };
                format!("{root}\\{key} → {name}")
            }
            TargetRef::Service { name } => format!("服务 {name} 的启动类型"),
            // 界面上功能名是单独显示的，这里不再带功能 ID
            TargetRef::Script { .. } => "小药箱脚本改的设置".to_owned(),
        }
    }

    fn state_label(state: &State) -> String {
        match state {
            State::Registry { value, .. } => display_opt(value.as_ref()),
            State::Service { start_type } => start_type.map_or("（服务不存在）", StartType::label).to_owned(),
            State::Script { data } => match data {
                Value::Null => "（无）".to_owned(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
        }
    }

    /// 读一个原语的目标位置现在的状态。
    fn current(&self, a: &Action) -> Result<(TargetRef, State)> {
        match a {
            Action::Registry(r) => {
                let (root, key) = self.resolve_registry(r)?;
                let value = self.platform.reg_get(&root, &key, &r.name)?;
                Ok((
                    TargetRef::Registry { root, key, name: r.name.clone() },
                    State::Registry { value, created_keys: Vec::new() },
                ))
            }
            Action::Service(s) => {
                let start_type = self.platform.service_get(&s.name)?;
                Ok((TargetRef::Service { name: s.name.clone() }, State::Service { start_type }))
            }
        }
    }

    fn matches(desired: &Desired, state: &State) -> bool {
        match (desired, state) {
            (Desired::Value(v), State::Registry { value, .. }) => value.as_ref() == Some(v),
            (Desired::Absent, State::Registry { value, .. }) => value.is_none(),
            (Desired::Start(st), State::Service { start_type }) => *start_type == Some(*st),
            _ => false,
        }
    }

    /// `hive` 为空时用现在的登录用户；撤销时传入执行那一刻记下的值。
    fn script_args(&self, f: &Feature, hive: Option<&str>) -> Map<String, Value> {
        let mut args = Map::new();
        if f.target == Target::CurrentUser {
            let hive = hive.map_or_else(|| self.user_hive(), str::to_owned);
            args.insert("UserHive".into(), Value::String(hive));
        }
        args
    }

    pub fn feature_detect(&self, id: &str) -> Result<FeatureState> {
        let f = self.feature(id)?;
        Ok(match self.detect_inner(f) {
            Ok((state, details)) => FeatureState { id: f.id.clone(), state, details, error: None },
            Err(e) => FeatureState {
                id: f.id.clone(),
                state: FeatureStateKind::Unknown,
                details: Vec::new(),
                error: Some(e.to_string()),
            },
        })
    }

    fn detect_inner(&self, f: &Feature) -> Result<(FeatureStateKind, Vec<String>)> {
        if let Some(check_id) = &f.verify {
            let check = self.catalog.check(check_id).ok_or_else(|| Error::not_found("检测", check_id))?;
            let r = self.run_check_inner(check);
            let state = match r.status {
                Status::Ok | Status::Na => FeatureStateKind::Applied,
                Status::Advice | Status::Manual => FeatureStateKind::NotApplied,
                Status::Unknown => FeatureStateKind::Unknown,
            };
            return Ok((state, vec![r.message]));
        }
        if !f.is_primitive() {
            let detect = f.detect.as_ref().ok_or_else(|| Error::Catalog(format!("{} 没有 detect", f.id)))?;
            let v = self.runner.run(&detect.script, &self.script_args(f, None), SCRIPT_FEATURE_TIMEOUT)?;
            let state = FeatureStateKind::parse(v.get("state").and_then(Value::as_str).unwrap_or(""));
            let details = v
                .get("facts")
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| format!("{k}：{}", crate::render::format_fact(v))).collect())
                .unwrap_or_default();
            return Ok((state, details));
        }
        let mut applied = 0;
        let mut details = Vec::new();
        for a in &f.actions {
            let desired = Desired::of(a)?;
            let (target, state) = self.current(a)?;
            let ok = Self::matches(&desired, &state);
            applied += usize::from(ok);
            details.push(format!(
                "{}：现在是 {}，目标是 {}",
                Self::target_label(&target),
                Self::state_label(&state),
                desired.display()
            ));
        }
        let state = match applied {
            0 => FeatureStateKind::NotApplied,
            n if n == f.actions.len() => FeatureStateKind::Applied,
            _ => FeatureStateKind::Partial,
        };
        Ok((state, details))
    }

    pub fn feature_preview(&self, id: &str) -> Result<Preview> {
        let f = self.feature(id)?;
        let mut notes = Vec::new();
        if let Some(reason) = self.applicability(f) {
            notes.push(format!("不能执行：{reason}"));
        }
        let changes = if f.is_primitive() {
            f.actions
                .iter()
                .map(|a| {
                    let desired = Desired::of(a)?;
                    let (target, state) = self.current(a)?;
                    Ok(PreviewChange {
                        target: Self::target_label(&target),
                        current: Self::state_label(&state),
                        planned: desired.display(),
                    })
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            // 脚本类功能没有逐个位置可列，用它自己的检测说明现在的状态；要改什么见功能说明
            let current = match self.detect_inner(f) {
                Ok((FeatureStateKind::Applied, _)) => "已经是这样了",
                Ok((FeatureStateKind::NotApplied, _)) => "还没改",
                Ok((FeatureStateKind::Partial, _)) => "改了一部分",
                _ => "没查出来",
            };
            vec![PreviewChange {
                target: "由小药箱的脚本完成".to_owned(),
                current: current.to_owned(),
                planned: "按上面的说明修改".to_owned(),
            }]
        };
        if f.target == Target::CurrentUser {
            match (self.platform.interactive_user(), self.platform.process_user()) {
                (None, _) => notes.push("没找到登录用户，改动会写到运行本程序的账户上。".to_owned()),
                (Some(a), Some(b)) if a.sid != b.sid => {
                    notes.push(format!("你是用别的管理员账户运行的；改动会写到登录用户 {} 身上。", a.name))
                }
                _ => {}
            }
        }
        match f.reboot {
            crate::model::Reboot::None => {}
            crate::model::Reboot::Explorer => notes.push("改完要重启资源管理器（或注销）才能看到效果。".to_owned()),
            crate::model::Reboot::Logoff => notes.push("改完要注销再登录才生效。".to_owned()),
            crate::model::Reboot::Reboot => notes.push("改完要重启电脑才生效。".to_owned()),
        }
        if !f.reversible() {
            let why = f.irreversible_reason.as_ref().map(|t| t.get(&self.lang).to_owned()).unwrap_or_default();
            notes.push(format!("这一项改了就不能撤销：{why}"));
        }
        Ok(Preview {
            feature: self.feature_summary(f),
            changes,
            will_create_restore_point: f.risk >= Risk::Caution,
            notes,
        })
    }

    // ───────────── 功能：执行 ─────────────

    pub fn feature_apply(&self, id: &str) -> Result<ApplyResult> {
        let f = self.feature(id)?;
        if let Some(reason) = self.applicability(f) {
            return Err(Error::NotApplicable(reason));
        }
        let _guard = self.apply_lock.lock().unwrap();
        // 本来就是好的：不建还原点，也不写修改日志
        if let Ok((FeatureStateKind::Applied, _)) = self.detect_inner(f) {
            let mut r = self.result(f, true, Vec::new(), "这一项本来就是好的，不用改。".to_owned(), Vec::new());
            r.verified = FeatureStateKind::Applied;
            r.reboot = crate::model::Reboot::None;
            return Ok(r);
        }
        let mut notes = Vec::new();
        if f.risk >= Risk::Caution {
            notes.push(self.create_restore_point(f));
        }
        if f.is_primitive() { self.apply_primitives(f, notes) } else { self.apply_script(f, notes) }
    }

    fn create_restore_point(&self, f: &Feature) -> String {
        let mut args = Map::new();
        args.insert("Description".into(), Value::String(format!("medkit: {}", f.id)));
        match self.runner.run(RESTORE_POINT_SCRIPT, &args, RESTORE_POINT_TIMEOUT) {
            Ok(v) if v.get("result").and_then(Value::as_str) == Some("created") => "已经创建系统还原点。".to_owned(),
            Ok(_) => "24 小时内已经建过还原点，这次没有再建（Windows 的限制）；修改日志仍然可以撤销。".to_owned(),
            Err(e) => format!("没能创建还原点（{e}）；修改日志仍然可以撤销。"),
        }
    }

    fn result(
        &self,
        f: &Feature,
        ok: bool,
        entry_ids: Vec<String>,
        message: String,
        notes: Vec<String>,
    ) -> ApplyResult {
        ApplyResult {
            feature: f.id.clone(),
            session_id: self.session.clone(),
            entry_ids,
            ok,
            verified: FeatureStateKind::Unknown,
            message,
            // 没改成（已经退回）就不用重启
            reboot: if ok { f.reboot } else { crate::model::Reboot::None },
            notes,
            error: None,
        }
    }

    fn apply_primitives(&self, f: &Feature, notes: Vec<String>) -> Result<ApplyResult> {
        let mut done: Vec<(ApplyRecord, State)> = Vec::new();
        let mut entry_ids = Vec::new();
        for (i, a) in f.actions.iter().enumerate() {
            // 已经是目标状态的就不动，撤销时也就不会碰它
            if let (Ok(desired), Ok((_, state))) = (Desired::of(a), self.current(a))
                && Self::matches(&desired, &state)
            {
                continue;
            }
            match self.apply_one(&f.id, i, a) {
                Ok((rec, after)) => {
                    entry_ids.push(rec.id.clone());
                    done.push((rec, after));
                }
                Err(step) => {
                    entry_ids.extend(step.entry);
                    // 不信返回值，退完以后读回来和原值比：一致才算退干净了
                    let mut rolled_back = !step.left_changes;
                    for (rec, after) in done.iter().rev() {
                        let _ = self.revert(rec, Some(after), UndoReason::Rollback, true);
                        if self.is_back(rec) != Some(true) {
                            rolled_back = false;
                        }
                    }
                    let message = if rolled_back {
                        "没有改成功，已经把这次改过的部分退回原样。".to_owned()
                    } else {
                        "没有改成功，而且有部分改动没能自动退回，请在修改日志里手动恢复。".to_owned()
                    };
                    let mut r = self.result(f, false, entry_ids, message, notes);
                    r.error = Some(step.error.to_string());
                    return Ok(r);
                }
            }
        }
        let mut r = self.result(f, true, entry_ids, String::new(), notes);
        r.verified = self.detect_inner(f).map_or(FeatureStateKind::Unknown, |(s, _)| s);
        r.message = match r.verified {
            FeatureStateKind::Applied => "已经改好了。".to_owned(),
            _ => "改完了，但复查时发现没有完全生效。".to_owned(),
        };
        Ok(r)
    }

    /// 执行一个原语：先写 apply（原值），再改，再写 commit（结果）。
    /// 失败时返回（已写入的 apply 记录 ID，错误）。
    fn apply_one(
        &self,
        feature_id: &str,
        index: usize,
        a: &Action,
    ) -> std::result::Result<(ApplyRecord, State), StepError> {
        let desired = Desired::of(a).map_err(StepError::early)?;
        let (target, mut before) = self.current(a).map_err(StepError::early)?;

        // 记下为了写这个值要新建哪些键，撤销时一并删掉（如果那时已经空了）
        if let (Desired::Value(_), TargetRef::Registry { root, key, .. }, State::Registry { created_keys, .. }) =
            (&desired, &target, &mut before)
        {
            for k in key_ancestors(key) {
                if !self.platform.reg_key_exists(root, &k).map_err(|e| StepError::early(e.into()))? {
                    created_keys.push(k);
                }
            }
        }
        if let (Desired::Start(_), State::Service { start_type: None }) = (&desired, &before) {
            let name = match &target {
                TargetRef::Service { name } => name.clone(),
                _ => String::new(),
            };
            return Err(StepError::early(Error::NotApplicable(format!("这台电脑上没有服务 {name}"))));
        }

        let rec = ApplyRecord {
            v: RECORD_VERSION,
            id: new_id(),
            session: self.session.clone(),
            time: now_rfc3339(),
            feature: feature_id.to_owned(),
            action: index,
            target: target.clone(),
            before,
        };
        self.journal.append(&Record::Apply(rec.clone())).map_err(StepError::early)?;

        let change = match (&desired, &target) {
            (Desired::Value(v), TargetRef::Registry { root, key, name }) => self.platform.reg_set(root, key, name, v),
            (Desired::Absent, TargetRef::Registry { root, key, name }) => {
                self.platform.reg_delete_value(root, key, name)
            }
            (Desired::Start(st), TargetRef::Service { name }) => self.platform.service_set(name, *st),
            _ => Ok(()),
        };
        let after = self.current(a).map(|(_, mut s)| {
            if let (State::Registry { created_keys, .. }, State::Registry { created_keys: before_keys, .. }) =
                (&mut s, &rec.before)
            {
                created_keys.clone_from(before_keys);
            }
            s
        });

        let (ok, error, after_state) = match (&change, &after) {
            (Ok(()), Ok(s)) if Self::matches(&desired, s) => (true, None, Some(s.clone())),
            (Ok(()), Ok(s)) => (false, Some("写入后读回的值不对".to_owned()), Some(s.clone())),
            (Ok(()), Err(e)) => (false, Some(e.to_string()), None),
            (Err(e), _) => (false, Some(e.to_string()), after.as_ref().ok().cloned()),
        };
        let commit = CommitRecord {
            v: RECORD_VERSION,
            reference: rec.id.clone(),
            time: now_rfc3339(),
            ok,
            after: after_state.clone(),
            error: error.clone(),
            left_changes: false,
        };
        if let Err(e) = self.journal.append(&Record::Commit(commit)) {
            // 改了但记不下来：马上退回，宁可不改也不能留下没有记录的改动
            let _ = self.restore(&rec);
            let left_changes = self.is_back(&rec) != Some(true);
            return Err(StepError { entry: Some(rec.id), error: e, left_changes });
        }
        match (ok, after_state) {
            (true, Some(after)) => Ok((rec, after)),
            _ => {
                // 没改成功的这一项也可能改了一半（比如键建好了、值没写进去），按原值退回
                let _ = self.revert(&rec, None, UndoReason::Rollback, true);
                let left_changes = self.is_back(&rec) != Some(true);
                if left_changes {
                    // 记下「这一项还留着改动」，修改日志里才会给「恢复原状」（原来这一条当时没改成功，不给恢复）
                    let _ = self.journal.append(&Record::Commit(CommitRecord {
                        v: RECORD_VERSION,
                        reference: rec.id.clone(),
                        time: now_rfc3339(),
                        ok: false,
                        after: None,
                        error: error.clone(),
                        left_changes: true,
                    }));
                }
                let msg = error.unwrap_or_else(|| "未知错误".to_owned());
                let error = Error::Invalid(format!("{}：{msg}", Self::target_label(&target)));
                Err(StepError { entry: Some(rec.id), error, left_changes })
            }
        }
    }

    fn apply_script(&self, f: &Feature, notes: Vec<String>) -> Result<ApplyResult> {
        let run = f.run.as_ref().ok_or_else(|| Error::Catalog(format!("{} 没有 run", f.id)))?;
        // 记下这次改的是哪个用户的注册表，撤销时原样传回
        let hive = (f.target == Target::CurrentUser).then(|| self.user_hive());
        let before = if f.reversible() {
            let prepare = f.prepare.as_ref().ok_or_else(|| Error::Catalog(format!("{} 没有 prepare", f.id)))?;
            let mut args = self.script_args(f, hive.as_deref());
            args.insert("Prepare".into(), Value::Bool(true));
            let snapshot = self.runner.run(&prepare.script, &args, SCRIPT_FEATURE_TIMEOUT)?;
            snapshot
                .get("before")
                .filter(|v| !v.is_null())
                .cloned()
                .ok_or_else(|| Error::Invalid("执行前快照没有返回原状态，已取消修改".to_owned()))?
        } else {
            Value::Null
        };
        let rec = ApplyRecord {
            v: RECORD_VERSION,
            id: new_id(),
            session: self.session.clone(),
            time: now_rfc3339(),
            feature: f.id.clone(),
            action: 0,
            target: TargetRef::Script { feature: f.id.clone(), hive: hive.clone() },
            before: State::Script { data: before.clone() },
        };
        self.journal.append(&Record::Apply(rec.clone()))?;
        let mut args = self.script_args(f, hive.as_deref());
        if f.reversible() {
            args.insert("Before".into(), Value::String(before.to_string()));
        }
        let outcome = self.runner.run(&run.script, &args, SCRIPT_FEATURE_TIMEOUT);
        let rollback = if outcome.is_err() && f.reversible() {
            Some(self.run_undo_script(f, hive.as_deref(), &before))
        } else {
            None
        };
        let (ok, after, error, left_changes) = match &outcome {
            Ok(v) if v.get("skipped").and_then(Value::as_bool) == Some(true) => (
                false,
                None,
                Some(v.get("reason").and_then(Value::as_str).unwrap_or("执行前的状态已变化，请重试").to_owned()),
                false,
            ),
            Ok(v) => (
                true,
                Some(State::Script {
                    data: serde_json::json!({
                        "before": before,
                        "after": v.get("after").cloned().unwrap_or(Value::Null),
                    }),
                }),
                None,
                false,
            ),
            Err(e) => (false, None, Some(e.to_string()), rollback.as_ref().is_some_and(|r| r.is_err())),
        };
        let committed = self.journal.append(&Record::Commit(CommitRecord {
            v: RECORD_VERSION,
            reference: rec.id.clone(),
            time: now_rfc3339(),
            ok,
            after,
            error: error.clone(),
            left_changes,
        }));
        if let Err(e) = committed {
            // 改了但记不下来：马上用撤销脚本按原值退回，宁可不改也不能留下没有记录的改动
            if f.reversible() && rollback.is_none() {
                let _ = self.run_undo_script(f, hive.as_deref(), &before);
            }
            return Err(e);
        }
        if !ok {
            let message = if left_changes {
                "修改失败，自动恢复也失败了；可以在修改日志里重试恢复。"
            } else if outcome.as_ref().ok().is_some_and(|v| v.get("skipped").and_then(Value::as_bool) == Some(true)) {
                "执行前的状态已变化，没有修改；请重新检测后再试。"
            } else {
                "修改失败，已尝试恢复原状；请重新检测这一项。"
            }
            .to_owned();
            let mut r = self.result(f, false, vec![rec.id], message, notes);
            r.error = error;
            return Ok(r);
        }
        let mut r = self.result(f, true, vec![rec.id], String::new(), notes);
        r.verified = self.detect_inner(f).map_or(FeatureStateKind::Unknown, |(s, _)| s);
        r.message = match r.verified {
            FeatureStateKind::Applied => "已经改好了。".to_owned(),
            _ => "改完了，但复查时发现没有完全生效。".to_owned(),
        };
        Ok(r)
    }

    /// 测试和虚拟机自动化用：把功能要修的地方改成「故障状态」（不写修改日志）。
    pub fn break_feature(&self, id: &str) -> Result<()> {
        let f = self.feature(id)?;
        if !f.is_primitive() {
            let b = f.break_script.as_ref().ok_or_else(|| Error::Catalog(format!("{} 没有 break 脚本", f.id)))?;
            self.runner.run(&b.script, &self.script_args(f, None), SCRIPT_FEATURE_TIMEOUT)?;
            return Ok(());
        }
        let actions = if f.break_actions.is_empty() { &f.windows_default } else { &f.break_actions };
        for a in actions {
            let desired = Desired::of(a)?;
            let (target, _) = self.current(a)?;
            match (&desired, &target) {
                (Desired::Value(v), TargetRef::Registry { root, key, name }) => {
                    self.platform.reg_set(root, key, name, v)?
                }
                (Desired::Absent, TargetRef::Registry { root, key, name }) => {
                    self.platform.reg_delete_value(root, key, name)?
                }
                (Desired::Start(st), TargetRef::Service { name }) => self.platform.service_set(name, *st)?,
                _ => {}
            }
        }
        Ok(())
    }

    // ───────────── 开机启动项 ─────────────

    /// 列出开机启动项。开关状态由引擎自己读，和改的时候读写同一个位置（登录用户的 HKCU）。
    pub fn startup_list(&self) -> Result<Vec<StartupItem>> {
        let mut args = Map::new();
        args.insert("UserHive".into(), Value::String(self.user_hive()));
        let v = self
            .runner
            .run(startup::LIST_SCRIPT, &args, STARTUP_LIST_TIMEOUT)
            .map_err(|e| Error::Invalid(format!("没能列出开机启动项：{e}")))?;
        let raw = startup::parse_list(&v).map_err(Error::Invalid)?;
        let mut items = Vec::with_capacity(raw.len());
        for r in &raw {
            let enabled = self.startup_enabled(r.source, &r.name)?;
            items.push(Self::startup_view(r, enabled));
        }
        *self.startup_items.lock().unwrap() = raw;
        Ok(items)
    }

    /// 停用或恢复一个开机启动项：和任务管理器「启动应用」写同一个开关，只停用、不删除，记进修改日志，能撤销。
    /// 只认最近一次列出来的启动项。
    pub fn startup_set(&self, id: &str, enabled: bool) -> Result<ApplyResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let (source, name) = self
            .startup_items
            .lock()
            .unwrap()
            .iter()
            .find(|i| startup::item_id(i.source, &i.name) == id)
            .map(|i| (i.source, i.name.clone()))
            .ok_or_else(|| Error::Invalid("这个启动项不在刚才的列表里了，请刷新一下再试。".to_owned()))?;
        let mut r = ApplyResult {
            feature: startup::FEATURE_ID.to_owned(),
            session_id: self.session.clone(),
            entry_ids: Vec::new(),
            ok: true,
            verified: FeatureStateKind::Applied,
            message: "本来就是这样，不用改。".to_owned(),
            reboot: crate::model::Reboot::None,
            notes: Vec::new(),
            error: None,
        };
        if self.startup_enabled(source, &name)? == enabled {
            return Ok(r);
        }
        let action = Self::startup_action(source, &name, &startup::approved_value(enabled, SystemTime::now()));
        match self.apply_one(startup::FEATURE_ID, 0, &action) {
            Ok((rec, _)) => {
                r.entry_ids.push(rec.id);
                r.message = if enabled {
                    "已经恢复：下次开机登录时，它会自动启动。".to_owned()
                } else {
                    "已经停用：下次开机登录时，它不会再自动启动。软件本身还在，想用时照样能打开；想改回来，在这里或者任务管理器的「启动应用」里都行。".to_owned()
                };
            }
            Err(step) => {
                r.ok = false;
                r.verified = FeatureStateKind::Unknown;
                r.entry_ids.extend(step.entry);
                r.message = if step.left_changes {
                    "没有改成功，而且改动没能自动退回，请在修改日志里手动恢复。".to_owned()
                } else {
                    "没有改成功，已经退回原样。".to_owned()
                };
                r.error = Some(step.error.to_string());
            }
        }
        Ok(r)
    }

    fn startup_action(source: startup::Source, name: &str, value: &[u8]) -> Action {
        Action::Registry(RegistryAction {
            key: source.approved_key(),
            name: name.to_owned(),
            value_type: Some(RegType::Binary),
            value: Some(Value::String(hex::encode(value))),
            delete: false,
        })
    }

    fn startup_enabled(&self, source: startup::Source, name: &str) -> Result<bool> {
        let (_, state) = self.current(&Self::startup_action(source, name, &[]))?;
        Ok(match state {
            State::Registry { value: Some(RegValue::Binary(bytes)), .. } => startup::is_enabled(Some(&bytes)),
            // 没有这个值是启用；类型不对的值任务管理器也不认，按启用算
            _ => true,
        })
    }

    fn startup_view(r: &startup::RawItem, enabled: bool) -> StartupItem {
        let (advice, reason) = startup::advise(r);
        let program = r.path.rsplit(['\\', '/']).next().unwrap_or_default().to_owned();
        let title = if r.description.trim().is_empty() { r.name.clone() } else { r.description.trim().to_owned() };
        let publisher =
            [&r.signer, &r.company].into_iter().map(|s| s.trim()).find(|s| !s.is_empty()).map(str::to_owned);
        StartupItem {
            id: startup::item_id(r.source, &r.name),
            source: r.source,
            name: r.name.clone(),
            title,
            program,
            path: r.path.clone(),
            exists: r.exists,
            publisher,
            signature: r.signature,
            location: r.source.label().to_owned(),
            enabled,
            advice,
            reason: reason.to_owned(),
        }
    }

    /// 修改日志里启动项开关的状态，说人话。
    fn startup_state_label(state: &State) -> String {
        match state {
            State::Registry { value: Some(RegValue::Binary(bytes)), .. } if !startup::is_enabled(Some(bytes)) => {
                "不自动启动（已停用）".to_owned()
            }
            State::Registry { value: None, .. } => "开机自动启动（默认）".to_owned(),
            State::Registry { .. } => "开机自动启动".to_owned(),
            other => Self::state_label(other),
        }
    }

    // ───────────── 撤销 ─────────────

    /// 把一条修改恢复成 before（不做漂移检查，不写日志）。
    fn restore(&self, rec: &ApplyRecord) -> Result<()> {
        match (&rec.target, &rec.before) {
            (TargetRef::Registry { root, key, name }, State::Registry { value, created_keys }) => {
                match value {
                    Some(v) => self.platform.reg_set(root, key, name, v)?,
                    None => self.platform.reg_delete_value(root, key, name)?,
                }
                for k in created_keys.iter().rev() {
                    // 清理为写值而新建的空键只是收尾：失败了（比如被安全软件锁住）也不影响值已经恢复，
                    // 不能因为它把整条撤销判成失败（那样重试时还会误报「被改过」）
                    let _ = self.platform.reg_delete_key_if_empty(root, k);
                }
                Ok(())
            }
            (TargetRef::Service { name }, State::Service { start_type: Some(st) }) => {
                Ok(self.platform.service_set(name, *st)?)
            }
            _ => Err(Error::Invalid("这条记录没有可以恢复的原值".to_owned())),
        }
    }

    /// 撤销一条修改并写日志。`after` 为 `None` 时跳过漂移检查。
    fn revert(&self, rec: &ApplyRecord, after: Option<&State>, reason: UndoReason, force: bool) -> Result<UndoResult> {
        let label = Self::target_label(&rec.target);
        // 改的是某个用户的注册表，而这个用户现在没登录：写到 HKU\<SID> 什么都改不到，
        // 还会被当成「恢复成功」。不写撤销记录，让用户等那个账户登录以后再来。
        if let Some(sid) = Self::record_sid(&rec.target)
            && !self.platform.user_hive_loaded(sid)
        {
            return Ok(UndoResult {
                reboot: crate::model::Reboot::None,
                entry_id: rec.id.clone(),
                ok: false,
                drift: false,
                message: format!(
                    "{label}属于另一个账户，那个账户现在没有登录，改不到。请等那个账户登录以后，再打开小药箱恢复。"
                ),
                error: None,
            });
        }
        if !force
            && let TargetRef::Script { feature, hive } = &rec.target
            && let Some(message) = self.script_drift(feature, hive.as_deref())
        {
            return Ok(UndoResult {
                reboot: crate::model::Reboot::None,
                entry_id: rec.id.clone(),
                ok: false,
                drift: true,
                message,
                error: None,
            });
        }
        if !force && let Some(after) = after {
            let drifted = match self.state_now(rec)? {
                Some(now) => !Self::same_state(&now, after),
                None => false,
            };
            if drifted {
                return Ok(UndoResult {
                    reboot: crate::model::Reboot::None,
                    entry_id: rec.id.clone(),
                    ok: false,
                    drift: true,
                    message: format!("{label} 后来被别的程序或你自己改过，恢复原状会覆盖那次改动。"),
                    error: None,
                });
            }
        }
        let outcome = match &rec.target {
            TargetRef::Script { feature, hive } => self.undo_script(feature, hive.as_deref(), rec),
            // 写回以后读出来核对，不一致就不能算恢复成功
            _ => self.restore(rec).and_then(|()| match self.is_back(rec) {
                Some(false) => Err(Error::Invalid("恢复以后读回来的值和原来的不一样".to_owned())),
                _ => Ok(()),
            }),
        };
        let (ok, error) = match &outcome {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };
        self.journal.append(&Record::Undo(UndoRecord {
            v: RECORD_VERSION,
            id: new_id(),
            session: self.session.clone(),
            time: now_rfc3339(),
            reference: rec.id.clone(),
            reason,
            forced: force,
            ok,
            error: error.clone(),
        }))?;
        Ok(UndoResult {
            reboot: crate::model::Reboot::None,
            entry_id: rec.id.clone(),
            ok,
            drift: false,
            message: if ok { format!("{label} 已经恢复原状。") } else { format!("{label} 没能恢复。") },
            error,
        })
    }

    fn undo_script(&self, feature: &str, hive: Option<&str>, rec: &ApplyRecord) -> Result<()> {
        let f = self.feature(feature)?;
        let before = match &rec.before {
            State::Script { data } if !data.is_null() => data.clone(),
            _ => self
                .journal
                .entries()?
                .into_iter()
                .find(|e| e.apply.id == rec.id)
                .and_then(|e| e.commit)
                .and_then(|c| c.after)
                .and_then(|s| match s {
                    State::Script { data } => data.get("before").cloned(),
                    _ => None,
                })
                .unwrap_or(Value::Null),
        };
        if before.is_null() {
            return Err(Error::Invalid("这一项执行时没来得及记下原来的状态，没法自动恢复".to_owned()));
        }
        self.run_undo_script(f, hive, &before)
    }

    fn run_undo_script(&self, f: &Feature, hive: Option<&str>, before: &Value) -> Result<()> {
        let Undo::Script(undo) = &f.undo else {
            return Err(Error::Invalid("这个功能不能撤销".to_owned()));
        };
        let mut args = self.script_args(f, hive);
        args.insert("Before".into(), Value::String(before.to_string()));
        self.runner.run(&undo.script, &args, SCRIPT_FEATURE_TIMEOUT).map(|_| ()).map_err(Error::from)
    }

    /// 脚本类修改撤销前的核对：用功能自己的检测脚本（不是 verify）看修复还在不在。
    /// 不在了（被别的程序或用户改过，例如又开了一个新代理）就返回提示；没法核对时也返回提示，由用户决定。
    fn script_drift(&self, feature: &str, hive: Option<&str>) -> Option<String> {
        let f = self.catalog.feature(feature)?;
        let detect = f.detect.as_ref()?;
        let title = f.title.get(&self.lang);
        match self.runner.run(&detect.script, &self.script_args(f, hive), SCRIPT_FEATURE_TIMEOUT) {
            Ok(v) if v.get("state").and_then(Value::as_str) == Some("applied") => None,
            Ok(_) => Some(format!("「{title}」后来被别的程序或你自己改过，恢复原状会覆盖那次改动。")),
            Err(e) => Some(format!("没能核对「{title}」现在的状态（{e}），恢复原状可能会覆盖后来的改动。")),
        }
    }

    /// 这条记录改的是哪个用户的注册表（`HKU\<SID>`）；和具体用户无关时返回 `None`。
    fn record_sid(target: &TargetRef) -> Option<&str> {
        match target {
            TargetRef::Registry { root: RegRoot::User(sid), .. } => Some(sid),
            TargetRef::Script { hive: Some(h), .. } => h.strip_prefix("Registry::HKEY_USERS\\"),
            _ => None,
        }
    }

    /// 目标位置现在是不是已经回到记录里的原值（只读、不改）。脚本类没法核对，返回 `None`。
    fn is_back(&self, rec: &ApplyRecord) -> Option<bool> {
        match self.state_now(rec) {
            Ok(Some(now)) => Some(Self::same_state(&now, &rec.before)),
            Ok(None) => None,
            Err(_) => Some(false),
        }
    }

    /// 目标位置现在的状态；脚本类修改返回 `None`（不做漂移检查）。
    fn state_now(&self, rec: &ApplyRecord) -> Result<Option<State>> {
        Ok(match &rec.target {
            TargetRef::Registry { root, key, name } => {
                Some(State::Registry { value: self.platform.reg_get(root, key, name)?, created_keys: Vec::new() })
            }
            TargetRef::Service { name } => Some(State::Service { start_type: self.platform.service_get(name)? }),
            TargetRef::Script { .. } => None,
        })
    }

    fn same_state(a: &State, b: &State) -> bool {
        match (a, b) {
            (State::Registry { value: x, .. }, State::Registry { value: y, .. }) => x == y,
            (State::Service { start_type: x }, State::Service { start_type: y }) => x == y,
            _ => false,
        }
    }

    fn find_entry(&self, entry_id: &str) -> Result<Entry> {
        self.journal
            .entries()?
            .into_iter()
            .find(|e| e.apply.id == entry_id)
            .ok_or_else(|| Error::not_found("修改记录", entry_id))
    }

    pub fn journal_undo(&self, entry_id: &str, force: bool) -> Result<UndoResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let entry = self.find_entry(entry_id)?;
        self.undo_entry(&entry, force)
    }

    /// 这条修改是不是脚本类的、而且功能声明了不能撤销。
    fn irreversible(&self, entry: &Entry) -> bool {
        matches!(&entry.apply.target, TargetRef::Script { feature, .. }
            if self.catalog.feature(feature).is_none_or(|f| !f.reversible()))
    }

    /// 新记录从 apply.before 恢复；旧记录仍可从 commit.after 读取原状态。
    fn script_state_lost(entry: &Entry) -> bool {
        matches!(entry.apply.target, TargetRef::Script { .. })
            && !matches!(&entry.apply.before, State::Script { data } if !data.is_null())
            && entry.commit.as_ref().is_none_or(|c| {
                !matches!(
                    &c.after,
                    Some(State::Script { data }) if data.get("before").is_some_and(|v| !v.is_null())
                )
            })
    }

    fn can_undo(&self, entry: &Entry) -> bool {
        entry.undo.is_none() && entry.maybe_applied() && !self.irreversible(entry) && !Self::script_state_lost(entry)
    }

    fn undo_entry(&self, entry: &Entry, force: bool) -> Result<UndoResult> {
        if entry.undo.is_some() {
            return Err(Error::Invalid("这一项已经恢复过了".to_owned()));
        }
        if !entry.maybe_applied() {
            return Err(Error::Invalid("这一项当时就没有改成功，不需要恢复".to_owned()));
        }
        if self.irreversible(entry) {
            return Err(Error::Invalid("这一项改了就不能撤销".to_owned()));
        }
        if Self::script_state_lost(entry) {
            return Err(Error::Invalid("这一项执行时没来得及记下原来的状态，没法自动恢复".to_owned()));
        }
        // 崩溃在中途、没有 commit 的记录：没法核对，按强制恢复处理
        let after = entry.commit.as_ref().and_then(|c| c.after.as_ref());
        let force = force || entry.is_pending();
        let mut r = self.revert(&entry.apply, after, UndoReason::User, force)?;
        if r.ok {
            // 恢复原状和当初修改一样，要重启资源管理器、注销之后才看得到
            r.reboot = self.catalog.feature(&entry.apply.feature).map_or_else(Default::default, |f| f.reboot);
        }
        Ok(r)
    }

    pub fn journal_undo_session(&self, session_id: &str) -> Result<Vec<UndoResult>> {
        let _guard = self.apply_lock.lock().unwrap();
        let entries = self.journal.entries()?;
        let mut results = Vec::new();
        // 倒序：后改的先恢复，同一个位置被改过多次时才能一路核对回去
        for entry in entries.iter().rev() {
            if entry.apply.session != session_id || !self.can_undo(entry) {
                continue;
            }
            // 一条失败不影响其他条
            results.push(self.undo_entry(entry, false).unwrap_or_else(|e| UndoResult {
                reboot: crate::model::Reboot::None,
                entry_id: entry.apply.id.clone(),
                ok: false,
                drift: false,
                message: format!("{} 没能恢复。", Self::target_label(&entry.apply.target)),
                error: Some(e.to_string()),
            }));
        }
        Ok(results)
    }

    pub fn journal_list(&self) -> Result<Vec<JournalSession>> {
        let entries = self.journal.entries()?;
        let mut sessions: Vec<JournalSession> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for e in entries {
            // 开机启动项的开关不是数据文件里的功能：标题用启动项的名字，状态说「自动启动 / 已停用」
            let startup_entry = e.apply.feature == startup::FEATURE_ID
                && matches!(&e.apply.target, TargetRef::Registry { key, .. } if startup::is_approved_key(key));
            let label = |s: &State| if startup_entry { Self::startup_state_label(s) } else { Self::state_label(s) };
            let feature_title = match &e.apply.target {
                TargetRef::Registry { name, .. } if startup_entry => format!("开机启动项：{name}"),
                _ if e.apply.feature == LEGACY_STARTUP_FEATURE => "停用开机启动项".to_owned(),
                _ => self
                    .catalog
                    .feature(&e.apply.feature)
                    .map_or_else(|| e.apply.feature.clone(), |f| f.title.get(&self.lang).to_owned()),
            };
            let (ok, after, error) = match &e.commit {
                Some(c) => (c.ok, c.after.as_ref().map(label).unwrap_or_default(), c.error.clone()),
                None => (
                    false,
                    "（状态不确定）".to_owned(),
                    Some("程序在修改过程中退出，状态不确定；可以尝试恢复原状。".to_owned()),
                ),
            };
            // 脚本类修改的原状态是脚本自己的数据（例如一段十六进制），给用户看没有意义，说清楚记没记下就行
            let saved_before = matches!(&e.apply.before, State::Script { data } if !data.is_null());
            let (before, after) = match (&e.apply.target, &e.commit) {
                (TargetRef::Registry { .. }, _) if e.apply.feature == LEGACY_STARTUP_FEATURE => {
                    ("启动命令已保存（可恢复）".to_owned(), if ok { "已停用自启" } else { "（状态不确定）" }.to_owned())
                }
                (TargetRef::Script { .. }, Some(c)) => {
                    let recorded = saved_before
                        || matches!(&c.after, Some(State::Script { data })
                        if data.get("before").is_some_and(|b| !b.is_null()));
                    let before = if recorded {
                        "原来的设置（已经记下，可以恢复）"
                    } else {
                        "（没记下原来的设置）"
                    };
                    let after = if c.ok { "已按这一项修改" } else { "（没改成）" };
                    (before.to_owned(), after.to_owned())
                }
                (TargetRef::Script { .. }, None) => (
                    if saved_before {
                        "原来的设置（已经记下，可以恢复）"
                    } else {
                        "（程序中途退出，没记下原来的设置）"
                    }
                    .to_owned(),
                    after,
                ),
                _ => (label(&e.apply.before), after),
            };
            let view = JournalEntryView {
                id: e.apply.id.clone(),
                session_id: e.apply.session.clone(),
                time: e.apply.time.clone(),
                feature: e.apply.feature.clone(),
                feature_title,
                target: Self::target_label(&e.apply.target),
                before,
                after,
                ok,
                pending: e.is_pending(),
                undone: e.undo.is_some(),
                undone_at: e.undo.as_ref().map(|u| u.time.clone()),
                can_undo: self.can_undo(&e),
                error: match (&e.undo, error) {
                    (Some(u), None) if u.reason == UndoReason::Rollback => {
                        Some("同一项修改里后面的步骤失败了，这一步已自动退回。".to_owned())
                    }
                    (_, err) => err,
                },
            };
            let slot = *index.entry(e.apply.session.clone()).or_insert_with(|| {
                sessions.push(JournalSession {
                    id: e.apply.session.clone(),
                    started_at: e.apply.time.clone(),
                    entries: Vec::new(),
                });
                sessions.len() - 1
            });
            sessions[slot].entries.push(view);
        }
        sessions.reverse();
        Ok(sessions)
    }

    // ───────────── 报告 ─────────────

    pub fn report_generate(&self) -> Result<String> {
        let info = self.system_info();
        let os = self.platform.os_info();
        let mut out = String::new();
        let line = |out: &mut String, s: &str| {
            out.push_str(s);
            out.push('\n');
        };
        line(&mut out, "电脑小药箱诊断报告");
        line(&mut out, &format!("生成时间：{}", self.local_time(&now_rfc3339())));
        line(&mut out, &format!("程序版本：{}（数据 {}）", info.app_version, info.catalog_version));
        line(
            &mut out,
            &format!("系统：{} {}（版本号 {}，{}）", os.caption, os.display_version, os.build, os.edition_id),
        );
        line(&mut out, &format!("管理员权限：{}", if info.is_admin { "是" } else { "否" }));
        if info.elevated_user_mismatch {
            line(&mut out, "注意：程序是用另一个管理员账户运行的。");
        }
        line(&mut out, "");

        let results = self.last_results.lock().unwrap().clone();
        line(&mut out, "== 最近一次体检 ==");
        if results.is_empty() {
            line(&mut out, "（还没有做过体检）");
        } else if let Some(t) = self.last_profile_time.lock().unwrap().as_deref() {
            line(&mut out, &format!("（体检时间：{}；之后单独重查过的项目已经更新）", self.local_time(t)));
        }
        let print = |out: &mut String, r: &CheckResult| {
            let tag = match r.status {
                Status::Ok => "正常",
                Status::Advice => "建议处理",
                Status::Manual => "需要人工",
                Status::Unknown | Status::Na => "没查出来",
            };
            line(out, &format!("[{tag}] {}：{}", r.title, r.message));
            if let Some(e) = &r.error {
                line(out, &format!("    原因：{e}"));
            }
        };
        for r in results.iter().filter(|r| r.status != Status::Na) {
            print(&mut out, r);
        }
        line(&mut out, "");
        let others = self.other_results.lock().unwrap().clone();
        if !others.is_empty() {
            line(&mut out, "== 单独检查过的项目（例如在「按症状修」里） ==");
            for r in others.iter().filter(|r| r.status != Status::Na) {
                print(&mut out, r);
            }
            line(&mut out, "");
        }

        line(&mut out, "== 最近的修改 ==");
        let sessions = self.journal_list()?;
        let recent: Vec<&JournalEntryView> = sessions.iter().flat_map(|s| s.entries.iter()).take(30).collect();
        if recent.is_empty() {
            line(&mut out, "（没有修改记录）");
        }
        for e in recent {
            let status = match (e.ok, e.undone) {
                (true, true) => "已恢复原状",
                (false, true) => "失败，已自动退回",
                (true, false) => "成功",
                (false, false) => "失败",
            };
            line(
                &mut out,
                &format!(
                    "{} {}（{}）：{} → {}，{status}",
                    self.local_time(&e.time),
                    e.feature_title,
                    e.target,
                    e.before,
                    e.after
                ),
            );
        }

        let mut secrets = vec![os.computer_name.clone()];
        // 检测事实里可能带着单位的代理服务器、自动配置脚本地址、VPN 连接名，一律隐藏（本机地址保留）
        for r in results.iter().chain(others.iter()) {
            for key in SENSITIVE_FACTS {
                if let Some(v) = r.facts.get(*key).and_then(Value::as_str)
                    && !is_local_address(v)
                {
                    secrets.extend(v.split(", ").map(str::to_owned));
                }
            }
        }
        for u in [self.platform.interactive_user(), self.platform.process_user()].into_iter().flatten() {
            secrets.push(u.name.clone());
            if let Some((_, short)) = u.name.rsplit_once('\\') {
                secrets.push(short.to_owned());
            }
        }
        Ok(redact(&out, &secrets))
    }
}

/// 值里可能有单位名、学校名、VPN 名的检测事实，写报告时隐藏。
const SENSITIVE_FACTS: &[&str] =
    &["proxy_address", "proxy_server", "pac_url", "connection", "dialup_proxy", "dead_dialup"];

/// 指向本机的代理地址（127.x、localhost、::1），说明的是「本机有个代理软件」，不涉及隐私。
fn is_local_address(v: &str) -> bool {
    let v = v.trim().to_ascii_lowercase();
    let v = v.split_once("://").map_or(v.as_str(), |(_, rest)| rest);
    v.starts_with("127.")
        || v.starts_with("localhost")
        || v.starts_with("[::1]")
        || v.starts_with("::1")
        || v.is_empty()
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn check<T: Send + Sync>() {}
    check::<Engine>();
}
