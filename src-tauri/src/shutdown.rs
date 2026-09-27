//! 定时关机、定时重启：到时间 Windows 自己关机或重启（和 `shutdown /s /t 秒数` 一样，到时间强制关掉所有程序），
//! 退出小药箱也照样会关；到时间以前随时能取消。
//!
//! 系统不提供「现在有没有安排关机」的查询，这里只记着小药箱自己安排的那一次（重新打开小药箱就不知道了，
//! 所以取消总是可以点：没有安排的话系统会说没有）。
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// 最少等 1 分钟
pub const MIN_DELAY_SECS: u32 = 60;
/// 最多等 24 小时（「到几点」算出来的可能多几十秒）
pub const MAX_DELAY_SECS: u32 = 24 * 3600 + 60;

/// 显示在系统通知里的话
#[cfg_attr(not(windows), allow(dead_code))]
const MESSAGE: &str = "这是在电脑小药箱里安排的。要取消：打开电脑小药箱，到「工具箱」的「定时关机、定时重启」里取消。";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownPlan {
    /// 到什么时候（Unix 毫秒）
    pub at: u64,
    /// 重启（不是关机）
    pub restart: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownStatus {
    /// 小药箱安排的那一次；没安排、取消了、或者时间早就过了是 None
    pub plan: Option<ShutdownPlan>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownCancel {
    /// 取消了一次安排好的关机或重启；false：本来就没有安排（可能已经在别处取消了）
    pub cancelled: bool,
}

#[derive(Default)]
pub struct ShutdownState {
    plan: Option<ShutdownPlan>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis().try_into().unwrap_or(u64::MAX))
}

impl ShutdownState {
    pub fn status(&mut self) -> ShutdownStatus {
        // 时间过了两分钟小药箱还开着：没关成（在别处取消了），不再显示
        if self.plan.is_some_and(|p| p.at + 120_000 < now_ms()) {
            self.plan = None;
        }
        ShutdownStatus { plan: self.plan }
    }

    /// `seconds` 秒以后关机（`restart` 时重启）。小药箱已经安排过的先取消，换成新的时间。
    pub fn schedule(&mut self, seconds: u32, restart: bool) -> Result<ShutdownStatus, String> {
        if !(MIN_DELAY_SECS..=MAX_DELAY_SECS).contains(&seconds) {
            return Err("时间要在 1 分钟以后、24 小时以内。".into());
        }
        if self.plan.is_some() {
            // 取消不了就还是原来那一次
            platform::abort()?;
            self.plan = None;
        }
        match platform::schedule(seconds, restart)? {
            Scheduled::Yes => {
                self.plan = Some(ShutdownPlan { at: now_ms() + u64::from(seconds) * 1000, restart });
                Ok(self.status())
            }
            Scheduled::AlreadyScheduled => Err(
                "已经有一次关机或重启安排好了（不是在这里安排的，也可能是别的程序）。要换成这个时间，先点「取消已经安排的关机或重启」，再安排。"
                    .into(),
            ),
        }
    }

    /// 取消已经安排的关机或重启（不管是谁安排的）。
    pub fn cancel(&mut self) -> Result<ShutdownCancel, String> {
        let cancelled = platform::abort()?;
        self.plan = None;
        Ok(ShutdownCancel { cancelled })
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
enum Scheduled {
    Yes,
    AlreadyScheduled,
}

#[cfg(windows)]
mod platform {
    use medkit_core::platform::shutdown::{self, ShutdownRequest};

    use super::{MESSAGE, Scheduled};

    pub fn schedule(seconds: u32, restart: bool) -> Result<Scheduled, String> {
        match shutdown::schedule(seconds, restart, MESSAGE) {
            Ok(ShutdownRequest::Scheduled) => Ok(Scheduled::Yes),
            Ok(ShutdownRequest::AlreadyScheduled) => Ok(Scheduled::AlreadyScheduled),
            Err(e) => Err(format!("没能安排：{e}")),
        }
    }

    pub fn abort() -> Result<bool, String> {
        shutdown::abort().map_err(|e| format!("没能取消：{e}"))
    }
}

/// 不是 Windows（开发机上跑测试）：什么也不做，当作系统接受了
#[cfg(not(windows))]
mod platform {
    use super::Scheduled;

    pub fn schedule(_seconds: u32, _restart: bool) -> Result<Scheduled, String> {
        Ok(Scheduled::Yes)
    }

    pub fn abort() -> Result<bool, String> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_replaces_and_cancels() {
        let mut s = ShutdownState::default();
        assert_eq!(s.status(), ShutdownStatus { plan: None });
        let before = now_ms();
        let plan = s.schedule(3600, false).unwrap().plan.unwrap();
        assert!(!plan.restart);
        assert!(plan.at >= before + 3_600_000 && plan.at <= now_ms() + 3_600_000);
        let plan = s.schedule(60, true).unwrap().plan.unwrap();
        assert!(plan.restart && plan.at <= now_ms() + 60_000, "换成新的时间");
        assert_eq!(s.cancel().unwrap(), ShutdownCancel { cancelled: true });
        assert_eq!(s.status(), ShutdownStatus { plan: None });
    }

    #[test]
    fn the_delay_must_be_between_a_minute_and_a_day() {
        let mut s = ShutdownState::default();
        assert!(s.schedule(59, false).is_err());
        assert!(s.schedule(MAX_DELAY_SECS + 1, false).is_err());
        assert!(s.schedule(MAX_DELAY_SECS, false).is_ok());
    }

    #[test]
    fn a_plan_long_past_is_forgotten() {
        let mut s = ShutdownState { plan: Some(ShutdownPlan { at: now_ms() - 121_000, restart: false }) };
        assert_eq!(s.status(), ShutdownStatus { plan: None });
        let recent = ShutdownPlan { at: now_ms() - 60_000, restart: false };
        let mut s = ShutdownState { plan: Some(recent) };
        assert_eq!(s.status(), ShutdownStatus { plan: Some(recent) }, "刚到时间：可能正在关机");
    }
}
