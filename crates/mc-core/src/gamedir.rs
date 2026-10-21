//! 游戏目录（.minecraft）：路径约定、自动识别、校验，以及写给 PCL / 官方启动器的配置。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct GameDir {
    pub root: PathBuf,
}

impl GameDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, name: &str) -> PathBuf {
        self.versions().join(name)
    }
    pub fn version_json(&self, name: &str) -> PathBuf {
        self.version_dir(name).join(format!("{name}.json"))
    }
    pub fn version_jar(&self, name: &str) -> PathBuf {
        self.version_dir(name).join(format!("{name}.jar"))
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn library(&self, rel: &str) -> PathBuf {
        let mut p = self.libraries();
        for part in rel.split('/') {
            p.push(part);
        }
        p
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    /// 版本隔离时游戏运行目录是版本文件夹，否则是 .minecraft 本身。
    pub fn run_dir(&self, name: &str, isolated: bool) -> PathBuf {
        if isolated {
            self.version_dir(name)
        } else {
            self.root.clone()
        }
    }
}

/// %APPDATA%\.minecraft（PCL 和官方启动器下载 Java 的位置也在这里的 runtime 子目录）。
pub fn appdata_minecraft() -> Option<PathBuf> {
    if cfg!(windows) {
        dirs::config_dir().map(|d| d.join(".minecraft"))
    } else if cfg!(target_os = "macos") {
        dirs::config_dir().map(|d| d.join("minecraft"))
    } else {
        dirs::home_dir().map(|d| d.join(".minecraft"))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirInfo {
    pub path: String,
    /// 来源说明，如“PCL 所在目录”。
    pub source: String,
    pub exists: bool,
    pub writable: bool,
    pub is_pcl: bool,
    pub version_count: usize,
    /// 给用户看的一句话状态。
    pub message: String,
}

/// 检查一个路径能不能作为游戏目录使用。
pub fn inspect(path: &Path, source: &str) -> DirInfo {
    let exists = path.is_dir();
    let is_pcl = pcl_dir_of(path).is_some();
    let version_count = count_versions(path);
    let writable = if exists { can_write(path) } else { parent_writable(path) };
    let message = if !writable {
        "没有写入权限，换一个目录，或者用管理员身份运行".to_string()
    } else if !exists {
        "目录还不存在，安装时会自动创建".to_string()
    } else if is_pcl {
        format!("PCL 的游戏目录，已有 {version_count} 个版本")
    } else if path.join("versions").is_dir() || path.join("libraries").is_dir() {
        format!("已有的游戏目录，{version_count} 个版本")
    } else if fs::read_dir(path).map(|mut d| d.next().is_none()).unwrap_or(false) {
        "空文件夹，可以直接使用".to_string()
    } else {
        "这个文件夹里有其他文件，建议选一个名为 .minecraft 的文件夹".to_string()
    };
    DirInfo {
        path: path.to_string_lossy().into_owned(),
        source: source.into(),
        exists,
        writable,
        is_pcl,
        version_count,
        message,
    }
}

fn count_versions(root: &Path) -> usize {
    let Ok(rd) = fs::read_dir(root.join("versions")) else { return 0 };
    rd.filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            e.path().join(format!("{name}.json")).is_file()
        })
        .count()
}

fn can_write(dir: &Path) -> bool {
    let probe = dir.join(".block-installer-probe");
    let ok = fs::write(&probe, b"ok").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

fn parent_writable(path: &Path) -> bool {
    let mut p = path.parent();
    while let Some(dir) = p {
        if dir.is_dir() {
            return can_write(dir);
        }
        p = dir.parent();
    }
    false
}

fn is_pcl_exe(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe") && (lower.starts_with("plain craft launcher") || lower.starts_with("pcl"))
}

/// 如果这个 .minecraft 属于某个 PCL（同级目录里有 PCL 的 exe 或 PCL 配置文件夹），返回 PCL 所在目录。
pub fn pcl_dir_of(game_dir: &Path) -> Option<PathBuf> {
    let parent = game_dir.parent()?;
    if dir_has_pcl(parent) {
        Some(parent.to_path_buf())
    } else {
        None
    }
}

fn dir_has_pcl(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    rd.filter_map(Result::ok).any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        (p.is_file() && is_pcl_exe(&name)) || (name == "PCL" && p.join("Setup.ini").is_file())
    })
}

/// 读 PCL 本地配置里“当前选择的游戏目录”。`$` 开头表示相对 PCL 所在目录。
fn pcl_selected_folder(pcl_dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(pcl_dir.join("PCL").join("Setup.ini")).ok()?;
    let value = text.lines().find_map(|l| l.strip_prefix("LaunchFolderSelect:"))?.trim();
    if value.is_empty() {
        return None;
    }
    let path = match value.strip_prefix('$') {
        Some(rel) => pcl_dir.join(rel.trim_start_matches(['\\', '/'])),
        None => PathBuf::from(value),
    };
    Some(PathBuf::from(path.to_string_lossy().trim_end_matches(['\\', '/'])))
}

/// 自动识别候选游戏目录：本程序旁边、常见位置里找到的 PCL、%APPDATA%\.minecraft。
pub fn detect_candidates(exe_dir: Option<&Path>) -> Vec<DirInfo> {
    let mut out: Vec<DirInfo> = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |p: PathBuf, source: &str, out: &mut Vec<DirInfo>| {
        let key = p.to_string_lossy().to_lowercase();
        if seen.insert(key) {
            out.push(inspect(&p, source));
        }
    };

    if let Some(dir) = exe_dir {
        if dir_has_pcl(dir) {
            let p = pcl_selected_folder(dir).unwrap_or_else(|| dir.join(".minecraft"));
            push(p, "PCL 所在目录（和本程序放在一起）", &mut out);
        } else if dir.join(".minecraft").is_dir() {
            push(dir.join(".minecraft"), "本程序所在目录", &mut out);
        }
    }

    for pcl in find_pcl_installs() {
        let p = pcl_selected_folder(&pcl).unwrap_or_else(|| pcl.join(".minecraft"));
        push(p, "找到的 PCL", &mut out);
    }

    if let Some(p) = appdata_minecraft() {
        if p.is_dir() {
            push(p, "官方启动器默认目录", &mut out);
        } else if out.is_empty() {
            push(p, "推荐位置（新建）", &mut out);
        }
    }
    out
}

/// 在桌面、下载、文档和各磁盘根目录下两层以内找 PCL。
fn find_pcl_installs() -> Vec<PathBuf> {
    // PCL 只有 Windows 版；其他系统上扫描用户目录还会触发隐私权限弹窗。
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut roots: Vec<PathBuf> = [dirs::desktop_dir(), dirs::download_dir(), dirs::document_dir()]
        .into_iter()
        .flatten()
        .collect();
    if cfg!(windows) {
        for letter in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if root.is_dir() {
                roots.push(root);
            }
        }
    }
    let mut found = Vec::new();
    let mut budget = 4000usize;
    for root in roots {
        scan_for_pcl(&root, 0, &mut budget, &mut found);
        if found.len() >= 8 {
            break;
        }
    }
    found
}

fn scan_for_pcl(dir: &Path, depth: u32, budget: &mut usize, found: &mut Vec<PathBuf>) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    let mut has_pcl = false;
    for e in rd.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_file() && is_pcl_exe(&name) {
            has_pcl = true;
        } else if ft.is_dir() && depth < 2 && !name.starts_with('.') && !name.starts_with('$') && !is_system_dir(&name) {
            subdirs.push(e.path());
        }
    }
    if has_pcl && !found.contains(&dir.to_path_buf()) {
        found.push(dir.to_path_buf());
    }
    for d in subdirs {
        scan_for_pcl(&d, depth + 1, budget, found);
    }
}

fn is_system_dir(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "windows" | "program files" | "program files (x86)" | "programdata" | "appdata" | "node_modules" | "library" | "system volume information"
    )
}

/// 确保有 launcher_profiles.json，并为新版本加一个配置（官方启动器靠它显示版本）。
pub fn ensure_launcher_profile(dir: &GameDir, version_name: &str, game_dir: Option<&Path>) -> Result<()> {
    fs::create_dir_all(&dir.root)?;
    let path = dir.root.join("launcher_profiles.json");
    let mut v: Value = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    if !v.is_object() {
        v = json!({});
    }
    let obj = v.as_object_mut().unwrap();
    obj.entry("profiles").or_insert_with(|| json!({}));
    obj.entry("selectedProfile").or_insert_with(|| json!("(Default)"));
    obj.entry("authenticationDatabase").or_insert_with(|| json!({}));
    obj.entry("version").or_insert_with(|| json!(3));
    if let Some(profiles) = obj.get_mut("profiles").and_then(Value::as_object_mut) {
        let mut p = json!({
            "name": version_name,
            "type": "custom",
            "lastVersionId": version_name,
            "icon": "Grass",
        });
        if let Some(g) = game_dir {
            p["gameDir"] = json!(g.to_string_lossy());
        }
        profiles.insert(version_name.to_string(), p);
    }
    fs::write(&path, serde_json::to_vec_pretty(&v)?).context("写入 launcher_profiles.json 失败")?;
    Ok(())
}

/// 写 PCL 的版本配置（versions/<name>/PCL/Setup.ini，每行 `键:值`），这里只设置版本隔离。
pub fn write_pcl_isolation(dir: &GameDir, name: &str, isolated: bool) -> Result<()> {
    let ini_dir = dir.version_dir(name).join("PCL");
    fs::create_dir_all(&ini_dir)?;
    let path = ini_dir.join("Setup.ini");
    let mut lines: Vec<(String, String)> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();
    let mut set = |k: &str, v: &str| match lines.iter_mut().find(|(key, _)| key == k) {
        Some(e) => e.1 = v.into(),
        None => lines.push((k.into(), v.into())),
    };
    set("VersionArgumentIndie", if isolated { "1" } else { "0" });
    set("VersionArgumentIndieV2", if isolated { "True" } else { "False" });
    let text: String = lines.iter().map(|(k, v)| format!("{k}:{v}\r\n")).collect();
    fs::write(&path, text)?;
    Ok(())
}

/// 读 PCL 版本配置里的隔离设置；没有配置时返回 None。
pub fn read_pcl_isolation(dir: &GameDir, name: &str) -> Option<bool> {
    let text = fs::read_to_string(dir.version_dir(name).join("PCL").join("Setup.ini")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("VersionArgumentIndieV2:"))
        .map(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// 设置游戏语言：没有 options.txt 时新建；已有时只改 lang 这一行。
pub fn write_language(run_dir: &Path, lang: &str, legacy_case: bool) -> Result<()> {
    fs::create_dir_all(run_dir)?;
    let lang = if legacy_case {
        // 1.10 及更早用 zh_CN 这种大小写
        match lang.split_once('_') {
            Some((a, b)) => format!("{a}_{}", b.to_uppercase()),
            None => lang.to_string(),
        }
    } else {
        lang.to_lowercase()
    };
    let path = run_dir.join("options.txt");
    let text = match fs::read_to_string(&path) {
        Ok(old) => {
            let mut found = false;
            let mut lines: Vec<String> = old
                .lines()
                .map(|l| {
                    if l.starts_with("lang:") {
                        found = true;
                        format!("lang:{lang}")
                    } else {
                        l.to_string()
                    }
                })
                .collect();
            if !found {
                lines.push(format!("lang:{lang}"));
            }
            lines.join("\n") + "\n"
        }
        Err(_) => format!("lang:{lang}\n"),
    };
    fs::write(&path, text)?;
    Ok(())
}

/// 文件夹名能否作为版本名（Windows 文件名规则）。返回错误说明。
pub fn validate_version_name(dir: &GameDir, name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Some("版本名称不能为空".into());
    }
    if trimmed != name {
        return Some("版本名称首尾不能有空格".into());
    }
    if name.ends_with('.') {
        return Some("版本名称不能以点结尾".into());
    }
    if let Some(c) = name.chars().find(|c| r#"\/:*?"<>|"#.contains(*c) || c.is_control()) {
        return Some(format!("版本名称不能包含字符 {c}"));
    }
    let upper = name.to_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL") || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit()) {
        return Some("这个名称是 Windows 保留名，换一个".into());
    }
    // 只有版本描述文件的才算已有版本；上次没装完留下的空文件夹可以继续用。
    let taken = fs::read_dir(dir.versions())
        .map(|rd| {
            rd.filter_map(Result::ok).any(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.eq_ignore_ascii_case(name) && e.path().join(format!("{n}.json")).is_file()
            })
        })
        .unwrap_or(false);
    if taken {
        return Some("不可与现有文件夹重名！".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let tmp = std::env::temp_dir().join("mc-core-name-test");
        let dir = GameDir::new(&tmp);
        assert!(validate_version_name(&dir, "1.20.1-Forge_47.4.8").is_none());
        assert!(validate_version_name(&dir, "a:b").is_some());
        assert!(validate_version_name(&dir, "CON").is_some());
        assert!(validate_version_name(&dir, "").is_some());
    }

    #[test]
    fn language_file() {
        let tmp = std::env::temp_dir().join(format!("mc-core-lang-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_language(&tmp, "zh_cn", false).unwrap();
        assert_eq!(fs::read_to_string(tmp.join("options.txt")).unwrap(), "lang:zh_cn\n");
        fs::write(tmp.join("options.txt"), "fov:0.5\nlang:en_us\n").unwrap();
        write_language(&tmp, "zh_cn", true).unwrap();
        assert_eq!(fs::read_to_string(tmp.join("options.txt")).unwrap(), "fov:0.5\nlang:zh_CN\n");
        let _ = fs::remove_dir_all(&tmp);
    }
}
