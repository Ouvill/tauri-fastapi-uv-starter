use std::process::Child;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const FASTAPI_PORT: u16 = 8000;

/// アプリ状態：FastAPI サブプロセスを保持
/// Drop 時に kill するので、アプリ終了時に自動クリーンアップされる
pub struct BackendProcess(Option<Child>);

impl Drop for BackendProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
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

/// FastAPI サーバーを起動する
/// - uv run で依存関係を自動解決してから uvicorn を起動
/// - venv は app_local_data_dir に作成（インストール先が読み取り専用でも動作）
fn start_backend(app: &AppHandle) -> Result<Child, String> {
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

    // venv の保存先（リリースビルドではリソースが読み取り専用になりうるため
    // app_local_data_dir に置く）
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

    // uv run が依存関係のインストール＋uvicorn の起動を一括で行う
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
    Ok(child)
}

/// フロントエンドから API ポート番号を取得するコマンド
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
        .manage(Mutex::new(BackendProcess(None)) as BackendState)
        .setup(|app| {
            match start_backend(app.handle()) {
                Ok(child) => {
                    *app.state::<BackendState>().lock().unwrap() = BackendProcess(Some(child));
                    println!("[backend] FastAPI is running on http://127.0.0.1:{FASTAPI_PORT}");
                }
                Err(e) => {
                    eprintln!("[backend] ERROR: {e}");
                    // 起動失敗してもアプリは続行（API 呼び出し時にエラーになる）
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
