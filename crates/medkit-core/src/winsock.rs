//! Winsock 目录（内置检测 `winsock`）：有没有第三方插进去的网络组件（分层服务提供程序，LSP）、有没有文件已经不在的
//! 组件、TCP/IP 还在不在。读目录的是 [`crate::platform::Platform::winsock_catalog`]（Windows 上用微软公开的
//! WSCEnumProtocols、WSCGetProviderPath，64 位和 32 位的两份目录都读），这里只管路径怎么解释和结论。
//!
//! - LSP：协议链长度不是 1 的项（0 是分层协议本身，大于 1 是经过它的协议链；微软 WSAPROTOCOLCHAIN 文档）。
//!   Windows 8 起 LSP 已经不推荐用了（改用 Windows 筛选平台），Windows 自己不装 LSP；DLL 在 Windows 文件夹里、
//!   版本信息里的公司是 Microsoft 的不算第三方。
//! - 文件不在了：软件卸载、被清理或者被杀毒软件删掉时没有把它从目录里去掉，用到它的程序全都上不了网（常说的「LSP 损坏」）。
//! - 修法都是 `netsh winsock reset`（功能 network.winsock-reset，撤销不了、要重启）；它不动名称空间提供程序（NSP），
//!   所以这里也只看协议目录。
//! - 隐私：结论和事实里只有 DLL 的文件名和版本信息里的产品名、公司，没有路径。

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::platform::WinsockEntry;

/// 地址族 IPv4（AF_INET）
pub const AF_INET: i32 = 2;
/// 流式套接字（SOCK_STREAM，TCP 用的）
pub const SOCK_STREAM: i32 = 1;

/// 把 WSCGetProviderPath 给的路径变成这台电脑上的文件路径：展开 `%SystemRoot%` 这类环境变量（`var` 查环境变量，
/// 不分大小写）。32 位目录里的项（`wow64`）是给 32 位程序用的：`%ProgramFiles%` 是 `Program Files (x86)`，
/// Windows 文件夹下的 System32 实际是 SysWOW64。没写文件夹的只有文件名，按系统文件夹里的算。
pub fn resolve_provider_path(raw: &str, wow64: bool, var: impl Fn(&str) -> Option<String>) -> String {
    let lookup = |name: &str| -> Option<String> {
        if wow64 {
            let x86 = match name.to_ascii_lowercase().as_str() {
                "programfiles" => var("ProgramFiles(x86)"),
                "commonprogramfiles" => var("CommonProgramFiles(x86)"),
                _ => None,
            };
            if x86.is_some() {
                return x86;
            }
        }
        var(name)
    };
    let mut out = String::new();
    let mut rest = raw.trim();
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else { break };
        let name = &after[..end];
        out.push_str(&rest[..start]);
        match lookup(name).filter(|_| !name.is_empty()) {
            Some(value) => out.push_str(&value),
            None => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);

    let windows = var("SystemRoot").unwrap_or_else(|| r"C:\Windows".to_owned());
    let windows = windows.trim_end_matches('\\');
    if !out.contains(['\\', '/']) {
        out = format!(r"{windows}\System32\{out}");
    }
    if wow64 {
        let system32 = format!(r"{windows}\System32\");
        if out.len() >= system32.len() && out[..system32.len()].eq_ignore_ascii_case(&system32) {
            out = format!(r"{windows}\SysWOW64\{}", &out[system32.len()..]);
        }
    }
    out
}

/// 路径在 Windows 文件夹里（不分大小写）。
pub fn in_windows_folder(path: &str, windows: &str) -> bool {
    let prefix = format!(r"{}\", windows.trim_end_matches('\\'));
    path.len() > prefix.len() && path[..prefix.len()].eq_ignore_ascii_case(&prefix)
}

/// Windows 自己的组件：在 Windows 文件夹里，版本信息里的公司是 Microsoft。
fn windows_own(e: &WinsockEntry) -> bool {
    e.in_windows && e.company.as_deref().is_some_and(|c| c.contains("Microsoft"))
}

/// 给人看的名字：「产品名（文件名）」，没有产品名用公司，都没有只写文件名。
fn label(e: &WinsockEntry) -> String {
    match e.product.as_deref().or(e.company.as_deref()).map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{name}（{}）", e.file),
        None => e.file.clone(),
    }
}

/// 按文件名去重（同一个 LSP 在目录里有好几项：分层协议本身和经过它的每一条协议链，64 位、32 位各一套）。
fn labels<'a>(entries: impl Iterator<Item = &'a WinsockEntry>) -> Vec<String> {
    let mut by_file: BTreeMap<String, String> = BTreeMap::new();
    for e in entries {
        by_file.entry(e.file.to_lowercase()).or_insert_with(|| label(e));
    }
    by_file.into_values().collect()
}

/// 结论。结果代码（先满足哪个算哪个）：
/// - `missing-file`：有组件的 DLL 已经不在了；
/// - `no-tcpip`：64 位或者 32 位的目录里没有 IPv4 的 TCP 基础提供程序（目录坏了）；
/// - `lsp`：有第三方的 LSP；
/// - `ok-others`：没有 LSP，但有第三方的基础协议（虚拟机的 vSockets 这类，一般不影响上网）；
/// - `ok`：都是 Windows 自己的。
///
/// 事实：entries（一共几项）、missing / missing_count、lsp / lsp_count、others / others_count（名字用「、」隔开）。
pub fn verdict(entries: &[WinsockEntry]) -> Value {
    let missing = labels(entries.iter().filter(|e| !e.exists));
    let lsp = labels(entries.iter().filter(|e| e.chain_len != 1 && e.exists && !windows_own(e)));
    let others = labels(entries.iter().filter(|e| e.chain_len == 1 && e.exists && !windows_own(e)));
    let has_tcp = |wow64: bool| {
        entries.iter().any(|e| {
            e.wow64 == wow64 && e.chain_len == 1 && e.family == AF_INET && e.socket_type == SOCK_STREAM && e.exists
        })
    };
    // 64 位的目录必须有 TCP/IP；32 位的那一份读到了（有任何一项）才要求
    let tcp_ok = has_tcp(false) && (!entries.iter().any(|e| e.wow64) || has_tcp(true));
    let result = if !missing.is_empty() {
        "missing-file"
    } else if !tcp_ok {
        "no-tcpip"
    } else if !lsp.is_empty() {
        "lsp"
    } else if !others.is_empty() {
        "ok-others"
    } else {
        "ok"
    };
    json!({
        "result": result,
        "facts": {
            "entries": entries.len(),
            "missing": missing.join("、"),
            "missing_count": missing.len(),
            "lsp": lsp.join("、"),
            "lsp_count": lsp.len(),
            "others": others.join("、"),
            "others_count": others.len(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name.to_ascii_lowercase().as_str() {
            "systemroot" | "windir" => Some(r"C:\Windows".into()),
            "programfiles" => Some(r"C:\Program Files".into()),
            "programfiles(x86)" => Some(r"C:\Program Files (x86)".into()),
            _ => None,
        }
    }

    #[test]
    fn provider_paths_are_expanded_like_the_program_that_loads_them() {
        let p = |raw: &str, wow64: bool| resolve_provider_path(raw, wow64, env);
        assert_eq!(p(r"%SystemRoot%\system32\mswsock.dll", false), r"C:\Windows\system32\mswsock.dll");
        assert_eq!(p(r"%SystemRoot%\system32\mswsock.dll", true), r"C:\Windows\SysWOW64\mswsock.dll");
        assert_eq!(p(r"%windir%\System32\hvsocket.dll", true), r"C:\Windows\SysWOW64\hvsocket.dll");
        assert_eq!(p(r"%ProgramFiles%\Acme\lsp.dll", false), r"C:\Program Files\Acme\lsp.dll");
        assert_eq!(p(r"%ProgramFiles%\Acme\lsp.dll", true), r"C:\Program Files (x86)\Acme\lsp.dll");
        assert_eq!(p(r"D:\Tools\x.dll", true), r"D:\Tools\x.dll");
        assert_eq!(p("mswsock.dll", false), r"C:\Windows\System32\mswsock.dll");
        assert_eq!(p("mswsock.dll", true), r"C:\Windows\SysWOW64\mswsock.dll");
        // 不认识的变量、落单的 % 原样留着
        assert_eq!(p(r"%NoSuchVar%\a.dll", false), r"%NoSuchVar%\a.dll");
        assert_eq!(p(r"C:\50%\a.dll", false), r"C:\50%\a.dll");
        assert!(in_windows_folder(r"c:\windows\system32\mswsock.dll", r"C:\Windows"));
        assert!(!in_windows_folder(r"C:\WindowsApps\x.dll", r"C:\Windows"));
        assert!(!in_windows_folder(r"C:\Program Files\x.dll", r"C:\Windows"));
    }

    fn entry(file: &str, chain_len: i32, wow64: bool) -> WinsockEntry {
        WinsockEntry {
            protocol: "MSAFD Tcpip [TCP/IP]".into(),
            file: file.into(),
            in_windows: true,
            exists: true,
            company: Some("Microsoft Corporation".into()),
            product: Some("Microsoft® Windows® Operating System".into()),
            chain_len,
            family: AF_INET,
            socket_type: SOCK_STREAM,
            wow64,
        }
    }

    fn clean() -> Vec<WinsockEntry> {
        vec![entry("mswsock.dll", 1, false), entry("mswsock.dll", 1, true)]
    }

    fn result(entries: &[WinsockEntry]) -> (String, Value) {
        let v = verdict(entries);
        (v["result"].as_str().unwrap().to_owned(), v["facts"].clone())
    }

    #[test]
    fn a_clean_catalog_is_ok() {
        let (r, facts) = result(&clean());
        assert_eq!(r, "ok");
        assert_eq!(facts["entries"], 2);
        assert_eq!(facts["lsp_count"], 0);
    }

    #[test]
    fn third_party_layered_providers_are_named_once() {
        let mut entries = clean();
        for (chain, wow64) in [(0, false), (2, false), (2, false), (0, true), (2, true)] {
            let mut e = entry("NL_LSP.dll", chain, wow64);
            e.in_windows = false;
            e.company = Some("Locktime Software".into());
            e.product = Some("NetLimiter".into());
            entries.push(e);
        }
        let (r, facts) = result(&entries);
        assert_eq!(r, "lsp");
        assert_eq!(facts["lsp"], "NetLimiter（NL_LSP.dll）");
        assert_eq!(facts["lsp_count"], 1);
    }

    #[test]
    fn a_missing_file_comes_first_and_old_malware_in_system32_is_not_windows() {
        let mut entries = clean();
        let mut gone = entry("oldaccel.dll", 0, true);
        gone.exists = false;
        gone.in_windows = false;
        gone.company = None;
        gone.product = None;
        entries.push(gone);
        // 放在 System32 里、公司不是 Microsoft 的 LSP 照样算第三方
        let mut fake = entry("webhdll.dll", 0, false);
        fake.company = Some("WebHancer Corporation".into());
        fake.product = None;
        entries.push(fake);
        let (r, facts) = result(&entries);
        assert_eq!(r, "missing-file");
        assert_eq!(facts["missing"], "oldaccel.dll");
        assert_eq!(facts["lsp"], "WebHancer Corporation（webhdll.dll）");
    }

    #[test]
    fn a_catalog_without_tcp_is_broken() {
        let (r, _) = result(&[]);
        assert_eq!(r, "no-tcpip");
        // 32 位的那一份读到了，却没有 TCP
        let mut udp = entry("mswsock.dll", 1, true);
        udp.socket_type = 2;
        let (r, _) = result(&[entry("mswsock.dll", 1, false), udp]);
        assert_eq!(r, "no-tcpip");
        // 32 位的那一份一项都没有（32 位 Windows、读不到）时只看 64 位的
        let (r, _) = result(&[entry("mswsock.dll", 1, false)]);
        assert_eq!(r, "ok");
    }

    #[test]
    fn third_party_base_protocols_are_only_mentioned() {
        let mut entries = clean();
        let mut vmware = entry("vsocklib.dll", 1, false);
        vmware.company = Some("VMware, Inc.".into());
        vmware.product = Some("VMware Tools".into());
        vmware.family = 40;
        entries.push(vmware);
        let (r, facts) = result(&entries);
        assert_eq!(r, "ok-others");
        assert_eq!(facts["others"], "VMware Tools（vsocklib.dll）");
    }
}
