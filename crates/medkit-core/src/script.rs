//! 运行 PowerShell 脚本：常驻宿主进程 + 一行一个 JSON 的协议（见 docs/architecture.md 5.3）。

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::bundle::sha256_hex;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ScriptError {
    /// 脚本自己抛出的错误
    #[error("{0}")]
    Failed(String),
    #[error("运行超时（{0} 秒）")]
    Timeout(u64),
    #[error("脚本文件校验失败：{0}")]
    Integrity(String),
    #[error("脚本宿主出错：{0}")]
    Host(String),
}

impl ScriptError {
    /// 是否是基础设施问题（而不是脚本本身报的错）。冒烟测试用它区分。
    pub fn is_infrastructure(&self) -> bool {
        !matches!(self, Self::Failed(_))
    }
}

pub trait ScriptRunner: Send + Sync {
    /// `script` 是相对于 scripts/ 的路径；返回脚本输出的那个对象。
    fn run(&self, script: &str, args: &Map<String, Value>, timeout: Duration) -> Result<Value, ScriptError>;
}

/// 相对路径检查：只允许 `a/b/c.ps1` 这种形式。
pub fn validate_script_path(p: &str) -> Result<(), ScriptError> {
    let ok = !p.is_empty()
        && p.ends_with(".ps1")
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b));
    if ok { Ok(()) } else { Err(ScriptError::Integrity(format!("脚本路径不合法：{p}"))) }
}

/// 运行目录里每个脚本应有的 SHA-256。执行前逐个核对。
#[derive(Debug, Clone)]
pub struct ScriptManifest {
    root: PathBuf,
    hashes: BTreeMap<String, String>,
}

impl ScriptManifest {
    pub fn new(root: impl Into<PathBuf>, hashes: BTreeMap<String, String>) -> Self {
        Self { root: root.into(), hashes }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn verify(&self, rel: &str) -> Result<(), ScriptError> {
        validate_script_path(rel)?;
        let expected = self.hashes.get(rel).ok_or_else(|| ScriptError::Integrity(format!("{rel} 不在脚本清单里")))?;
        let bytes =
            std::fs::read(self.root.join(rel)).map_err(|e| ScriptError::Integrity(format!("读不到 {rel}：{e}")))?;
        if &sha256_hex(&bytes) != expected {
            return Err(ScriptError::Integrity(format!("{rel} 被改动过，拒绝执行")));
        }
        Ok(())
    }
}

pub const HOST_SCRIPT: &str = "host/Host.ps1";

pub struct HostConfig {
    /// `powershell.exe`（Windows）或 `pwsh`（开发和测试）
    pub program: PathBuf,
    /// 放脚本的根目录（里面有 host/Host.ps1）
    pub scripts_root: PathBuf,
    /// 有清单时，每次执行前校验哈希
    pub manifest: Option<ScriptManifest>,
}

struct HostProc {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

/// 常驻的 PowerShell 宿主。一次只跑一个脚本；超时或崩溃后自动重启。
pub struct PowerShellHost {
    cfg: HostConfig,
    proc: Mutex<Option<HostProc>>,
    next_id: AtomicU64,
}

impl PowerShellHost {
    pub fn new(cfg: HostConfig) -> Self {
        Self { cfg, proc: Mutex::new(None), next_id: AtomicU64::new(1) }
    }

    fn spawn(&self) -> Result<HostProc, ScriptError> {
        if let Some(m) = &self.cfg.manifest {
            m.verify(HOST_SCRIPT)?;
        }
        let host = self.cfg.scripts_root.join(HOST_SCRIPT);
        let mut cmd = Command::new(&self.cfg.program);
        cmd.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&host)
            .current_dir(&self.cfg.scripts_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child =
            cmd.spawn().map_err(|e| ScriptError::Host(format!("启动 {} 失败：{e}", self.cfg.program.display())))?;
        let stdin = child.stdin.take().ok_or_else(|| ScriptError::Host("拿不到标准输入".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| ScriptError::Host("拿不到标准输出".into()))?;
        let stderr_pipe = child.stderr.take().ok_or_else(|| ScriptError::Host("拿不到标准错误".into()))?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let sink = Arc::clone(&stderr);
        thread::spawn(move || {
            for line in BufReader::new(stderr_pipe).lines() {
                let Ok(line) = line else { break };
                let mut buf = sink.lock().unwrap();
                if buf.len() >= 20 {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
        });
        Ok(HostProc { child, stdin, lines: rx, stderr })
    }

    fn stop(slot: &mut Option<HostProc>) {
        if let Some(mut p) = slot.take() {
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
    }

    fn exchange(p: &mut HostProc, id: u64, request: &str, timeout: Duration) -> Result<Value, ScriptError> {
        p.stdin
            .write_all(request.as_bytes())
            .and_then(|()| p.stdin.write_all(b"\n"))
            .and_then(|()| p.stdin.flush())
            .map_err(|e| ScriptError::Host(format!("写入请求失败：{e}")))?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match p.lines.recv_timeout(remaining) {
                Ok(line) => {
                    let Ok(resp) = serde_json::from_str::<Value>(line.trim_start_matches('\u{feff}')) else {
                        continue; // 不是协议行（例如脚本误写的输出），忽略
                    };
                    if resp.get("id").and_then(Value::as_u64) != Some(id) {
                        continue; // 上一次超时请求的迟到回复
                    }
                    if resp.get("ok") == Some(&Value::Bool(true)) {
                        return Ok(resp.get("data").cloned().unwrap_or(Value::Null));
                    }
                    let msg = resp.get("error").and_then(Value::as_str).unwrap_or("未知错误");
                    return Err(ScriptError::Failed(msg.trim().to_owned()));
                }
                Err(RecvTimeoutError::Timeout) => return Err(ScriptError::Timeout(timeout.as_secs())),
                Err(RecvTimeoutError::Disconnected) => {
                    let tail: Vec<String> = p.stderr.lock().unwrap().iter().cloned().collect();
                    return Err(ScriptError::Host(format!("PowerShell 意外退出：{}", tail.join(" | "))));
                }
            }
        }
    }
}

impl ScriptRunner for PowerShellHost {
    fn run(&self, script: &str, args: &Map<String, Value>, timeout: Duration) -> Result<Value, ScriptError> {
        validate_script_path(script)?;
        if let Some(m) = &self.cfg.manifest {
            m.verify(script)?;
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({ "id": id, "script": script, "args": args }).to_string();
        let mut slot = self.proc.lock().unwrap();
        if slot.is_none() {
            *slot = Some(self.spawn()?);
        }
        let result = Self::exchange(slot.as_mut().unwrap(), id, &request, timeout);
        if matches!(result, Err(ScriptError::Timeout(_) | ScriptError::Host(_))) {
            Self::stop(&mut slot);
        }
        result
    }
}

impl Drop for PowerShellHost {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.proc.lock() {
            Self::stop(&mut slot);
        }
    }
}

type Handler = Box<dyn Fn(&Map<String, Value>) -> Result<Value, ScriptError> + Send + Sync>;

/// 测试用的假执行器：按脚本路径返回预设结果。
#[derive(Default)]
pub struct MockRunner {
    handlers: Mutex<HashMap<String, Handler>>,
    calls: Mutex<Vec<(String, Map<String, Value>)>>,
}

impl MockRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(
        &self,
        script: &str,
        f: impl Fn(&Map<String, Value>) -> Result<Value, ScriptError> + Send + Sync + 'static,
    ) {
        self.handlers.lock().unwrap().insert(script.to_owned(), Box::new(f));
    }

    /// 让脚本固定返回某个值。
    pub fn returns(&self, script: &str, value: Value) {
        self.on(script, move |_| Ok(value.clone()));
    }

    pub fn calls(&self) -> Vec<(String, Map<String, Value>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl ScriptRunner for MockRunner {
    fn run(&self, script: &str, args: &Map<String, Value>, _timeout: Duration) -> Result<Value, ScriptError> {
        validate_script_path(script)?;
        self.calls.lock().unwrap().push((script.to_owned(), args.clone()));
        match self.handlers.lock().unwrap().get(script) {
            Some(h) => h(args),
            None => Err(ScriptError::Failed(format!("（模拟）没有为 {script} 准备结果"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_paths() {
        assert!(validate_script_path("checks/disk/free.ps1").is_ok());
        assert!(validate_script_path("host/Host.ps1").is_ok());
        for bad in
            ["", "../x.ps1", "a/../b.ps1", "/abs.ps1", r"a\b.ps1", "C:/x.ps1", "a//b.ps1", "a/b.txt", "a/b c.ps1"]
        {
            assert!(validate_script_path(bad).is_err(), "{bad} 应该被拒绝");
        }
    }

    #[test]
    fn manifest_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("checks")).unwrap();
        std::fs::write(dir.path().join("checks/a.ps1"), b"param()\n'x'").unwrap();
        let hashes = BTreeMap::from([("checks/a.ps1".to_owned(), sha256_hex(b"param()\n'x'"))]);
        let m = ScriptManifest::new(dir.path(), hashes);
        assert!(m.verify("checks/a.ps1").is_ok());
        std::fs::write(dir.path().join("checks/a.ps1"), b"param()\n'evil'").unwrap();
        assert!(matches!(m.verify("checks/a.ps1"), Err(ScriptError::Integrity(_))));
        assert!(matches!(m.verify("checks/b.ps1"), Err(ScriptError::Integrity(_))));
    }
}
