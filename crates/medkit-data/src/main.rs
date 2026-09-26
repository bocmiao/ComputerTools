//! 数据工具：校验 catalog 和脚本、生成 JSON Schema、打包。贡献者和 CI 都用它。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use medkit_core::bundle::Bundle;
use medkit_core::catalog::{Problem, Severity};
use medkit_core::model::{Check, Feature, Profile, Symptom, Tool};
use medkit_core::script::HOST_SCRIPT;

const USAGE: &str = "\
用法：
  medkit-data check  [--root <仓库根目录>]               校验 catalog/ 和 scripts/
  medkit-data schema [--check] [--root <仓库根目录>]     生成 schema/*.schema.json；--check 只比较、不写入
  medkit-data bundle --out <文件> [--root <仓库根目录>]  把数据和脚本打包成一个 JSON

不写 --root 时用当前目录。";

struct Args {
    command: String,
    root: PathBuf,
    check: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let command = it.next().ok_or("缺少子命令")?;
    let mut args = Args { command, root: PathBuf::from("."), check: false, out: None };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => args.root = it.next().ok_or("--root 后面要写目录")?.into(),
            "--out" => args.out = Some(it.next().ok_or("--out 后面要写文件名")?.into()),
            "--check" => args.check = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("不认识的参数：{other}")),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let ok = match args.command.as_str() {
        "check" => check(&args.root).is_some(),
        "schema" => schema(&args.root, args.check),
        "bundle" => match &args.out {
            Some(out) => bundle(&args.root, out),
            None => {
                eprintln!("bundle 需要 --out <文件>");
                false
            }
        },
        "help" => {
            println!("{USAGE}");
            true
        }
        other => {
            eprintln!("不认识的子命令：{other}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// 在 GitHub Actions 里额外输出注解，问题会直接标在 PR 的文件上。
fn report(problems: &[Problem]) {
    let annotate = std::env::var_os("GITHUB_ACTIONS").is_some();
    for p in problems {
        eprintln!("{p}");
        if annotate {
            let level = match p.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            // 注解的消息里换行要转义
            let msg = p.message.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A");
            println!("::{level} file={}::{msg}", p.file);
        }
    }
}

fn check(root: &Path) -> Option<Bundle> {
    // 用宿主脚本认仓库根目录：它总是在，catalog/ 在项目早期可能还是空的
    if !root.join("scripts").join(HOST_SCRIPT).is_file() {
        eprintln!(
            "{} 不像是仓库根目录（找不到 scripts/{HOST_SCRIPT}）；请在仓库根目录运行，或者用 --root 指定",
            root.display()
        );
        return None;
    }
    let (bundle, problems) = Bundle::from_repo(root);
    report(&problems);
    let errors = problems.iter().filter(|p| p.severity == Severity::Error).count();
    let warnings = problems.len() - errors;
    match &bundle {
        Some(b) => {
            let c = &b.catalog;
            eprintln!(
                "通过：{} 个检测、{} 个功能、{} 个症状、{} 个检测清单、{} 个脚本；{warnings} 个警告。数据哈希 {}",
                c.checks.len(),
                c.features.len(),
                c.symptoms.len(),
                c.profiles.len(),
                b.scripts.len(),
                b.hash
            );
        }
        None => eprintln!("没通过：{errors} 个错误、{warnings} 个警告"),
    }
    bundle
}

fn schemas() -> Vec<(&'static str, String)> {
    fn one<T: schemars::JsonSchema>() -> String {
        let schema = schemars::schema_for!(T);
        let mut s = serde_json::to_string_pretty(&schema).expect("schema 可以序列化");
        s.push('\n');
        s
    }
    vec![
        ("check.schema.json", one::<Check>()),
        ("feature.schema.json", one::<Feature>()),
        ("symptom.schema.json", one::<Symptom>()),
        ("profile.schema.json", one::<Profile>()),
        ("tool.schema.json", one::<Tool>()),
    ]
}

fn schema(root: &Path, check_only: bool) -> bool {
    let dir = root.join("schema");
    let mut ok = true;
    for (name, content) in schemas() {
        let path = dir.join(name);
        // 比较时忽略换行符差异（Windows 上 git 可能转换成 CRLF）
        let current = std::fs::read_to_string(&path).ok().map(|s| s.replace("\r\n", "\n"));
        if current.as_deref() == Some(content.as_str()) {
            continue;
        }
        if check_only {
            eprintln!("schema/{name} 和代码里的类型对不上；运行 cargo run -p medkit-data -- schema 更新");
            ok = false;
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, &content)) {
            eprintln!("写 schema/{name} 失败：{e}");
            ok = false;
        } else {
            eprintln!("已更新 schema/{name}");
        }
    }
    ok
}

fn bundle(root: &Path, out: &Path) -> bool {
    let Some(b) = check(root) else { return false };
    match std::fs::write(out, b.to_json()) {
        Ok(()) => {
            eprintln!("已写入 {}", out.display());
            true
        }
        Err(e) => {
            eprintln!("写入 {} 失败：{e}", out.display());
            false
        }
    }
}
