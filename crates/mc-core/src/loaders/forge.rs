//! Forge 与 NeoForge：版本列表、解析安装器、执行安装器里的处理器。
//!
//! 新版安装器（1.13 起）不能只放文件，需要用 Java 跑一组“处理器”生成补丁后的客户端；
//! 这里按安装器的 install_profile.json 自己执行，依赖库走我们自己的下载引擎（可用镜像）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::{compare_versions, zip_extract, zip_read, LoaderKind, LoaderVersion};
use crate::gamedir::GameDir;
use crate::http;
use crate::source::{candidates, Source, BMCLAPI};
use crate::version::{maven_path, LibFile};

// ---------- 版本列表 ----------

pub async fn list_forge(client: &reqwest::Client, mc: &str, source: Source) -> Result<Vec<LoaderVersion>> {
    let mut out = match source {
        Source::Bmclapi => match list_forge_bmclapi(client, mc).await {
            Ok(v) if !v.is_empty() => v,
            _ => list_forge_official(client, mc).await?,
        },
        Source::Official => list_forge_official(client, mc).await?,
    };
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));

    // 推荐 / 最新 标记来自 Forge 官方的 promotions。
    let promos: Option<Value> = http::get_json(
        client,
        &["https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json".to_string()],
    )
    .await
    .ok();
    let promo = |k: &str| {
        promos
            .as_ref()
            .and_then(|p| p.get("promos"))
            .and_then(|p| p.get(format!("{mc}-{k}")))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let (recommended, latest) = (promo("recommended"), promo("latest"));
    for v in out.iter_mut() {
        if Some(&v.version) == recommended.as_ref() {
            v.tag = Some("推荐".into());
        } else if Some(&v.version) == latest.as_ref() {
            v.tag = Some("最新".into());
        }
    }
    if latest.is_none() {
        if let Some(first) = out.first_mut() {
            first.tag.get_or_insert_with(|| "最新".into());
        }
    }
    Ok(out)
}

async fn list_forge_bmclapi(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let arr: Vec<Value> = http::get_json(client, &[format!("{BMCLAPI}/forge/minecraft/{mc}")]).await?;
    Ok(arr
        .iter()
        .filter_map(|e| {
            let version = e.get("version")?.as_str()?.to_string();
            let files = e.get("files").and_then(Value::as_array);
            let installer = files.and_then(|fs| {
                fs.iter().find(|f| {
                    f.get("category").and_then(Value::as_str) == Some("installer")
                        && f.get("format").and_then(Value::as_str) == Some("jar")
                })
            });
            // 没有安装器的极老版本（1.5.1 及更早）不支持。
            installer?;
            let branch = e.get("branch").and_then(Value::as_str).filter(|b| !b.is_empty());
            let full = match branch {
                Some(b) => format!("{mc}-{version}-{b}"),
                None => format!("{mc}-{version}"),
            };
            Some(LoaderVersion {
                url: Some(forge_installer_url(&full)),
                sha1: installer.and_then(|f| f.get("hash")).and_then(Value::as_str).map(str::to_string),
                id: full,
                version,
                date: e.get("modified").and_then(Value::as_str).map(str::to_string),
                stable: true,
                tag: None,
                note: None,
            })
        })
        .collect())
}

async fn list_forge_official(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let meta: Value = http::get_json(
        client,
        &["https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json".to_string()],
    )
    .await?;
    let list = meta.get(mc).and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(Value::as_str)
        .map(|full| {
            let rest = full.strip_prefix(&format!("{mc}-")).unwrap_or(full);
            let version = rest.split('-').next().unwrap_or(rest).to_string();
            LoaderVersion {
                id: full.to_string(),
                version,
                date: None,
                stable: true,
                tag: None,
                note: None,
                url: Some(forge_installer_url(full)),
                sha1: None,
            }
        })
        .collect())
}

pub fn forge_installer_url(full: &str) -> String {
    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar")
}

/// NeoForge 版本号前缀：1.21.1 → "21.1."，26.3 → "26.3.0."。
fn neoforge_prefix(mc: &str) -> String {
    let parts: Vec<&str> = mc.split('.').collect();
    if parts.first() == Some(&"1") {
        format!("{}.{}.", parts.get(1).unwrap_or(&"0"), parts.get(2).unwrap_or(&"0"))
    } else {
        format!("{}.{}.{}.", parts.first().unwrap_or(&"0"), parts.get(1).unwrap_or(&"0"), parts.get(2).unwrap_or(&"0"))
    }
}

pub async fn list_neoforge(client: &reqwest::Client, mc: &str, source: Source) -> Result<Vec<LoaderVersion>> {
    // 官方仓库的列表最完整（BMCLAPI 的列表偶尔缺最新几个），优先用官方，失败再用镜像。
    let mut out = match list_neoforge_official(client, mc).await {
        Ok(v) if !v.is_empty() => v,
        Ok(_) | Err(_) if source == Source::Bmclapi => list_neoforge_bmclapi(client, mc).await.unwrap_or_default(),
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    if let Some(first) = out.first_mut() {
        first.tag = Some("最新".into());
    }
    if let Some(stable) = out.iter_mut().find(|v| v.stable) {
        if stable.tag.is_none() {
            stable.tag = Some("推荐".into());
        }
    }
    Ok(out)
}

async fn list_neoforge_official(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let artifact = if mc == "1.20.1" { "forge" } else { "neoforge" };
    let url = format!("https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/{artifact}");
    let v: Value = http::get_json(client, &[url]).await?;
    let prefix = if mc == "1.20.1" { "1.20.1-".to_string() } else { neoforge_prefix(mc) };
    Ok(v.get("versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|raw| raw.starts_with(&prefix))
        .map(|raw| neo_entry(artifact, raw))
        .collect())
}

async fn list_neoforge_bmclapi(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let arr: Vec<Value> = http::get_json(client, &[format!("{BMCLAPI}/neoforge/list/{mc}")]).await?;
    Ok(arr
        .iter()
        .filter_map(|e| {
            // installerPath 形如 /maven/net/neoforged/<artifact>/<version>/...
            if let Some(path) = e.get("installerPath").and_then(Value::as_str) {
                let parts: Vec<&str> = path.split('/').collect();
                let i = parts.iter().position(|p| *p == "neoforged")?;
                return Some(neo_entry(parts.get(i + 1)?, parts.get(i + 2)?));
            }
            let raw = e.get("rawVersion")?.as_str()?;
            Some(match raw.strip_prefix("neoforge-") {
                Some(v) => neo_entry("neoforge", v),
                None => neo_entry("forge", &raw.replace("-forge-", "-")),
            })
        })
        .collect())
}

fn neo_entry(artifact: &str, artifact_version: &str) -> LoaderVersion {
    // 1.20.1 的 NeoForge 沿用 forge 坐标，版本号带 "1.20.1-" 前缀，显示时去掉。
    let display = artifact_version.strip_prefix("1.20.1-").unwrap_or(artifact_version).to_string();
    LoaderVersion {
        id: format!("{artifact}:{artifact_version}"),
        stable: !display.contains("beta") && !display.contains("alpha"),
        version: display,
        date: None,
        tag: None,
        note: None,
        url: Some(format!(
            "https://maven.neoforged.net/releases/net/neoforged/{artifact}/{artifact_version}/{artifact}-{artifact_version}-installer.jar"
        )),
        sha1: None,
    }
}

pub fn installer_url(kind: LoaderKind, id: &str) -> String {
    match kind {
        LoaderKind::NeoForge => {
            let (artifact, ver) = id.split_once(':').unwrap_or(("neoforge", id));
            format!("https://maven.neoforged.net/releases/net/neoforged/{artifact}/{ver}/{artifact}-{ver}-installer.jar")
        }
        _ => forge_installer_url(id),
    }
}

// ---------- 解析安装器 ----------

pub enum Installer {
    /// 1.12.2 及更早：版本描述在 install_profile.json 的 versionInfo 里，主文件直接从安装器里解压。
    Legacy { version_json: Value, universal_coord: String, universal_entry: String },
    /// 1.13 起：install_profile.json + version.json + 处理器。
    Modern { profile: Value, version_json: Value },
}

impl Installer {
    pub fn version_json(&self) -> &Value {
        match self {
            Installer::Legacy { version_json, .. } | Installer::Modern { version_json, .. } => version_json,
        }
    }
}

pub fn parse_installer(path: &Path) -> Result<Installer> {
    let profile_bytes = zip_read(path, "install_profile.json")?.ok_or_else(|| anyhow!("安装器里没有 install_profile.json"))?;
    let profile: Value = serde_json::from_slice(&profile_bytes).context("install_profile.json 格式不对")?;
    if let Some(info) = profile.get("versionInfo") {
        let install = profile.get("install").ok_or_else(|| anyhow!("旧版安装器缺少 install 字段"))?;
        return Ok(Installer::Legacy {
            version_json: info.clone(),
            universal_coord: install.get("path").and_then(Value::as_str).unwrap_or_default().to_string(),
            universal_entry: install.get("filePath").and_then(Value::as_str).unwrap_or_default().to_string(),
        });
    }
    let json_entry = profile.get("json").and_then(Value::as_str).unwrap_or("/version.json").trim_start_matches('/').to_string();
    let vj = zip_read(path, &json_entry)?.ok_or_else(|| anyhow!("安装器里没有 {json_entry}"))?;
    Ok(Installer::Modern { version_json: serde_json::from_slice(&vj)?, profile })
}

/// 安装器自带（maven/ 目录）的库文件，直接解压，不用下载。
pub fn extract_bundled(installer: &Path, lib_rel_paths: &[String], dir: &GameDir) -> Result<Vec<String>> {
    let mut extracted = Vec::new();
    for rel in lib_rel_paths {
        let dest = dir.library(rel);
        if zip_extract(installer, &format!("maven/{rel}"), &dest)? {
            extracted.push(rel.clone());
        }
    }
    Ok(extracted)
}

/// 处理器需要的依赖库（install_profile.libraries）。
pub fn processor_libraries(profile: &Value) -> Vec<LibFile> {
    let platform = crate::version::Platform::windows();
    profile
        .get("libraries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|l| crate::version::library_files(l, &platform))
        .collect()
}

/// 旧版安装器：把 universal 解压到 libraries。
pub fn install_legacy_universal(installer: &Path, coord: &str, entry: &str, dir: &GameDir) -> Result<()> {
    let rel = maven_path(coord).ok_or_else(|| anyhow!("无法解析 {coord}"))?;
    if !zip_extract(installer, entry, &dir.library(&rel))? {
        bail!("安装器里没有 {entry}");
    }
    Ok(())
}

// ---------- 执行处理器 ----------

pub struct ProcessorContext<'a> {
    pub installer: &'a Path,
    pub dir: &'a GameDir,
    pub mc_version: &'a str,
    /// 原版客户端 jar（versions/<名称>/<名称>.jar）。
    pub minecraft_jar: &'a Path,
    pub java: &'a Path,
    pub work_dir: &'a Path,
    /// 原版版本描述（DOWNLOAD_MOJMAPS 需要从这里拿映射表地址）。
    pub vanilla_json: &'a Value,
}

/// 一个处理器在执行前需要的“替代下载”：DOWNLOAD_MOJMAPS 改由我们自己下载。
pub struct MojmapsDownload {
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub output: PathBuf,
}

pub struct PreparedProcessor {
    pub main_class: String,
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    pub outputs: Vec<(PathBuf, String)>,
    pub jar_name: String,
}

pub enum Step {
    Run(PreparedProcessor),
    Mojmaps(MojmapsDownload),
}

fn lib_path(dir: &GameDir, coord: &str) -> Result<PathBuf> {
    Ok(dir.library(&maven_path(coord).ok_or_else(|| anyhow!("无法解析库坐标 {coord}"))?))
}

/// 把 data 字段解析成实际值（库路径、字面量、安装器内文件）。
fn resolve_data(ctx: &ProcessorContext, profile: &Value) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    map.insert("SIDE".into(), "client".into());
    map.insert("MINECRAFT_JAR".into(), ctx.minecraft_jar.to_string_lossy().into_owned());
    map.insert("MINECRAFT_VERSION".into(), ctx.mc_version.to_string());
    map.insert("ROOT".into(), ctx.dir.root.to_string_lossy().into_owned());
    map.insert("INSTALLER".into(), ctx.installer.to_string_lossy().into_owned());
    map.insert("LIBRARY_DIR".into(), ctx.dir.libraries().to_string_lossy().into_owned());
    if let Some(data) = profile.get("data").and_then(Value::as_object) {
        for (k, v) in data {
            let Some(raw) = v.get("client").and_then(Value::as_str) else { continue };
            let value = if raw.starts_with('[') && raw.ends_with(']') {
                lib_path(ctx.dir, &raw[1..raw.len() - 1])?.to_string_lossy().into_owned()
            } else if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
                raw[1..raw.len() - 1].to_string()
            } else if let Some(entry) = raw.strip_prefix('/') {
                let dest = entry.split('/').fold(ctx.work_dir.to_path_buf(), |p, s| p.join(s));
                if !zip_extract(ctx.installer, entry, &dest)? {
                    bail!("安装器里没有 {entry}");
                }
                dest.to_string_lossy().into_owned()
            } else {
                raw.to_string()
            };
            map.insert(k.clone(), value);
        }
    }
    Ok(map)
}

fn substitute(arg: &str, data: &HashMap<String, String>, dir: &GameDir) -> Result<String> {
    if arg.starts_with('[') && arg.ends_with(']') {
        return Ok(lib_path(dir, &arg[1..arg.len() - 1])?.to_string_lossy().into_owned());
    }
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}') else { break };
        let key = &rest[start + 1..start + end];
        out.push_str(&rest[..start]);
        match data.get(key) {
            Some(v) => out.push_str(v),
            None => bail!("处理器参数里有未知变量 {{{key}}}"),
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn main_class_of(jar: &Path) -> Result<String> {
    let mf = zip_read(jar, "META-INF/MANIFEST.MF")?.ok_or_else(|| anyhow!("{} 没有 MANIFEST.MF", jar.display()))?;
    let text = String::from_utf8_lossy(&mf).replace("\r\n", "\n");
    // 清单里长行会以空格续行，先拼回来。
    let text = text.replace("\n ", "");
    text.lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|s| s.trim().to_string())
        .ok_or_else(|| anyhow!("{} 没有 Main-Class", jar.display()))
}

/// 根据 install_profile 准备要执行的处理器（只取客户端的）。
pub fn prepare_processors(ctx: &ProcessorContext, profile: &Value) -> Result<Vec<Step>> {
    let data = resolve_data(ctx, profile)?;
    let mut steps = Vec::new();
    for p in profile.get("processors").and_then(Value::as_array).into_iter().flatten() {
        if let Some(sides) = p.get("sides").and_then(Value::as_array) {
            if !sides.iter().any(|s| s.as_str() == Some("client")) {
                continue;
            }
        }
        let jar_coord = p.get("jar").and_then(Value::as_str).ok_or_else(|| anyhow!("处理器缺少 jar"))?;
        let raw_args: Vec<String> = p
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let args: Vec<String> = raw_args.iter().map(|a| substitute(a, &data, ctx.dir)).collect::<Result<_>>()?;

        let task = raw_args.iter().position(|a| a == "--task").and_then(|i| raw_args.get(i + 1));
        if task.map(String::as_str) == Some("DOWNLOAD_MOJMAPS") {
            let output = args
                .iter()
                .position(|a| a == "--output")
                .and_then(|i| args.get(i + 1))
                .ok_or_else(|| anyhow!("DOWNLOAD_MOJMAPS 缺少 --output"))?;
            let m = &ctx.vanilla_json["downloads"]["client_mappings"];
            let url = m["url"].as_str().ok_or_else(|| anyhow!("原版描述里没有 client_mappings"))?;
            steps.push(Step::Mojmaps(MojmapsDownload {
                url: url.to_string(),
                sha1: m["sha1"].as_str().map(str::to_string),
                size: m["size"].as_u64(),
                output: PathBuf::from(output),
            }));
            continue;
        }

        let jar = lib_path(ctx.dir, jar_coord)?;
        let main_class = main_class_of(&jar)?;
        let mut classpath = vec![jar];
        for c in p.get("classpath").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            classpath.push(lib_path(ctx.dir, c)?);
        }
        let mut outputs = Vec::new();
        if let Some(o) = p.get("outputs").and_then(Value::as_object) {
            for (k, v) in o {
                let path = substitute(k, &data, ctx.dir)?;
                let sha = substitute(v.as_str().unwrap_or_default(), &data, ctx.dir)?;
                outputs.push((PathBuf::from(path), sha.trim_matches('\'').to_lowercase()));
            }
        }
        steps.push(Step::Run(PreparedProcessor {
            main_class,
            classpath,
            args,
            outputs,
            jar_name: jar_coord.split(':').nth(1).unwrap_or(jar_coord).to_string(),
        }));
    }
    Ok(steps)
}

fn outputs_ok(outputs: &[(PathBuf, String)]) -> bool {
    !outputs.is_empty()
        && outputs.iter().all(|(p, sha)| {
            std::fs::read(p).map(|d| sha.is_empty() || http::sha1_hex(&d) == *sha).unwrap_or(false)
        })
}

/// 运行一个处理器。`log` 收到处理器的输出行；`cancelled` 返回 true 时终止进程。
pub async fn run_processor(
    java: &Path,
    p: &PreparedProcessor,
    log: &(dyn Fn(String) + Send + Sync),
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<()> {
    if outputs_ok(&p.outputs) {
        log(format!("{} 的输出已存在，跳过", p.jar_name));
        return Ok(());
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = p.classpath.iter().map(|c| c.to_string_lossy().into_owned()).collect::<Vec<_>>().join(sep);
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-cp").arg(cp).arg(&p.main_class).args(&p.args);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::java::hide_window(&mut cmd);
    let mut child = cmd.spawn().with_context(|| format!("无法启动 Java：{}", java.display()))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let pump = |r: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>| {
        let tail = tail.clone();
        async move {
            let Some(r) = r else { return Vec::new() };
            let mut lines = BufReader::new(r).lines();
            let mut got = Vec::new();
            while let Ok(Some(l)) = lines.next_line().await {
                {
                    let mut t = tail.lock().unwrap();
                    t.push(l.clone());
                    if t.len() > 20 {
                        t.remove(0);
                    }
                }
                got.push(l);
            }
            got
        }
    };
    let out_fut = pump(stdout.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>));
    let err_fut = pump(stderr.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>));
    let reader = tokio::spawn(async move {
        let (a, b) = tokio::join!(out_fut, err_fut);
        a.into_iter().chain(b).collect::<Vec<_>>()
    });
    let status = loop {
        tokio::select! {
            s = child.wait() => break s?,
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                if cancelled() {
                    let _ = child.kill().await;
                    bail!("已取消");
                }
            }
        }
    };
    let lines = reader.await.unwrap_or_default();
    let shown: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    for l in &shown[shown.len().saturating_sub(40)..] {
        log(format!("  {l}"));
    }
    if !status.success() {
        let last = tail.lock().unwrap().iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_default();
        bail!("{} 执行失败（退出码 {:?}）：{}", p.jar_name, status.code(), last);
    }
    for (path, sha) in &p.outputs {
        let data = std::fs::read(path).with_context(|| format!("{} 没有生成 {}", p.jar_name, path.display()))?;
        if !sha.is_empty() && http::sha1_hex(&data) != *sha {
            bail!("{} 生成的 {} 校验不符", p.jar_name, path.display());
        }
    }
    Ok(())
}

/// 官方地址 → 候选地址；BMCLAPI 另外提供按版本下载 Forge 安装器的接口，作为额外备选。
pub fn installer_candidates(kind: LoaderKind, id: &str, source: Source) -> Vec<String> {
    let mut c = candidates(&installer_url(kind, id), source);
    if kind == LoaderKind::Forge && source == Source::Bmclapi {
        let parts: Vec<&str> = id.splitn(3, '-').collect();
        if parts.len() >= 2 {
            let mut u = format!(
                "{BMCLAPI}/forge/download?mcversion={}&version={}&category=installer&format=jar",
                parts[0], parts[1]
            );
            if let Some(branch) = parts.get(2) {
                u.push_str(&format!("&branch={branch}"));
            }
            c.push(u);
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neo_prefix() {
        assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("26.3"), "26.3.0.");
    }

    #[test]
    fn substitution() {
        let dir = GameDir::new("/mc");
        let mut data = HashMap::new();
        data.insert("SIDE".to_string(), "client".to_string());
        assert_eq!(substitute("{SIDE}", &data, &dir).unwrap(), "client");
        assert_eq!(substitute("--side={SIDE}", &data, &dir).unwrap(), "--side=client");
        assert!(substitute("[net.minecraft:client:1.20.1:srg]", &data, &dir).unwrap().ends_with("client-1.20.1-srg.jar"));
        assert!(substitute("{NOPE}", &data, &dir).is_err());
    }
}
