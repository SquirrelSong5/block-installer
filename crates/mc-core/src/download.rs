//! 并发下载引擎：多地址回退、SHA1 校验、断点续传、取消。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::http::sha1_hex;

#[derive(Clone, Debug)]
pub struct FileTask {
    /// 候选地址，按顺序尝试。
    pub urls: Vec<String>,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub executable: bool,
}

impl FileTask {
    pub fn new(urls: Vec<String>, path: PathBuf) -> Self {
        Self { urls, path, sha1: None, size: None, executable: false }
    }
    pub fn sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1.filter(|s| !s.is_empty()).map(|s| s.to_lowercase());
        self
    }
    pub fn size(mut self, size: Option<u64>) -> Self {
        self.size = size.filter(|s| *s > 0);
        self
    }
}

/// 一批下载的进度计数。
#[derive(Default, Debug)]
pub struct Counters {
    pub total_files: AtomicU64,
    pub done_files: AtomicU64,
    pub total_bytes: AtomicU64,
    pub done_bytes: AtomicU64,
}

impl Counters {
    pub fn fraction(&self) -> f64 {
        let tb = self.total_bytes.load(Ordering::Relaxed);
        let db = self.done_bytes.load(Ordering::Relaxed);
        let tf = self.total_files.load(Ordering::Relaxed);
        let df = self.done_files.load(Ordering::Relaxed);
        if tf == 0 {
            return 0.0;
        }
        if df >= tf {
            return 1.0;
        }
        if tb > 0 {
            (db as f64 / tb as f64).min(0.999)
        } else {
            df as f64 / tf as f64
        }
    }
    pub fn remaining_files(&self) -> u64 {
        self.total_files.load(Ordering::Relaxed).saturating_sub(self.done_files.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
    sem: Arc<Semaphore>,
    concurrency: usize,
    cancel: Arc<AtomicBool>,
    /// 整个任务实际从网络读到的字节数，用于算速度。
    net_bytes: Arc<AtomicU64>,
}

impl Downloader {
    pub fn new(client: reqwest::Client, concurrency: usize, cancel: Arc<AtomicBool>) -> Self {
        let concurrency = concurrency.clamp(1, 256);
        Self {
            client,
            sem: Arc::new(Semaphore::new(concurrency)),
            concurrency,
            cancel,
            net_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn net_bytes(&self) -> Arc<AtomicU64> {
        self.net_bytes.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// 下载一批文件。已存在且校验通过的文件直接跳过。全部完成才返回 Ok。
    pub async fn run(&self, tasks: Vec<FileTask>, counters: Arc<Counters>) -> Result<()> {
        let mut seen = HashSet::new();
        let tasks: Vec<FileTask> = tasks.into_iter().filter(|t| seen.insert(t.path.clone())).collect();
        counters.total_files.fetch_add(tasks.len() as u64, Ordering::Relaxed);
        let known: u64 = tasks.iter().filter_map(|t| t.size).sum();
        counters.total_bytes.fetch_add(known, Ordering::Relaxed);

        let results: Vec<(PathBuf, Result<()>)> = futures::stream::iter(tasks)
            .map(|t| {
                let counters = counters.clone();
                async move {
                    let path = t.path.clone();
                    let r = self.one(t, &counters).await;
                    (path, r)
                }
            })
            .buffer_unordered(self.concurrency)
            .collect()
            .await;

        if self.is_cancelled() {
            bail!("已取消");
        }
        let failed: Vec<_> = results.into_iter().filter_map(|(p, r)| r.err().map(|e| (p, e))).collect();
        if let Some((p, e)) = failed.first() {
            bail!("{} 个文件下载失败，例如 {}：{:#}", failed.len(), p.display(), e);
        }
        Ok(())
    }

    async fn one(&self, t: FileTask, counters: &Counters) -> Result<()> {
        let _permit = self.sem.acquire().await?;
        if self.is_cancelled() {
            bail!("已取消");
        }
        if file_ok(&t.path, t.sha1.as_deref(), t.size).await {
            self.finish(&t, counters, true);
            return Ok(());
        }
        if t.urls.is_empty() {
            bail!("缺少下载地址");
        }
        let mut last = anyhow!("下载失败");
        for round in 0..3 {
            for url in &t.urls {
                if self.is_cancelled() {
                    bail!("已取消");
                }
                match self.fetch_to(url, &t, counters).await {
                    Ok(()) => {
                        self.finish(&t, counters, false);
                        return Ok(());
                    }
                    Err(e) => last = e.context(url.clone()),
                }
            }
            tokio::time::sleep(Duration::from_millis(500 * (round + 1))).await;
        }
        Err(last)
    }

    fn finish(&self, t: &FileTask, counters: &Counters, skipped: bool) {
        counters.done_files.fetch_add(1, Ordering::Relaxed);
        // 本地已有的文件没有走字节计数，按声明大小补上。
        if skipped {
            if let Some(size) = t.size {
                counters.done_bytes.fetch_add(size, Ordering::Relaxed);
            }
        }
    }

    async fn fetch_to(&self, url: &str, t: &FileTask, counters: &Counters) -> Result<()> {
        if let Some(dir) = t.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let part = part_path(&t.path);
        let mut offset = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
        if let Some(size) = t.size {
            if offset >= size {
                offset = 0;
            }
        }
        let mut req = self.client.get(url);
        if offset > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            bail!("HTTP {status}");
        }
        let resume = offset > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = if resume {
            tokio::fs::OpenOptions::new().append(true).open(&part).await?
        } else {
            offset = 0;
            tokio::fs::File::create(&part).await?
        };
        // 续传时已有的部分先算作完成，失败重下时再扣回。
        let declared = t.size.is_some();
        if declared {
            counters.done_bytes.fetch_add(offset, Ordering::Relaxed);
        }
        let mut written = offset;
        let mut stream = resp.bytes_stream();
        let result: Result<()> = async {
            while let Some(chunk) = stream.next().await {
                if self.is_cancelled() {
                    bail!("已取消");
                }
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                written += chunk.len() as u64;
                self.net_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                if declared {
                    counters.done_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
            }
            file.flush().await?;
            Ok(())
        }
        .await;
        drop(file);
        if let Err(e) = result {
            if declared {
                counters.done_bytes.fetch_sub(written.min(counters.done_bytes.load(Ordering::Relaxed)), Ordering::Relaxed);
            }
            return Err(e);
        }

        let check = verify_file(&part, t.sha1.as_deref(), t.size).await;
        if let Err(e) = check {
            let _ = tokio::fs::remove_file(&part).await;
            if declared {
                counters.done_bytes.fetch_sub(written.min(counters.done_bytes.load(Ordering::Relaxed)), Ordering::Relaxed);
            }
            return Err(e);
        }
        tokio::fs::rename(&part, &t.path).await?;
        #[cfg(unix)]
        if t.executable {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&t.path, std::fs::Permissions::from_mode(0o755)).await;
        }
        Ok(())
    }
}

fn part_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// 文件存在且大小、SHA1 符合（没给的项不检查）。
pub async fn file_ok(path: &Path, sha1: Option<&str>, size: Option<u64>) -> bool {
    verify_file(path, sha1, size).await.is_ok()
}

pub async fn verify_file(path: &Path, sha1: Option<&str>, size: Option<u64>) -> Result<()> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| anyhow!("文件不存在"))?;
    if !meta.is_file() {
        bail!("不是文件");
    }
    if let Some(size) = size {
        if meta.len() != size {
            bail!("大小不符：{} ≠ {}", meta.len(), size);
        }
    } else if meta.len() == 0 && sha1.is_none() {
        bail!("文件为空");
    }
    if let Some(expect) = sha1 {
        let data = tokio::fs::read(path).await?;
        let actual = sha1_hex(&data);
        if !actual.eq_ignore_ascii_case(expect) {
            bail!("SHA1 不符：{actual} ≠ {expect}");
        }
    }
    Ok(())
}
