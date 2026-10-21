//! 版本描述文件（versions/&lt;id&gt;/&lt;id&gt;.json）：规则判断、依赖库解析、继承合并。

use std::collections::HashSet;

use serde_json::{Map, Value};

/// 目标平台。产品只面向 Windows，但架构按本机判断。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86,
    X64,
    Arm64,
}

#[derive(Clone, Copy, Debug)]
pub struct Platform {
    pub os: &'static str,
    pub arch: Arch,
}

impl Platform {
    /// Windows + 本机架构。
    pub fn windows() -> Self {
        let arch = if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else if cfg!(target_arch = "x86") {
            Arch::X86
        } else {
            Arch::X64
        };
        Self { os: "windows", arch }
    }

    fn bits(&self) -> &'static str {
        match self.arch {
            Arch::X86 => "32",
            _ => "64",
        }
    }

    fn arch_matches(&self, rule_arch: &str) -> bool {
        match rule_arch {
            "x86" => self.arch == Arch::X86,
            "x86_64" | "amd64" | "x64" => self.arch == Arch::X64,
            "arm64" | "aarch64" => self.arch == Arch::Arm64,
            _ => true,
        }
    }
}

/// 规则判断（Mojang 语义：最后一条匹配的规则决定结果；没有规则即允许）。
pub fn rules_allow(rules: Option<&Value>, p: &Platform) -> bool {
    let Some(arr) = rules.and_then(Value::as_array) else { return true };
    if arr.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in arr {
        if rule_matches(r, p) {
            allowed = r.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    allowed
}

fn rule_matches(r: &Value, p: &Platform) -> bool {
    // 启动器特性（演示模式、自定义分辨率等）一律视为关闭。
    if let Some(features) = r.get("features").and_then(Value::as_object) {
        if features.values().any(|v| v.as_bool() != Some(false)) {
            return false;
        }
    }
    if let Some(os) = r.get("os") {
        if let Some(name) = os.get("name").and_then(Value::as_str) {
            if name != p.os {
                return false;
            }
        }
        if let Some(arch) = os.get("arch").and_then(Value::as_str) {
            if !p.arch_matches(arch) {
                return false;
            }
        }
    }
    true
}

/// `group:artifact:version[:classifier][@ext]` → 相对 libraries 目录的路径。
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = match coord.split_once('@') {
        Some((c, e)) => (c, e),
        None => (coord, "jar"),
    };
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    Some(format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}",
        group.replace('.', "/")
    ))
}

/// 去掉版本号的库标识，用于继承合并时去重：group:artifact[:classifier]。
fn library_key(name: &str) -> String {
    let name = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = name.split(':').collect();
    match parts.len() {
        0..=2 => name.to_string(),
        3 => format!("{}:{}", parts[0], parts[1]),
        _ => format!("{}:{}:{}", parts[0], parts[1], parts[3..].join(":")),
    }
}

/// 一个需要放到 libraries 目录下的文件。
#[derive(Clone, Debug)]
pub struct LibFile {
    pub path: String,
    /// 官方地址；为 None 时说明需要由安装器生成或从安装器里解压。
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

const DEFAULT_LIB_BASE: &str = "https://libraries.minecraft.net/";

/// 解析一个 library 条目在当前平台上需要的文件（主文件 + natives）。
pub fn library_files(lib: &Value, p: &Platform) -> Vec<LibFile> {
    let mut out = Vec::new();
    if !rules_allow(lib.get("rules"), p) {
        return out;
    }
    if lib.get("clientreq").and_then(Value::as_bool) == Some(false) {
        return out;
    }
    let Some(name) = lib.get("name").and_then(Value::as_str) else { return out };
    let downloads = lib.get("downloads");
    let natives = lib.get("natives").and_then(|n| n.get(p.os)).and_then(Value::as_str);

    if let Some(artifact) = downloads.and_then(|d| d.get("artifact")) {
        if let Some(f) = artifact_file(artifact, name) {
            out.push(f);
        }
    } else if downloads.is_none() && natives.is_none() {
        // 老式写法：只有 name 和可选的仓库地址（Fabric、旧版 Forge）。
        if let Some(path) = maven_path(name) {
            let base = lib.get("url").and_then(Value::as_str).unwrap_or(DEFAULT_LIB_BASE);
            let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
            out.push(LibFile {
                url: Some(format!("{base}{path}")),
                path,
                sha1: lib.get("sha1").and_then(Value::as_str).map(str::to_string),
                size: lib.get("size").and_then(Value::as_u64),
            });
        }
    }

    if let Some(classifier) = natives {
        let classifier = classifier.replace("${arch}", p.bits());
        let from_downloads = downloads
            .and_then(|d| d.get("classifiers"))
            .and_then(|c| c.get(&classifier))
            .and_then(|a| artifact_file(a, &format!("{name}:{classifier}")));
        if let Some(f) = from_downloads {
            out.push(f);
        } else if let Some(path) = maven_path(&format!("{name}:{classifier}")) {
            let base = lib.get("url").and_then(Value::as_str).unwrap_or(DEFAULT_LIB_BASE);
            let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
            out.push(LibFile { url: Some(format!("{base}{path}")), path, sha1: None, size: None });
        }
    }
    out
}

fn artifact_file(a: &Value, name: &str) -> Option<LibFile> {
    let path = a
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| maven_path(name))?;
    let url = a.get("url").and_then(Value::as_str).filter(|u| !u.is_empty()).map(str::to_string);
    Some(LibFile {
        path,
        url,
        sha1: a.get("sha1").and_then(Value::as_str).map(str::to_string),
        size: a.get("size").and_then(Value::as_u64),
    })
}

/// 把子版本（加载器）合并进父版本（原版），得到不依赖 inheritsFrom 的独立版本描述。
pub fn merge(parent: &Value, child: &Value) -> Value {
    let mut out = parent.as_object().cloned().unwrap_or_default();
    let Some(child_obj) = child.as_object() else { return Value::Object(out) };

    for (k, v) in child_obj {
        match k.as_str() {
            "inheritsFrom" | "jar" => {}
            "libraries" => {
                let child_libs = v.as_array().cloned().unwrap_or_default();
                let keys: HashSet<String> = child_libs
                    .iter()
                    .filter_map(|l| l.get("name").and_then(Value::as_str))
                    .map(library_key)
                    .collect();
                let mut libs = child_libs;
                if let Some(parent_libs) = out.get("libraries").and_then(Value::as_array) {
                    for l in parent_libs {
                        let keep = l
                            .get("name")
                            .and_then(Value::as_str)
                            .map(|n| !keys.contains(&library_key(n)))
                            .unwrap_or(true);
                        if keep {
                            libs.push(l.clone());
                        }
                    }
                }
                out.insert("libraries".into(), Value::Array(libs));
            }
            "arguments" => {
                let mut args = out.get("arguments").and_then(Value::as_object).cloned().unwrap_or_default();
                if let Some(child_args) = v.as_object() {
                    for (side, list) in child_args {
                        let mut merged = args.get(side).and_then(Value::as_array).cloned().unwrap_or_default();
                        merged.extend(list.as_array().cloned().unwrap_or_default());
                        args.insert(side.clone(), Value::Array(merged));
                    }
                }
                out.insert("arguments".into(), Value::Object(args));
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out.remove("inheritsFrom");
    out.remove("jar");
    Value::Object(out)
}

/// 给游戏参数追加一组参数，兼容新旧两种写法。
pub fn append_game_args(json: &mut Value, extra: &[&str]) {
    let Some(obj) = json.as_object_mut() else { return };
    if let Some(s) = obj.get("minecraftArguments").and_then(Value::as_str) {
        let joined = format!("{} {}", s, extra.join(" "));
        obj.insert("minecraftArguments".into(), Value::String(joined));
        return;
    }
    let args = obj.entry("arguments").or_insert_with(|| Value::Object(Map::new()));
    if let Some(args) = args.as_object_mut() {
        let game = args.entry("game").or_insert_with(|| Value::Array(vec![]));
        if let Some(arr) = game.as_array_mut() {
            arr.extend(extra.iter().map(|s| Value::String(s.to_string())));
        }
    }
}

/// 版本要求的 Java：(组件名, 主版本号)。老版本没写时按 Java 8 处理。
pub fn java_requirement(json: &Value) -> (String, u32) {
    let jv = json.get("javaVersion");
    let major = jv.and_then(|j| j.get("majorVersion")).and_then(Value::as_u64).unwrap_or(8) as u32;
    let component = jv
        .and_then(|j| j.get("component"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| component_for_major(major).to_string());
    (component, major)
}

pub fn component_for_major(major: u32) -> &'static str {
    match major {
        0..=8 => "jre-legacy",
        9..=16 => "java-runtime-alpha",
        17..=20 => "java-runtime-gamma",
        21..=24 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn win() -> Platform {
        Platform { os: "windows", arch: Arch::X64 }
    }

    #[test]
    fn maven_paths() {
        assert_eq!(maven_path("org.lwjgl:lwjgl:3.3.1").unwrap(), "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar");
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        );
    }

    #[test]
    fn rules() {
        let osx_only = json!([{"action":"allow","os":{"name":"osx"}}]);
        assert!(!rules_allow(Some(&osx_only), &win()));
        let not_osx = json!([{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]);
        assert!(rules_allow(Some(&not_osx), &win()));
        let x86 = json!([{"action":"allow","os":{"name":"windows","arch":"x86"}}]);
        assert!(!rules_allow(Some(&x86), &win()));
        let demo = json!([{"action":"allow","features":{"is_demo_user":true}}]);
        assert!(!rules_allow(Some(&demo), &win()));
    }

    #[test]
    fn old_natives() {
        let lib = json!({
            "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
            "natives": {"windows": "natives-windows-${arch}", "osx": "natives-osx"},
            "downloads": {"classifiers": {
                "natives-windows-64": {"path": "a/b-natives-windows-64.jar", "url": "https://libraries.minecraft.net/a/b-natives-windows-64.jar", "sha1": "x", "size": 1}
            }}
        });
        let files = library_files(&lib, &win());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a/b-natives-windows-64.jar");
    }

    #[test]
    fn fabric_style() {
        let lib = json!({"name": "net.fabricmc:fabric-loader:0.16.14", "url": "https://maven.fabricmc.net/"});
        let f = &library_files(&lib, &win())[0];
        assert_eq!(f.url.as_deref().unwrap(), "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar");
    }

    #[test]
    fn merge_child_wins() {
        let parent = json!({
            "id": "1.20.1", "mainClass": "net.minecraft.client.main.Main",
            "libraries": [{"name":"org.ow2.asm:asm:9.3"},{"name":"com.mojang:brigadier:1.1.8"}],
            "arguments": {"game": ["--username"], "jvm": ["-cp"]}
        });
        let child = json!({
            "id": "fabric", "inheritsFrom": "1.20.1", "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [{"name":"org.ow2.asm:asm:9.6"}],
            "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]}
        });
        let m = merge(&parent, &child);
        assert_eq!(m["mainClass"], "net.fabricmc.loader.impl.launch.knot.KnotClient");
        let libs: Vec<&str> = m["libraries"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
        assert_eq!(libs, vec!["org.ow2.asm:asm:9.6", "com.mojang:brigadier:1.1.8"]);
        assert_eq!(m["arguments"]["jvm"].as_array().unwrap().len(), 2);
        assert!(m.get("inheritsFrom").is_none());
    }
}
