//! 一次完整安装：原版 + 可选加载器 + Java + 配置。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::download::{Counters, Downloader, FileTask};
use crate::gamedir::{self, GameDir};
use crate::java::{self, JavaInfo};
use crate::loaders::{fabric, forge, optifine, zip_extract, LoaderKind};
use crate::manifest::{self, VersionList};
use crate::source::{candidates, Source};
use crate::task::TaskHandle;
use crate::version::{self, Platform};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricApiPick {
    pub file_name: String,
    pub url: String,
    pub sha1: Option<String>,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub game_dir: String,
    pub mc_version: String,
    pub name: String,
    pub forge: Option<String>,
    pub neoforge: Option<String>,
    pub fabric: Option<String>,
    pub fabric_api: Option<FabricApiPick>,
    pub quilt: Option<String>,
    /// "HD_U/I6" 这种 type/patch 形式。
    pub optifine: Option<String>,
    pub isolated: bool,
    /// 例如 "zh_cn"；为空则不改语言。
    pub language: Option<String>,
    /// 用户手动指定的 Java；为空则自动选择。
    pub java_path: Option<String>,
    /// 找不到合适的 Java 时自动下载官方运行时。
    pub auto_java: bool,
}

#[derive(Clone)]
pub struct InstallOptions {
    pub source: Source,
    pub concurrency: usize,
    pub known_java: Vec<JavaInfo>,
    /// 可选：已拉取的版本清单，省一次请求。
    pub manifest: Option<VersionList>,
}

impl InstallRequest {
    /// 检查加载器组合是否合法，返回错误说明。
    pub fn conflicts(&self) -> Option<String> {
        let bases: Vec<&str> = [
            self.forge.as_ref().map(|_| "Forge"),
            self.neoforge.as_ref().map(|_| "NeoForge"),
            self.fabric.as_ref().map(|_| "Fabric"),
            self.quilt.as_ref().map(|_| "Quilt"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if bases.len() > 1 {
            return Some(format!("{} 和 {} 不能同时安装", bases[0], bases[1]));
        }
        if self.optifine.is_some() && bases.iter().any(|b| *b != "Forge") {
            return Some(format!("OptiFine 不能和 {} 一起安装", bases[0]));
        }
        if self.fabric_api.is_some() && self.fabric.is_none() {
            return Some("Fabric API 需要先选择 Fabric".into());
        }
        None
    }
}

const STEP_META: &str = "meta";
const STEP_CLIENT: &str = "client";
const STEP_LIBS: &str = "libraries";
const STEP_ASSETS: &str = "assets";
const STEP_JAVA: &str = "java";
const STEP_LOADER: &str = "loader";
const STEP_OPTIFINE: &str = "optifine";
const STEP_FABRIC_API: &str = "fabricApi";
const STEP_CONFIG: &str = "config";

/// 预先登记步骤，界面上一开始就能看到完整流程。
pub fn plan_steps(req: &InstallRequest, task: &TaskHandle) {
    task.add_step(STEP_META, "获取版本信息", 3.0);
    task.add_step(STEP_CLIENT, "下载游戏本体", 6.0);
    task.add_step(STEP_LIBS, "下载依赖库", 15.0);
    task.add_step(STEP_ASSETS, "下载资源文件", 35.0);
    task.add_step(STEP_JAVA, "准备 Java", 10.0);
    if req.forge.is_some() {
        task.add_step(STEP_LOADER, "安装 Forge", 20.0);
    } else if req.neoforge.is_some() {
        task.add_step(STEP_LOADER, "安装 NeoForge", 20.0);
    }
    if req.optifine.is_some() {
        task.add_step(STEP_OPTIFINE, "安装 OptiFine", 6.0);
    }
    if req.fabric_api.is_some() {
        task.add_step(STEP_FABRIC_API, "下载 Fabric API", 3.0);
    }
    task.add_step(STEP_CONFIG, "写入配置", 2.0);
}

struct Prepared {
    vanilla: Value,
    merged: Value,
    asset_index: Option<Value>,
    forge: Option<(PathBuf, forge::Installer)>,
    optifine_jar: Option<PathBuf>,
    java_major: u32,
    java_component: String,
}

pub async fn install(req: &InstallRequest, opts: &InstallOptions, task: &TaskHandle) -> Result<()> {
    if let Some(msg) = req.conflicts() {
        bail!(msg);
    }
    let dir = GameDir::new(&req.game_dir);
    if let Some(msg) = gamedir::validate_version_name(&dir, &req.name) {
        bail!(msg);
    }
    plan_steps(req, task);
    let platform = Platform::windows();
    let dl = Downloader::new(crate::http::client(), opts.concurrency, task.cancel.clone());
    task.attach_net_counter(dl.net_bytes());
    let work = std::env::temp_dir().join("block-installer").join(format!("task-{}", task.id()));
    std::fs::create_dir_all(&work)?;

    let result = run(req, opts, task, &dir, &dl, &work, &platform).await;
    let _ = std::fs::remove_dir_all(&work);
    result
}

async fn run(
    req: &InstallRequest,
    opts: &InstallOptions,
    task: &TaskHandle,
    dir: &GameDir,
    dl: &Downloader,
    work: &Path,
    platform: &Platform,
) -> Result<()> {
    let src = opts.source;

    // 1. 版本信息
    task.start(STEP_META, "读取版本清单");
    let p = prepare(req, opts, task, dir, dl, work).await.context("获取版本信息失败")?;
    task.done(STEP_META, &format!("Minecraft {}", req.mc_version));
    check_cancel(task)?;

    // 2. Java 选择（下载与其他文件并行）
    let required = p.java_major;
    let mut java: Option<JavaInfo> = None;
    if let Some(path) = &req.java_path {
        let info = java::probe(Path::new(path)).await.with_context(|| format!("指定的 Java 无法使用：{path}"))?;
        task.log(format!("使用指定的 Java {}：{}", info.version, info.path));
        java = Some(info);
    } else if let Some(found) = java::pick(&opts.known_java, required) {
        task.log(format!("使用本机 Java {}：{}", found.version, found.path));
        java = Some(found);
    }
    let needs_java_now = needs_java_for_install(req, &p);
    let download_java = java.is_none() && (req.auto_java || needs_java_now);

    // 3. 并行下载：本体、依赖库、资源、Java
    let client_tasks = client_jar_tasks(&p.vanilla, dir, &req.name, src)?;
    let (lib_tasks, missing_libs) = library_tasks(&p, dir, platform, src, work)?;
    if !missing_libs.is_empty() {
        task.log(format!("{} 个库将由安装器生成：{}", missing_libs.len(), missing_libs.join(", ")));
    }
    let asset_tasks = asset_tasks(&p, dir, src);

    let c_client = Arc::new(Counters::default());
    let c_libs = Arc::new(Counters::default());
    let c_assets = Arc::new(Counters::default());
    let c_java = Arc::new(Counters::default());
    task.attach_counters(STEP_CLIENT, c_client.clone());
    task.attach_counters(STEP_LIBS, c_libs.clone());
    task.attach_counters(STEP_ASSETS, c_assets.clone());
    task.start(STEP_CLIENT, "");
    task.start(STEP_LIBS, "");
    task.start(STEP_ASSETS, "");

    let java_fut = async {
        if let Some(j) = &java {
            task.done(STEP_JAVA, &format!("使用已有 Java {}", j.version));
            return Ok::<Option<JavaInfo>, anyhow::Error>(None);
        }
        if !download_java {
            task.done(STEP_JAVA, &format!("未找到 Java {required}，启动器启动时会提示下载"));
            return Ok(None);
        }
        task.start(STEP_JAVA, &format!("下载 Java {required}"));
        task.attach_counters(STEP_JAVA, c_java.clone());
        let info = java::install_runtime(dl, &p.java_component, src, c_java.clone())
            .await
            .with_context(|| format!("下载 Java {required} 失败"))?;
        task.done(STEP_JAVA, &format!("已下载 Java {}", info.version));
        task.log(format!("Java 已安装到 {}", info.path));
        Ok(Some(info))
    };
    let client_fut = async {
        dl.run(client_tasks, c_client.clone()).await.context("下载游戏本体失败")?;
        task.done(STEP_CLIENT, "已校验");
        Ok::<(), anyhow::Error>(())
    };
    let libs_fut = async {
        dl.run(lib_tasks, c_libs.clone()).await.context("下载依赖库失败")?;
        task.done(STEP_LIBS, &format!("{} 个文件", c_libs.total_files.load(std::sync::atomic::Ordering::Relaxed)));
        Ok::<(), anyhow::Error>(())
    };
    let assets_fut = async {
        dl.run(asset_tasks, c_assets.clone()).await.context("下载资源文件失败")?;
        task.done(STEP_ASSETS, &format!("{} 个文件", c_assets.total_files.load(std::sync::atomic::Ordering::Relaxed)));
        Ok::<(), anyhow::Error>(())
    };
    let (_, _, _, new_java) = tokio::try_join!(client_fut, libs_fut, assets_fut, java_fut)?;
    if new_java.is_some() {
        java = new_java;
    }
    check_cancel(task)?;

    let mut final_json = p.merged.clone();

    // 4. Forge / NeoForge
    if let Some((installer_path, installer)) = &p.forge {
        task.start(STEP_LOADER, "");
        if matches!(installer, forge::Installer::Modern { .. }) {
            let java = java
                .as_ref()
                .ok_or_else(|| anyhow!("安装 Forge 需要 Java {required}，请在设置里指定 Java 或开启自动下载"))?;
            run_forge(req, task, dir, dl, work, installer_path, installer, &p.vanilla, java, src).await?;
        } else {
            task.log("旧版 Forge 已直接放入依赖库，无需执行处理器");
        }
        task.done(STEP_LOADER, "完成");
    }
    check_cancel(task)?;

    // 5. OptiFine
    if let (Some(jar), Some(kind_patch)) = (&p.optifine_jar, &req.optifine) {
        task.start(STEP_OPTIFINE, "");
        if req.forge.is_some() {
            let mods = dir.run_dir(&req.name, req.isolated).join("mods");
            std::fs::create_dir_all(&mods)?;
            let dest = mods.join(optifine::mod_file_name(&req.mc_version, kind_patch));
            std::fs::copy(jar, &dest)?;
            task.done(STEP_OPTIFINE, "已作为模组放入 mods");
        } else {
            let java = java.as_ref();
            install_optifine_vanilla(req, task, dir, dl, jar, kind_patch, java, &mut final_json, src).await?;
            task.done(STEP_OPTIFINE, "完成");
        }
    }

    // 6. Fabric API
    if let Some(api) = &req.fabric_api {
        task.start(STEP_FABRIC_API, &api.version);
        let mods = dir.run_dir(&req.name, req.isolated).join("mods");
        let t = FileTask::new(vec![api.url.clone()], mods.join(&api.file_name)).sha1(api.sha1.clone());
        dl.run(vec![t], Arc::new(Counters::default())).await.context("下载 Fabric API 失败")?;
        task.done(STEP_FABRIC_API, &api.version);
    }
    check_cancel(task)?;

    // 7. 写入配置
    task.start(STEP_CONFIG, "");
    write_config(req, task, dir, &p, final_json)?;
    task.done(STEP_CONFIG, "完成");
    Ok(())
}

fn check_cancel(task: &TaskHandle) -> Result<()> {
    if task.is_cancelled() {
        bail!("已取消");
    }
    Ok(())
}

fn needs_java_for_install(req: &InstallRequest, p: &Prepared) -> bool {
    matches!(p.forge, Some((_, forge::Installer::Modern { .. })))
        || (req.optifine.is_some() && req.forge.is_none())
}

async fn prepare(
    req: &InstallRequest,
    opts: &InstallOptions,
    task: &TaskHandle,
    dir: &GameDir,
    dl: &Downloader,
    work: &Path,
) -> Result<Prepared> {
    let src = opts.source;
    let client = dl.client();
    let list = match &opts.manifest {
        Some(m) if m.find(&req.mc_version).is_some() => m.clone(),
        _ => manifest::fetch(client, src).await?,
    };
    let entry = list.find(&req.mc_version).ok_or_else(|| anyhow!("版本清单里没有 {}", req.mc_version))?;
    task.detail(STEP_META, "下载版本描述");
    let vanilla = manifest::fetch_version_json(client, entry, src).await?;
    let (java_component, java_major) = version::java_requirement(&vanilla);

    // 资源索引
    task.detail(STEP_META, "下载资源索引");
    let asset_index = match vanilla.get("assetIndex") {
        Some(ai) => {
            let id = ai["id"].as_str().unwrap_or("legacy");
            let url = ai["url"].as_str().ok_or_else(|| anyhow!("资源索引缺少地址"))?;
            let path = dir.assets().join("indexes").join(format!("{id}.json"));
            let t = FileTask::new(candidates(url, src), path.clone())
                .sha1(ai["sha1"].as_str().map(str::to_string))
                .size(ai["size"].as_u64());
            dl.run(vec![t], Arc::new(Counters::default())).await?;
            Some(serde_json::from_slice::<Value>(&std::fs::read(&path)?)?)
        }
        None => None,
    };

    let mut merged = vanilla.clone();
    let mut forge_installer = None;
    if let Some(loader) = &req.fabric {
        task.detail(STEP_META, "获取 Fabric 信息");
        let prof = fabric::profile(client, LoaderKind::Fabric, &req.mc_version, loader, src).await?;
        merged = version::merge(&merged, &prof);
    } else if let Some(loader) = &req.quilt {
        task.detail(STEP_META, "获取 Quilt 信息");
        let prof = fabric::profile(client, LoaderKind::Quilt, &req.mc_version, loader, src).await?;
        merged = version::merge(&merged, &prof);
    } else if let Some((kind, id)) = req
        .forge
        .as_ref()
        .map(|f| (LoaderKind::Forge, f))
        .or(req.neoforge.as_ref().map(|n| (LoaderKind::NeoForge, n)))
    {
        task.detail(STEP_META, &format!("下载 {} 安装器", kind.display()));
        let path = work.join("installer.jar");
        let t = FileTask::new(forge::installer_candidates(kind, id, src), path.clone());
        dl.run(vec![t], Arc::new(Counters::default()))
            .await
            .with_context(|| format!("下载 {} 安装器失败", kind.display()))?;
        let inst = forge::parse_installer(&path)?;
        merged = version::merge(&merged, inst.version_json());
        forge_installer = Some((path, inst));
    }

    let mut optifine_jar = None;
    if let Some(kind_patch) = &req.optifine {
        task.detail(STEP_META, "下载 OptiFine");
        let (kind, patch) = kind_patch.split_once('/').ok_or_else(|| anyhow!("OptiFine 版本标识不对"))?;
        let path = work.join("optifine.jar");
        let url = format!("{}/optifine/{}/{}/{}", crate::source::BMCLAPI, req.mc_version, kind, patch);
        dl.run(vec![FileTask::new(vec![url], path.clone())], Arc::new(Counters::default()))
            .await
            .context("下载 OptiFine 失败")?;
        optifine_jar = Some(path);
    }

    Ok(Prepared { vanilla, merged, asset_index, forge: forge_installer, optifine_jar, java_major, java_component })
}

fn client_jar_tasks(vanilla: &Value, dir: &GameDir, name: &str, src: Source) -> Result<Vec<FileTask>> {
    let c = &vanilla["downloads"]["client"];
    let url = c["url"].as_str().ok_or_else(|| anyhow!("版本描述里没有客户端下载地址"))?;
    Ok(vec![FileTask::new(candidates(url, src), dir.version_jar(name))
        .sha1(c["sha1"].as_str().map(str::to_string))
        .size(c["size"].as_u64())])
}

/// 依赖库下载列表。没有下载地址的库先尝试从安装器里解压；仍然没有的交给处理器生成（返回其路径列表）。
fn library_tasks(p: &Prepared, dir: &GameDir, platform: &Platform, src: Source, _work: &Path) -> Result<(Vec<FileTask>, Vec<String>)> {
    let mut files: Vec<version::LibFile> = p
        .merged
        .get("libraries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|l| version::library_files(l, platform))
        .collect();
    if let Some((_, forge::Installer::Modern { profile, .. })) = &p.forge {
        files.extend(forge::processor_libraries(profile));
    }

    let mut provided: HashSet<String> = HashSet::new();
    if let Some((installer, inst)) = &p.forge {
        match inst {
            forge::Installer::Legacy { universal_coord, universal_entry, .. } => {
                forge::install_legacy_universal(installer, universal_coord, universal_entry, dir)?;
                if let Some(rel) = version::maven_path(universal_coord) {
                    provided.insert(rel);
                }
            }
            forge::Installer::Modern { .. } => {
                let all: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
                provided.extend(forge::extract_bundled(installer, &all, dir)?);
            }
        }
    }

    let mut tasks = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for f in files {
        if !seen.insert(f.path.clone()) || provided.contains(&f.path) {
            continue;
        }
        match &f.url {
            Some(url) => tasks.push(
                FileTask::new(candidates(url, src), dir.library(&f.path)).sha1(f.sha1.clone()).size(f.size),
            ),
            None => missing.push(f.path.rsplit('/').next().unwrap_or(&f.path).to_string()),
        }
    }

    // 日志配置
    if let Some(file) = p.vanilla.get("logging").and_then(|l| l.get("client")).and_then(|c| c.get("file")) {
        if let (Some(id), Some(url)) = (file["id"].as_str(), file["url"].as_str()) {
            tasks.push(
                FileTask::new(candidates(url, src), dir.assets().join("log_configs").join(id))
                    .sha1(file["sha1"].as_str().map(str::to_string))
                    .size(file["size"].as_u64()),
            );
        }
    }
    Ok((tasks, missing))
}

fn asset_tasks(p: &Prepared, dir: &GameDir, src: Source) -> Vec<FileTask> {
    let Some(index) = &p.asset_index else { return vec![] };
    let Some(objects) = index.get("objects").and_then(Value::as_object) else { return vec![] };
    objects
        .values()
        .filter_map(|o| {
            let hash = o.get("hash")?.as_str()?;
            let prefix = hash.get(0..2)?;
            let url = format!("https://resources.download.minecraft.net/{prefix}/{hash}");
            Some(
                FileTask::new(candidates(&url, src), dir.assets().join("objects").join(prefix).join(hash))
                    .sha1(Some(hash.to_string()))
                    .size(o.get("size").and_then(Value::as_u64)),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn run_forge(
    req: &InstallRequest,
    task: &TaskHandle,
    dir: &GameDir,
    dl: &Downloader,
    work: &Path,
    installer_path: &Path,
    installer: &forge::Installer,
    vanilla: &Value,
    java: &JavaInfo,
    src: Source,
) -> Result<()> {
    let forge::Installer::Modern { profile, .. } = installer else {
        task.log("旧版 Forge，无需执行处理器");
        return Ok(());
    };
    let java_exe = java::console_java(&java.path);
    let jar = dir.version_jar(&req.name);
    let ctx = forge::ProcessorContext {
        installer: installer_path,
        dir,
        mc_version: &req.mc_version,
        minecraft_jar: &jar,
        java: &java_exe,
        work_dir: &work.join("data"),
        vanilla_json: vanilla,
    };
    let steps = forge::prepare_processors(&ctx, profile)?;
    let total = steps.len().max(1);
    for (i, step) in steps.iter().enumerate() {
        check_cancel(task)?;
        match step {
            forge::Step::Mojmaps(m) => {
                task.progress(STEP_LOADER, i as f64 / total as f64, "下载官方映射表");
                let t = FileTask::new(candidates(&m.url, src), m.output.clone()).sha1(m.sha1.clone()).size(m.size);
                dl.run(vec![t], Arc::new(Counters::default())).await.context("下载映射表失败")?;
            }
            forge::Step::Run(proc) => {
                task.progress(STEP_LOADER, i as f64 / total as f64, &format!("执行 {}（{}/{}）", proc.jar_name, i + 1, total));
                task.log(format!("执行处理器 {}", proc.jar_name));
                let log_task = task.clone();
                let cancel_task = task.clone();
                forge::run_processor(&java_exe, proc, &move |l| log_task.log(l), &move || cancel_task.is_cancelled()).await?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn install_optifine_vanilla(
    req: &InstallRequest,
    task: &TaskHandle,
    dir: &GameDir,
    dl: &Downloader,
    jar: &Path,
    kind_patch: &str,
    java: Option<&JavaInfo>,
    json: &mut Value,
    src: Source,
) -> Result<()> {
    let plan = optifine::plan_vanilla(jar, &req.mc_version, kind_patch)?;
    // launchwrapper
    let wrapper_dest = optifine::lib_dest(dir, &plan.wrapper_coord)?;
    match &plan.wrapper_bundled_entry {
        Some(entry) => {
            if !zip_extract(jar, entry, &wrapper_dest)? {
                bail!("OptiFine 包里缺少 {entry}");
            }
        }
        None => {
            let url = "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar";
            dl.run(vec![FileTask::new(candidates(url, src), wrapper_dest)], Arc::new(Counters::default())).await?;
        }
    }
    // OptiFine 本体
    let dest = optifine::lib_dest(dir, &plan.optifine_coord)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if plan.needs_patch {
        let java = java.ok_or_else(|| anyhow!("安装 OptiFine 需要 Java，请在设置里指定或允许自动下载"))?;
        task.detail(STEP_OPTIFINE, "生成 OptiFine 库文件");
        let mc_jar = dir.version_jar(&req.name);
        let mut cmd = tokio::process::Command::new(java::console_java(&java.path));
        cmd.arg("-cp").arg(jar).arg("optifine.Patcher").arg(&mc_jar).arg(jar).arg(&dest);
        cmd.stdin(std::process::Stdio::null());
        java::hide_window(&mut cmd);
        let out = cmd.output().await.context("无法运行 OptiFine 安装程序")?;
        if !out.status.success() || !dest.is_file() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("OptiFine 安装失败：{}", err.lines().last().unwrap_or("未知错误"));
        }
    } else {
        std::fs::copy(jar, &dest)?;
    }
    optifine::apply_to_json(json, &plan);
    Ok(())
}

fn write_config(req: &InstallRequest, task: &TaskHandle, dir: &GameDir, p: &Prepared, mut json: Value) -> Result<()> {
    // 版本描述
    json["id"] = json!(req.name);
    json["clientVersion"] = json!(req.mc_version);
    let path = dir.version_json(&req.name);
    std::fs::create_dir_all(dir.version_dir(&req.name))?;
    std::fs::write(&path, serde_json::to_vec_pretty(&json)?)?;
    task.log(format!("已写入 {}", path.display()));

    let run_dir = dir.run_dir(&req.name, req.isolated);
    // 老版本资源：virtual / map_to_resources 需要按原始路径复制一份
    if let (Some(index), Some(ai)) = (&p.asset_index, p.vanilla.get("assetIndex")) {
        let id = ai["id"].as_str().unwrap_or("legacy");
        let virtual_ = index.get("virtual").and_then(Value::as_bool).unwrap_or(false);
        let to_resources = index.get("map_to_resources").and_then(Value::as_bool).unwrap_or(false);
        if virtual_ || to_resources {
            let target = if to_resources { run_dir.join("resources") } else { dir.assets().join("virtual").join(id) };
            let mut n = 0;
            for (name, o) in index["objects"].as_object().into_iter().flatten() {
                let Some(hash) = o["hash"].as_str() else { continue };
                let src = dir.assets().join("objects").join(&hash[..2]).join(hash);
                let dest = name.split('/').fold(target.clone(), |acc, s| acc.join(s));
                if dest.is_file() {
                    continue;
                }
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if std::fs::copy(&src, &dest).is_ok() {
                    n += 1;
                }
            }
            task.log(format!("已整理 {n} 个旧版资源文件"));
        }
    }

    if let Some(lang) = req.language.as_deref().filter(|l| !l.is_empty()) {
        let legacy = is_legacy_lang(&req.mc_version);
        gamedir::write_language(&run_dir, lang, legacy)?;
    }
    gamedir::write_pcl_isolation(dir, &req.name, req.isolated)?;
    if req.isolated {
        std::fs::create_dir_all(&run_dir)?;
    }
    let game_dir = if req.isolated { Some(run_dir.as_path()) } else { None };
    if let Err(e) = gamedir::ensure_launcher_profile(dir, &req.name, game_dir) {
        task.log(format!("写入 launcher_profiles.json 失败（不影响 PCL）：{e:#}"));
    }
    Ok(())
}

/// 1.10.2 及更早语言代码用 zh_CN 形式。
fn is_legacy_lang(mc: &str) -> bool {
    let mut parts = mc.split('.');
    if parts.next() != Some("1") {
        return false;
    }
    parts.next().and_then(|m| m.parse::<u32>().ok()).map(|m| m <= 10).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_rules() {
        let mut r = InstallRequest { forge: Some("x".into()), fabric: Some("y".into()), ..Default::default() };
        assert!(r.conflicts().is_some());
        r.fabric = None;
        r.optifine = Some("HD_U/I6".into());
        assert!(r.conflicts().is_none());
        r.forge = None;
        r.quilt = Some("q".into());
        assert!(r.conflicts().is_some());
    }

    #[test]
    fn legacy_lang() {
        assert!(is_legacy_lang("1.7.10"));
        assert!(is_legacy_lang("1.10.2"));
        assert!(!is_legacy_lang("1.11"));
        assert!(!is_legacy_lang("26.3"));
    }
}
