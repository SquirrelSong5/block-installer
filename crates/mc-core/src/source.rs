//! 下载源：BMCLAPI 镜像与官方源之间的地址换算。

use serde::{Deserialize, Serialize};

pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// BMCLAPI 优先，失败后回退官方源。
    #[default]
    Bmclapi,
    /// 只用官方源。
    Official,
}

/// 官方地址前缀 → BMCLAPI 地址前缀。
const MIRROR_RULES: &[(&str, &str)] = &[
    ("https://piston-meta.mojang.com", ""),
    ("https://launchermeta.mojang.com", ""),
    ("https://piston-data.mojang.com", ""),
    ("https://launcher.mojang.com", ""),
    ("https://resources.download.minecraft.net", "/assets"),
    ("https://libraries.minecraft.net", "/maven"),
    ("https://maven.minecraftforge.net", "/maven"),
    ("https://files.minecraftforge.net/maven", "/maven"),
    ("https://maven.neoforged.net/releases", "/maven"),
    ("https://maven.fabricmc.net", "/maven"),
    ("https://meta.fabricmc.net", "/fabric-meta"),
];

/// 把 http:// 统一成 https://，并修正老版本 Forge 里的旧域名。
pub fn normalize(url: &str) -> String {
    let mut u = url.trim().to_string();
    if let Some(rest) = u.strip_prefix("http://") {
        u = format!("https://{rest}");
    }
    if let Some(rest) = u.strip_prefix("https://files.minecraftforge.net/maven/") {
        u = format!("https://maven.minecraftforge.net/{rest}");
    }
    u
}

/// 官方地址对应的 BMCLAPI 地址；没有镜像时返回 None。
pub fn mirror_of(url: &str) -> Option<String> {
    let u = normalize(url);
    MIRROR_RULES.iter().find_map(|(official, mirror)| {
        u.strip_prefix(official)
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            .map(|rest| format!("{BMCLAPI}{mirror}{rest}"))
    })
}

/// 按下载源设置给出一个官方地址的候选下载地址列表（按尝试顺序）。
pub fn candidates(url: &str, source: Source) -> Vec<String> {
    let official = normalize(url);
    match (source, mirror_of(&official)) {
        (Source::Bmclapi, Some(m)) => vec![m, official],
        _ => vec![official],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_hosts() {
        assert_eq!(
            mirror_of("https://libraries.minecraft.net/org/lwjgl/lwjgl.jar").unwrap(),
            "https://bmclapi2.bangbang93.com/maven/org/lwjgl/lwjgl.jar"
        );
        assert_eq!(
            mirror_of("http://files.minecraftforge.net/maven/net/minecraftforge/forge/x.jar").unwrap(),
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/x.jar"
        );
        assert_eq!(
            mirror_of("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").unwrap(),
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        );
        assert!(mirror_of("https://maven.quiltmc.org/repository/release/x.jar").is_none());
        assert!(mirror_of("https://libraries.minecraft.netx/a").is_none());
    }

    #[test]
    fn official_only_has_single_candidate() {
        let c = candidates("https://libraries.minecraft.net/a.jar", Source::Official);
        assert_eq!(c, vec!["https://libraries.minecraft.net/a.jar".to_string()]);
        let c = candidates("https://libraries.minecraft.net/a.jar", Source::Bmclapi);
        assert_eq!(c.len(), 2);
    }
}
