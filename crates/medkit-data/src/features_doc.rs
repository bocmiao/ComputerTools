//! 功能清单：从 catalog 生成 docs/features.md，并更新 README.md 里「现在能做什么」的总表（两个标记之间的部分）。
//! 加了检测、功能、症状、小工具以后运行 `cargo run -p medkit-data -- features`；CI 用 `--check` 检查是不是最新的。

use std::fmt::Write as _;

use medkit_core::catalog::CatalogData;
use medkit_core::model::{
    Check, Feature, Maturity, Reboot, Risk, Symptom, SymptomCategory, Tool, ToolGroup, Undo, UndoKeyword,
};

pub const DOC_PATH: &str = "docs/features.md";
pub const README_PATH: &str = "README.md";
pub const README_START: &str = "<!-- features:start -->";
pub const README_END: &str = "<!-- features:end -->";

/// 「常用设置」页上的分类和顺序，和 app/src/labels.ts 的 settingsCategories 一样（有测试对照）。
/// 这些分类的功能算常用设置，其余的功能是体检、症状页上的修复。
const SETTINGS_CATEGORIES: &[(&str, &str)] = &[
    ("explorer", "资源管理器"),
    ("desktop", "桌面"),
    ("taskbar", "任务栏"),
    ("start", "开始菜单"),
    ("ads", "推荐和广告"),
    ("input", "键盘和鼠标"),
    ("power", "电源"),
    ("windows-update", "Windows 更新"),
];

/// 检测和修复的分类，按这个顺序分组。新加分类要在这里写上中文名。
const OTHER_CATEGORIES: &[(&str, &str)] = &[
    ("network", "网络"),
    ("system", "系统"),
    ("boot", "开机"),
    ("disk", "硬盘"),
    ("update", "Windows 更新"),
    ("security", "安全"),
    ("privacy", "隐私"),
    ("printer", "打印机"),
    ("display", "显示"),
    ("audio", "声音"),
    ("hardware", "硬件"),
];

/// 小工具的分组：标题和一句说明。
const TOOL_SECTIONS: [(&str, &str); 6] = [
    ("一键处理", "点一下就做完，不留下持久的改动。"),
    ("查看信息", "只看不改，结果显示成表格。"),
    ("打开系统工具", "打开 Windows 自带的工具。"),
    ("打开「设置」里的页面", "直接打开「设置」里对应的那一页。"),
    ("微软的疑难解答", "运行「获取帮助」里微软自己的自动疑难解答。"),
    ("官方网页", "用你自己的浏览器打开核实过的官方网页。"),
];

pub struct Rendered {
    /// docs/features.md 的全文
    pub doc: String,
    /// README.md 里两个标记之间的总表
    pub readme_table: String,
}

/// 表格的一格：换行变成空格，竖线、尖括号转义。
fn cell(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        match ch {
            '\r' => {}
            '\n' => out.push(' '),
            '|' => out.push_str("\\|"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 按分类分组，组的顺序照 `order`，组里保持目录里的顺序（按文件名）。分类不在 `order` 里的报错。
fn group<'a, T>(
    items: &[&'a T],
    category: impl Fn(&T) -> &str,
    id: impl Fn(&T) -> &str,
    order: &[(&'static str, &'static str)],
) -> Result<Vec<(&'static str, Vec<&'a T>)>, String> {
    for item in items {
        let c = category(item);
        if !order.iter().any(|(k, _)| *k == c) {
            return Err(format!(
                "{} 的分类 {c} 没有中文名：在 crates/medkit-data/src/features_doc.rs 的分类表里加上",
                id(item)
            ));
        }
    }
    Ok(order
        .iter()
        .map(|(k, title)| (*title, items.iter().copied().filter(|i| category(i) == *k).collect::<Vec<_>>()))
        .filter(|(_, v)| !v.is_empty())
        .collect())
}

fn tool_section(t: &Tool) -> Result<usize, String> {
    Ok(match t.group {
        ToolGroup::Action => 0,
        ToolGroup::Info => 1,
        ToolGroup::Open => match &t.open {
            Some(o) if o.program.is_some() => 2,
            Some(o) if o.settings.is_some() => 3,
            Some(o) if o.troubleshooter.is_some() => 4,
            Some(o) if o.website.is_some() => 5,
            _ => return Err(format!("小工具 {} 是 open，但没写打开什么", t.id)),
        },
    })
}

fn feature_notes(f: &Feature) -> String {
    let mut notes = Vec::new();
    match f.risk {
        Risk::Safe => {}
        Risk::Caution => notes.push("要注意"),
        Risk::Danger => notes.push("要谨慎"),
    }
    if matches!(f.undo, Undo::Keyword(UndoKeyword::None)) {
        notes.push("不能撤销");
    }
    match f.reboot {
        Reboot::None => {}
        Reboot::Explorer => notes.push("重启资源管理器后生效"),
        Reboot::Logoff => notes.push("注销后生效"),
        Reboot::Reboot => notes.push("重启后生效"),
    }
    notes.join("；")
}

fn maturity_label(m: Maturity) -> &'static str {
    match m {
        Maturity::OneClick => "一键修复",
        Maturity::Semi => "半自动",
        Maturity::Guide => "图文指引",
    }
}

fn check_table(out: &mut String, checks: &[&Check]) {
    out.push_str("| 检测 | 查什么 |\n| --- | --- |\n");
    for c in checks {
        let desc = c.description.as_ref().map(|d| d.zh()).unwrap_or("");
        let _ = writeln!(out, "| {} | {} |", cell(c.title.zh()), cell(desc));
    }
}

fn feature_table(out: &mut String, head: &str, features: &[&Feature]) {
    let _ = writeln!(out, "| {head} | 说明 | 备注 |\n| --- | --- | --- |");
    for f in features {
        let _ = writeln!(out, "| {} | {} | {} |", cell(f.title.zh()), cell(f.description.zh()), feature_notes(f));
    }
}

fn symptom_table(out: &mut String, symptoms: &[&Symptom]) {
    out.push_str("| 症状 | 说明 | 怎么修 |\n| --- | --- | --- |\n");
    for s in symptoms {
        let summary = s.summary.as_ref().map(|t| t.zh()).unwrap_or("");
        let _ = writeln!(out, "| {} | {} | {} |", cell(s.title.zh()), cell(summary), maturity_label(s.maturity));
    }
}

fn tool_table(out: &mut String, tools: &[&Tool]) {
    out.push_str("| 小工具 | 说明 |\n| --- | --- |\n");
    for t in tools {
        let helper = if matches!(t.audience, medkit_core::model::Audience::Helper) { "（给懂哥）" } else { "" };
        let _ = writeln!(out, "| {}{helper} | {} |", cell(t.title.zh()), cell(t.description.zh()));
    }
}

pub fn render(c: &CatalogData) -> Result<Rendered, String> {
    let healthcheck = c
        .profiles
        .iter()
        .find(|p| p.id == "healthcheck")
        .ok_or("找不到体检的检测清单（catalog/profiles/healthcheck.yaml）")?;
    let health: Vec<&Check> = healthcheck
        .checks
        .iter()
        .map(|id| c.checks.iter().find(|ch| &ch.id == id).ok_or(format!("体检里的检测不存在：{id}")))
        .collect::<Result<_, _>>()?;

    let all_categories: Vec<(&str, &str)> = SETTINGS_CATEGORIES.iter().chain(OTHER_CATEGORIES).copied().collect();
    let checks: Vec<&Check> = c.checks.iter().collect();
    let check_groups = group(&checks, |x| &x.category, |x| &x.id, &all_categories)?;

    let (settings, fixes): (Vec<&Feature>, Vec<&Feature>) =
        c.features.iter().partition(|f| SETTINGS_CATEGORIES.iter().any(|(k, _)| *k == f.category));
    let setting_groups = group(&settings, |x| &x.category, |x| &x.id, SETTINGS_CATEGORIES)?;
    let fix_groups = group(&fixes, |x| &x.category, |x| &x.id, OTHER_CATEGORIES)?;

    let mut tool_groups: Vec<Vec<&Tool>> = vec![Vec::new(); TOOL_SECTIONS.len()];
    for t in &c.tools {
        tool_groups[tool_section(t)?].push(t);
    }

    let symptom_groups: Vec<(SymptomCategory, Vec<&Symptom>)> = SymptomCategory::ALL
        .iter()
        .map(|cat| (*cat, c.symptoms.iter().filter(|s| s.category == *cat).collect::<Vec<_>>()))
        .filter(|(_, v)| !v.is_empty())
        .collect();

    let (n_health, n_symptoms, n_settings, n_fixes, n_tools, n_checks) =
        (health.len(), c.symptoms.len(), settings.len(), fixes.len(), c.tools.len(), c.checks.len());

    let mut d = String::new();
    d.push_str("# 功能清单\n\n");
    d.push_str("> 这个文件由 `cargo run -p medkit-data -- features` 从 `catalog/` 里的数据生成，不要手改。\n\n");
    let _ = writeln!(
        d,
        "现在一共有：体检 {n_health} 项、症状 {n_symptoms} 个、常用设置 {n_settings} 项、修复 {n_fixes} 项、小工具 {n_tools} 个、检测 {n_checks} 项。\
         在本机处理文字、图片和文件的工具箱，见 README 里的[工具箱](../README.md#工具箱)。\n"
    );
    d.push_str("- [体检](#体检)\n- [按症状修](#按症状修)\n- [常用设置](#常用设置)\n- [修复](#修复)\n- [小工具](#小工具)\n- [全部检测](#全部检测)\n\n");

    d.push_str("## 体检\n\n点「开始体检」一次查完下面这些，只看不改。查出来的问题里小药箱能修的，集中列在一起，勾上一次修好。\n\n");
    check_table(&mut d, &health);

    d.push_str(
        "\n## 按症状修\n\n在「按症状修」里搜大白话或者错误代码。每个症状一步步查原因：能修的给修复按钮，要自己动手的给图文步骤，修不了的说清楚下一步找谁。\n",
    );
    for (cat, symptoms) in &symptom_groups {
        let _ = writeln!(d, "\n### {}（{} 个）\n", cat.title(), symptoms.len());
        symptom_table(&mut d, symptoms);
    }

    d.push_str("\n## 常用设置\n\n在「常用设置」页上，改之前能预览要改什么，改完能在「修改日志」里撤销。\n");
    for (title, features) in &setting_groups {
        let _ = writeln!(d, "\n### {title}（{} 项）\n", features.len());
        feature_table(&mut d, "设置", features);
    }

    d.push_str(
        "\n## 修复\n\n体检和症状页查出问题时给的修复按钮，把被「优化」软件、病毒、网上的教程改坏的地方改回来。改之前能预览，不能撤销的会事先说明。\n",
    );
    for (title, features) in &fix_groups {
        let _ = writeln!(d, "\n### {title}（{} 项）\n", features.len());
        feature_table(&mut d, "修复", features);
    }

    d.push_str("\n## 小工具\n\n在「小工具」页上；相关症状、检测结果的下面也有按钮直接打开。\n");
    for ((title, note), tools) in TOOL_SECTIONS.iter().zip(&tool_groups) {
        if tools.is_empty() {
            continue;
        }
        let _ = writeln!(d, "\n### {title}（{} 个）\n\n{note}\n", tools.len());
        tool_table(&mut d, tools);
    }

    d.push_str("\n## 全部检测\n\n体检和症状页用到的全部检测，都只看不改。\n");
    for (title, checks) in &check_groups {
        let _ = writeln!(d, "\n### {title}（{} 项）\n", checks.len());
        check_table(&mut d, checks);
    }

    let mut t = String::new();
    t.push_str("| 模块 | 数量 | 做什么 |\n| --- | --- | --- |\n");
    let rows = [
        (
            "体检",
            "体检",
            format!("{n_health} 项"),
            "一次查完硬盘、系统、更新、网络、安全这些，只看不改；能修的集中列出，勾上一次修好",
        ),
        (
            "按症状修",
            "按症状修",
            format!("{n_symptoms} 个"),
            "用大白话或者错误代码搜，一步步查原因；能修的给修复按钮，修不了的说清楚下一步找谁",
        ),
        (
            "常用设置",
            "常用设置",
            format!("{n_settings} 项"),
            "资源管理器、任务栏、开始菜单、推荐和广告、键盘鼠标、电源这些设置，改之前预览，改完能撤销",
        ),
        ("修复", "修复", format!("{n_fixes} 项"), "把被「优化」软件、病毒、网上的教程改坏的设置改回来，大多能撤销"),
        (
            "小工具",
            "小工具",
            format!("{n_tools} 个"),
            "一键处理（刷新 DNS、重建图标缓存……）、查看信息（WiFi 密码、最近的蓝屏……），还有直达系统设置和微软疑难解答的按钮",
        ),
        ("检测", "全部检测", format!("{n_checks} 项"), "体检和症状页用到的全部检测，都只看不改"),
    ];
    for (name, anchor, count, what) in rows {
        let _ = writeln!(t, "| [{name}](docs/features.md#{anchor}) | {count} | {what} |");
    }

    Ok(Rendered { doc: d, readme_table: t })
}

/// 把 README 里两个标记之间换成新的总表。
pub fn update_readme(readme: &str, table: &str) -> Result<String, String> {
    let (Some(start), Some(end)) = (readme.find(README_START), readme.find(README_END)) else {
        return Err(format!("{README_PATH} 里找不到 {README_START} 和 {README_END}"));
    };
    if end < start {
        return Err(format!("{README_PATH} 里 {README_END} 写在了 {README_START} 前面"));
    }
    let before = &readme[..start + README_START.len()];
    Ok(format!("{before}\n{table}{}", &readme[end..]))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn cells_escape_table_syntax() {
        assert_eq!(cell(" a|b\r\nc <d> "), "a\\|b c &lt;d&gt;");
    }

    /// 常用设置的分类要和界面上「常用设置」页的分组一致，不然清单里的「常用设置」和「修复」会分错
    #[test]
    fn settings_categories_match_the_settings_page() {
        let labels =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/labels.ts")).unwrap();
        let start = labels.find("export const settingsCategories").expect("labels.ts 里有 settingsCategories");
        let block = &labels[start..start + labels[start..].find("] as const").unwrap()];
        let ui: Vec<(String, String)> = block
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let id = l.strip_prefix("{ id: '")?.split('\'').next()?;
                let title = l.split("title: '").nth(1)?.split('\'').next()?;
                Some((id.to_owned(), title.to_owned()))
            })
            .collect();
        let ours: Vec<(String, String)> =
            SETTINGS_CATEGORIES.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        assert_eq!(ui, ours);
    }

    #[test]
    fn readme_table_goes_between_the_markers() {
        let readme = format!("前面\n{README_START}\n旧的\n{README_END}\n后面\n");
        let new = update_readme(&readme, "| 新的 |\n").unwrap();
        assert_eq!(new, format!("前面\n{README_START}\n| 新的 |\n{README_END}\n后面\n"));
        assert_eq!(update_readme(&new, "| 新的 |\n").unwrap(), new);
        assert!(update_readme("没有标记", "x").is_err());
        assert!(update_readme(&format!("{README_END}{README_START}"), "x").is_err());
    }

    /// 仓库里的数据能生成清单，每个症状、功能、小工具、检测都列在里面
    #[test]
    fn every_item_of_the_repository_is_listed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (bundle, problems) = medkit_core::bundle::Bundle::from_repo(&root);
        let bundle = bundle.unwrap_or_else(|| panic!("{problems:?}"));
        let c = &bundle.catalog;
        let r = render(c).unwrap();
        let titles = c
            .symptoms
            .iter()
            .map(|s| &s.title)
            .chain(c.features.iter().map(|f| &f.title))
            .chain(c.tools.iter().map(|t| &t.title))
            .chain(c.checks.iter().map(|x| &x.title));
        for title in titles {
            assert!(r.doc.contains(&format!("| {}", cell(title.zh()))), "清单里没有 {}", title.zh());
        }
        let rows = r.doc.lines().filter(|l| l.starts_with("| ") && !l.starts_with("| ---")).count();
        let headers = r.doc.matches("\n| --- |").count();
        let health = c.profiles.iter().find(|p| p.id == "healthcheck").unwrap().checks.len();
        assert_eq!(
            rows - headers,
            health + c.symptoms.len() + c.features.len() + c.tools.len() + c.checks.len(),
            "每一项正好一行"
        );
    }
}
