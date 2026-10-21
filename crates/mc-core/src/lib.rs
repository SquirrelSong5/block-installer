//! Minecraft 客户端安装核心。
//!
//! 只负责“把游戏装好”：读取版本元数据、下载本体/依赖库/资源、安装加载器、准备 Java，
//! 并按 PCL / 官方启动器能识别的结构写入 `.minecraft`。不包含启动游戏和 Mod 管理。

pub mod download;
pub mod gamedir;
pub mod http;
pub mod install;
pub mod installed;
pub mod java;
pub mod loaders;
pub mod manifest;
pub mod source;
pub mod task;
pub mod version;

pub use reqwest;
pub use source::Source;
