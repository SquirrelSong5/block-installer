//! 桌面程序外壳：把安装核心包装成界面可以调用的命令，并管理设置与任务队列。

mod settings;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mc_core::gamedir::{self, DirInfo, GameDir};
use mc_core::install::{InstallOptions, InstallRequest};
use mc_core::installed::{self, InstalledVersion, VerifyReport};
use mc_core::java::{self, JavaInfo};
use mc_core::loaders::{fabric, fabric_api, forge, optifine, LoaderKind, LoaderVersion};
use mc_core::manifest::{self, VersionList};
use mc_core::task::{TaskHandle, TaskSnapshot, TaskStatus};
use settings::Settings;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

struct AppState {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    client: mc_core::reqwest::Client,
    tasks: Mutex<Vec<TaskHandle>>,
    next_id: AtomicU64,
    /// 安装任务排队执行，同一时间只跑一个。
    queue: Arc<tokio::sync::Mutex<()>>,
    manifest: Mutex<Option<VersionList>>,
    java: Mutex<Option<Vec<JavaInfo>>>,
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

// ---------- 设置 ----------

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    settings.save(&state.settings_path).map_err(err)?;
    let java_changed = state.settings.lock().unwrap().java_paths != settings.java_paths;
    *state.settings.lock().unwrap() = settings;
    if java_changed {
        *state.java.lock().unwrap() = None;
    }
    Ok(())
}

// ---------- 游戏目录 ----------

#[tauri::command]
async fn detect_game_dirs() -> Vec<DirInfo> {
    tauri::async_runtime::spawn_blocking(|| gamedir::detect_candidates(exe_dir().as_deref()))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn inspect_game_dir(path: String) -> DirInfo {
    gamedir::inspect(std::path::Path::new(path.trim()), "手动选择")
}

// ---------- Java ----------

async fn known_java(state: &AppState, force: bool) -> Vec<JavaInfo> {
    if !force {
        if let Some(list) = state.java.lock().unwrap().clone() {
            return list;
        }
    }
    let manual: Vec<String> = state.settings.lock().unwrap().java_paths.clone();
    let mut list = java::scan(&[]).await;
    for p in manual {
        if list.iter().any(|j| j.path.eq_ignore_ascii_case(&p)) {
            continue;
        }
        if let Ok(info) = java::probe(std::path::Path::new(&p)).await {
            list.push(info);
        }
    }
    list.sort_by(|a, b| b.major.cmp(&a.major));
    *state.java.lock().unwrap() = Some(list.clone());
    list
}

#[tauri::command]
async fn scan_java(state: State<'_, AppState>, force: bool) -> CmdResult<Vec<JavaInfo>> {
    Ok(known_java(&state, force).await)
}

/// 检查用户手动选的 java.exe（或它所在的 JAVA_HOME / bin 目录）。
#[tauri::command]
async fn probe_java(path: String) -> CmdResult<JavaInfo> {
    let mut p = PathBuf::from(path.trim().trim_matches('"'));
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    if p.is_dir() {
        if p.join(exe).is_file() {
            p = p.join(exe);
        } else if p.join("bin").join(exe).is_file() {
            p = p.join("bin").join(exe);
        }
    }
    if p.file_name().and_then(|n| n.to_str()).map(|n| n.eq_ignore_ascii_case("javaw.exe")).unwrap_or(false) {
        p = p.with_file_name("java.exe");
    }
    if !p.is_file() {
        return Err("找不到 java.exe，请选择 Java 安装目录里 bin 文件夹下的 java.exe".into());
    }
    java::probe(&p).await.map_err(err)
}

// ---------- 版本与加载器 ----------

#[tauri::command]
async fn get_version_list(state: State<'_, AppState>, force: bool) -> CmdResult<VersionList> {
    if !force {
        if let Some(m) = state.manifest.lock().unwrap().clone() {
            return Ok(m);
        }
    }
    let source = state.settings.lock().unwrap().source;
    let list = manifest::fetch(&state.client, source).await.map_err(err)?;
    *state.manifest.lock().unwrap() = Some(list.clone());
    Ok(list)
}

#[tauri::command]
async fn list_loader(state: State<'_, AppState>, kind: LoaderKind, mc: String) -> CmdResult<Vec<LoaderVersion>> {
    let source = state.settings.lock().unwrap().source;
    let c = &state.client;
    let r = match kind {
        LoaderKind::Forge => forge::list_forge(c, &mc, source).await,
        LoaderKind::NeoForge => forge::list_neoforge(c, &mc, source).await,
        LoaderKind::Fabric | LoaderKind::Quilt => fabric::list(c, kind, &mc, source).await,
        LoaderKind::OptiFine => optifine::list(c, &mc).await,
        LoaderKind::FabricApi => fabric_api::list(c, &mc).await,
    };
    r.map_err(err)
}

#[tauri::command]
fn validate_name(dir: String, name: String) -> Option<String> {
    gamedir::validate_version_name(&GameDir::new(dir), &name)
}

// ---------- 任务 ----------

fn new_task(state: &AppState, kind: &str, title: &str, name: &str) -> TaskHandle {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let t = TaskHandle::new(id, kind, title, name);
    state.tasks.lock().unwrap().push(t.clone());
    t
}

fn finish(task: &TaskHandle, r: anyhow::Result<()>) {
    match r {
        Ok(()) => task.set_status(TaskStatus::Done),
        Err(e) if task.is_cancelled() => {
            task.fail_running("已取消");
            task.log(format!("{e:#}"));
            task.set_status(TaskStatus::Cancelled);
        }
        Err(e) => {
            let msg = format!("{e:#}");
            task.log(format!("失败：{msg}"));
            task.fail_running(&msg);
            task.set_status(TaskStatus::Failed);
        }
    }
}

#[tauri::command]
async fn start_install(app: AppHandle, state: State<'_, AppState>, req: InstallRequest) -> CmdResult<u64> {
    if let Some(msg) = req.conflicts() {
        return Err(msg);
    }
    if let Some(msg) = gamedir::validate_version_name(&GameDir::new(&req.game_dir), &req.name) {
        return Err(msg);
    }
    if state.tasks.lock().unwrap().iter().any(|t| {
        let s = t.snapshot();
        s.version_name == req.name && matches!(s.status, TaskStatus::Queued | TaskStatus::Running)
    }) {
        return Err("这个版本名称已经在安装队列里了".into());
    }
    let task = new_task(&state, "install", &format!("Minecraft 安装：{}", req.name), &req.name);
    mc_core::install::plan_steps(&req, &task);
    let id = task.id();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _turn = state.queue.lock().await;
        if task.is_cancelled() {
            task.set_status(TaskStatus::Cancelled);
            return;
        }
        task.set_status(TaskStatus::Running);
        let (source, concurrency) = {
            let s = state.settings.lock().unwrap();
            (s.source, s.concurrency)
        };
        let opts = InstallOptions {
            source,
            concurrency,
            known_java: known_java(&state, false).await,
            manifest: state.manifest.lock().unwrap().clone(),
        };
        let r = mc_core::install::install(&req, &opts, &task).await;
        let downloaded_java = r.is_ok() && task.snapshot().steps.iter().any(|s| s.key == "java" && s.detail.starts_with("已下载"));
        finish(&task, r);
        if downloaded_java {
            *state.java.lock().unwrap() = None;
        }
    });
    Ok(id)
}

#[tauri::command]
async fn start_repair(app: AppHandle, state: State<'_, AppState>, dir: String, name: String) -> CmdResult<u64> {
    let task = new_task(&state, "repair", &format!("补全文件：{name}"), &name);
    let id = task.id();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _turn = state.queue.lock().await;
        task.set_status(TaskStatus::Running);
        let (source, concurrency) = {
            let s = state.settings.lock().unwrap();
            (s.source, s.concurrency)
        };
        let r = installed::repair(&GameDir::new(&dir), &name, source, concurrency, &task).await;
        finish(&task, r);
    });
    Ok(id)
}

#[tauri::command]
fn get_tasks(state: State<AppState>) -> Vec<TaskSnapshot> {
    state.tasks.lock().unwrap().iter().map(TaskHandle::snapshot).collect()
}

#[tauri::command]
fn cancel_task(state: State<AppState>, id: u64) {
    if let Some(t) = state.tasks.lock().unwrap().iter().find(|t| t.id() == id) {
        t.request_cancel();
    }
}

/// 从列表里移除已结束的任务。
#[tauri::command]
fn dismiss_task(state: State<AppState>, id: u64) {
    state.tasks.lock().unwrap().retain(|t| {
        t.id() != id || matches!(t.snapshot().status, TaskStatus::Queued | TaskStatus::Running)
    });
}

// ---------- 已安装 ----------

#[tauri::command]
async fn list_installed(dir: String) -> Vec<InstalledVersion> {
    tauri::async_runtime::spawn_blocking(move || installed::list(&GameDir::new(dir))).await.unwrap_or_default()
}

#[tauri::command]
async fn verify_version(state: State<'_, AppState>, dir: String, name: String) -> CmdResult<VerifyReport> {
    let source = state.settings.lock().unwrap().source;
    installed::verify(&GameDir::new(dir), &name, false, source).await.map(|(r, _)| r).map_err(err)
}

#[tauri::command]
fn delete_version(dir: String, name: String) -> CmdResult<()> {
    installed::delete(&GameDir::new(dir), &name).map_err(err)
}

/// 在资源管理器里打开文件夹（不存在时打开最近的上级目录）。
#[tauri::command]
fn open_folder(path: String) -> CmdResult<()> {
    let mut p = PathBuf::from(&path);
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => return Err("文件夹不存在".into()),
        }
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(&p).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn run_dir_of(dir: String, name: String, isolated: bool) -> String {
    GameDir::new(dir).run_dir(&name, isolated).to_string_lossy().into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);
            app.manage(AppState {
                settings: Mutex::new(settings),
                settings_path,
                client: mc_core::http::client(),
                tasks: Mutex::new(vec![]),
                next_id: AtomicU64::new(1),
                queue: Arc::new(tokio::sync::Mutex::new(())),
                manifest: Mutex::new(None),
                java: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            detect_game_dirs,
            inspect_game_dir,
            scan_java,
            probe_java,
            get_version_list,
            list_loader,
            validate_name,
            start_install,
            start_repair,
            get_tasks,
            cancel_task,
            dismiss_task,
            list_installed,
            verify_version,
            delete_version,
            open_folder,
            run_dir_of,
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
