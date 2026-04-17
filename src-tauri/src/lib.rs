use std::process::Child;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const FASTAPI_PORT: u16 = 8000;

/// アプリ状態：FastAPI サブプロセスと（Windows では）Job Object を保持
pub struct BackendProcess {
    child: Option<Child>,
    /// ハンドルを保持し続けることで Drop 時に OS が KILL_ON_JOB_CLOSE を発火する
    #[cfg(windows)]
    _job: Option<win32job::Job>,
}

impl BackendProcess {
    fn none() -> Self {
        BackendProcess {
            child: None,
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

    let mut info = match job.query_extended_limit_info() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[backend] query_extended_limit_info failed: {e}");
            return None;
        }
    };
    info.limit_kill_on_job_close();
    if let Err(e) = job.set_extended_limit_info(&mut info) {
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

    let child = std::process::Command::new(&uv)
        .env("UV_PROJECT_ENVIRONMENT", &venv_dir)
        .args([
            "run",
            "uvicorn",
            "main:app",
            "--host",
            "127.0.0.1",
            "--port",
            &FASTAPI_PORT.to_string(),
            "--no-access-log",
        ])
        .current_dir(&python_dir)
        .spawn()
        .map_err(|e| format!("FastAPI 起動失敗: {e}"))?;

    println!("[backend] FastAPI started (pid={})", child.id());

    Ok(BackendProcess {
        child: Some(child),
        #[cfg(windows)]
        _job: attach_job_object(&child),
    })
}

#[tauri::command]
fn get_api_port() -> u16 {
    FASTAPI_PORT
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
                    *app.state::<BackendState>().lock().unwrap() = backend;
                    println!("[backend] FastAPI is running on http://127.0.0.1:{FASTAPI_PORT}");
                }
                Err(e) => {
                    eprintln!("[backend] ERROR: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
