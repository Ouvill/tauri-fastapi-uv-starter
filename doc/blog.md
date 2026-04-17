# Tauri アプリに FastAPI バックエンドを組み込む — uv を同梱してゼロセットアップを実現する

Tauri でデスクトップアプリを作っていると、「Python のライブラリをどうしても使いたい」場面が出てくる。機械学習モデルの推論、pandas によるデータ処理、あるいは既存の Python 資産の再利用など。

この記事では **FastAPI を Tauri のバックエンドとして動かし**、Python ランタイムの依存関係解決ツール **uv をアプリに同梱**することで、ユーザーが Python や pip を別途インストールしなくても動くデスクトップアプリを作る方法を解説する。

## 全体アーキテクチャ

```
┌─────────────────────────────────────────┐
│  Tauri アプリ                            │
│                                         │
│  ┌──────────────┐   fetch    ┌────────┐ │
│  │  フロントエンド │ ────────► │FastAPI │ │
│  │  (React/Vite) │ ◄──────── │ :random│ │
│  └──────────────┘           └────────┘ │
│                                  ▲      │
│  ┌──────────────┐          spawn │      │
│  │  Rust (lib.rs)│ ─────────────┘      │
│  │  uv sync --locked で初回セットアップ │
│  └──────────────┘                       │
│         ▲ バンドルに含まれる              │
│   ┌─────┴──────┐                        │
│   │ uv.exe     │  ← uv が venv を管理   │
│   │ python/    │  ← FastAPI ソースコード │
│   └────────────┘                        │
└─────────────────────────────────────────┘
```

Rust 側が起動時に `uv sync --locked` を実行して環境を準備し、準備完了後に `uv run --no-sync uvicorn ...` を子プロセスとして起動する。ポートは `127.0.0.1:0` で空きポートを確保して使う。フロントエンドは Tauri コマンド経由で実ポートを取得して `fetch` する。

## なぜ uv なのか

Python の依存関係解決ツールには pip, Poetry, Pipenv などがあるが、uv を選ぶ理由は明確だ。

- **単一バイナリ**。Python のインストール不要で動作する
- **高速**。仮想環境の作成・パッケージのインストールが pip より桁違いに速い
- **`uv run` が便利**。「venv がなければ作って、依存関係をインストールして、コマンドを実行する」を一発でやってくれる

Tauri に同梱するバイナリとして非常に扱いやすい。

## プロジェクト構成

```
tauri-fastapi-uv-starter/
├── python/
│   ├── pyproject.toml       # FastAPI/uvicorn の依存関係
│   └── main.py              # FastAPI アプリ本体
├── scripts/
│   ├── download-uv.ps1      # Windows 向け uv ダウンロードスクリプト
│   └── download-uv.sh       # macOS/Linux 向け
└── src-tauri/
    ├── build.rs             # ビルド時に uv を自動取得
    ├── resources/
    │   └── uv.exe           # 同梱 uv バイナリ（gitignore）
    ├── src/lib.rs           # FastAPI プロセス管理
    ├── tauri.conf.json      # python/ をリソースとして同梱
    └── tauri.windows.conf.json  # Windows: uv.exe をリソースに追加
```

## Python 側の実装

### pyproject.toml

```toml
[project]
name = "tauri-fastapi-backend"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.115.0",
    "uvicorn[standard]>=0.30.0",
]

[tool.uv]
package = false
```

`[tool.uv] package = false` の指定が重要だ。これがないと uv が「このディレクトリをパッケージとしてインストールしようとする」ため、`pyproject.toml` に `[build-system]` がないと失敗する。

### main.py

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Tauri FastAPI Backend")

app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ],
    allow_origin_regex=r"^https?://(localhost|127\.0\.0\.1)(:\d+)?$",
    allow_methods=["*"],
    allow_headers=["*"],
)

@app.get("/")
def root():
    return {"status": "ok", "message": "FastAPI backend is running"}

@app.get("/hello/{name}")
def hello(name: str):
    return {"message": f"Hello, {name}! From FastAPI!"}
```

CORS ミドルウェアの設定が必要な点に注意。ここではワイルドカードを使わず、Tauri の origin とローカル開発 origin（任意ポート）を許可している。

## Tauri リソース設定

### tauri.conf.json

```json
{
  "bundle": {
    "resources": {
      "../python": "python"
    }
  }
}
```

`resources` のマッピングはキーがソースパス（`tauri.conf.json` からの相対パス）、値がバンドル内の配置先パスだ。`"../python": "python"` で、プロジェクトルートの `python/` ディレクトリがリソースディレクトリの `python/` として同梱される。

### tauri.windows.conf.json

Tauri v2 はプラットフォーム別の設定ファイルをサポートしており、ビルド時に自動でベース設定とディープマージされる。uv のバイナリはプラットフォームによって異なるため、ここに分離する。

```json
{
  "bundle": {
    "resources": {
      "resources/uv.exe": "uv.exe"
    }
  }
}
```

macOS では `tauri.macos.conf.json` に `"resources/uv": "uv"` を、Linux では `tauri.linux.conf.json` に同様の設定を書く。

## Rust 側の実装

### build.rs — ビルド時の uv 自動取得

ビルド時に `resources/uv.exe` が存在しなければダウンロードスクリプトを自動実行する。これにより `git clone` 直後の `cargo check` でも開発者が手動でセットアップしなくて済む。

```rust
fn main() {
    ensure_uv();
    tauri_build::build()
}

fn ensure_uv() {
    #[cfg(target_os = "windows")]
    let uv_bin = "resources/uv.exe";
    #[cfg(not(target_os = "windows"))]
    let uv_bin = "resources/uv";

    if std::path::Path::new(uv_bin).exists() {
        return;
    }

    println!("cargo:warning=uv not found, downloading...");
    if let Err(e) = download_uv() {
        panic!("uv bootstrap failed: {e}");
    }
}

fn download_uv() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        for shell in &["pwsh", "powershell"] {
            let status = std::process::Command::new(shell)
                .args(["-ExecutionPolicy", "Bypass", "-File", "../scripts/download-uv.ps1"])
                .status();
            if let Ok(s) = status {
                if s.success() { return Ok(()); }
            }
        }
        Err("powershell script failed".into())
    }
    // macOS/Linux は省略
}
```

ダウンロードスクリプト側では GitHub Release の `SHA256SUMS` を取得し、アーカイブのハッシュ検証後に展開するようにしている。これにより同梱バイナリの供給チェーンリスクを下げられる。

### lib.rs — FastAPI プロセスの管理

```rust
use std::process::Child;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct BackendProcess {
    child: Option<Child>,
    port: Option<u16>,
    /// ハンドルを保持し続けることで Drop 時に OS が KILL_ON_JOB_CLOSE を発火する
    #[cfg(windows)]
    _job: Option<win32job::Job>,
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

type BackendState = Mutex<BackendProcess>;

#[derive(Clone)]
pub struct BootstrapStateData {
    phase: String,
    message: String,
}

type BootstrapState = Mutex<BootstrapStateData>;

#[derive(Serialize)]
struct BootstrapStatus {
    phase: String,
    message: String,
}
```

`Drop` トレイトの実装がポイントだ。`BackendState` がアプリ終了時に破棄されると `Drop::drop` が呼ばれ、FastAPI プロセスが自動的に kill される。`on_window_event` でクリーンアップを書く必要がなく、Rust らしい RAII パターンで安全に後処理できる。

### Windows でのプロセスツリー終了問題

`child.kill()` だけでは Windows で問題が生じる。`uv run uvicorn` は `uv.exe` → `python.exe` という**孫プロセス**を生成するため、`child.kill()` で `uv.exe` を止めても `python.exe` が残り続ける。また `Drop` はクリーンな終了時にしか実行が保証されず、強制終了やクラッシュ時には呼ばれない場合がある。

解決策は **Windows Job Object** だ。`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` フラグを設定したジョブに子プロセスをアタッチしておくと、Tauri プロセスが終了してジョブのハンドルが閉じられた瞬間に、OS がジョブ内のプロセスツリー全体を kill する。これはカーネルが保証する動作なので、クラッシュや `taskkill /F` でも確実に機能する。

`Cargo.toml` に Windows 専用依存として追加する:

```toml
[target.'cfg(windows)'.dependencies]
win32job = "2"
```

`attach_job_object` 関数の実装:

```rust
#[cfg(windows)]
fn attach_job_object(child: &Child) -> Option<win32job::Job> {
    use std::os::windows::io::AsRawHandle;

    let job = win32job::Job::create().ok()?;

    let mut info = job.query_extended_limit_info().ok()?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&mut info).ok()?;

    let raw = child.as_raw_handle() as isize;
    job.assign_process(raw).ok()?;

    Some(job)
}
```

`BackendProcess` の `_job` フィールドがこのジョブのハンドルを保持する。`BackendProcess` が Drop されるとハンドルが閉じられ、OS がプロセスツリーを kill する。フィールドを「持っているだけ」で仕事をするのが RAII らしい設計だ。

次に起動ロジック:

```rust
fn ensure_python_environment(app: &AppHandle) -> Result<(), String> {
    let paths = resolve_runtime_paths(app)?;

    let status = std::process::Command::new(&paths.uv)
        .env("UV_PROJECT_ENVIRONMENT", &paths.venv_dir)
        .args(["sync", "--locked"])
        .current_dir(&paths.python_dir)
        .status()
        .map_err(|e| format!("uv sync 実行失敗: {e}"))?;

    if status.success() { Ok(()) } else { Err(format!("uv sync が失敗しました (exit={status})")) }
}

fn start_backend(app: &AppHandle) -> Result<BackendProcess, String> {
    let paths = resolve_runtime_paths(app)?;
    let port = allocate_backend_port()?;
    let port_arg = port.to_string();

    let child = std::process::Command::new(&paths.uv)
        .env("UV_PROJECT_ENVIRONMENT", &paths.venv_dir)
        .args([
            "run", "--no-sync", "uvicorn", "main:app",
            "--host", "127.0.0.1",
            "--port", &port_arg,
            "--no-access-log",
        ])
        .current_dir(&paths.python_dir)
        .spawn()
        .map_err(|e| format!("FastAPI 起動失敗: {e}"))?;

    Ok(BackendProcess {
        child: Some(child),
        port: Some(port),
        #[cfg(windows)]
        _job: attach_job_object(&child),
    })
}
```

ここで注目すべき点が `UV_PROJECT_ENVIRONMENT` 環境変数だ。

通常 uv は `pyproject.toml` と同じディレクトリに `.venv` を作成しようとするが、プロダクションビルドではリソースディレクトリが `C:\Program Files\...` などの**書き込み不可の場所**に配置されることがある。`UV_PROJECT_ENVIRONMENT` に書き込み可能なパスを渡すことで、venv の作成先を明示的に制御できる。

```rust
#[tauri::command]
fn get_api_port(state: tauri::State<BackendState>) -> Option<u16> {
    let mut backend = state.lock().unwrap();
    if refresh_backend_state(&mut backend) {
        backend.port
    } else {
        None
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(BackendProcess::none()) as BackendState)
        .manage(Mutex::new(BootstrapStateData::new()) as BootstrapState)
        .setup(|app| {
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                set_bootstrap_status(&app_handle, "syncing", "Setting up Python dependencies...");
                if let Err(e) = ensure_python_environment(&app_handle) {
                    set_bootstrap_status(&app_handle, "failed", format!("Python setup failed: {e}"));
                    return;
                }

                set_bootstrap_status(&app_handle, "starting", "Starting backend service...");
                match start_backend(&app_handle) {
                    Ok(backend) => {
                        *app_handle.state::<BackendState>().lock().unwrap() = backend;
                        set_bootstrap_status(&app_handle, "running", "Backend is ready.");
                    }
                    Err(e) => {
                        set_bootstrap_status(&app_handle, "failed", format!("Backend failed to start: {e}"));
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port, is_backend_running, get_backend_bootstrap_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## フロントエンドから FastAPI を呼ぶ

```tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [apiPort, setApiPort] = useState<number | null>(null);
  const [response, setResponse] = useState("");

  useEffect(() => {
    // Rust コマンド経由でポート番号を取得
    invoke<number>("get_api_port").then(setApiPort);
  }, []);

  async function callApi(name: string) {
    const res = await fetch(`http://127.0.0.1:${apiPort}/hello/${name}`);
    const data = await res.json();
    setResponse(data.message);
  }

  // ...
}
```

`get_api_port` コマンドで実際に起動したポートをフロントに渡している。ランダムポート運用でも競合せずに接続できる。

## uv の初回起動について

この構成では初回起動時に明示的に `uv sync --locked` を実行する。これにより、初回セットアップの進捗を UI で明示できる。

1. Python インタープリタのダウンロード（`requires-python` を満たすバージョン）
2. `.venv` の作成
3. `uv.lock` に基づく依存関係の同期
4. 準備完了後に `uv run --no-sync` でサーバー起動

2 回目以降は多くの場合セットアップが短時間で終わる。初回起動時はネットワーク通信が発生する点に注意が必要だ。オフライン環境での利用が想定される場合は、インストーラーの段階で依存関係を事前に取得するか、[uv のオフラインモード](https://docs.astral.sh/uv/concepts/cache/)を活用する。

## 動作確認

```bash
# 初回（uv は cargo check/build 時に自動ダウンロードされる）
npm run tauri dev
```

アプリが起動すると Rust のログに次のような出力が出れば成功だ:

```
[backend] uv: C:\...\resources\uv.exe
[backend] python_dir: C:\...\resources\python
[backend] venv: C:\...\python\.venv
[backend] selected port: 52341
[backend] FastAPI started (pid=12345)
[backend] FastAPI is running on http://127.0.0.1:52341
```

フロントエンドのテキストボックスに名前を入れて「Call FastAPI」ボタンを押すと、FastAPI から `Hello, {name}! From FastAPI!` が返ってくる。

## プロダクションビルド

```bash
npm run tauri build
```

ビルド成果物には `uv.exe`（Windows の場合）と `python/` ディレクトリが同梱され、ユーザーは Python も pip も uv も手動インストールする必要がない。

## まとめ

この構成のポイントをまとめる。

| 課題 | 解決策 |
|---|---|
| Python 環境をユーザーに要求したくない | uv バイナリをアプリに同梱 |
| venv の作成場所が書き込み不可になりうる | `UV_PROJECT_ENVIRONMENT` で書き込み可能パスに誘導 |
| git clone 後すぐ開発を始めたい | `build.rs` で uv を自動ダウンロード |
| アプリ終了時に FastAPI を確実に止めたい（macOS/Linux） | `Drop` トレイトで RAII パターン |
| Windows で孫プロセス（python.exe）が残る | Windows Job Object で OS レベルのプロセスツリー kill |
| プラットフォームごとに uv バイナリが異なる | `tauri.{platform}.conf.json` で分岐 |
| ポート競合が起きる | `127.0.0.1:0` で空きポートを自動確保 |
| 初回起動が遅い | `uv sync --locked` を明示実行し、UI に進捗を表示 |
| バイナリ配布の信頼性 | `SHA256SUMS` による uv ダウンロード検証 |

Tauri + Rust + FastAPI という組み合わせは一見複雑に見えるが、実装してみると意外とシンプルに整理できる。Python の豊富なエコシステムを Tauri アプリから活用したい場合の参考になれば幸いだ。

---

サンプルコードは [GitHub](https://github.com/Ouvill/tauri-fastapi-uv-starter) で公開している。
