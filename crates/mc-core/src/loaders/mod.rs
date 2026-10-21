//! 加载器：版本列表与安装。

pub mod fabric;
pub mod fabric_api;
pub mod forge;
pub mod optifine;

use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoaderKind {
    Forge,
    NeoForge,
    Fabric,
    FabricApi,
    Quilt,
    OptiFine,
}

impl LoaderKind {
    pub fn display(&self) -> &'static str {
        match self {
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::Fabric => "Fabric",
            LoaderKind::FabricApi => "Fabric API",
            LoaderKind::Quilt => "Quilt",
            LoaderKind::OptiFine => "OptiFine",
        }
    }
}

/// 列表里的一个加载器版本。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    /// 安装时用的标识（Forge 是完整版本号，OptiFine 是 type/patch 等）。
    pub id: String,
    /// 给用户看的版本号。
    pub version: String,
    /// 发布时间（ISO 8601），没有就为空。
    pub date: Option<String>,
    pub stable: bool,
    /// “推荐” / “最新”。
    pub tag: Option<String>,
    /// 补充说明，例如 OptiFine 兼容的 Forge 版本。
    pub note: Option<String>,
    /// 安装包下载地址（部分加载器需要）。
    pub url: Option<String>,
    pub sha1: Option<String>,
}

/// 按数字逐段比较版本号（"47.4.10" > "47.4.8"）。
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let seg = |s: &str| -> Vec<(u64, String)> {
        s.split(|c: char| c == '.' || c == '-' || c == '_' || c == '+')
            .map(|p| {
                let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                (digits.parse().unwrap_or(0), p[digits.len()..].to_string())
            })
            .collect()
    };
    let (x, y) = (seg(a), seg(b));
    for i in 0..x.len().max(y.len()) {
        match (x.get(i), y.get(i)) {
            (Some(p), Some(q)) => {
                let o = p.0.cmp(&q.0).then_with(|| match (p.1.is_empty(), q.1.is_empty()) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => p.1.cmp(&q.1),
                });
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            // 多出来的一段是 beta / pre / rc 这类字母后缀时，没有后缀的更新。
            (Some(p), None) => {
                return if is_prerelease(p) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }
            (None, Some(q)) => {
                return if is_prerelease(q) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less };
            }
            (None, None) => break,
        }
    }
    std::cmp::Ordering::Equal
}

fn is_prerelease(seg: &(u64, String)) -> bool {
    seg.0 == 0 && seg.1.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
}

pub fn zip_read(zip_path: &Path, entry: &str) -> Result<Option<Vec<u8>>> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| anyhow!("无法打开 {}：{e}", zip_path.display()))?;
    let result = match z.by_name(entry) {
        Ok(mut file) => {
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf)?;
            Some(buf)
        }
        Err(_) => None,
    };
    Ok(result)
}

pub fn zip_has(zip_path: &Path, entry: &str) -> bool {
    std::fs::File::open(zip_path)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .map(|mut z| z.by_name(entry).is_ok())
        .unwrap_or(false)
}

pub fn zip_extract(zip_path: &Path, entry: &str, dest: &Path) -> Result<bool> {
    match zip_read(zip_path, entry)? {
        Some(data) => {
            if let Some(dir) = dest.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(dest, data)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn version_order() {
        assert_eq!(compare_versions("47.4.10", "47.4.8"), Ordering::Greater);
        assert_eq!(compare_versions("21.1.77", "21.1.77-beta"), Ordering::Greater);
        assert_eq!(compare_versions("0.16.14", "0.17.2"), Ordering::Less);
        assert_eq!(compare_versions("10.13.4.1614", "10.13.4.1614"), Ordering::Equal);
    }
}
