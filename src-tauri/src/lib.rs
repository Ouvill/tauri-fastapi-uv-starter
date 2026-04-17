use std::process::Child;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// アプリ状態：FastAPI サブプロセスと（Windows では）Job Object を保持
pub struct BackendProcess {
    child: Option<Child>,
    port: Option<u16>,
    /// ハンドルを保持し続けることで Drop 時に OS が KILL_ON_JOB_CLOSE を発火する
    #[cfg(windows)]
    _job: Option<win32job::Job>,
}

impl BackendProcess {
    fn none() -> Self {
        BackendProcess {
            child: None,
            port: None,
            #[cfg(windows)]
            _job: None,
        }
    }
}

impl Drop for BackendProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.port = None;
        // Windows: ここで _job が Drop → OS がプロセスツリー全体を kill
    }
}

pub type BackendState = Mutex<BackendProcess>;

/// uv バイナリのパスを返す（プラットフォーム別）
fn uv_path(resource_dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    return resource_dir.join("uv.exe");
    #[cfg(not(target_os = "windows"))]
    return resource_dir.join("uv");
}

/// 127.0.0.1 上の空きポートを動的に確保する。
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

/// Windows Job Object を作成して子プロセスをアタッチする。
/// KILL_ON_JOB_CLOSE を設定することで、Tauri プロセスが終了すると
/// OS がジョブ内の全プロセスを自動 kill する。
#[cfg(windows)]
fn attach_job_object(child: &Child) -> Option<win32job::Job> {
    use std::os::windows::io::AsRawHandle;

    let job = match win32job::Job::create() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[backend] Job::create failed: {e}");
            return None;
        }
    };

    let info = match job.query_extended_limit_info() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[backend] query_extended_limit_info failed: {e}");
            return None;
        }
    };
    let mut info = info;
    info.limit_kill_on_job_close();
    if let Err(e) = job.set_extended_limit_info(&info) {
        eprintln!("[backend] set_extended_limit_info failed: {e}");
        return None;
    }

    let raw = child.as_raw_handle() as isize;
    if let Err(e) = job.assign_process(raw) {
        eprintln!("[backend] assign_process failed: {e}");
        return None;
    }

    println!("[backend] Job Object attached (KILL_ON_JOB_CLOSE, pid={})", child.id());
    Some(job)
}

/// FastAPI サーバーを起動する
fn start_backend(app: &AppHandle) -> Result<BackendProcess, String> {
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

    println!("[backend] uv: {}", uv.display());
    println!("[backend] python_dir: {}", python_dir.display());
    println!("[backend] venv: {}", venv_dir.display());

    let port = allocate_backend_port()?;
    let port_arg = port.to_string();
    println!("[backend] selected port: {port}");

    let child = std::process::Command::new(&uv)
        .env("UV_PROJECT_ENVIRONMENT", &venv_dir)
        .args([
            "run",
            "uvicorn",
            "main:app",
            "--host",
            "127.0.0.1",
            "--port",
            &port_arg,
            "--no-access-log",
        ])
        .current_dir(&python_dir)
        .spawn()
        .map_err(|e| format!("FastAPI 起動失敗: {e}"))?;

    println!("[backend] FastAPI started (pid={})", child.id());

    #[cfg(windows)]
    let job = attach_job_object(&child);

    Ok(BackendProcess {
        child: Some(child),
        port: Some(port),
        #[cfg(windows)]
        _job: job,
    })
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
        .setup(|app| {
            match start_backend(app.handle()) {
                Ok(backend) => {
                    let running_port = backend.port;
                    *app.state::<BackendState>().lock().unwrap() = backend;
                    if let Some(port) = running_port {
                        println!("[backend] FastAPI is running on http://127.0.0.1:{port}");
                    }
                }
                Err(e) => {
                    eprintln!("[backend] ERROR: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port, is_backend_running])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
