//! PowerShell 脚本的静态检查（见 docs/architecture.md 5.1）。
//!
//! 这不是安全边界（真正的边界是代码审核 + 哈希校验），而是把最常见、最危险的写法挡在 CI 里。

use std::sync::LazyLock;

use regex::Regex;

use crate::catalog::Problem;

struct Rule {
    pattern: Regex,
    message: &'static str,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let rule = |p: &str, message: &'static str| Rule {
        pattern: Regex::new(&format!("(?i){p}")).expect("lint 规则"),
        message,
    };
    vec![
        rule(r"\bWrite-Host\b", "不要用 Write-Host：只输出一个结果对象"),
        rule(r"\bInvoke-Expression\b|(^|[\s;|(])iex\b", "不要用 Invoke-Expression / iex"),
        rule(
            r"DownloadString|DownloadFile|Invoke-WebRequest|Invoke-RestMethod|Start-BitsTransfer|Net\.WebClient|\biwr\b|\birm\b",
            "脚本不允许下载内容（计划书第五节第 8 条）",
        ),
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

    for (i, line) in content.lines().enumerate() {
        let code = strip_comment(line);
        for rule in RULES.iter() {
            if rule.pattern.is_match(code) {
                out.push(Problem::error(&file, format!("第 {} 行：{}", i + 1, rule.message)));
            }
        }
    }

    if !content.contains("[CmdletBinding()]") {
        out.push(Problem::error(&file, "缺少 [CmdletBinding()] param(...)".to_owned()));
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
    fn comments_and_strings_are_ok() {
        let script = format!("{OK}# never use Write-Host here\n$x = 'a#b'\n");
        assert!(lint_script("checks/a.ps1", &script).is_empty());
    }

    #[test]
    fn non_ascii_rejected() {
        let script = format!("{OK}# 中文注释\n");
        assert_eq!(lint_script("checks/a.ps1", &script).len(), 1);
    }
}
