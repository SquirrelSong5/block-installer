//! 用户设置，保存在应用配置目录的 settings.json。

use std::path::Path;

use mc_core::Source;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 首次引导是否已完成。
    pub onboarded: bool,
    /// 用户添加过的游戏目录。
    pub game_dirs: Vec<String>,
    /// 当前使用的游戏目录。
    pub current_dir: Option<String>,
    pub source: Source,
    /// 同时下载的文件数。
    pub concurrency: usize,
    /// 新安装的版本默认开启版本隔离。
    pub isolate: bool,
    /// 写入 options.txt 的语言，空字符串表示不修改。
    pub language: String,
    /// 找不到合适的 Java 时自动下载官方运行时。
    pub auto_java: bool,
    /// 用户手动添加的 java.exe。
    pub java_paths: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            game_dirs: vec![],
            current_dir: None,
            source: Source::Bmclapi,
            concurrency: 48,
            isolate: true,
            language: "zh_cn".into(),
            auto_java: true,
            java_paths: vec![],
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
