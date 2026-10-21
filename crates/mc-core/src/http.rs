//! HTTP 客户端与带候选地址重试的请求封装。

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;

pub const USER_AGENT: &str = concat!("BlockInstaller/", env!("CARGO_PKG_VERSION"));

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(64)
        .build()
        .expect("初始化 HTTP 客户端失败")
}

/// 依次尝试候选地址，每个地址最多试两次，返回第一个成功的响应体。
pub async fn get_bytes(client: &reqwest::Client, urls: &[String]) -> Result<Vec<u8>> {
    let mut last_err = anyhow!("没有可用的下载地址");
    for url in urls {
        for attempt in 0..2 {
            match fetch(client, url).await {
                Ok(b) => return Ok(b),
                Err(e) => {
                    last_err = e.context(format!("请求失败：{url}"));
                    if attempt == 0 {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                    }
                }
            }
        }
    }
    Err(last_err)
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("HTTP {status}"));
    }
    Ok(resp.bytes().await?.to_vec())
}

pub async fn get_json<T: DeserializeOwned>(client: &reqwest::Client, urls: &[String]) -> Result<T> {
    let bytes = get_bytes(client, urls).await?;
    serde_json::from_slice(&bytes).with_context(|| format!("解析 JSON 失败：{}", urls.first().map(String::as_str).unwrap_or("")))
}

pub fn sha1_hex(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    hex::encode(Sha1::digest(data))
}
