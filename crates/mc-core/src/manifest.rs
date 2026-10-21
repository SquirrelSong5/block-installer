//! Mojang 版本清单与分类（正式版 / 预览版 / 远古版 / 愚人节版）。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http;
use crate::source::{candidates, Source};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Release,
    Snapshot,
    Old,
    AprilFools,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    pub category: Category,
    /// 原始 type 字段：release / snapshot / old_beta / old_alpha。
    pub kind: String,
    pub url: String,
    pub sha1: Option<String>,
    pub release_time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionList {
    pub latest_release: Option<String>,
    pub latest_snapshot: Option<String>,
    /// 按发布时间从新到旧。
    pub versions: Vec<VersionEntry>,
}

impl VersionList {
    pub fn find(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

#[derive(Deserialize)]
struct RawManifest {
    latest: Option<RawLatest>,
    versions: Vec<RawVersion>,
}
#[derive(Deserialize)]
struct RawLatest {
    release: Option<String>,
    snapshot: Option<String>,
}
#[derive(Deserialize)]
struct RawVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    sha1: Option<String>,
    #[serde(rename = "releaseTime")]
    release_time: String,
}

const APRIL_FOOLS: &[&str] = &[
    "2point0_blue",
    "2point0_red",
    "2point0_purple",
    "15w14a",
    "1.rv-pre1",
    "3d shareware v1.34",
    "20w14infinite",
    "22w13oneblockatatime",
    "23w13a_or_b",
    "24w14potato",
    "25w14craftmine",
    "26w14a",
];

pub fn classify(id: &str, kind: &str, release_time: &str) -> Category {
    let lower = id.to_lowercase();
    if APRIL_FOOLS.contains(&lower.as_str()) {
        return Category::AprilFools;
    }
    match kind {
        "release" => Category::Release,
        "snapshot" | "pending" => {
            if is_april_first(release_time) {
                Category::AprilFools
            } else {
                Category::Snapshot
            }
        }
        "special" => Category::AprilFools,
        _ => Category::Old,
    }
}

/// 发布时间按 UTC+2 算落在 4 月 1 日（Mojang 的愚人节版本通常在欧洲时间当天发布）。
fn is_april_first(t: &str) -> bool {
    let num = |a: usize, b: usize| t.get(a..b).and_then(|s| s.parse::<u32>().ok());
    let (Some(month), Some(day), Some(hour)) = (num(5, 7), num(8, 10), num(11, 13)) else { return false };
    (month == 4 && day == 1 && hour < 22) || (month == 3 && day == 31 && hour >= 22)
}

pub async fn fetch(client: &reqwest::Client, source: Source) -> Result<VersionList> {
    let raw: RawManifest = http::get_json(client, &candidates(MANIFEST_URL, source)).await?;
    let mut versions: Vec<VersionEntry> = raw
        .versions
        .into_iter()
        .map(|v| VersionEntry {
            category: classify(&v.id, &v.kind, &v.release_time),
            id: v.id,
            kind: v.kind,
            url: v.url,
            sha1: v.sha1,
            release_time: v.release_time,
        })
        .collect();
    versions.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    let latest = raw.latest.unwrap_or(RawLatest { release: None, snapshot: None });
    Ok(VersionList { latest_release: latest.release, latest_snapshot: latest.snapshot, versions })
}

/// 下载某个版本的原版描述文件。
pub async fn fetch_version_json(client: &reqwest::Client, entry: &VersionEntry, source: Source) -> Result<Value> {
    let bytes = http::get_bytes(client, &candidates(&entry.url, source)).await?;
    if let Some(expect) = &entry.sha1 {
        let actual = http::sha1_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expect) {
            return Err(anyhow!("版本描述文件校验失败"));
        }
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(classify("1.20.1", "release", "2023-06-12T13:25:51+00:00"), Category::Release);
        assert_eq!(classify("24w14potato", "snapshot", "2024-04-01T12:00:00+00:00"), Category::AprilFools);
        assert_eq!(classify("24w14a", "snapshot", "2024-04-03T12:00:00+00:00"), Category::Snapshot);
        assert_eq!(classify("b1.7.3", "old_beta", "2011-07-07T22:00:00+00:00"), Category::Old);
        assert_eq!(classify("x", "snapshot", "2019-03-31T23:00:00+00:00"), Category::AprilFools);
    }
}
