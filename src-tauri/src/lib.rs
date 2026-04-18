use std::sync::Mutex;
use process_wrap::std::{CommandWrap, ChildWrapper};
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
#[cfg(windows)]
use process_wrap::std::JobObject;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// アプリ状態：FastAPI サブプロセスを保持。
/// process-wrap により Unix はプロセスグループ、Windows は Job Object で
/// 孫プロセスも含めたプロセスツリーを一括 kill する。
pub struct BackendProcess {
    child: Option<Box<dyn ChildWrapper + Send>>,
    port: Option<u16>,
}

impl BackendProcess {
    fn none() -> Self {
        BackendProcess {
            child: None,
            port: None,
        }
    }
}

impl Drop for BackendProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

type BackendState = Mutex<BackendProcess>;

#[derive(Clone)]
pub struct BootstrapStateData {
    phase: String,
    message: String,
}

impl BootstrapStateData {
    fn new() -> Self {
        Self {
            phase: "initializing".into(),
            message: "Preparing Python environment...".into(),
        }
    }
}

type BootstrapState = Mutex<BootstrapStateData>;

#[derive(Serialize)]
struct BootstrapStatus {
    phase: String,
    message: String,
}

struct RuntimePaths {
    uv: std::path::PathBuf,
    python_dir: std::path::PathBuf,
    venv_dir: std::path::PathBuf,
}

fn uv_path(resource_dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    return resource_dir.join("uv.exe");
    #[cfg(not(target_os = "windows"))]
    return resource_dir.join("uv");
}

fn allocate_backend_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("空きポート確保に失敗: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("確保ポート取得に失敗: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn refresh_backend_state(backend: &mut BackendProcess) -> bool {
    if let Some(child) = backend.child.as_mut() {
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_status)) => {
                backend.child = None;
                backend.port = None;
                false
            }
            Err(e) => {
                eprintln!("[backend] try_wait failed: {e}");
                false
            }
        }
    } else {
        false
    }
}

fn set_bootstrap_status(app: &AppHandle, phase: &str, message: impl Into<String>) {
    let state_handle = app.state::<BootstrapState>();
    let mut state = state_handle.lock().unwrap();
    state.phase = phase.to_string();
    state.message = message.into();
}

fn resolve_runtime_paths(app: &AppHandle) -> Result<RuntimePaths, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("resource_dir 取得失敗: {e}"))?;

    let uv = uv_path(&resource_dir);
    if !uv.exists() {
        return Err(format!(
            "uv バイナリが見つかりません: {}\n\
             scripts/download-uv.ps1 (Windows) または scripts/download-uv.sh を実行してください。",
            uv.display()
        ));
    }

    let python_dir = resource_dir.join("python");
    if !python_dir.exists() {
        return Err(format!("python ディレクトリが見つかりません: {}", python_dir.display()));
    }

    let venv_dir = {
        #[cfg(debug_assertions)]
        {
            python_dir.join(".venv")
        }
        #[cfg(not(debug_assertions))]
        {
            app.path()
                .app_local_data_dir()
                .map_err(|e| format!("app_local_data_dir 取得失敗: {e}"))?
                .join(".venv")
        }
    };

    Ok(RuntimePaths {
        uv,
        python_dir,
        venv_dir,
    })
}

fn ensure_python_environment(app: &AppHandle) -> Result<(), String> {
    let paths = resolve_runtime_paths(app)?;

    println!("[backend] uv: {}", paths.uv.display());
    println!("[backend] python_dir: {}", paths.python_dir.display());
    println!("[backend] venv: {}", paths.venv_dir.display());

    let status = std::process::Command::new(&paths.uv)
        .env("UV_PROJECT_ENVIRONMENT", &paths.venv_dir)
        .args(["sync", "--locked"])
        .current_dir(&paths.python_dir)
        .status()
        .map_err(|e| format!("uv sync 実行失敗: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("uv sync が失敗しました (exit={status})"))
    }
}

/// FastAPI サーバーを起動する。
/// Unix: ProcessGroup::leader() でプロセスグループを作成し、kill 時に孫まで一括終了。
/// Windows: JobObject で同様のプロセスツリー kill を実現。
fn start_backend(app: &AppHandle) -> Result<BackendProcess, String> {
    let paths = resolve_runtime_paths(app)?;

    let port = allocate_backend_port()?;
    let port_arg = port.to_string();
    println!("[backend] selected port: {port}");

    let mut wrap = CommandWrap::with_new(&paths.uv, |cmd| {
        cmd.env("UV_PROJECT_ENVIRONMENT", &paths.venv_dir)
            .args([
                "run",
                "--no-sync",
                "uvicorn",
                "main:app",
                "--host",
                "127.0.0.1",
                "--port",
                &port_arg,
                "--no-access-log",
            ])
            .current_dir(&paths.python_dir);
    });

    #[cfg(unix)]
    wrap.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    wrap.wrap(JobObject);

    let child = wrap
        .spawn()
        .map_err(|e| format!("FastAPI 起動失敗: {e}"))?;

    println!("[backend] FastAPI started (pid={})", child.id());

    Ok(BackendProcess {
        child: Some(child),
        port: Some(port),
    })
}

#[tauri::command]
fn get_backend_bootstrap_status(state: tauri::State<BootstrapState>) -> BootstrapStatus {
    let current = state.lock().unwrap().clone();
    BootstrapStatus {
        phase: current.phase,
        message: current.message,
    }
}

#[tauri::command]
fn get_api_port(state: tauri::State<BackendState>) -> Option<u16> {
    let mut backend = state.lock().unwrap();
    if refresh_backend_state(&mut backend) {
        backend.port
    } else {
        None
    }
}

#[tauri::command]
fn is_backend_running(state: tauri::State<BackendState>) -> bool {
    let mut backend = state.lock().unwrap();
    refresh_backend_state(&mut backend)
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(BackendProcess::none()) as BackendState)
        .manage(Mutex::new(BootstrapStateData::new()) as BootstrapState)
        .setup(|app| {
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                set_bootstrap_status(&app_handle, "syncing", "Setting up Python dependencies (first launch may take time)...");
                match ensure_python_environment(&app_handle) {
                    Ok(_) => {
                        set_bootstrap_status(&app_handle, "starting", "Starting backend service...");
                        match start_backend(&app_handle) {
                            Ok(backend) => {
                                let running_port = backend.port;
                                *app_handle.state::<BackendState>().lock().unwrap() = backend;
                                if let Some(port) = running_port {
                                    println!("[backend] FastAPI is running on http://127.0.0.1:{port}");
                                }
                                set_bootstrap_status(&app_handle, "running", "Backend is ready.");
                            }
                            Err(e) => {
                                eprintln!("[backend] ERROR: {e}");
                                set_bootstrap_status(&app_handle, "failed", format!("Backend failed to start: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[backend] setup ERROR: {e}");
                        set_bootstrap_status(&app_handle, "failed", format!("Python setup failed: {e}"));
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port, is_backend_running, get_backend_bootstrap_status])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<BackendState>();
                let mut backend = state.lock().unwrap();
                if let Some(mut child) = backend.child.take() {
                    eprintln!("[backend] killing process group on exit...");
                    let _ = child.kill();
                }
            }
        });
}
