//! 安装任务的状态：步骤、进度、速度、日志。界面定时读取快照。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::download::Counters;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StepState {
    Waiting,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepSnapshot {
    pub key: String,
    pub title: String,
    pub state: StepState,
    pub progress: f64,
    pub detail: String,
    #[serde(skip)]
    weight: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: u64,
    pub kind: String,
    pub title: String,
    pub version_name: String,
    pub status: TaskStatus,
    pub steps: Vec<StepSnapshot>,
    pub progress: f64,
    pub speed: f64,
    pub remaining_files: u64,
    pub error: Option<String>,
    pub logs: Vec<String>,
}

struct Inner {
    snap: TaskSnapshot,
    counters: HashMap<String, Arc<Counters>>,
    last_sample: Option<(Instant, u64)>,
    speed: f64,
}

#[derive(Clone)]
pub struct TaskHandle {
    inner: Arc<Mutex<Inner>>,
    pub cancel: Arc<AtomicBool>,
    net_bytes: Arc<Mutex<Option<Arc<AtomicU64>>>>,
}

const MAX_LOGS: usize = 300;

impl TaskHandle {
    pub fn new(id: u64, kind: &str, title: &str, version_name: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snap: TaskSnapshot {
                    id,
                    kind: kind.into(),
                    title: title.into(),
                    version_name: version_name.into(),
                    status: TaskStatus::Queued,
                    steps: vec![],
                    progress: 0.0,
                    speed: 0.0,
                    remaining_files: 0,
                    error: None,
                    logs: vec![],
                },
                counters: HashMap::new(),
                last_sample: None,
                speed: 0.0,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            net_bytes: Arc::new(Mutex::new(None)),
        }
    }

    pub fn id(&self) -> u64 {
        self.inner.lock().unwrap().snap.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn attach_net_counter(&self, c: Arc<AtomicU64>) {
        *self.net_bytes.lock().unwrap() = Some(c);
    }

    pub fn add_step(&self, key: &str, title: &str, weight: f64) {
        let mut g = self.inner.lock().unwrap();
        if g.snap.steps.iter().any(|s| s.key == key) {
            return;
        }
        g.snap.steps.push(StepSnapshot {
            key: key.into(),
            title: title.into(),
            state: StepState::Waiting,
            progress: 0.0,
            detail: String::new(),
            weight,
        });
    }

    fn with_step(&self, key: &str, f: impl FnOnce(&mut StepSnapshot)) {
        let mut g = self.inner.lock().unwrap();
        if let Some(s) = g.snap.steps.iter_mut().find(|s| s.key == key) {
            f(s);
        }
    }

    pub fn set_status(&self, status: TaskStatus) {
        self.inner.lock().unwrap().snap.status = status;
    }

    pub fn start(&self, key: &str, detail: &str) {
        self.with_step(key, |s| {
            s.state = StepState::Running;
            s.detail = detail.into();
        });
    }

    pub fn attach_counters(&self, key: &str, c: Arc<Counters>) {
        self.inner.lock().unwrap().counters.insert(key.into(), c);
    }

    pub fn progress(&self, key: &str, fraction: f64, detail: &str) {
        self.with_step(key, |s| {
            s.progress = fraction.clamp(0.0, 1.0);
            s.detail = detail.into();
        });
    }

    pub fn detail(&self, key: &str, detail: &str) {
        self.with_step(key, |s| s.detail = detail.into());
    }

    pub fn done(&self, key: &str, detail: &str) {
        let mut g = self.inner.lock().unwrap();
        g.counters.remove(key);
        if let Some(s) = g.snap.steps.iter_mut().find(|s| s.key == key) {
            s.state = StepState::Done;
            s.progress = 1.0;
            s.detail = detail.into();
        }
    }

    pub fn fail_running(&self, msg: &str) {
        let mut g = self.inner.lock().unwrap();
        for s in g.snap.steps.iter_mut().filter(|s| s.state == StepState::Running) {
            s.state = StepState::Failed;
        }
        g.snap.error = Some(msg.into());
    }

    pub fn log(&self, line: impl Into<String>) {
        let mut g = self.inner.lock().unwrap();
        g.snap.logs.push(line.into());
        let n = g.snap.logs.len();
        if n > MAX_LOGS {
            g.snap.logs.drain(0..n - MAX_LOGS);
        }
    }

    /// 生成快照；顺带根据网络字节计数更新速度。
    pub fn snapshot(&self) -> TaskSnapshot {
        let net = self.net_bytes.lock().unwrap().as_ref().map(|c| c.load(Ordering::Relaxed));
        let mut g = self.inner.lock().unwrap();

        if let Some(bytes) = net {
            let now = Instant::now();
            match g.last_sample {
                Some((t, b)) if now.duration_since(t).as_millis() >= 400 => {
                    let dt = now.duration_since(t).as_secs_f64();
                    let instant = bytes.saturating_sub(b) as f64 / dt;
                    g.speed = if g.speed == 0.0 { instant } else { g.speed * 0.5 + instant * 0.5 };
                    g.last_sample = Some((now, bytes));
                }
                None => g.last_sample = Some((now, bytes)),
                _ => {}
            }
        }

        let counters: Vec<(String, Arc<Counters>)> = g.counters.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut remaining = 0;
        for (key, c) in &counters {
            remaining += c.remaining_files();
            let done = c.done_files.load(Ordering::Relaxed);
            let total = c.total_files.load(Ordering::Relaxed);
            let frac = c.fraction();
            if let Some(s) = g.snap.steps.iter_mut().find(|s| &s.key == key) {
                if s.state == StepState::Running {
                    s.progress = frac;
                    if total > 0 {
                        s.detail = format!("{done} / {total}");
                    }
                }
            }
        }
        let total_w: f64 = g.snap.steps.iter().map(|s| s.weight).sum();
        let progress = if total_w > 0.0 {
            g.snap.steps.iter().map(|s| s.weight * s.progress).sum::<f64>() / total_w
        } else {
            0.0
        };
        let running = g.snap.status == TaskStatus::Running;
        g.snap.progress = if g.snap.status == TaskStatus::Done { 1.0 } else { progress };
        g.snap.remaining_files = remaining;
        g.snap.speed = if running { g.speed } else { 0.0 };
        g.snap.clone()
    }
}
