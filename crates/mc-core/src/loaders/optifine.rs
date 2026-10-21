//! OptiFine：官网没有直链，列表和下载都走 BMCLAPI。
//!
//! 和 Forge 同装时作为模组放进 mods；单独安装时按 launchwrapper 方式写进版本描述。

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::{zip_has, zip_read, LoaderVersion};
use crate::gamedir::GameDir;
use crate::http;
use crate::source::BMCLAPI;
use crate::version::maven_path;

pub async fn list(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let arr: Vec<Value> = http::get_json(client, &[format!("{BMCLAPI}/optifine/{mc}")]).await?;
    let mut out: Vec<LoaderVersion> = arr
        .iter()
        .filter_map(|e| {
            let kind = e.get("type")?.as_str()?;
            let patch = e.get("patch")?.as_str()?;
            let preview = patch.starts_with("pre") || e.get("filename").and_then(Value::as_str).map(|f| f.starts_with("preview")).unwrap_or(false);
            let version = if preview { format!("{kind}_{patch}") } else { format!("{kind}_{patch}") };
            let forge = e.get("forge").and_then(Value::as_str).unwrap_or("");
            let note = if forge.is_empty() || forge.contains("N/A") {
                Some("不兼容 Forge".to_string())
            } else {
                Some(format!("兼容 {forge}"))
            };
            Some(LoaderVersion {
                id: format!("{kind}/{patch}"),
                version,
                date: None,
                stable: !preview,
                tag: None,
                note,
                url: Some(format!("{BMCLAPI}/optifine/{mc}/{kind}/{patch}")),
                sha1: None,
            })
        })
        .collect();
    out.sort_by(|a, b| b.stable.cmp(&a.stable).then_with(|| b.version.cmp(&a.version)));
    if let Some(first) = out.iter_mut().find(|v| v.stable) {
        first.tag = Some("推荐".into());
    }
    Ok(out)
}

/// OptiFine 要求的 Forge 版本（“Forge 47.2.18” → "47.2.18"），用于兼容提示。
pub fn required_forge(note: &str) -> Option<String> {
    note.split_whitespace().last().filter(|s| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)).map(str::to_string)
}

/// 单独安装（不装 Forge）时需要放进 libraries 的内容，以及要写进版本描述的修改。
pub struct VanillaPlan {
    /// 最终的 OptiFine 库坐标与文件。
    pub optifine_coord: String,
    /// 需要运行 Patcher 生成库文件时为 true。
    pub needs_patch: bool,
    /// launchwrapper 的库坐标；`bundled_entry` 为 Some 时从 OptiFine 包里解压，否则从 Mojang 下载。
    pub wrapper_coord: String,
    pub wrapper_bundled_entry: Option<String>,
}

pub fn plan_vanilla(jar: &Path, mc: &str, kind_patch: &str) -> Result<VanillaPlan> {
    let (kind, patch) = kind_patch.split_once('/').ok_or_else(|| anyhow!("OptiFine 版本标识不对：{kind_patch}"))?;
    let optifine_coord = format!("optifine:OptiFine:{mc}_{kind}_{patch}");
    let needs_patch = zip_has(jar, "optifine/Patcher.class");
    let (wrapper_coord, wrapper_bundled_entry) = if zip_has(jar, "launchwrapper-2.0.jar") {
        ("optifine:launchwrapper:2.0".to_string(), Some("launchwrapper-2.0.jar".to_string()))
    } else if let Some(txt) = zip_read(jar, "launchwrapper-of.txt")? {
        let v = String::from_utf8_lossy(&txt).trim().to_string();
        (format!("optifine:launchwrapper-of:{v}"), Some(format!("launchwrapper-of-{v}.jar")))
    } else {
        ("net.minecraft:launchwrapper:1.12".to_string(), None)
    };
    Ok(VanillaPlan { optifine_coord, needs_patch, wrapper_coord, wrapper_bundled_entry })
}

pub fn lib_dest(dir: &GameDir, coord: &str) -> Result<std::path::PathBuf> {
    Ok(dir.library(&maven_path(coord).ok_or_else(|| anyhow!("无法解析 {coord}"))?))
}

/// 修改版本描述：OptiFine 和 launchwrapper 放在依赖库最前面，主类换成 launchwrapper。
pub fn apply_to_json(json: &mut Value, plan: &VanillaPlan) {
    let mut libs = vec![json!({ "name": plan.optifine_coord }), json!({ "name": plan.wrapper_coord })];
    if plan.wrapper_bundled_entry.is_none() {
        libs[1]["url"] = json!("https://libraries.minecraft.net/");
    }
    if let Some(existing) = json.get("libraries").and_then(Value::as_array) {
        libs.extend(existing.iter().cloned());
    }
    json["libraries"] = Value::Array(libs);
    json["mainClass"] = json!("net.minecraft.launchwrapper.Launch");
    crate::version::append_game_args(json, &["--tweakClass", "optifine.OptiFineTweaker"]);
}

/// 和 Forge 同装时放进 mods 的文件名。
pub fn mod_file_name(mc: &str, kind_patch: &str) -> String {
    let (kind, patch) = kind_patch.split_once('/').unwrap_or((kind_patch, ""));
    if patch.starts_with("pre") {
        format!("preview_OptiFine_{mc}_{kind}_{patch}.jar")
    } else {
        format!("OptiFine_{mc}_{kind}_{patch}.jar")
    }
}
