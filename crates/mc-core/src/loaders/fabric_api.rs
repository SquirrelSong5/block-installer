//! Fabric API：Fabric 的前置模组，PCL 在安装 Fabric 时也会一并提供。从 Modrinth 获取。

use anyhow::Result;
use serde_json::Value;

use super::LoaderVersion;
use crate::http;

const PROJECT: &str = "P7dR8mSH";

pub async fn list(client: &reqwest::Client, mc: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{PROJECT}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{mc}%22%5D"
    );
    let arr: Vec<Value> = http::get_json(client, &[url]).await?;
    let mut out: Vec<LoaderVersion> = arr
        .iter()
        .filter_map(|v| {
            let files = v.get("files")?.as_array()?;
            let file = files
                .iter()
                .find(|f| f.get("primary").and_then(Value::as_bool) == Some(true))
                .or_else(|| files.first())?;
            Some(LoaderVersion {
                id: file.get("filename")?.as_str()?.to_string(),
                version: v.get("version_number")?.as_str()?.to_string(),
                date: v.get("date_published").and_then(Value::as_str).map(str::to_string),
                stable: v.get("version_type").and_then(Value::as_str) == Some("release"),
                tag: None,
                note: None,
                url: file.get("url").and_then(Value::as_str).map(str::to_string),
                sha1: file.get("hashes").and_then(|h| h.get("sha1")).and_then(Value::as_str).map(str::to_string),
            })
        })
        .collect();
    if let Some(first) = out.first_mut() {
        first.tag = Some("最新".into());
    }
    Ok(out)
}
