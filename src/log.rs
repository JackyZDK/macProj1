//! 轻量日志：内存环形缓冲（用于界面显示）+ 文件追加（用于持久化）。

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const RING_CAPACITY: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sev {
    Info,
    Warn,
    Error,
}

impl Sev {
    pub fn label(&self) -> &'static str {
        match self {
            Sev::Info => "INFO",
            Sev::Warn => "WARN",
            Sev::Error => "ERROR",
        }
    }
}

#[derive(Clone)]
pub struct LogEntry {
    pub sev: Sev,
    pub msg: String,
    pub ts: String, // "HH:MM:SS"
}

/// 应用内日志收集器。线程安全：GUI 线程与命令执行线程共用（用 Mutex 保证）。
#[derive(Default)]
pub struct AppLogs {
    ring: VecDeque<LogEntry>,
}

fn now_hhmmss() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 本地时间格式化：直接用 libc 转换。
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t: libc::time_t = secs as libc::time_t;
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

impl AppLogs {
    pub fn log_file() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        std::path::Path::new(&home)
            .join("Library")
            .join("Logs")
            .join("nicmgr.log")
    }

    /// 写入日志：追加到内存环形缓冲 + 追加到文件。
    pub fn push(&mut self, sev: Sev, msg: impl Into<String>) {
        let entry = LogEntry {
            sev,
            msg: msg.into(),
            ts: now_hhmmss(),
        };
        if self.ring.len() >= RING_CAPACITY {
            self.ring.pop_front();
        }
        self.ring.push_back(entry.clone());

        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(Self::log_file())
        {
            let _ = writeln!(f, "[{}] [{}] {}", entry.ts, entry.sev.label(), entry.msg);
        }
    }

    /// 最新条目在前，供界面自顶向下显示最新日志。
    pub fn latest_first(&self) -> impl DoubleEndedIterator<Item = &LogEntry> {
        self.ring.iter().rev()
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}