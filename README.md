# tauri-fastapi-uv-sandbox

Tauri + uv + FastAPI のサイドカーパターンのサンプルコード。

Python を exe 化せず、uv バイナリをアプリに同梱することで依存関係を自動解決し、ユーザーが Python や pip を別途インストールしなくても動くデスクトップアプリを実現する。

依存関係の再現性のため、`python/uv.lock` をコミットしておく。

## アーキテクチャ

```text
Tauri アプリ
├── フロントエンド (React/Vite)
│     └── fetch → http://127.0.0.1:<runtime_port>
├── Rust (lib.rs)
│     └── 起動時に空きポートを選び uv run uvicorn を spawn
│         終了時に Job Object でプロセスツリーごと kill (Windows)
└── リソース (同梱)
      ├── uv.exe / uv
      └── python/         ← FastAPI ソースコード + pyproject.toml
```

## 開発環境のセットアップ

```bash
# uv バイナリを取得（初回のみ）
# Windows
pwsh scripts/download-uv.ps1
# macOS / Linux
bash scripts/download-uv.sh

# 開発サーバー起動
npm run tauri dev
```

`cargo check` / `cargo build` 実行時は `build.rs` が uv の存在を確認し、なければ自動ダウンロードする。

## プロダクションビルド

```bash
npm run tauri build
```

ビルド成果物に `uv.exe`（Windows）と `python/` が同梱される。エンドユーザーは Python も pip も不要。

## 技術的なポイント

| 課題 | 解決策 |
| --- | --- |
| Python 環境をユーザーに要求したくない | uv バイナリをアプリに同梱 |
| venv の作成場所が書き込み不可になりうる | `UV_PROJECT_ENVIRONMENT` で書き込み可能パスに誘導 |
| git clone 後すぐ開発を始めたい | `build.rs` で uv を自動ダウンロード |
| アプリ終了時に FastAPI を確実に止めたい（macOS/Linux） | `Drop` トレイトで RAII パターン |
| Windows で孫プロセス（python.exe）が残る | Windows Job Object で OS レベルのプロセスツリー kill |
| プラットフォームごとに uv バイナリが異なる | `tauri.{platform}.conf.json` で分岐 |
| 起動失敗時に UI で気付きにくい | `is_backend_running` コマンドで起動状態を表示 |
| ポート競合が起きる | 起動時に `127.0.0.1:0` で空きポートを自動確保 |

## 参考

詳細な解説は [doc/blog.md](doc/blog.md) を参照。
