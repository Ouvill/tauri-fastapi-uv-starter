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
│  │  (React/Vite) │ ◄──────── │ :8000  │ │
│  └──────────────┘           └────────┘ │
│                                  ▲      │
│  ┌──────────────┐          spawn │      │
│  │  Rust (lib.rs)│ ─────────────┘      │
│  └──────────────┘                       │
│         ▲ バンドルに含まれる              │
│   ┌─────┴──────┐                        │
│   │ uv.exe     │  ← uv が venv を管理   │
│   │ python/    │  ← FastAPI ソースコード │
│   └────────────┘                        │
└─────────────────────────────────────────┘
```

Rust のプロセスがアプリ起動時に `uv run uvicorn ...` を子プロセスとして起動し、アプリ終了時に kill する。フロントエンドは `fetch` で `http://127.0.0.1:8000` の FastAPI エンドポイントを叩く。

## なぜ uv なのか

Python の依存関係解決ツールには pip, Poetry, Pipenv などがあるが、uv を選ぶ理由は明確だ。

- **単一バイナリ**。Python のインストール不要で動作する
- **高速**。仮想環境の作成・パッケージのインストールが pip より桁違いに速い
- **`uv run` が便利**。「venv がなければ作って、依存関係をインストールして、コマンドを実行する」を一発でやってくれる

Tauri に同梱するバイナリとして非常に扱いやすい。

## プロジェクト構成

```
tauri-fastapi-uv-sandbox/
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

`[tool.uv] package = false` の指定が重要だ。これがないと uv が「このディレクトリをパッケージとしてインストールしようとする」ため、`pyproject.toml` に `[build-system]` がないと `uv run` が失敗する。

### main.py

```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI(title="Tauri FastAPI Backend")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # tauri://localhost と http://localhost:*
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

CORS ミドルウェアの設定が必要な点に注意。Tauri のフロントエンドからのリクエストは `tauri://localhost` または `http://localhost:1420`（開発時）から来るため、CORS を許可しないと fetch がブロックされる。

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
    let _ = download_uv();
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

### lib.rs — FastAPI プロセスの管理

```rust
use std::process::Child;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const FASTAPI_PORT: u16 = 8000;

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
```

`Drop` トレイトの実装がポイントだ。`BackendState` がアプリ終了時に破棄されると `Drop::drop` が呼ばれ、FastAPI プロセスが自動的に kill される。`on_window_event` でクリーンアップを書く必要がなく、Rust らしい RAII パターンで安全に後処理できる。

次に起動ロジック:

```rust
fn start_backend(app: &AppHandle) -> Result<Child, String> {
    let resource_dir = app.path().resource_dir()
        .map_err(|e| format!("resource_dir 取得失敗: {e}"))?;

    #[cfg(target_os = "windows")]
    let uv = resource_dir.join("uv.exe");
    #[cfg(not(target_os = "windows"))]
    let uv = resource_dir.join("uv");

    let python_dir = resource_dir.join("python");

    // venv の保存先を分ける
    // デバッグ時: python/.venv（手軽）
    // リリース時: AppLocalData/.venv（リソースが読み取り専用になりうるため）
    let venv_dir = {
        #[cfg(debug_assertions)]
        { python_dir.join(".venv") }
        #[cfg(not(debug_assertions))]
        {
            app.path().app_local_data_dir()
                .map_err(|e| format!("app_local_data_dir 取得失敗: {e}"))?
                .join(".venv")
        }
    };

    let child = std::process::Command::new(&uv)
        .env("UV_PROJECT_ENVIRONMENT", &venv_dir)
        .args([
            "run", "uvicorn", "main:app",
            "--host", "127.0.0.1",
            "--port", &FASTAPI_PORT.to_string(),
            "--no-access-log",
        ])
        .current_dir(&python_dir)
        .spawn()
        .map_err(|e| format!("FastAPI 起動失敗: {e}"))?;

    Ok(child)
}
```

ここで注目すべき点が `UV_PROJECT_ENVIRONMENT` 環境変数だ。

通常 uv は `pyproject.toml` と同じディレクトリに `.venv` を作成しようとするが、プロダクションビルドではリソースディレクトリが `C:\Program Files\...` などの**書き込み不可の場所**に配置されることがある。`UV_PROJECT_ENVIRONMENT` に書き込み可能なパスを渡すことで、venv の作成先を明示的に制御できる。

```rust
#[tauri::command]
fn get_api_port() -> u16 {
    FASTAPI_PORT
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(BackendProcess(None)) as BackendState)
        .setup(|app| {
            match start_backend(app.handle()) {
                Ok(child) => {
                    *app.state::<BackendState>().lock().unwrap() = BackendProcess(Some(child));
                }
                Err(e) => eprintln!("[backend] ERROR: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_api_port])
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

`get_api_port` コマンドでポート番号をフロントに渡している。今回は固定ポート 8000 なので不要に見えるかもしれないが、将来的にランダムポートに変更する際の拡張ポイントとして入れておくと便利だ。

## uv の初回起動について

`uv run` は初回実行時に以下を自動で行う:

1. Python インタープリタのダウンロード（`requires-python` を満たすバージョン）
2. `.venv` の作成
3. `pyproject.toml` の依存関係のインストール
4. 指定したコマンドの実行

2 回目以降はロックファイル（`uv.lock`）を参照して差分のみ更新するため、起動時間の増大はほぼない。ただし初回起動時はネットワーク通信が発生する点に注意が必要だ。オフライン環境での利用が想定される場合は、インストーラーの段階で依存関係を事前に取得するか、[uv のオフラインモード](https://docs.astral.sh/uv/concepts/cache/)を活用する。

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
[backend] FastAPI started (pid=12345)
[backend] FastAPI is running on http://127.0.0.1:8000
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
| アプリ終了時に FastAPI を確実に止めたい | `Drop` トレイトで RAII パターン |
| プラットフォームごとに uv バイナリが異なる | `tauri.{platform}.conf.json` で分岐 |

Tauri + Rust + FastAPI という組み合わせは一見複雑に見えるが、実装してみると意外とシンプルに整理できる。Python の豊富なエコシステムを Tauri アプリから活用したい場合の参考になれば幸いだ。

---

サンプルコードは [GitHub](https://github.com/Ouvill/tauri-fastapi-uv-sandbox) で公開している。
