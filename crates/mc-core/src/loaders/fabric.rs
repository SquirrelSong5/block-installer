//! Fabric 与 Quilt：两者都提供现成的版本描述文件，合并进原版即可。

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{LoaderKind, LoaderVersion};
use crate::http;
use crate::source::{candidates, Source};

fn meta_base(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Quilt => "https://meta.quiltmc.org/v3",
        _ => "https://meta.fabricmc.net/v2",
    }
}

pub async fn list(client: &reqwest::Client, kind: LoaderKind, mc: &str, source: Source) -> Result<Vec<LoaderVersion>> {
    let url = format!("{}/versions/loader/{}", meta_base(kind), mc);
    let arr: Vec<Value> = http::get_json(client, &candidates(&url, source)).await?;
    let mut out: Vec<LoaderVersion> = arr
        .iter()
        .filter_map(|e| {
            let loader = e.get("loader")?;
            let version = loader.get("version")?.as_str()?.to_string();
            let stable = match loader.get("stable").and_then(Value::as_bool) {
                Some(s) => s,
                None => !version.contains("beta") && !version.contains("pre") && !version.contains("rc"),
            };
            Some(LoaderVersion { id: version.clone(), version, date: None, stable, tag: None, note: None, url: None, sha1: None })
        })
        .collect();
    // Quilt 的接口不保证顺序。
    out.sort_by(|a, b| super::compare_versions(&b.version, &a.version));
    if let Some(first) = out.first_mut() {
        first.tag = Some("最新".into());
    }
    if let Some(stable) = out.iter_mut().find(|v| v.stable) {
        stable.tag = Some(if stable.tag.is_some() { "最新稳定版".into() } else { "推荐".into() });
    }
    Ok(out)
}

/// 下载加载器的版本描述文件（inheritsFrom 原版）。
pub async fn profile(client: &reqwest::Client, kind: LoaderKind, mc: &str, loader: &str, source: Source) -> Result<Value> {
    let url = format!("{}/versions/loader/{}/{}/profile/json", meta_base(kind), mc, loader);
    let v: Value = http::get_json(client, &candidates(&url, source)).await?;
    if v.get("mainClass").is_none() {
        return Err(anyhow!("{} 返回的版本描述不完整", kind.display()));
    }
    Ok(v)
}
