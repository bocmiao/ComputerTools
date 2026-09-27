//! 别让电脑自己睡着：小药箱开着的时候，电脑不因为「一段时间没操作」自动睡眠（可以选屏幕也不关）。
//!
//! 用 SetThreadExecutionState：在一个专门的线程上设好，线程一直等着；关掉开关、或者退出小药箱时
//! 线程结束，系统马上回到原来的电源设置。不改任何电源设置，不用撤销；合上笔记本盖子、按电源键、
//! 「开始 → 电源 → 睡眠」照常能睡。
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AwakeStatus {
    /// 现在开着
    pub on: bool,
    /// 屏幕也保持亮着
    pub display: bool,
}

#[derive(Default)]
pub struct AwakeState {
    /// 丢掉发送端，线程就收到信号、恢复原样、结束
    worker: Option<(Sender<()>, JoinHandle<()>)>,
    display: bool,
}

impl AwakeState {
    pub fn status(&self) -> AwakeStatus {
        AwakeStatus { on: self.worker.is_some(), display: self.worker.is_some() && self.display }
    }

    /// 打开（按 `display` 决定屏幕要不要也亮着）或者关掉。
    pub fn set(&mut self, on: bool, display: bool) -> Result<AwakeStatus, String> {
        self.stop();
        if on {
            let (stop_tx, stop_rx) = mpsc::channel::<()>();
            let (ready_tx, ready_rx) = mpsc::channel::<bool>();
            let handle = std::thread::Builder::new()
                .name("medkit-awake".into())
                .spawn(move || {
                    let _ = ready_tx.send(hold(display));
                    // 发送端被丢掉时 recv 返回错误：该恢复了
                    let _ = stop_rx.recv();
                    release();
                })
                .map_err(|e| format!("没能打开：{e}"))?;
            if ready_rx.recv() != Ok(true) {
                drop(stop_tx);
                let _ = handle.join();
                return Err("系统没有接受「别睡」的请求。".into());
            }
            self.worker = Some((stop_tx, handle));
            self.display = display;
        }
        Ok(self.status())
    }

    fn stop(&mut self) {
        if let Some((stop_tx, handle)) = self.worker.take() {
            drop(stop_tx);
            let _ = handle.join();
        }
    }
}

impl Drop for AwakeState {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
fn hold(display: bool) -> bool {
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };
    let flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED | if display { ES_DISPLAY_REQUIRED } else { 0 };
    // SAFETY: 只传标志位；返回 0 表示失败
    unsafe { SetThreadExecutionState(flags) != 0 }
}

#[cfg(windows)]
fn release() {
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, SetThreadExecutionState};
    // SAFETY: 只传标志位
    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
}

#[cfg(not(windows))]
fn hold(_display: bool) -> bool {
    true
}

#[cfg(not(windows))]
fn release() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_on_off_and_switches_the_screen_option() {
        let mut s = AwakeState::default();
        assert_eq!(s.status(), AwakeStatus { on: false, display: false });
        assert_eq!(s.set(true, true).unwrap(), AwakeStatus { on: true, display: true });
        assert_eq!(s.set(true, false).unwrap(), AwakeStatus { on: true, display: false });
        assert_eq!(s.set(false, true).unwrap(), AwakeStatus { on: false, display: false });
        assert!(s.worker.is_none(), "关掉以后线程要结束");
    }
}
