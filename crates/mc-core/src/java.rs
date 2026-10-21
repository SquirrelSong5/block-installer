//! Java：扫描本机已有的 Java、判断能否用于某个版本、下载 Mojang 官方运行时。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::download::{Counters, Downloader, FileTask};
use crate::http;
use crate::source::{candidates, Source};

pub const RUNTIME_INDEX: &str =
    "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    /// java.exe（或 unix 下 java）的完整路径。
    pub path: String,
    pub major: u32,
    pub version: String,
    pub is_64bit: bool,
    /// 来源说明，如“Mojang 运行时”“系统安装”。
    pub source: String,
}

const JAVA_EXE: &str = if cfg!(windows) { "java.exe" } else { "java" };

/// 解析 "1.8.0_431" / "17.0.8" / "21" 这类版本号的主版本。
pub fn parse_major(version: &str) -> Option<u32> {
    let v = version.trim().trim_matches('"');
    let mut parts = v.split(|c: char| c == '.' || c == '_' || c == '-' || c == '+');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// 读 JAVA_HOME/release 文件得到版本和架构，不用启动进程。
fn read_release(home: &Path) -> Option<(String, bool)> {
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().trim_matches('"').to_string())
    };
    let version = get("JAVA_VERSION=")?;
    let arch = get("OS_ARCH=").unwrap_or_default();
    let is_64 = !(arch == "x86" || arch == "i386" || arch == "i586");
    Some((version, is_64))
}

/// 运行 `java -version` 识别版本（release 文件缺失时的兜底）。
pub async fn probe(java: &Path) -> Result<JavaInfo> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-version").stdin(std::process::Stdio::null());
    hide_window(&mut cmd);
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| anyhow!("运行超时"))?
        .with_context(|| format!("无法运行 {}", java.display()))?;
    let text = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let version = text
        .lines()
        .find_map(|l| {
            let start = l.find('"')?;
            let rest = &l[start + 1..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .ok_or_else(|| anyhow!("看不懂 java -version 的输出：{}", text.lines().next().unwrap_or("")))?;
    let major = parse_major(&version).ok_or_else(|| anyhow!("无法识别版本号 {version}"))?;
    let is_64bit = text.contains("64-Bit") || text.contains("64-bit");
    Ok(JavaInfo { path: java.to_string_lossy().into_owned(), major, version, is_64bit, source: "手动指定".into() })
}

pub fn hide_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// 识别一个 JAVA_HOME（含 bin/java）。
async fn inspect_home(home: &Path, source: &str) -> Option<JavaInfo> {
    let exe = home.join("bin").join(JAVA_EXE);
    if !exe.is_file() {
        return None;
    }
    if let Some((version, is_64bit)) = read_release(home) {
        let major = parse_major(&version)?;
        return Some(JavaInfo { path: exe.to_string_lossy().into_owned(), major, version, is_64bit, source: source.into() });
    }
    let mut info = probe(&exe).await.ok()?;
    info.source = source.into();
    Some(info)
}

/// 在若干根目录下三层以内找 JAVA_HOME。
fn collect_homes(root: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if root.join("bin").join(JAVA_EXE).is_file() {
        out.push(root.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.filter_map(Result::ok) {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                collect_homes(&e.path(), depth - 1, out);
            }
        }
    }
}

/// 扫描本机所有能找到的 Java。
pub async fn scan(extra_roots: &[PathBuf]) -> Vec<JavaInfo> {
    let mut homes: Vec<(PathBuf, &'static str)> = Vec::new();
    let add_root = |root: PathBuf, depth: u32, source: &'static str, homes: &mut Vec<(PathBuf, &'static str)>| {
        let mut found = Vec::new();
        collect_homes(&root, depth, &mut found);
        homes.extend(found.into_iter().map(|h| (h, source)));
    };

    if let Some(mc) = crate::gamedir::appdata_minecraft() {
        add_root(mc.join("runtime"), 4, "Minecraft 运行时", &mut homes);
    }
    if let Some(local) = dirs::data_local_dir() {
        add_root(
            local.join("Packages").join("Microsoft.4297127D64EC6_8wekyb3d8bbwe").join("LocalCache").join("Local").join("runtime"),
            4,
            "Minecraft 运行时",
            &mut homes,
        );
    }
    for root in extra_roots {
        add_root(root.clone(), 3, "其他目录", &mut homes);
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        add_root(PathBuf::from(home), 0, "JAVA_HOME", &mut homes);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join(JAVA_EXE).is_file() {
                if let Some(home) = dir.parent() {
                    let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
                    homes.push((strip_unc(home), "环境变量 PATH"));
                }
            }
        }
    }

    #[cfg(windows)]
    {
        for home in registry_homes() {
            homes.push((home, "系统安装"));
        }
        for base in ["C:\\Program Files", "C:\\Program Files (x86)"] {
            for vendor in ["Java", "Eclipse Adoptium", "Eclipse Foundation", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "Oracle"] {
                add_root(PathBuf::from(base).join(vendor), 2, "系统安装", &mut homes);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        add_root(PathBuf::from("/Library/Java/JavaVirtualMachines"), 3, "系统安装", &mut homes);
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (home, source) in homes {
        let key = home.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Some(info) = inspect_home(&home, source).await {
            if !out.iter().any(|j: &JavaInfo| j.path.eq_ignore_ascii_case(&info.path)) {
                out.push(info);
            }
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then(b.version.cmp(&a.version)));
    out
}

fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p,
    }
}

#[cfg(windows)]
fn registry_homes() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;
    let mut out = Vec::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let bases = [
        r"SOFTWARE\JavaSoft\Java Runtime Environment",
        r"SOFTWARE\JavaSoft\Java Development Kit",
        r"SOFTWARE\JavaSoft\JRE",
        r"SOFTWARE\JavaSoft\JDK",
    ];
    for flag in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        for base in bases {
            let Ok(key) = hklm.open_subkey_with_flags(base, KEY_READ | flag) else { continue };
            for name in key.enum_keys().filter_map(Result::ok) {
                if let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ | flag) {
                    if let Ok(home) = sub.get_value::<String, _>("JavaHome") {
                        out.push(PathBuf::from(home));
                    }
                }
            }
        }
    }
    out
}

/// 版本要求 `required` 时这个 Java 能不能用：Java 8 版本只用 8；其余允许更高版本。
pub fn compatible(java: &JavaInfo, required: u32) -> bool {
    if required <= 8 {
        java.major == 8
    } else {
        java.major >= required
    }
}

/// 从已知 Java 里挑最合适的：主版本完全一致的优先，其次最接近的更高版本；64 位优先。
pub fn pick(list: &[JavaInfo], required: u32) -> Option<JavaInfo> {
    let mut ok: Vec<&JavaInfo> = list.iter().filter(|j| compatible(j, required)).collect();
    ok.sort_by_key(|j| (j.major != required, j.major, !j.is_64bit));
    ok.first().map(|j| (*j).clone())
}

/// Mojang 运行时在当前系统上的平台名。
pub fn runtime_platform() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else if cfg!(target_arch = "x86") {
            "windows-x86"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") { "mac-os-arm64" } else { "mac-os" }
    } else {
        "linux"
    }
}

/// Java 运行时安装目录：%APPDATA%\.minecraft\runtime\<组件>，和 PCL 一致，PCL 会自动识别。
pub fn runtime_dir(component: &str) -> Result<PathBuf> {
    let mc = crate::gamedir::appdata_minecraft().ok_or_else(|| anyhow!("找不到 %APPDATA% 目录"))?;
    Ok(mc.join("runtime").join(component))
}

/// 准备好下载 Mojang 运行时需要的文件清单。返回 (文件列表, 安装后的 java 路径, 显示用版本号)。
pub async fn runtime_tasks(
    client: &reqwest::Client,
    component: &str,
    dest: &Path,
    source: Source,
) -> Result<(Vec<FileTask>, PathBuf, String)> {
    let index: Value = http::get_json(client, &candidates(RUNTIME_INDEX, source)).await?;
    let platform = runtime_platform();
    let entry = |plat: &str| {
        index
            .get(plat)
            .and_then(|p| p.get(component))
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .cloned()
    };
    // ARM 版 Windows 缺少的组件用 x64 版代替（系统自带转译）。
    let entry = entry(platform)
        .or_else(|| if platform == "windows-arm64" { entry("windows-x64") } else { None })
        .ok_or_else(|| anyhow!("Mojang 没有为 {platform} 提供 {component}"))?;
    let manifest_url = entry["manifest"]["url"].as_str().ok_or_else(|| anyhow!("运行时清单缺少地址"))?;
    let version_name = entry["version"]["name"].as_str().unwrap_or(component).to_string();
    let manifest: Value = http::get_json(client, &candidates(manifest_url, source)).await?;
    let files = manifest["files"].as_object().ok_or_else(|| anyhow!("运行时清单格式不对"))?;

    let mut tasks = Vec::new();
    for (rel, f) in files {
        let path = rel.split('/').fold(dest.to_path_buf(), |p, part| p.join(part));
        match f["type"].as_str() {
            Some("directory") => {
                std::fs::create_dir_all(&path)?;
            }
            Some("file") => {
                let raw = &f["downloads"]["raw"];
                let Some(url) = raw["url"].as_str() else { continue };
                let mut t = FileTask::new(candidates(url, source), path)
                    .sha1(raw["sha1"].as_str().map(str::to_string))
                    .size(raw["size"].as_u64());
                t.executable = f["executable"].as_bool().unwrap_or(false);
                tasks.push(t);
            }
            Some("link") => {
                #[cfg(unix)]
                if let Some(target) = f["target"].as_str() {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let _ = std::fs::remove_file(&path);
                    let _ = std::os::unix::fs::symlink(target, &path);
                }
            }
            _ => {}
        }
    }
    let java = if cfg!(target_os = "macos") {
        dest.join("jre.bundle").join("Contents").join("Home").join("bin").join(JAVA_EXE)
    } else {
        dest.join("bin").join(JAVA_EXE)
    };
    Ok((tasks, java, version_name))
}

/// 下载并安装一个 Mojang 运行时，返回 java 路径。
pub async fn install_runtime(dl: &Downloader, component: &str, source: Source, counters: Arc<Counters>) -> Result<JavaInfo> {
    let dest = runtime_dir(component)?;
    let (tasks, java, version) = runtime_tasks(dl.client(), component, &dest, source).await?;
    dl.run(tasks, counters).await?;
    if !java.is_file() {
        bail!("Java 下载完成但找不到 {}", java.display());
    }
    let _ = std::fs::write(dest.join(".version"), &version);
    let major = parse_major(&version).unwrap_or(8);
    Ok(JavaInfo {
        path: java.to_string_lossy().into_owned(),
        major,
        version,
        is_64bit: !runtime_platform().ends_with("x86"),
        source: "Mojang 运行时".into(),
    })
}

/// 运行安装器工具时用 java.exe（有控制台输出）；游戏启动由启动器决定，这里不关心 javaw。
pub fn console_java(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    match p.file_name().and_then(|n| n.to_str()) {
        Some(n) if n.eq_ignore_ascii_case("javaw.exe") => p.with_file_name("java.exe"),
        _ => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors() {
        assert_eq!(parse_major("1.8.0_431"), Some(8));
        assert_eq!(parse_major("17.0.8"), Some(17));
        assert_eq!(parse_major("21"), Some(21));
        assert_eq!(parse_major("\"25.0.1\""), Some(25));
        assert_eq!(parse_major("8u51"), None);
    }

    fn j(major: u32, is_64bit: bool) -> JavaInfo {
        JavaInfo { path: format!("/j{major}"), major, version: major.to_string(), is_64bit, source: String::new() }
    }

    #[test]
    fn picking() {
        let list = vec![j(8, true), j(21, true), j(17, false), j(17, true), j(25, true)];
        assert_eq!(pick(&list, 8).unwrap().major, 8);
        let p = pick(&list, 17).unwrap();
        assert_eq!((p.major, p.is_64bit), (17, true));
        assert_eq!(pick(&list, 16).unwrap().major, 17);
        assert_eq!(pick(&list, 21).unwrap().major, 21);
        assert!(pick(&[j(17, true)], 8).is_none());
    }
}
