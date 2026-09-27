//! 用真实的 PowerShell 跑宿主协议：Windows 上是 powershell.exe（5.1），其他系统上是 pwsh（没装就跳过）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use medkit_core::bundle::sha256_hex;
use medkit_core::script::{HOST_SCRIPT, HostConfig, PowerShellHost, ScriptError, ScriptManifest, ScriptRunner};
use serde_json::{Map, Value, json};

const T: Duration = Duration::from_secs(60);

const SCRIPTS: &[(&str, &str)] = &[
    (
        "t/ok.ps1",
        "[CmdletBinding()]\r\nparam([string]$Name = 'nobody')\r\n\
         [pscustomobject]@{ result = 'ok'; facts = @{ name = $Name; half = 1.5; yes = $true; list = @('a', 'b') } }\r\n",
    ),
    ("t/fail.ps1", "[CmdletBinding()]\r\nparam()\r\nthrow 'boom'\r\n"),
    ("t/slow.ps1", "[CmdletBinding()]\r\nparam()\r\nStart-Sleep -Seconds 30\r\n@{ result = 'late' }\r\n"),
    (
        "t/noisy.ps1",
        "[CmdletBinding()]\r\nparam()\r\n\
         Write-Warning 'w'\r\nWrite-Verbose 'v' -Verbose\r\nWrite-Information 'i'\r\n\
         [Console]::Out.WriteLine('not json')\r\n@{ result = 'ok' }\r\n",
    ),
    ("t/echo.ps1", "[CmdletBinding()]\r\nparam([string]$Text)\r\n@{ echo = $Text; length = $Text.Length }\r\n"),
    ("t/two.ps1", "[CmdletBinding()]\r\nparam()\r\n@{ n = 1 }\r\n@{ n = 2 }\r\n"),
    ("t/nothing.ps1", "[CmdletBinding()]\r\nparam()\r\n$null = 1\r\n"),
    ("t/exit.ps1", "[CmdletBinding()]\r\nparam()\r\n[Environment]::Exit(3)\r\n"),
    (
        "t/modules.ps1",
        "[CmdletBinding()]\r\nparam()\r\n@{ path = $env:PSModulePath; own = (Join-Path $PSHOME 'Modules') }\r\n",
    ),
];

fn program() -> Option<PathBuf> {
    if cfg!(windows) {
        return Some(PathBuf::from("powershell.exe"));
    }
    std::env::var_os("PATH")
        .and_then(|paths| std::env::split_paths(&paths).map(|p| p.join("pwsh")).find(|p| p.is_file()))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 在临时目录里放好宿主和测试脚本，返回（目录，清单）。
fn setup() -> (tempfile::TempDir, ScriptManifest) {
    let dir = tempfile::tempdir().unwrap();
    let mut hashes = BTreeMap::new();
    let host = std::fs::read(repo_root().join("scripts").join(HOST_SCRIPT)).unwrap();
    let mut files: Vec<(&str, Vec<u8>)> = vec![(HOST_SCRIPT, host)];
    files.extend(SCRIPTS.iter().map(|(p, c)| (*p, c.as_bytes().to_vec())));
    for (rel, bytes) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        hashes.insert(rel.to_owned(), sha256_hex(&bytes));
    }
    let manifest = ScriptManifest::new(dir.path(), hashes);
    (dir, manifest)
}

fn host() -> Option<(tempfile::TempDir, PowerShellHost)> {
    let Some(program) = program() else {
        eprintln!("没有找到 PowerShell，跳过");
        return None;
    };
    let (dir, manifest) = setup();
    let host =
        PowerShellHost::new(HostConfig { program, scripts_root: dir.path().to_owned(), manifest: Some(manifest) });
    Some((dir, host))
}

fn args(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::String((*v).to_owned()))).collect()
}

#[test]
fn protocol_round_trip() {
    let Some((_dir, h)) = host() else { return };

    let v = h.run("t/ok.ps1", &args(&[("Name", "xiaoming")]), T).unwrap();
    assert_eq!(
        v,
        json!({ "result": "ok", "facts": { "name": "xiaoming", "half": 1.5, "yes": true, "list": ["a", "b"] } })
    );

    // 参数和输出里的中文、反斜杠、引号都要原样来回
    let text = "你好，C:\\Users\\小明\\桌面 \"引号\" 'single' 😀";
    let v = h.run("t/echo.ps1", &args(&[("Text", text)]), T).unwrap();
    assert_eq!(v["echo"], text);

    let e = h.run("t/fail.ps1", &Map::new(), T).unwrap_err();
    assert!(matches!(&e, ScriptError::Failed(m) if m.contains("boom")), "{e:?}");

    // 警告、详细、信息流和直接写到控制台的杂字都不能破坏协议
    assert_eq!(h.run("t/noisy.ps1", &Map::new(), T).unwrap(), json!({ "result": "ok" }));

    // 多个输出时取最后一个；没有输出时是 null
    assert_eq!(h.run("t/two.ps1", &Map::new(), T).unwrap(), json!({ "n": 2 }));
    assert_eq!(h.run("t/nothing.ps1", &Map::new(), T).unwrap(), Value::Null);
}

/// 从 PowerShell 7 里启动时，继承来的模块路径排在前面的是 7 的模块，5.1 加载不了（签名检查就会失败）。
#[test]
fn modules_load_only_from_powershells_own_folder() {
    let Some((_dir, h)) = host() else { return };
    let v = h.run("t/modules.ps1", &Map::new(), T).unwrap();
    assert_eq!(v["path"], v["own"], "{v}");
}

#[test]
fn timeout_kills_and_restarts_the_host() {
    let Some((_dir, h)) = host() else { return };
    // 先让宿主启动起来，下面的超时只算脚本本身
    h.run("t/ok.ps1", &Map::new(), T).unwrap();
    let e = h.run("t/slow.ps1", &Map::new(), Duration::from_secs(2)).unwrap_err();
    assert!(matches!(e, ScriptError::Timeout(2)), "{e:?}");
    assert_eq!(h.run("t/ok.ps1", &Map::new(), T).unwrap()["result"], "ok");
}

#[test]
fn host_crash_is_reported_and_recovered() {
    let Some((_dir, h)) = host() else { return };
    let e = h.run("t/exit.ps1", &Map::new(), T).unwrap_err();
    assert!(matches!(e, ScriptError::Host(_)), "{e:?}");
    assert!(e.is_infrastructure());
    assert_eq!(h.run("t/ok.ps1", &Map::new(), T).unwrap()["result"], "ok");
}

#[test]
fn tampered_scripts_are_refused() {
    let Some((dir, h)) = host() else { return };
    std::fs::write(dir.path().join("t/ok.ps1"), "[CmdletBinding()]\r\nparam()\r\n@{ result = 'evil' }\r\n").unwrap();
    let e = h.run("t/ok.ps1", &Map::new(), T).unwrap_err();
    assert!(matches!(e, ScriptError::Integrity(_)), "{e:?}");

    // 不在清单里的脚本、越界路径一律拒绝
    std::fs::write(dir.path().join("t/extra.ps1"), "@{}").unwrap();
    assert!(matches!(h.run("t/extra.ps1", &Map::new(), T), Err(ScriptError::Integrity(_))));
    assert!(matches!(h.run("../outside.ps1", &Map::new(), T), Err(ScriptError::Integrity(_))));
}

#[test]
fn tampered_host_is_refused() {
    let Some((dir, h)) = host() else { return };
    std::fs::write(dir.path().join(HOST_SCRIPT), "# replaced\r\n").unwrap();
    let e = h.run("t/ok.ps1", &Map::new(), T).unwrap_err();
    assert!(matches!(e, ScriptError::Integrity(_)), "{e:?}");
}
