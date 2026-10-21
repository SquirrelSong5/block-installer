//! 开发用命令行：不开界面直接测试安装核心。
//!
//! mcinst install --dir <.minecraft> --mc 1.20.1 [--name N] [--forge ID|--neoforge ID|--fabric V|--quilt V]
//!                [--optifine TYPE/PATCH] [--java PATH] [--no-auto-java] [--official] [--isolated]
//! mcinst loaders <forge|neoforge|fabric|quilt|optifine|fabricapi> <mc>
//! mcinst java
//! mcinst dirs
//! mcinst installed --dir <.minecraft>
//! mcinst verify --dir <.minecraft> --name N

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use mc_core::install::{InstallOptions, InstallRequest};
use mc_core::loaders::{fabric, fabric_api, forge, optifine, LoaderKind};
use mc_core::task::{TaskHandle, TaskStatus};
use mc_core::{gamedir, installed, java, Source};

fn parse_flags(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(k) = args[i].strip_prefix("--") {
            let v = args.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
            if v.is_some() {
                i += 1;
            }
            m.insert(k.to_string(), v.unwrap_or_else(|| "true".into()));
        }
        i += 1;
    }
    m
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let flags = parse_flags(&args[1.min(args.len())..]);
    let source = if flags.contains_key("official") { Source::Official } else { Source::Bmclapi };
    let client = mc_core::http::client();

    match cmd.as_str() {
        "loaders" => {
            let kind = args.get(1).ok_or_else(|| anyhow!("缺少加载器类型"))?;
            let mc = args.get(2).ok_or_else(|| anyhow!("缺少 MC 版本"))?;
            let list = match kind.as_str() {
                "forge" => forge::list_forge(&client, mc, source).await?,
                "neoforge" => forge::list_neoforge(&client, mc, source).await?,
                "fabric" => fabric::list(&client, LoaderKind::Fabric, mc, source).await?,
                "quilt" => fabric::list(&client, LoaderKind::Quilt, mc, source).await?,
                "optifine" => optifine::list(&client, mc).await?,
                "fabricapi" => fabric_api::list(&client, mc).await?,
                _ => return Err(anyhow!("未知类型 {kind}")),
            };
            println!("共 {} 个", list.len());
            for v in list.iter().take(12) {
                println!(
                    "  {:<28} {:<14} {:<10} {} {}",
                    v.id,
                    v.version,
                    v.tag.clone().unwrap_or_default(),
                    v.date.clone().unwrap_or_default(),
                    v.note.clone().unwrap_or_default()
                );
            }
        }
        "java" => {
            for j in java::scan(&[]).await {
                println!("Java {:<3} {:<12} 64位={} {} [{}]", j.major, j.version, j.is_64bit, j.path, j.source);
            }
        }
        "dirs" => {
            let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
            for d in gamedir::detect_candidates(exe_dir.as_deref()) {
                println!("{} | {} | {}", d.path, d.source, d.message);
            }
        }
        "installed" => {
            let dir = gamedir::GameDir::new(flags.get("dir").ok_or_else(|| anyhow!("缺少 --dir"))?);
            for v in installed::list(&dir) {
                println!("{:<40} mc={:<10} {:?} 隔离={} {:?}", v.name, v.mc_version, v.loaders, v.isolated, v.problem);
            }
        }
        "verify" => {
            let dir = gamedir::GameDir::new(flags.get("dir").ok_or_else(|| anyhow!("缺少 --dir"))?);
            let name = flags.get("name").ok_or_else(|| anyhow!("缺少 --name"))?;
            let (r, _) = installed::verify(&dir, name, flags.contains_key("deep"), source).await?;
            println!("{r:?}");
        }
        "install" => {
            let mc = flags.get("mc").cloned().ok_or_else(|| anyhow!("缺少 --mc"))?;
            let req = InstallRequest {
                game_dir: flags.get("dir").cloned().ok_or_else(|| anyhow!("缺少 --dir"))?,
                name: flags.get("name").cloned().unwrap_or_else(|| mc.clone()),
                mc_version: mc,
                forge: flags.get("forge").cloned(),
                neoforge: flags.get("neoforge").cloned(),
                fabric: flags.get("fabric").cloned(),
                fabric_api: None,
                quilt: flags.get("quilt").cloned(),
                optifine: flags.get("optifine").cloned(),
                isolated: flags.contains_key("isolated"),
                language: Some("zh_cn".into()),
                java_path: flags.get("java").cloned(),
                auto_java: !flags.contains_key("no-auto-java"),
            };
            let opts = InstallOptions {
                source,
                concurrency: flags.get("threads").and_then(|t| t.parse().ok()).unwrap_or(48),
                known_java: java::scan(&[]).await,
                manifest: None,
            };
            let task = TaskHandle::new(1, "install", &req.name, &req.name);
            task.set_status(TaskStatus::Running);
            let t2 = task.clone();
            let ticker = tokio::spawn(async move {
                let mut printed = 0;
                loop {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    let s = t2.snapshot();
                    for l in s.logs.iter().skip(printed) {
                        println!("    | {l}");
                    }
                    printed = s.logs.len();
                    let steps: Vec<String> = s
                        .steps
                        .iter()
                        .map(|st| format!("{}:{:?}:{:.0}%", st.key, st.state, st.progress * 100.0))
                        .collect();
                    println!("[{:>5.1}%] {:>8.1} KB/s 剩余{} | {}", s.progress * 100.0, s.speed / 1024.0, s.remaining_files, steps.join(" "));
                }
            });
            let started = std::time::Instant::now();
            let r = mc_core::install::install(&req, &opts, &task).await;
            ticker.abort();
            let s = task.snapshot();
            for st in &s.steps {
                println!("  {:<10} {:?} {}", st.title, st.state, st.detail);
            }
            match r {
                Ok(()) => println!("安装完成，用时 {:.1}s", started.elapsed().as_secs_f64()),
                Err(e) => {
                    for l in s.logs.iter().rev().take(15).rev() {
                        println!("    | {l}");
                    }
                    println!("安装失败：{e:#}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("用法见源码顶部注释");
        }
    }
    Ok(())
}
