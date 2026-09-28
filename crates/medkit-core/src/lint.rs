//! PowerShell 脚本的静态检查（见 docs/architecture.md 5.1）。
//!
//! 这不是安全边界（真正的边界是代码审核 + 哈希校验），而是把最常见、最危险的写法挡在 CI 里。

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::catalog::Problem;

struct Rule {
    pattern: Regex,
    message: &'static str,
}

/// 下载内容的写法（计划书第五节第 8 条：不从网上下载脚本来执行）。
static DOWNLOAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)DownloadString|DownloadFile|DownloadData|Invoke-WebRequest|Invoke-RestMethod|Start-BitsTransfer|Net\.WebClient|WebRequest\]::Create|HttpWebRequest|HttpClient|\biwr\b|\birm\b|\bcurl\b|\bwget\b|\bbitsadmin\b|urlcache",
    )
    .expect("下载规则")
});

/// 例外：这几个脚本从微软官网下载官方安装包（VC++ 运行库），核对是微软签名的才运行（docs/architecture.md 5.1）。
/// 下载的永远不是脚本。它们另有两条要求：网址只能以 [`DOWNLOAD_PREFIXES`] 里的开头（也不能有 http://），
/// 要用 `Get-AuthenticodeSignature` 核对签名。名单写在代码里，改名单要过代码审核。
pub const DOWNLOAD_SCRIPTS: &[&str] = &["tools/system/install-vc-runtime.ps1"];

/// [`DOWNLOAD_SCRIPTS`] 里能写的网址开头：都是微软的下载地址
pub const DOWNLOAD_PREFIXES: &[&str] =
    &["https://aka.ms/", "https://download.microsoft.com/", "https://download.visualstudio.microsoft.com/"];

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(https?|ftp)://[^\s'\x22)]*").expect("网址"));

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let rule = |p: &str, message: &'static str| Rule {
        pattern: Regex::new(&format!("(?i){p}")).expect("lint 规则"),
        message,
    };
    vec![
        rule(r"\bWrite-Host\b", "不要用 Write-Host：只输出一个结果对象"),
        rule(r"\bInvoke-Expression\b|(^|[\s;|(])iex\b", "不要用 Invoke-Expression / iex"),
        rule(r"-EncodedCommand|FromBase64String", "不要用编码命令或 Base64（会触发杀软，也无法审核）"),
        rule(r"\b(Set|Add)-MpPreference\b", "不改 Defender 设置（计划书第五节第 4 条）"),
        rule(r"\bvssadmin\b", "不删卷影副本（计划书第五节第 24 条）"),
        rule(r"\bwevtutil\b", "不清事件日志（计划书第五节第 24 条）"),
        rule(r"\bbcdedit\b", "不改启动配置（计划书第五节第 24 条）"),
        rule(r"\bSet-ExecutionPolicy\b", "不改执行策略"),
        rule(r"\bDisable-ComputerRestore\b", "不关系统还原"),
    ]
});

/// 检查一个脚本，返回发现的问题。`rel` 是相对于 scripts/ 的路径。
pub fn lint_script(rel: &str, content: &str) -> Vec<Problem> {
    let file = format!("scripts/{rel}");
    let mut out = Vec::new();

    if let Some((line_no, line)) = content.lines().enumerate().find(|(_, l)| !l.is_ascii()) {
        out.push(Problem::error(
            &file,
            format!("第 {} 行有非 ASCII 字符（脚本里不要写中文，文字放在 YAML 里）：{}", line_no + 1, line.trim()),
        ));
    }
    if content.starts_with('\u{feff}') {
        out.push(Problem::error(&file, "文件开头有 BOM；脚本只用 ASCII，不需要 BOM".to_owned()));
    }

    let downloads = DOWNLOAD_SCRIPTS.contains(&rel);
    let mut verifies = false;
    for (i, line) in content.lines().enumerate() {
        let code = strip_comment(line);
        for rule in RULES.iter() {
            if rule.pattern.is_match(code) {
                out.push(Problem::error(&file, format!("第 {} 行：{}", i + 1, rule.message)));
            }
        }
        if !downloads {
            if DOWNLOAD.is_match(code) {
                out.push(Problem::error(&file, format!("第 {} 行：脚本不允许下载内容（计划书第五节第 8 条）", i + 1)));
            }
            continue;
        }
        verifies |= code.to_ascii_lowercase().contains("get-authenticodesignature");
        for url in URL.find_iter(code) {
            if !DOWNLOAD_PREFIXES.iter().any(|p| url.as_str().starts_with(p)) {
                out.push(Problem::error(
                    &file,
                    format!(
                        "第 {} 行：只能从微软的下载地址下载（{}）：{}",
                        i + 1,
                        DOWNLOAD_PREFIXES.join("、"),
                        url.as_str()
                    ),
                ));
            }
        }
    }
    if downloads && !verifies {
        out.push(Problem::error(&file, "下载安装包的脚本要用 Get-AuthenticodeSignature 核对是微软签名的".to_owned()));
    }

    if !content.contains("[CmdletBinding()]") {
        out.push(Problem::error(&file, "缺少 [CmdletBinding()] param(...)".to_owned()));
    }
    out
}

/// 共享代码块的开头和结尾：
/// `# ---- shared block <名字>: ...` 和 `# ---- end of shared block <名字> ----`。
static SHARED_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^# ---- shared block ([a-z0-9-]+):").expect("共享块开头"));
static SHARED_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^# ---- end of shared block ([a-z0-9-]+) ----").expect("共享块结尾"));

/// 检查共享代码块：脚本之间不能互相加载（加载进来的文件绕过了哈希校验），
/// 几个脚本要用同一段代码时只能各复制一份，用上面的注释标出来。
/// 同名的块在所有脚本里必须逐字相同（不计换行符），免得只改了其中一份。
/// `scripts` 是（相对于 scripts/ 的路径，内容）。
pub fn lint_shared_blocks<'a>(scripts: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<Problem> {
    let mut out = Vec::new();
    // 名字 → [(文件, 块的内容)]
    let mut blocks: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (rel, content) in scripts {
        let file = format!("scripts/{rel}");
        let mut open: Option<(String, usize, Vec<&str>)> = None;
        for (i, line) in content.lines().enumerate() {
            if let Some(c) = SHARED_START.captures(line) {
                if let Some((name, start, _)) = &open {
                    out.push(Problem::error(
                        &file,
                        format!("第 {} 行开始的共享代码块 {name} 还没结束，第 {} 行又开始了一个", start + 1, i + 1),
                    ));
                }
                open = Some((c[1].to_owned(), i, Vec::new()));
            } else if let Some(c) = SHARED_END.captures(line) {
                match open.take() {
                    Some((name, _, body)) if name == c[1] => {
                        blocks.entry(name).or_default().push((file.clone(), body.join("\n")));
                    }
                    Some((name, start, _)) => out.push(Problem::error(
                        &file,
                        format!("第 {} 行开始的共享代码块 {name}，第 {} 行的结尾写成了 {}", start + 1, i + 1, &c[1]),
                    )),
                    None => out.push(Problem::error(
                        &file,
                        format!("第 {} 行是共享代码块 {} 的结尾，但前面没有开头", i + 1, &c[1]),
                    )),
                }
            } else if let Some((_, _, body)) = &mut open {
                body.push(line);
            }
        }
        if let Some((name, start, _)) = open {
            out.push(Problem::error(&file, format!("第 {} 行开始的共享代码块 {name} 没有结尾", start + 1)));
        }
    }
    for (name, copies) in &blocks {
        let (first_file, first) = &copies[0];
        if copies.len() == 1 {
            out.push(Problem::error(
                first_file,
                format!("共享代码块 {name} 只出现在这一个脚本里；不共享的话，去掉开头和结尾的标记"),
            ));
        }
        for (file, body) in &copies[1..] {
            if body != first {
                out.push(Problem::error(
                    file,
                    format!("共享代码块 {name} 和 {first_file} 里的不一样；改了一份，其他几份要一起改"),
                ));
            }
        }
    }
    out
}

/// 去掉行尾的 `#` 注释（粗略处理：不在引号里的第一个 #）。
fn strip_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    for (i, c) in line.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return &line[..i],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "[CmdletBinding()]\nparam()\n[pscustomobject]@{ result = 'ok'; facts = @{} }\n";

    #[test]
    fn clean_script_passes() {
        assert!(lint_script("checks/a.ps1", OK).is_empty());
    }

    #[test]
    fn catches_banned_patterns() {
        for bad in [
            "Write-Host 'x'",
            "iex $s",
            "Invoke-Expression $s",
            "(New-Object Net.WebClient).DownloadString('http://x')",
            "powershell -EncodedCommand AAAA",
            "Set-MpPreference -DisableRealtimeMonitoring $true",
            "vssadmin delete shadows /all",
            "wevtutil cl System",
            "bcdedit /set {default} recoveryenabled No",
        ] {
            let script = format!("{OK}{bad}\n");
            assert!(!lint_script("checks/a.ps1", &script).is_empty(), "应该拦住：{bad}");
        }
    }

    #[test]
    fn catches_other_ways_to_download() {
        for bad in [
            "Invoke-WebRequest -Uri $u -OutFile $f",
            "$r = [Net.WebRequest]::Create($u)",
            "$r = [System.Net.HttpWebRequest]::Create($u)",
            "$c = New-Object System.Net.Http.HttpClient",
            "curl.exe -o $f $u",
            "bitsadmin /transfer x $u $f",
            "certutil -urlcache -split -f $u $f",
        ] {
            let script = format!("{OK}{bad}\n");
            assert!(!lint_script("tools/a.ps1", &script).is_empty(), "应该拦住：{bad}");
        }
    }

    #[test]
    fn download_scripts_only_fetch_signed_installers_from_microsoft() {
        let rel = DOWNLOAD_SCRIPTS[0];
        let good = format!(
            "{OK}$r = [Net.WebRequest]::Create('https://aka.ms/vc14/vc_redist.' + $a + '.exe')\n\
             $s = Get-AuthenticodeSignature -FilePath $f\n"
        );
        assert!(lint_script(rel, &good).is_empty(), "{:?}", lint_script(rel, &good));
        // 别的地址、http://、不核对签名，都不行
        for (bad, why) in [
            (good.replace("https://aka.ms/", "https://example.com/"), "别的网址"),
            (good.replace("https://aka.ms/", "http://aka.ms/"), "http"),
            (good.replace("Get-AuthenticodeSignature", "Get-Item"), "不核对签名"),
        ] {
            assert!(!lint_script(rel, &bad).is_empty(), "应该拦住：{why}");
        }
        // 名单外的脚本照样不能下载
        assert!(!lint_script("tools/system/other.ps1", &good).is_empty());
        // 其他规则照样管
        assert!(!lint_script(rel, &format!("{good}iex $s\n")).is_empty());
    }

    #[test]
    fn comments_and_strings_are_ok() {
        let script = format!("{OK}# never use Write-Host here\n$x = 'a#b'\n");
        assert!(lint_script("checks/a.ps1", &script).is_empty());
    }

    #[test]
    fn non_ascii_rejected() {
        let script = format!("{OK}# 中文注释\n");
        assert_eq!(lint_script("checks/a.ps1", &script).len(), 1);
    }

    fn shared(name: &str, body: &str) -> String {
        format!(
            "{OK}# ---- shared block {name}: keep identical ----\n{body}\n# ---- end of shared block {name} ----\n$y = 1\n"
        )
    }

    #[test]
    fn identical_shared_blocks_pass() {
        let a = shared("proxy-test", "function F {\n    1\n}");
        // 换行符不同（Windows 上检出的是 CRLF）不算不一样
        let b = shared("proxy-test", "function F {\n    1\n}").replace('\n', "\r\n");
        assert!(lint_shared_blocks([("checks/a.ps1", a.as_str()), ("features/b.ps1", b.as_str())]).is_empty());
    }

    #[test]
    fn different_shared_blocks_are_reported() {
        let a = shared("proxy-test", "function F {\n    1\n}");
        let b = shared("proxy-test", "function F {\n    2\n}");
        let problems = lint_shared_blocks([("checks/a.ps1", a.as_str()), ("features/b.ps1", b.as_str())]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].file, "scripts/features/b.ps1");
    }

    #[test]
    fn broken_or_lonely_shared_blocks_are_reported() {
        let unclosed = format!("{OK}# ---- shared block x: keep identical ----\nfunction F {{ 1 }}\n");
        assert_eq!(lint_shared_blocks([("checks/a.ps1", unclosed.as_str())]).len(), 1);
        let lonely = shared("x", "function F { 1 }");
        assert_eq!(lint_shared_blocks([("checks/a.ps1", lonely.as_str())]).len(), 1);
        let mismatched = format!("{OK}# ---- shared block x: a ----\n1\n# ---- end of shared block y ----\n");
        assert_eq!(lint_shared_blocks([("checks/a.ps1", mismatched.as_str())]).len(), 1);
    }
}
