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
use crate::context_menu;
use crate::error::{Error, Result};
use crate::journal::{
    ApplyRecord, CommitRecord, Entry, Journal, RECORD_VERSION, Record, State, TargetRef, UndoReason, UndoRecord,
    new_id, now_rfc3339,
};
use crate::keymap;
use crate::model::{
    Action, Check, Feature, RegType, RegistryAction, Risk, StartType, Status, Symptom, Target, Tool, ToolGroup, Undo,
};
use crate::new_menu;
use crate::ocr;
use crate::platform::{OpenRequest, Platform, PlatformError};
use crate::registry::{RegRoot, RegValue, SpecRoot, display_opt, is_sid, key_ancestors, split_key};
use crate::render::{label_fact, render};
use crate::report::redact;
use crate::script::ScriptRunner;
use crate::shell_places;
use crate::startup;
use crate::tools;
use crate::views::{
    ApplyResult, CatalogSummary, CheckResult, ContextMenuItem, ContextMenuKind, FeatureState, FeatureStateKind,
    FeatureSummary, FileLockReport, JournalEntryView, JournalSession, KeyMappingView, KeyOption, KeyRemapView,
    NewMenuItem, Preview, PreviewChange, ProfileSummary, ShellPlaceItem, StartupItem, SymptomDetail, SymptomStep,
    SymptomSummary, SystemInfo, ToolOpens, ToolResult, ToolSummary, UndoResult,
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
/// 列右键菜单要查程序的签名、读应用清单；脚本自己有时间上限，这里再留些余量
const CONTEXT_MENU_LIST_TIMEOUT: Duration = Duration::from_secs(120);
const NEW_MENU_LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SHELL_PLACES_LIST_TIMEOUT: Duration = Duration::from_secs(60);

/// 资源管理器里一个图标现在的开关。
struct ShellPlaceState {
    /// 登录用户的 NonEnum 里有它（外壳哪里都不列）
    non_enum_user: bool,
    /// 所有用户的 NonEnum 里有它
    non_enum_machine: bool,
    /// 导航栏的开关（只在导航栏里登记了的才读）
    pinned: shell_places::Pinned,
}

impl ShellPlaceState {
    fn non_enum(&self, hive: shell_places::Hive) -> bool {
        match hive {
            shell_places::Hive::User => self.non_enum_user,
            shell_places::Hive::Machine => self.non_enum_machine,
        }
    }

    /// 现在显示不显示：NonEnum 里有的哪里都不显示；「此电脑」里登记了的显示；只在导航栏里的看开关
    fn visible(&self, g: &shell_places::Group) -> bool {
        !self.non_enum_user && !self.non_enum_machine && (g.in_pc || self.pinned.shown())
    }

    /// 是在所有用户的设置里隐藏的（恢复时要改所有用户的那份）
    fn hidden_for_everyone(&self, g: &shell_places::Group) -> bool {
        let pinned_off = !g.in_pc && self.pinned.user.is_none() && self.pinned.machine == Some(0);
        !self.visible(g) && ((self.non_enum_machine && !self.non_enum_user) || pinned_off)
    }
}

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

/// 几个原语里有一个没改成功（见 `Engine::apply_actions`）。
struct ActionsFailed {
    error: Error,
    /// 前面改过的都退回原样了
    rolled_back: bool,
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
    /// 最近一次列出的右键菜单项目：改开关时只认这里面有的
    context_menu: Mutex<Vec<context_menu::Group>>,
    /// 最近一次列出的「新建」菜单项目：改开关时只认这里面有的
    new_menu: Mutex<Vec<new_menu::Group>>,
    /// 最近一次列出的资源管理器图标：改开关时只认这里面有的
    shell_places: Mutex<Vec<shell_places::Group>>,
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
            context_menu: Mutex::new(Vec::new()),
            new_menu: Mutex::new(Vec::new()),
            shell_places: Mutex::new(Vec::new()),
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
            links: s.links.clone(),
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
            builtin::run(name, &builtin::Env { os: &self.platform.os_info(), now, platform: &*self.platform })
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
                // 说明表查出的文字：模板里总能用（查不到就是空的）；事实里只放查到了的，免得详情里多出空行
                let mut text_facts = result.facts.clone();
                for (name, labels) in &check.fact_labels {
                    let text = label_fact(result.facts.get(&labels.from), labels, &self.lang);
                    if !text.is_empty() {
                        result.facts.insert(name.clone(), Value::String(text.clone()));
                    }
                    text_facts.insert(name.clone(), Value::String(text));
                }
                match code.as_deref().and_then(|c| check.results.get(c)) {
                    Some(spec) => {
                        result.status = spec.status;
                        result.message = render(spec.message.get(&self.lang), &text_facts);
                        result.fixer = spec.fixer;
                        result.next = spec.next.as_ref().map(|t| render(t.get(&self.lang), &text_facts));
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
            opens: t.open.as_ref().map(|o| {
                if o.program.is_some() {
                    ToolOpens::Program
                } else if o.troubleshooter.is_some() {
                    ToolOpens::GetHelp
                } else if o.website.is_some() {
                    ToolOpens::Website
                } else {
                    ToolOpens::Settings
                }
            }),
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

    /// 打开一个 open 小工具：系统自带的工具、「设置」里的一页、「获取帮助」里微软的疑难解答，或者一个网页。
    /// 返回打开以后要告诉用户的话（没有时界面说「已经打开了」）：网页要说清楚打开的是哪个品牌的页面、在上面搜什么。
    pub fn tool_open(&self, id: &str) -> Result<Option<String>> {
        let tool = self.tool(id)?;
        let title = tool.title.get(&self.lang);
        let mut notice = None;
        let request =
            match tool.open.as_ref().map(|o| {
                (o.program.as_deref(), o.settings.as_deref(), o.troubleshooter.as_deref(), o.website.as_deref())
            }) {
                Some((Some(name), None, None, None)) => {
                    let p = tools::program(name)
                        .ok_or_else(|| Error::Catalog(format!("{id} 的 open.program 不在名单里：{name}")))?;
                    OpenRequest::Program { exe: p.exe, args: p.args, console: p.console }
                }
                Some((None, Some(page), None, None)) => OpenRequest::Settings(
                    tools::settings_page(page)
                        .ok_or_else(|| Error::Catalog(format!("{id} 的 open.settings 不在名单里：{page}")))?,
                ),
                Some((None, None, Some(name), None)) => OpenRequest::GetHelp(
                    tools::troubleshooter(name)
                        .ok_or_else(|| Error::Catalog(format!("{id} 的 open.troubleshooter 不在名单里：{name}")))?,
                ),
                Some((None, None, None, Some(name))) => {
                    tools::website(name)
                        .ok_or_else(|| Error::Catalog(format!("{id} 的 open.website 不在名单里：{name}")))?;
                    match tools::fixed_website(name) {
                        Some(url) => OpenRequest::Web(url),
                        None => {
                            let (url, text) = self.oem_drivers_page()?;
                            notice = Some(text);
                            OpenRequest::Web(url)
                        }
                    }
                }
                _ => return Err(Error::Invalid(format!("「{title}」不是用来打开的工具"))),
            };
        self.platform.open(&request).map_err(|e| match (e, request) {
            (PlatformError::NotFound(_), OpenRequest::GetHelp(_)) => Error::Invalid(format!(
                "这台电脑上没有「获取帮助」应用（精简过的系统、服务器版常常没有），打不开微软的「{title}」。可以在 Microsoft Store 里搜「获取帮助」装上再试，或者到「设置」的「疑难解答」页里找。"
            )),
            // 不退回到直接打开：那样浏览器会跟着小药箱以管理员身份运行
            (PlatformError::NotFound(_), OpenRequest::Web(url)) => Error::Invalid(format!(
                "桌面（资源管理器）没在运行，小药箱没法用你的账户打开浏览器。请自己打开浏览器，输入这个网址：{url}"
            )),
            (e, OpenRequest::Web(url)) => {
                Error::Invalid(format!("没能打开浏览器：{}。请自己打开浏览器，输入这个网址：{url}", platform_text(&e)))
            }
            (PlatformError::NotFound(file), _) => {
                Error::Invalid(format!("这台电脑上没有「{title}」（找不到 {file}），可能被精简系统删掉了。"))
            }
            (e, _) => Error::Invalid(format!("没能打开「{title}」：{}", platform_text(&e))),
        })?;
        Ok(notice)
    }

    /// BIOS 里写的厂商和型号（见 [`tools::BiosInfo`]）。读不到的项是空字符串。
    fn bios_info(&self) -> tools::BiosInfo {
        let get = |name: &str| match self.platform.reg_get(&RegRoot::LocalMachine, tools::BIOS_KEY, name) {
            Ok(Some(RegValue::String(s) | RegValue::ExpandString(s))) => s.trim().to_owned(),
            _ => String::new(),
        };
        tools::BiosInfo {
            system_manufacturer: get("SystemManufacturer"),
            system_product: get("SystemProductName"),
            system_version: get("SystemVersion"),
            system_family: get("SystemFamily"),
            board_manufacturer: get("BaseBoardManufacturer"),
            board_product: get("BaseBoardProduct"),
            bios_vendor: get("BIOSVendor"),
        }
    }

    /// 这台电脑品牌官网的驱动下载页，和打开以后要告诉用户的话。认不出品牌、是虚拟机时不打开，说明原因。
    fn oem_drivers_page(&self) -> Result<(&'static str, String)> {
        use tools::OemMatch;
        const SEARCH_TIP: &str = "不要从搜索结果里的「驱动下载站」下载，那些常常捆绑别的软件。";
        match tools::oem_match(&self.bios_info()) {
            OemMatch::Brand { site, model } => {
                let find = match model {
                    Some(m) => format!("这台电脑的型号是「{m}」，在网页上搜这个型号就能找到它的驱动。"),
                    None => "在网页上搜这台电脑的型号（写在电脑底部的标签上）就能找到它的驱动。".to_owned(),
                };
                Ok((site.drivers, format!("已经在浏览器里打开了{}的驱动下载页。{find}", official_site(site.brand))))
            }
            OemMatch::Board { site, model } => {
                let find = match model {
                    Some(m) => format!("主板型号是「{m}」，在网页上搜这个型号。"),
                    None => "在网页上搜主板的型号（印在主板上）。".to_owned(),
                };
                Ok((
                    site.drivers,
                    format!(
                        "这台电脑没有写整机品牌，多半是自己组装的，驱动要按主板找：已经在浏览器里打开了主板品牌{}的下载页。{find}",
                        official_site(site.brand)
                    ),
                ))
            }
            OemMatch::VirtualMachine => Err(Error::Invalid(
                "这是一台虚拟机，没有品牌官网的驱动：虚拟机的驱动由虚拟机软件提供（比如 VMware Tools、VirtualBox 的增强功能），在虚拟机软件的菜单里安装。".into(),
            )),
            OemMatch::Unknown { manufacturer } => {
                let what = if manufacturer.is_empty() {
                    "BIOS 里没有写这台电脑的品牌，小药箱认不出来。".to_owned()
                } else {
                    format!("小药箱还不认识「{manufacturer}」这个品牌（这是 BIOS 里写的厂商），没有它的官网地址。")
                };
                Err(Error::Invalid(format!(
                    "{what}看看电脑底部的标签或者包装盒上的品牌和型号，到这个品牌官网的「服务与支持」里下载驱动；{SEARCH_TIP}"
                )))
            }
        }
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
        self.resolve_key(&r.key)
    }

    /// 带根的键（`HKLM\…`、`HKCU\…`、`HKU\.DEFAULT\…`）换成根和子键；HKCU 换成登录用户的。
    fn resolve_key(&self, key: &str) -> Result<(RegRoot, String)> {
        let (spec_root, sub) = split_key(key).map_err(Error::Catalog)?;
        let root = match spec_root {
            SpecRoot::Hklm => RegRoot::LocalMachine,
            SpecRoot::DefaultUser => RegRoot::DefaultUser,
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
                    RegRoot::DefaultUser => "HKU\\.DEFAULT（登录界面）".to_owned(),
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
                // 检测说这件事和这台电脑无关（例如没有休眠文件、是笔记本）：这个功能在这里不能用，理由就是检测的结论
                Status::Na => return Err(Error::NotApplicable(r.message)),
                Status::Ok => FeatureStateKind::Applied,
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
        let mut summary = self.feature_summary(f);
        let mut notes = Vec::new();
        if let Some(reason) = &summary.not_applicable_reason {
            notes.push(format!("不能执行：{reason}"));
        }
        // 脚本类功能用检测说明现在的状态；写了 verify 的，那个检测还决定这台电脑能不能用（na 时不能用，
        // 版本对，但没有要改的东西，或者不该改）：和版本不对一样，不给执行
        let detected = (!f.is_primitive() || f.verify.is_some()).then(|| self.detect_inner(f));
        if let Some(Err(Error::NotApplicable(reason))) = &detected
            && summary.applicable
        {
            notes.push(format!("不能执行：{reason}"));
            summary.applicable = false;
            summary.not_applicable_reason = Some(reason.clone());
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
            // 脚本类功能没有逐个位置可列；要改什么见功能说明
            let current = match detected {
                Some(Ok((FeatureStateKind::Applied, _))) => "已经是这样了",
                Some(Ok((FeatureStateKind::NotApplied, _))) => "还没改",
                Some(Ok((FeatureStateKind::Partial, _))) => "改了一部分",
                Some(Err(Error::NotApplicable(_))) => "不适用",
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
        Ok(Preview { feature: summary, changes, will_create_restore_point: f.risk >= Risk::Caution, notes })
    }

    // ───────────── 功能：执行 ─────────────

    pub fn feature_apply(&self, id: &str) -> Result<ApplyResult> {
        let f = self.feature(id)?;
        if let Some(reason) = self.applicability(f) {
            return Err(Error::NotApplicable(reason));
        }
        let _guard = self.apply_lock.lock().unwrap();
        match self.detect_inner(f) {
            // 本来就是好的：不建还原点，也不写修改日志
            Ok((FeatureStateKind::Applied, _)) => {
                let mut r = self.result(f, true, Vec::new(), "这一项本来就是好的，不用改。".to_owned(), Vec::new());
                r.verified = FeatureStateKind::Applied;
                r.reboot = crate::model::Reboot::None;
                return Ok(r);
            }
            // 复查用的检测说这台电脑用不了（预览里已经说明、不给执行）
            Err(Error::NotApplicable(reason)) => return Err(Error::NotApplicable(reason)),
            _ => {}
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
        let (entry_ids, failure) = self.apply_actions(&f.id, &f.actions);
        if let Some(failed) = failure {
            let message = if failed.rolled_back {
                "没有改成功，已经把这次改过的部分退回原样。".to_owned()
            } else {
                "没有改成功，而且有部分改动没能自动退回，请在修改日志里手动恢复。".to_owned()
            };
            let mut r = self.result(f, false, entry_ids, message, notes);
            r.error = Some(failed.error.to_string());
            return Ok(r);
        }
        let mut r = self.result(f, true, entry_ids, String::new(), notes);
        r.verified = self.detect_inner(f).map_or(FeatureStateKind::Unknown, |(s, _)| s);
        r.message = match r.verified {
            FeatureStateKind::Applied => "已经改好了。".to_owned(),
            _ => "改完了，但复查时发现没有完全生效。".to_owned(),
        };
        Ok(r)
    }

    /// 依次执行几个原语，当成一个整体：已经是目标状态的不动（撤销时也就不会碰它）；中途有一个失败，
    /// 前面改过的按倒序退回。返回写进修改日志的记录 ID，失败时还有原因和退没退干净。
    fn apply_actions(&self, feature_id: &str, actions: &[Action]) -> (Vec<String>, Option<ActionsFailed>) {
        let mut done: Vec<(ApplyRecord, State)> = Vec::new();
        let mut entry_ids = Vec::new();
        for (i, a) in actions.iter().enumerate() {
            if let (Ok(desired), Ok((_, state))) = (Desired::of(a), self.current(a))
                && Self::matches(&desired, &state)
            {
                continue;
            }
            match self.apply_one(feature_id, i, a) {
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
                    return (entry_ids, Some(ActionsFailed { error: step.error, rolled_back }));
                }
            }
        }
        (entry_ids, None)
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

    // ───────────── 改键（键位重映射） ─────────────

    /// 改键的那个值：`bytes` 为空时是删掉它（恢复 Windows 默认）。
    fn key_remap_action(bytes: Option<&[u8]>) -> Action {
        Action::Registry(RegistryAction {
            key: keymap::KEY.to_owned(),
            name: keymap::VALUE.to_owned(),
            value_type: bytes.map(|_| RegType::Binary),
            value: bytes.map(|b| Value::String(hex::encode(b))),
            delete: bytes.is_none(),
        })
    }

    /// 现在的 `Scancode Map`：没有这个值是 `None`；类型不对的值 Windows 也不认，当成认不出来的格式（空的字节）。
    fn key_remap_bytes(&self) -> Result<Option<Vec<u8>>> {
        let (_, state) = self.current(&Self::key_remap_action(None))?;
        Ok(match state {
            State::Registry { value: Some(RegValue::Binary(bytes)), .. } => Some(bytes),
            State::Registry { value: Some(_), .. } => Some(Vec::new()),
            _ => None,
        })
    }

    /// 一条映射小药箱认不认得：按下的键和变成的键都在名单里，按下的不是只能当目标的多媒体键。
    fn key_mapping_known(m: keymap::Mapping) -> bool {
        keymap::by_code(m.from).is_some_and(|k| !k.target_only) && (m.to == 0 || keymap::by_code(m.to).is_some())
    }

    /// 现在的改键，和能选的键。
    pub fn key_remap_get(&self) -> Result<KeyRemapView> {
        let bytes = self.key_remap_bytes()?;
        let keys = keymap::KEYS
            .iter()
            .map(|k| KeyOption { id: k.id.to_owned(), label: k.label.to_owned(), target_only: k.target_only })
            .collect();
        let mut view = KeyRemapView { keys, mappings: Vec::new(), foreign: false, foreign_text: None };
        match bytes.as_deref().map(keymap::parse) {
            None => {}
            Some(None) => {
                view.foreign = true;
                view.foreign_text = Some(keymap::describe_value(bytes.as_deref()));
            }
            Some(Some(list)) => {
                let mut unknown = Vec::new();
                for m in list {
                    if Self::key_mapping_known(m) {
                        view.mappings.push(KeyMappingView {
                            from: keymap::by_code(m.from).map(|k| k.id.to_owned()).unwrap_or_default(),
                            to: keymap::by_code(m.to).map(|k| k.id.to_owned()),
                            text: keymap::describe(m),
                        });
                    } else {
                        unknown.push(keymap::describe(m));
                    }
                }
                if !unknown.is_empty() {
                    view.foreign = true;
                    view.foreign_text = Some(unknown.join("；"));
                }
            }
        }
        Ok(view)
    }

    /// 把改键整个换成 `mappings`（空的：删掉这个值，恢复 Windows 默认），记进修改日志，能撤销，重启电脑以后生效。
    /// 现在的设置里有认不出来的键（别的改键软件设的）时，只能整个清掉，不在这里接着改。
    pub fn key_remap_set(&self, mappings: &[keymap::MappingInput]) -> Result<ApplyResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let wanted = keymap::resolve(mappings).map_err(Error::Invalid)?;
        let current = self.key_remap_bytes()?;
        let current_list = current.as_deref().map(keymap::parse);
        let foreign = match &current_list {
            None => false,
            Some(None) => true,
            Some(Some(list)) => !list.iter().all(|m| Self::key_mapping_known(*m)),
        };
        if foreign && !wanted.is_empty() {
            return Err(Error::Invalid(
                "这台电脑已经有别的软件设置的改键，里面有小药箱认不出来的键。为了不弄乱，小药箱只能把它整个清掉\
                 （点「全部恢复」），不能在这里接着改。"
                    .to_owned(),
            ));
        }
        let mut r = ApplyResult {
            feature: keymap::FEATURE_ID.to_owned(),
            session_id: self.session.clone(),
            entry_ids: Vec::new(),
            ok: true,
            verified: FeatureStateKind::Applied,
            message: "本来就是这样，不用改。".to_owned(),
            reboot: crate::model::Reboot::None,
            notes: Vec::new(),
            error: None,
        };
        let same = match &current_list {
            None => wanted.is_empty(),
            Some(Some(list)) => *list == wanted,
            Some(None) => false,
        };
        if same {
            return Ok(r);
        }
        let bytes = (!wanted.is_empty()).then(|| keymap::build(&wanted));
        let action = Self::key_remap_action(bytes.as_deref());
        match self.apply_one(keymap::FEATURE_ID, 0, &action) {
            Ok((rec, _)) => {
                r.entry_ids.push(rec.id);
                r.reboot = crate::model::Reboot::Reboot;
                r.message = if wanted.is_empty() {
                    "已经把改键全部去掉了，重启电脑以后所有键恢复原样。".to_owned()
                } else {
                    format!(
                        "已经改好了：{}。重启电脑以后生效。想改回来，在这里点「全部恢复」，或者在修改日志里撤销，也是重启以后生效。",
                        wanted.iter().map(|m| keymap::describe(*m)).collect::<Vec<_>>().join("；")
                    )
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

    /// 修改日志里改键的状态，说人话。
    fn key_remap_state_label(state: &State) -> String {
        match state {
            State::Registry { value: Some(RegValue::Binary(bytes)), .. } => keymap::describe_value(Some(bytes)),
            State::Registry { value: None, .. } => keymap::describe_value(None),
            other => Self::state_label(other),
        }
    }

    // ───────────── 图片转文字 ─────────────

    /// 用 Windows 自带的文字识别认出图片（PNG）里的字。只读：不改设置，不记修改日志。
    /// `image` 由调用的一方准备（小药箱数据文件夹里的临时文件），认完由它删掉。
    pub fn ocr_recognize(&self, image: &std::path::Path) -> Result<crate::views::OcrView> {
        let mut args = Map::new();
        args.insert("Path".into(), Value::String(image.to_string_lossy().into_owned()));
        let v = self
            .runner
            .run(ocr::SCRIPT, &args, ocr::TIMEOUT)
            .map_err(|e| Error::Invalid(format!("没能认出图片里的字：{e}")))?;
        ocr::parse(&v).map_err(Error::Invalid)
    }

    // ───────────── 文件删不掉：是谁占着 ─────────────

    /// 哪些程序在用这些文件（或者这个文件夹里的文件）。只读：不关程序，不动文件，不记修改日志。
    pub fn file_lockers(&self, target: &crate::lockers::LockTarget) -> Result<FileLockReport> {
        crate::lockers::find(self.platform.as_ref(), target)
            .map_err(|e| Error::Invalid(format!("没能查出是哪些程序在用：{e}")))
    }

    /// 「弹窗是哪个软件的」：鼠标现在指着的窗口是哪个程序的。第二项是程序的完整路径，只给外壳「打开所在的文件夹」用。
    pub fn window_owner(&self) -> Result<(crate::views::WindowOwnerReport, Option<std::path::PathBuf>)> {
        crate::window_owner::find(self.platform.as_ref())
            .map_err(|e| Error::Invalid(format!("没能看出是哪个程序的窗口：{e}")))
    }

    // ───────────── 右键菜单 ─────────────

    /// 列出软件加进右键菜单的项目（Windows 自带的不列）。显示不显示由引擎自己读，和改的时候读写同一个位置。
    pub fn context_menu_list(&self) -> Result<Vec<ContextMenuItem>> {
        let mut args = Map::new();
        args.insert("UserHive".into(), Value::String(self.user_hive()));
        let v = self
            .runner
            .run(context_menu::LIST_SCRIPT, &args, CONTEXT_MENU_LIST_TIMEOUT)
            .map_err(|e| Error::Invalid(format!("没能列出右键菜单：{e}")))?;
        let raw = context_menu::parse_list(&v).map_err(Error::Invalid)?;
        let groups = context_menu::group(raw);
        let mut items = Vec::with_capacity(groups.len());
        for g in &groups {
            let visible = self.menu_visible(&g.target)?;
            items.push(self.menu_view(g, visible));
        }
        items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        *self.context_menu.lock().unwrap() = groups;
        Ok(items)
    }

    /// 从右键菜单里拿掉（`visible` 为 false）或者恢复一项，记进修改日志，能撤销。只认最近一次列出来的项目。
    pub fn context_menu_set(&self, id: &str, visible: bool) -> Result<ApplyResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let group = self
            .context_menu
            .lock()
            .unwrap()
            .iter()
            .find(|g| g.target.id() == id)
            .cloned()
            .ok_or_else(|| Error::Invalid("这一项不在刚才的列表里了，请刷新一下再试。".to_owned()))?;
        let extension = matches!(group.target, context_menu::Target::Extension { .. });
        let mut r = ApplyResult {
            feature: context_menu::FEATURE_ID.to_owned(),
            session_id: self.session.clone(),
            entry_ids: Vec::new(),
            ok: true,
            verified: FeatureStateKind::Applied,
            message: "本来就是这样，不用改。".to_owned(),
            reboot: crate::model::Reboot::None,
            notes: Vec::new(),
            error: None,
        };
        if self.menu_visible(&group.target)? == visible {
            return Ok(r);
        }
        // 拿掉：写一个值；恢复：把能让它不显示的值都删掉（也包括别的工具写的）
        let actions: Vec<Action> = if visible {
            Self::menu_values(&group.target)
                .into_iter()
                .map(|(key, name)| Self::menu_action(key, name, false))
                .collect()
        } else {
            let (key, name) = Self::menu_values(&group.target).swap_remove(0);
            vec![Self::menu_action(key, name, true)]
        };
        let (entry_ids, failure) = self.apply_actions(context_menu::FEATURE_ID, &actions);
        r.entry_ids = entry_ids;
        if let Some(failed) = failure {
            r.ok = false;
            r.verified = FeatureStateKind::Unknown;
            r.message = if failed.rolled_back {
                "没有改成功，已经退回原样。".to_owned()
            } else {
                "没有改成功，而且改动没能自动退回，请在修改日志里手动恢复。".to_owned()
            };
            r.error = Some(failed.error.to_string());
            return Ok(r);
        }
        if self.menu_visible(&group.target)? != visible {
            r.verified = FeatureStateKind::NotApplied;
        }
        if extension {
            r.reboot = crate::model::Reboot::Explorer;
        }
        r.message = match (visible, extension) {
            (false, false) => "已经从右键菜单里拿掉了，下次右键就看不到了。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。",
            (false, true) => "已经拿掉了，重启资源管理器（或者注销再登录）以后生效。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。",
            (true, false) => "已经恢复了，下次右键就能看到。",
            (true, true) => "已经恢复了，重启资源管理器（或者注销再登录）以后就能看到。",
        }
        .to_owned();
        Ok(r)
    }

    /// 能让这一项不显示的值：第一个是小药箱拿掉时写的，后面的是别的工具可能写的（恢复时一起删）。
    fn menu_values(target: &context_menu::Target) -> Vec<(String, &str)> {
        match target {
            context_menu::Target::Verb { hive, scope, key } => {
                let k = context_menu::verb_key(*hive, scope, key);
                vec![(k.clone(), context_menu::HIDE_VALUE), (k, context_menu::LEGACY_HIDE_VALUE)]
            }
            context_menu::Target::Extension { clsid } => vec![
                (context_menu::blocked_key(context_menu::Hive::Machine), clsid.as_str()),
                (context_menu::blocked_key(context_menu::Hive::User), clsid.as_str()),
            ],
        }
    }

    /// 写一个空字符串值（拿掉），或者删掉这个值（恢复）。
    fn menu_action(key: String, name: &str, hide: bool) -> Action {
        Action::Registry(RegistryAction {
            key,
            name: name.to_owned(),
            value_type: hide.then_some(RegType::String),
            value: hide.then(|| Value::String(String::new())),
            delete: !hide,
        })
    }

    fn menu_visible(&self, target: &context_menu::Target) -> Result<bool> {
        for (key, name) in Self::menu_values(target) {
            let (_, state) = self.current(&Self::menu_action(key, name, false))?;
            if matches!(state, State::Registry { value: Some(_), .. }) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// 名字：命令用菜单上的字；扩展用程序文件里写的说明；应用用应用名。`@…`、`ms-resource:` 这类间接字符串解开再用。
    fn menu_title(&self, g: &context_menu::Group) -> String {
        let first = &g.entries[0];
        let resolve = |text: &str| -> Option<String> {
            let text = text.trim();
            let resolved = if text.starts_with('@') {
                self.platform.indirect_string(text)?
            } else if text.starts_with("ms-resource:") {
                let source = context_menu::package_resource(text, &first.package, &first.package_name)?;
                self.platform.indirect_string(&source)?
            } else {
                text.to_owned()
            };
            let clean = context_menu::strip_accelerator(&resolved);
            (!clean.is_empty() && clean.chars().count() <= 80).then_some(clean)
        };
        let description = || (!first.description.trim().is_empty()).then(|| first.description.trim().to_owned());
        let title = match first.kind {
            context_menu::Kind::Verb => resolve(&first.text).or_else(description),
            context_menu::Kind::Handler => description().or_else(|| resolve(&first.text)),
            context_menu::Kind::Packaged => {
                resolve(&first.text).or_else(|| (!first.package_name.is_empty()).then(|| first.package_name.clone()))
            }
        };
        title.unwrap_or_else(|| first.key.clone())
    }

    fn menu_view(&self, g: &context_menu::Group, visible: bool) -> ContextMenuItem {
        let first = &g.entries[0];
        let kind = match (&g.target, first.kind) {
            (context_menu::Target::Verb { .. }, _) => ContextMenuKind::Command,
            (_, context_menu::Kind::Packaged) => ContextMenuKind::App,
            _ => ContextMenuKind::Extension,
        };
        let publisher = [&first.signer, &first.company, &first.publisher]
            .into_iter()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
            .map(str::to_owned);
        let location = match &g.target {
            context_menu::Target::Verb { hive: context_menu::Hive::Machine, .. } => "所有用户",
            context_menu::Target::Verb { .. } => "当前用户",
            context_menu::Target::Extension { .. } => "",
        };
        let note = if g.entries.iter().any(|e| e.kind == context_menu::Kind::Handler && !e.class_found) {
            "这个扩展已经没有登记了，多半是软件卸载后留下的，拿掉没有坏处。"
        } else if !first.path.is_empty() && !first.exists {
            "找不到它要用的程序，软件可能已经卸载了，拿掉没有坏处。"
        } else if first.subcommands {
            "这一项下面还有子菜单，会一起拿掉。"
        } else if kind != ContextMenuKind::Command && g.entries.len() > 1 {
            "同一个软件在右键菜单里的几处会一起拿掉。"
        } else {
            ""
        };
        ContextMenuItem {
            id: g.target.id(),
            kind,
            title: self.menu_title(g),
            program: first.path.rsplit(['\\', '/']).next().unwrap_or_default().to_owned(),
            path: first.path.clone(),
            exists: first.exists,
            publisher,
            signature: first.signature,
            scopes: context_menu::scope_labels(&g.entries),
            location: location.to_owned(),
            visible,
            shift_only: g.entries.iter().any(|e| e.extended),
            note: note.to_owned(),
        }
    }

    /// 修改日志里右键菜单的标题：最近一次列表里的名字；程序重启以后没有列表，外壳扩展用它登记的类名，
    /// 命令用它的键名。
    fn menu_journal_title(&self, key: &str, name: &str) -> String {
        let clsid = name.to_ascii_uppercase();
        let cached = self.context_menu.lock().unwrap().iter().find_map(|g| match &g.target {
            context_menu::Target::Extension { clsid: c } if *c == clsid => Some(g.clone()),
            context_menu::Target::Verb { hive, scope, key: k } => {
                let full = context_menu::verb_key(*hive, scope, k);
                full[5..].eq_ignore_ascii_case(key).then(|| g.clone())
            }
            _ => None,
        });
        if let Some(g) = cached {
            return self.menu_title(&g);
        }
        if name.eq_ignore_ascii_case(context_menu::HIDE_VALUE)
            || name.eq_ignore_ascii_case(context_menu::LEGACY_HIDE_VALUE)
        {
            return key.rsplit('\\').next().unwrap_or(key).to_owned();
        }
        let class = format!(r"SOFTWARE\Classes\CLSID\{clsid}");
        match self.platform.reg_get(&RegRoot::LocalMachine, &class, "") {
            Ok(Some(RegValue::String(text))) if !text.trim().is_empty() => text.trim().to_owned(),
            _ => clsid,
        }
    }

    /// 修改日志里右键菜单开关的状态，说人话。
    fn menu_state_label(state: &State) -> String {
        match state {
            State::Registry { value: Some(_), .. } => "不显示（已拿掉）".to_owned(),
            State::Registry { value: None, .. } => "显示".to_owned(),
            other => Self::state_label(other),
        }
    }

    // ───────────── 「新建」菜单 ─────────────

    /// 「新建」菜单里的项（软件加的和 Windows 自带的），按名字排好。
    pub fn new_menu_list(&self) -> Result<Vec<NewMenuItem>> {
        let mut args = Map::new();
        args.insert("UserHive".into(), Value::String(self.user_hive()));
        let v = self
            .runner
            .run(new_menu::LIST_SCRIPT, &args, NEW_MENU_LIST_TIMEOUT)
            .map_err(|e| Error::Invalid(format!("没能列出「新建」菜单：{e}")))?;
        let groups = new_menu::group(new_menu::parse_list(&v).map_err(Error::Invalid)?);
        let mut items = Vec::with_capacity(groups.len());
        for g in &groups {
            items.push(NewMenuItem {
                id: g.ext.clone(),
                title: self.new_menu_title(g),
                ext: g.ext.clone(),
                windows_own: g.windows_own(),
                location: g.scope().to_owned(),
                // 按注册表里现在的值（和改开关时用的是同一个判断）
                visible: self.new_menu_visible(g)?,
            });
        }
        items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        *self.new_menu.lock().unwrap() = groups;
        Ok(items)
    }

    /// 从「新建」菜单里关掉（`visible` 为 false）或者恢复一项，记进修改日志，能撤销。只认最近一次列出来的项目。
    pub fn new_menu_set(&self, id: &str, visible: bool) -> Result<ApplyResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let group = self
            .new_menu
            .lock()
            .unwrap()
            .iter()
            .find(|g| g.ext == id)
            .cloned()
            .ok_or_else(|| Error::Invalid("这一项不在刚才的列表里了，请刷新一下再试。".to_owned()))?;
        let mut r = ApplyResult {
            feature: new_menu::FEATURE_ID.to_owned(),
            session_id: self.session.clone(),
            entry_ids: Vec::new(),
            ok: true,
            verified: FeatureStateKind::Applied,
            message: "本来就是这样，不用改。".to_owned(),
            reboot: crate::model::Reboot::None,
            notes: Vec::new(),
            error: None,
        };
        if self.new_menu_visible(&group)? == visible {
            return Ok(r);
        }
        // 关掉：让它出现的值改名；恢复：改回来。按键里现在的值来（列表可能是一会儿以前的）
        let mut actions = Vec::new();
        for loc in &group.locations {
            let key = loc.key();
            for name in new_menu::DEFINING {
                let (from, to) = if visible {
                    (new_menu::hidden_name(name), (*name).to_owned())
                } else {
                    ((*name).to_owned(), new_menu::hidden_name(name))
                };
                let Some(value) = self.registry_value(&key, &from)? else {
                    continue;
                };
                // 恢复时原来的名字已经有值了（软件自己又写了一遍）：留着它，只删掉改过名的
                if !(visible && self.registry_value(&key, &to)?.is_some()) {
                    actions.push(Self::value_action(&key, &to, Some(&value)));
                }
                actions.push(Self::value_action(&key, &from, None));
            }
        }
        let (entry_ids, failure) = self.apply_actions(new_menu::FEATURE_ID, &actions);
        r.entry_ids = entry_ids;
        if let Some(failed) = failure {
            r.ok = false;
            r.verified = FeatureStateKind::Unknown;
            r.message = if failed.rolled_back {
                "没有改成功，已经退回原样。".to_owned()
            } else {
                "没有改成功，而且改动没能自动退回，请在修改日志里手动恢复。".to_owned()
            };
            r.error = Some(failed.error.to_string());
            return Ok(r);
        }
        self.clear_new_menu_cache();
        if self.new_menu_visible(&group)? != visible {
            r.verified = FeatureStateKind::NotApplied;
        }
        r.message = if visible {
            "已经恢复了，下次在右键「新建」里就能看到。"
        } else {
            "已经从右键「新建」菜单里拿掉了。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。"
        }
        .to_owned();
        Ok(r)
    }

    /// 菜单上的字：MenuText，没有就是类型名（资源管理器也是这样显示的），再没有才用 ItemName（新建出来的文件的
    /// 名字，比如「新建 文本文档」）；`@…` 解开，去掉快捷键的 `&`；都没有就用扩展名。
    fn new_menu_title(&self, g: &new_menu::Group) -> String {
        [&g.menu_text, &g.type_name, &g.item_name]
            .into_iter()
            .find_map(|text| {
                let text = text.trim();
                let resolved =
                    if text.starts_with('@') { self.platform.indirect_string(text)? } else { text.to_owned() };
                let clean = context_menu::strip_accelerator(&resolved);
                (!clean.is_empty() && clean.chars().count() <= 80).then_some(clean)
            })
            .unwrap_or_else(|| g.ext.clone())
    }

    /// 现在显示不显示：资源管理器用的键里（按注册表里现在的值）有没有让它出现的值
    fn new_menu_visible(&self, g: &new_menu::Group) -> Result<bool> {
        for loc in g.locations.iter().filter(|l| l.counts(&g.current_progid)) {
            for name in new_menu::DEFINING {
                if self.registry_value(&loc.key(), name)?.is_some() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// 注册表里一个值现在的样子（`key` 带根，HKCU 换成登录用户的）。
    fn registry_value(&self, key: &str, name: &str) -> Result<Option<RegValue>> {
        Ok(match self.current(&Self::value_action(key, name, None))?.1 {
            State::Registry { value, .. } => value,
            _ => None,
        })
    }

    /// 写一个值（`value` 给了），或者删掉它。
    fn value_action(key: &str, name: &str, value: Option<&RegValue>) -> Action {
        let spec = value.map(RegValue::to_spec);
        Action::Registry(RegistryAction {
            key: key.to_owned(),
            name: name.to_owned(),
            delete: spec.is_none(),
            value_type: spec.as_ref().map(|(t, _)| *t),
            value: spec.map(|(_, v)| v),
        })
    }

    /// 删掉资源管理器对「新建」菜单的缓存（它下次打开菜单时重建），不记进修改日志：缓存不是设置。删不掉也不要紧。
    fn clear_new_menu_cache(&self) {
        let key = format!(r"HKCU\{}", new_menu::CACHE_KEY);
        for name in new_menu::CACHE_VALUES {
            let Action::Registry(action) = Self::value_action(&key, name, None) else {
                continue;
            };
            if let Ok((root, sub)) = self.resolve_registry(&action) {
                let _ = self.platform.reg_delete_value(&root, &sub, name);
            }
        }
    }

    fn new_menu_journal_title(&self, key: &str) -> String {
        let Some(ext) = new_menu::ext_of_key(key) else {
            return key.to_owned();
        };
        let cached = self.new_menu.lock().unwrap().iter().find(|g| g.ext == ext).cloned();
        cached.map_or(ext, |g| self.new_menu_title(&g))
    }

    /// 修改日志里「新建」菜单开关的状态：原来的值在是「显示」，改过名的值在是「不显示」。
    fn new_menu_state_label(state: &State, renamed: bool) -> String {
        match state {
            State::Registry { value, .. } if value.is_some() != renamed => "显示".to_owned(),
            State::Registry { .. } => "不显示（已关掉）".to_owned(),
            other => Self::state_label(other),
        }
    }

    // ───────────── 资源管理器里多出来的图标 ─────────────

    /// 软件加在资源管理器导航栏和「此电脑」里的图标（Windows 自己的基本位置不列），按名字排好。
    pub fn shell_places_list(&self) -> Result<Vec<ShellPlaceItem>> {
        let mut args = Map::new();
        args.insert("UserHive".into(), Value::String(self.user_hive()));
        let v = self
            .runner
            .run(shell_places::LIST_SCRIPT, &args, SHELL_PLACES_LIST_TIMEOUT)
            .map_err(|e| Error::Invalid(format!("没能列出资源管理器里的图标：{e}")))?;
        let groups = shell_places::group(shell_places::parse_list(&v).map_err(Error::Invalid)?);
        let mut items = Vec::with_capacity(groups.len());
        for g in &groups {
            // 按注册表里现在的值（和改开关时用的是同一个判断）
            let state = self.shell_place_state(g)?;
            let note = if state.hidden_for_everyone(g) {
                "是在所有用户的设置里隐藏的，恢复以后这台电脑上的所有用户都能看到。"
            } else {
                ""
            };
            items.push(ShellPlaceItem {
                id: g.clsid.clone(),
                title: self.shell_place_title(g),
                places: g.places(),
                windows_own: g.windows_own(),
                visible: state.visible(g),
                note: note.to_owned(),
            });
        }
        items.sort_by_key(|i| i.title.to_lowercase());
        *self.shell_places.lock().unwrap() = groups;
        Ok(items)
    }

    /// 隐藏（`visible` 为 false）或者恢复一个图标，记进修改日志，能撤销。只认最近一次列出来的项目。
    pub fn shell_places_set(&self, id: &str, visible: bool) -> Result<ApplyResult> {
        let _guard = self.apply_lock.lock().unwrap();
        let group = self
            .shell_places
            .lock()
            .unwrap()
            .iter()
            .find(|g| g.clsid == id)
            .cloned()
            .ok_or_else(|| Error::Invalid("这一项不在刚才的列表里了，请刷新一下再试。".to_owned()))?;
        let mut r = ApplyResult {
            feature: shell_places::FEATURE_ID.to_owned(),
            session_id: self.session.clone(),
            entry_ids: Vec::new(),
            ok: true,
            verified: FeatureStateKind::Applied,
            message: "本来就是这样，不用改。".to_owned(),
            reboot: crate::model::Reboot::None,
            notes: Vec::new(),
            error: None,
        };
        let state = self.shell_place_state(&group)?;
        if state.visible(&group) == visible {
            return Ok(r);
        }
        let mut actions = Vec::new();
        // 删掉值以后空了就删掉的键（隐藏时新建的那份用户的键）：不记进修改日志，撤销时写回值会把键建回来
        let mut drop_keys = Vec::new();
        let non_enum = [shell_places::Hive::User, shell_places::Hive::Machine]
            .map(|hive| (shell_places::non_enum_key(hive), hive));
        if !visible && group.in_pc {
            // 「此电脑」里有的：外壳哪里都不列它（导航栏里也登记了的一起隐藏）
            let hide = RegValue::Dword(1);
            actions.push(Self::value_action(&non_enum[0].0, &group.clsid, Some(&hide)));
        } else {
            if visible {
                for (key, hive) in &non_enum {
                    if state.non_enum(*hive) {
                        actions.push(Self::value_action(key, &group.clsid, None));
                    }
                }
            }
            // 导航栏的开关：「此电脑」里也有的，恢复时只去掉 NonEnum，不动它（那是软件自己的设置）
            if group.in_nav && !(visible && group.in_pc) {
                // 64 位程序（资源管理器）看的那一份，和 32 位程序看的那一份；按注册表里现在的值来
                for wow in [false, true] {
                    match shell_places::plan(self.shell_place_pinned(&group.clsid, wow)?, visible) {
                        Some(shell_places::Change::Write(hive, value)) => actions.push(Self::value_action(
                            &shell_places::class_key(hive, wow, &group.clsid),
                            shell_places::PINNED_VALUE,
                            Some(&RegValue::Dword(value)),
                        )),
                        Some(shell_places::Change::DropUser) => {
                            let key = shell_places::class_key(shell_places::Hive::User, wow, &group.clsid);
                            actions.push(Self::value_action(&key, shell_places::PINNED_VALUE, None));
                            drop_keys.push(key);
                        }
                        None => {}
                    }
                }
            }
        }
        if actions.is_empty() {
            // 列表以后注册表里的值变了（比如软件自己删掉了这个开关）
            r.ok = false;
            r.verified = FeatureStateKind::Unknown;
            r.message = "这一项的设置和刚才列出来的时候不一样了，请刷新一下列表再试。".to_owned();
            return Ok(r);
        }
        let (entry_ids, failure) = self.apply_actions(shell_places::FEATURE_ID, &actions);
        r.entry_ids = entry_ids;
        if let Some(failed) = failure {
            r.ok = false;
            r.verified = FeatureStateKind::Unknown;
            r.message = if failed.rolled_back {
                "没有改成功，已经退回原样。".to_owned()
            } else {
                "没有改成功，而且改动没能自动退回，请在修改日志里手动恢复。".to_owned()
            };
            r.error = Some(failed.error.to_string());
            return Ok(r);
        }
        for key in drop_keys {
            // 删不掉也不要紧：键里还有别的东西（那就不该删），或者被安全软件锁住了
            if let Ok((root, sub)) = self.resolve_key(&key) {
                let _ = self.platform.reg_delete_key_if_empty(&root, &sub);
            }
        }
        if self.shell_place_state(&group)?.visible(&group) != visible {
            r.verified = FeatureStateKind::NotApplied;
        }
        // 已经开着的资源管理器窗口不一定马上变，给「现在重启资源管理器」
        r.reboot = crate::model::Reboot::Explorer;
        r.message = if visible {
            "已经恢复了，新打开的资源管理器窗口里就能看到；还看不到的话，重启一下资源管理器。"
        } else {
            "已经隐藏了，新打开的资源管理器窗口里就看不到了；还看得到的话，重启一下资源管理器。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。"
        }
        .to_owned();
        Ok(r)
    }

    /// 名字：LocalizedString、CLSID 键的默认值、NameSpace 键的默认值，`@…` 解开再用；都没有就用 CLSID。
    fn shell_place_title(&self, g: &shell_places::Group) -> String {
        [&g.localized, &g.title, &g.name]
            .into_iter()
            .find_map(|text| {
                let text = text.trim();
                let resolved =
                    if text.starts_with('@') { self.platform.indirect_string(text)? } else { text.to_owned() };
                let clean = resolved.trim().to_owned();
                (!clean.is_empty() && clean.chars().count() <= 80).then_some(clean)
            })
            .unwrap_or_else(|| g.clsid.clone())
    }

    /// 一个图标现在的开关（按注册表里现在的值）。
    fn shell_place_state(&self, g: &shell_places::Group) -> Result<ShellPlaceState> {
        let on = |key: &str| -> Result<bool> {
            Ok(matches!(self.registry_value(key, &g.clsid)?, Some(RegValue::Dword(v)) if v != 0))
        };
        Ok(ShellPlaceState {
            non_enum_user: on(&shell_places::non_enum_key(shell_places::Hive::User))?,
            non_enum_machine: on(&shell_places::non_enum_key(shell_places::Hive::Machine))?,
            pinned: if g.in_nav { self.shell_place_pinned(&g.clsid, false)? } else { shell_places::Pinned::default() },
        })
    }

    /// 一处（`wow`：32 位程序看的那一份）System.IsPinnedToNameSpaceTree 现在的样子。
    fn shell_place_pinned(&self, clsid: &str, wow: bool) -> Result<shell_places::Pinned> {
        let number = |v: Option<RegValue>| match v {
            Some(RegValue::Dword(n)) => Some(u64::from(n)),
            Some(RegValue::Qword(n)) => Some(n),
            _ => None,
        };
        let value = |hive| {
            self.registry_value(&shell_places::class_key(hive, wow, clsid), shell_places::PINNED_VALUE).map(number)
        };
        Ok(shell_places::Pinned {
            user: value(shell_places::Hive::User)?,
            machine: value(shell_places::Hive::Machine)?,
        })
    }

    /// 修改日志里的标题：最近一次列表里的名字；程序重启以后没有列表，用 CLSID 键里登记的名字。
    fn shell_place_journal_title(&self, clsid: &str) -> String {
        let cached = self.shell_places.lock().unwrap().iter().find(|g| g.clsid == clsid).cloned();
        if let Some(g) = cached {
            return self.shell_place_title(&g);
        }
        for key in [
            shell_places::class_key(shell_places::Hive::User, false, clsid),
            shell_places::class_key(shell_places::Hive::Machine, false, clsid),
        ] {
            for name in ["LocalizedString", ""] {
                if let Ok(Some(RegValue::String(text) | RegValue::ExpandString(text))) = self.registry_value(&key, name)
                {
                    let text = text.trim();
                    let resolved =
                        if text.starts_with('@') { self.platform.indirect_string(text) } else { Some(text.to_owned()) };
                    if let Some(t) = resolved.filter(|t| !t.trim().is_empty()) {
                        return t.trim().to_owned();
                    }
                }
            }
        }
        clsid.to_owned()
    }

    /// 修改日志里图标开关的状态，说人话。`pinned`：是导航栏的 System.IsPinnedToNameSpaceTree（不然是 NonEnum 的）。
    fn shell_place_state_label(state: &State, pinned: bool) -> String {
        match state {
            State::Registry { value: Some(RegValue::Dword(0)), .. } if pinned => "不显示（已隐藏）".to_owned(),
            State::Registry { value: Some(_), .. } if pinned => "显示".to_owned(),
            State::Registry { value: None, .. } if pinned => "没有单独设置".to_owned(),
            State::Registry { value: Some(RegValue::Dword(v)), .. } if *v != 0 => "不显示（已隐藏）".to_owned(),
            State::Registry { .. } => "显示".to_owned(),
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
            // 恢复原状和当初修改一样，要重启资源管理器、注销之后才看得到；改键要重启电脑
            r.reboot = if entry.apply.feature == keymap::FEATURE_ID {
                crate::model::Reboot::Reboot
            } else {
                self.catalog.feature(&entry.apply.feature).map_or_else(Default::default, |f| f.reboot)
            };
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
            // 右键菜单的开关也一样：标题用项目的名字，状态说「显示 / 不显示」
            let menu_entry = e.apply.feature == context_menu::FEATURE_ID
                && matches!(&e.apply.target, TargetRef::Registry { key, name, .. } if context_menu::is_menu_target(key, name));
            // 「新建」菜单也一样；一次开关是改名，写新名字、删旧名字两条，状态都说「显示 / 不显示」
            let new_menu_entry = e.apply.feature == new_menu::FEATURE_ID
                && matches!(&e.apply.target, TargetRef::Registry { key, name, .. } if new_menu::is_new_menu_target(key, name));
            let renamed =
                matches!(&e.apply.target, TargetRef::Registry { name, .. } if new_menu::original_name(name).is_some());
            // 资源管理器里的图标：标题用图标的名字，状态说「显示 / 不显示」
            let place_clsid = match &e.apply.target {
                TargetRef::Registry { key, name, .. } if e.apply.feature == shell_places::FEATURE_ID => {
                    shell_places::target_clsid(key, name)
                }
                _ => None,
            };
            let pinned = matches!(&e.apply.target, TargetRef::Registry { name, .. }
                if name.eq_ignore_ascii_case(shell_places::PINNED_VALUE));
            // 改键：状态说成「Caps Lock → 左 Ctrl」这样，不显示一串十六进制
            let keymap_entry = e.apply.feature == keymap::FEATURE_ID;
            let label = |s: &State| {
                if startup_entry {
                    Self::startup_state_label(s)
                } else if keymap_entry {
                    Self::key_remap_state_label(s)
                } else if menu_entry {
                    Self::menu_state_label(s)
                } else if new_menu_entry {
                    Self::new_menu_state_label(s, renamed)
                } else if place_clsid.is_some() {
                    Self::shell_place_state_label(s, pinned)
                } else {
                    Self::state_label(s)
                }
            };
            let feature_title = match &e.apply.target {
                TargetRef::Registry { name, .. } if startup_entry => format!("开机启动项：{name}"),
                TargetRef::Registry { key, name, .. } if menu_entry => {
                    format!("右键菜单：{}", self.menu_journal_title(key, name))
                }
                TargetRef::Registry { key, .. } if new_menu_entry => {
                    format!("「新建」菜单：{}", self.new_menu_journal_title(key))
                }
                _ if place_clsid.is_some() => format!(
                    "资源管理器里的图标：{}",
                    self.shell_place_journal_title(place_clsid.as_deref().unwrap_or_default())
                ),
                _ if e.apply.feature == LEGACY_STARTUP_FEATURE => "停用开机启动项".to_owned(),
                _ if keymap_entry => "键位重映射（改键）".to_owned(),
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

    /// 诊断报告（纯文本、已脱敏）。`note` 是用户自己写的「遇到了什么问题」，放在最前面，
    /// 和报告的其余部分一起脱敏；太长的只留前 [`NOTE_MAX_CHARS`] 个字。
    pub fn report_generate(&self, note: Option<&str>) -> Result<String> {
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

        if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
            line(&mut out, "== 我遇到的问题 ==");
            let clean: String = note.chars().filter(|c| !c.is_control() || *c == '\n').collect();
            let mut kept: String = clean.chars().take(NOTE_MAX_CHARS).collect();
            if clean.chars().count() > NOTE_MAX_CHARS {
                kept.push_str("……（后面的省略了）");
            }
            line(&mut out, kept.trim_end());
            line(&mut out, "");
        }

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

/// 报告里「我遇到的问题」最多留多少个字。
pub const NOTE_MAX_CHARS: usize = 1000;

/// 值里可能有单位名、学校名、VPN 名的检测事实，写报告时隐藏。
const SENSITIVE_FACTS: &[&str] =
    &["proxy_address", "proxy_server", "pac_url", "connection", "dialup_proxy", "dead_dialup"];

/// 「联想官网」「微软 Surface 官网」：中文和英文、数字之间留一个空格（和界面上的写法一样）。
fn official_site(brand: &str) -> String {
    let gap = if brand.ends_with(|c: char| c.is_ascii_alphanumeric()) { " " } else { "" };
    format!("{brand}{gap}官网")
}

/// 平台错误说给用户听的话：`Other` 里已经是完整的一句，不再加「系统操作失败」。
fn platform_text(e: &PlatformError) -> String {
    match e {
        PlatformError::Other(msg) => msg.clone(),
        other => other.to_string(),
    }
}

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
