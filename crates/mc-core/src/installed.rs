//! 已安装版本：列出、识别加载器、检查完整性、补全缺失文件、删除。

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;

use crate::download::{file_ok, Counters, Downloader, FileTask};
use crate::gamedir::{self, GameDir};
use crate::source::{candidates, Source};
use crate::task::TaskHandle;
use crate::version::{self, Platform};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub name: String,
    pub mc_version: String,
    /// 例如 ["Forge 47.4.8", "OptiFine HD_U_I6"]；原版为空。
    pub loaders: Vec<String>,
    pub isolated: bool,
    /// 版本描述损坏或缺少父版本时的说明。
    pub problem: Option<String>,
    pub modified: Option<u64>,
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// 读取并展开 inheritsFrom，得到完整的版本描述。
pub fn resolve(dir: &GameDir, name: &str) -> Result<Value> {
    let mut json = read_json(&dir.version_json(name)).ok_or_else(|| anyhow!("版本描述文件损坏或不存在"))?;
    let mut depth = 0;
    while let Some(parent) = json.get("inheritsFrom").and_then(Value::as_str).map(str::to_string) {
        depth += 1;
        if depth > 5 {
            break;
        }
        let p = read_json(&dir.version_json(&parent)).ok_or_else(|| anyhow!("缺少父版本 {parent}"))?;
        // 合并时子版本的 jar 字段会被丢掉，这里先记下来。
        let jar = json.get("jar").cloned().or_else(|| Some(Value::String(parent.clone())));
        json = version::merge(&p, &json);
        if let Some(j) = jar {
            json["jar"] = j;
        }
    }
    Ok(json)
}

fn lib_version(json: &Value, prefix: &str) -> Option<String> {
    json.get("libraries")?.as_array()?.iter().find_map(|l| {
        let name = l.get("name")?.as_str()?;
        name.strip_prefix(prefix).map(|v| v.split(':').next().unwrap_or(v).to_string())
    })
}

fn game_arg(json: &Value, key: &str) -> Option<String> {
    let game = json.get("arguments")?.get("game")?.as_array()?;
    let i = game.iter().position(|a| a.as_str() == Some(key))?;
    game.get(i + 1)?.as_str().map(str::to_string)
}

pub fn detect_loaders(json: &Value, mods_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let is_neo = lib_version(json, "net.neoforged:").is_some() || lib_version(json, "net.neoforged.fancymodloader:").is_some();
    if let Some(v) = game_arg(json, "--fml.neoForgeVersion") {
        out.push(format!("NeoForge {v}"));
    } else if let Some(v) = game_arg(json, "--fml.forgeVersion") {
        out.push(format!("{} {v}", if is_neo { "NeoForge" } else { "Forge" }));
    } else if let Some(v) = lib_version(json, "net.minecraftforge:forge:").or_else(|| lib_version(json, "net.minecraftforge:minecraftforge:")) {
        let v = v.split('-').nth(1).unwrap_or(&v).to_string();
        out.push(format!("Forge {v}"));
    }
    if let Some(v) = lib_version(json, "net.fabricmc:fabric-loader:") {
        out.push(format!("Fabric {v}"));
    }
    if let Some(v) = lib_version(json, "org.quiltmc:quilt-loader:") {
        out.push(format!("Quilt {v}"));
    }
    if let Some(v) = lib_version(json, "optifine:OptiFine:") {
        let v = v.splitn(2, '_').nth(1).unwrap_or(&v).to_string();
        out.push(format!("OptiFine {v}"));
    } else if let Ok(rd) = std::fs::read_dir(mods_dir) {
        if rd.filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().to_lowercase().contains("optifine")) {
            out.push("OptiFine".into());
        }
    }
    out
}

pub fn list(dir: &GameDir) -> Vec<InstalledVersion> {
    let Ok(rd) = std::fs::read_dir(dir.versions()) else { return vec![] };
    let mut out: Vec<InstalledVersion> = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let json_path = dir.version_json(&name);
            if !json_path.is_file() {
                return None;
            }
            let modified = std::fs::metadata(&json_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let isolated = gamedir::read_pcl_isolation(dir, &name).unwrap_or_else(|| {
                let vd = dir.version_dir(&name);
                vd.join("saves").is_dir() || vd.join("mods").is_dir()
            });
            let mods = dir.run_dir(&name, isolated).join("mods");
            match resolve(dir, &name) {
                Ok(json) => {
                    let mc = json
                        .get("clientVersion")
                        .and_then(Value::as_str)
                        .or_else(|| json.get("jar").and_then(Value::as_str))
                        .map(str::to_string)
                        .or_else(|| game_arg(&json, "--fml.mcVersion"))
                        .or_else(|| json.get("assets").and_then(Value::as_str).filter(|a| a.starts_with("1.") && a.matches('.').count() >= 1 && !a.contains("legacy")).map(str::to_string))
                        .unwrap_or_else(|| json.get("id").and_then(Value::as_str).unwrap_or(&name).to_string());
                    Some(InstalledVersion { loaders: detect_loaders(&json, &mods), name, mc_version: mc, isolated, problem: None, modified })
                }
                Err(e) => Some(InstalledVersion {
                    name,
                    mc_version: String::new(),
                    loaders: vec![],
                    isolated,
                    problem: Some(format!("{e:#}")),
                    modified,
                }),
            }
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// 根据版本描述列出这个版本需要的所有文件（本体、依赖库、资源）。
pub fn required_files(dir: &GameDir, name: &str, json: &Value, source: Source) -> Vec<FileTask> {
    let platform = Platform::windows();
    let mut tasks = Vec::new();
    let jar_name = json.get("jar").and_then(Value::as_str).unwrap_or(name);
    if let Some(c) = json.get("downloads").and_then(|d| d.get("client")) {
        if let Some(url) = c["url"].as_str() {
            tasks.push(
                FileTask::new(candidates(url, source), dir.version_jar(jar_name))
                    .sha1(c["sha1"].as_str().map(str::to_string))
                    .size(c["size"].as_u64()),
            );
        }
    }
    for lib in json.get("libraries").and_then(Value::as_array).into_iter().flatten() {
        for f in version::library_files(lib, &platform) {
            let urls = f.url.as_deref().map(|u| candidates(u, source)).unwrap_or_default();
            tasks.push(FileTask::new(urls, dir.library(&f.path)).sha1(f.sha1).size(f.size));
        }
    }
    if let Some(id) = json.get("assetIndex").and_then(|a| a.get("id")).and_then(Value::as_str) {
        let idx_path = dir.assets().join("indexes").join(format!("{id}.json"));
        if let Some(url) = json["assetIndex"]["url"].as_str() {
            tasks.push(FileTask::new(candidates(url, source), idx_path.clone()).sha1(json["assetIndex"]["sha1"].as_str().map(str::to_string)));
        }
        if let Some(index) = read_json(&idx_path) {
            for o in index.get("objects").and_then(Value::as_object).into_iter().flat_map(|m| m.values()) {
                let Some(hash) = o.get("hash").and_then(Value::as_str) else { continue };
                let url = format!("https://resources.download.minecraft.net/{}/{hash}", &hash[..2]);
                tasks.push(
                    FileTask::new(candidates(&url, source), dir.assets().join("objects").join(&hash[..2]).join(hash))
                        .size(o.get("size").and_then(Value::as_u64))
                        .sha1(Some(hash.to_string())),
                );
            }
        }
    }
    tasks
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub total: usize,
    pub missing: usize,
    /// 缺失且没有下载地址（需要重新安装加载器才能恢复）的文件数。
    pub unrecoverable: usize,
    pub examples: Vec<String>,
}

/// 检查缺失文件。`deep` 为 true 时校验 SHA1（慢），否则只看是否存在和大小。
pub async fn verify(dir: &GameDir, name: &str, deep: bool, source: Source) -> Result<(VerifyReport, Vec<FileTask>)> {
    let json = resolve(dir, name)?;
    let tasks = required_files(dir, name, &json, source);
    let total = tasks.len();
    let mut missing = Vec::new();
    for t in tasks {
        let ok = if deep {
            file_ok(&t.path, t.sha1.as_deref(), t.size).await
        } else {
            file_ok(&t.path, None, t.size).await
        };
        if !ok {
            missing.push(t);
        }
    }
    let unrecoverable = missing.iter().filter(|t| t.urls.is_empty()).count();
    let examples = missing
        .iter()
        .take(5)
        .map(|t| t.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .collect();
    Ok((VerifyReport { total, missing: missing.len(), unrecoverable, examples }, missing))
}

/// 补全文件任务。
pub async fn repair(dir: &GameDir, name: &str, source: Source, concurrency: usize, task: &TaskHandle) -> Result<()> {
    task.add_step("check", "检查文件", 20.0);
    task.add_step("download", "下载缺失文件", 80.0);
    task.start("check", "");
    let (report, missing) = verify(dir, name, true, source).await?;
    task.done("check", &format!("缺少 {} 个文件", report.missing));
    let downloadable: Vec<FileTask> = missing.into_iter().filter(|t| !t.urls.is_empty()).collect();
    let dl = Downloader::new(crate::http::client(), concurrency, task.cancel.clone());
    task.attach_net_counter(dl.net_bytes());
    let c = Arc::new(Counters::default());
    task.attach_counters("download", c.clone());
    task.start("download", "");
    dl.run(downloadable, c).await?;
    if report.unrecoverable > 0 {
        task.done("download", &format!("{} 个加载器生成的文件无法补全，需要重新安装这个版本", report.unrecoverable));
    } else {
        task.done("download", "完成");
    }
    Ok(())
}

pub fn delete(dir: &GameDir, name: &str) -> Result<()> {
    let path = dir.version_dir(name);
    if !path.starts_with(dir.versions()) || name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(anyhow!("版本名称不合法"));
    }
    std::fs::remove_dir_all(&path)?;
    Ok(())
}
