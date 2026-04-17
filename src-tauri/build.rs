fn main() {
    // uv バイナリが resources/ にない場合はダウンロードスクリプトを自動実行
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

    println!("cargo:warning=uv binary not found at {uv_bin}, running download script...");

    let result = download_uv();
    match result {
        Ok(_) => println!("cargo:warning=uv downloaded successfully."),
        Err(e) => {
            println!("cargo:warning=Failed to download uv: {e}");
            println!("cargo:warning=Please run manually: npm run setup");
            panic!("uv bootstrap failed: {e}");
        }
    }
}

fn download_uv() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // pwsh または powershell を試みる
        for shell in &["pwsh", "powershell"] {
            let status = std::process::Command::new(shell)
                .args(["-ExecutionPolicy", "Bypass", "-File", "../scripts/download-uv.ps1"])
                .status();
            if let Ok(s) = status {
                if s.success() {
                    return Ok(());
                }
            }
        }
        Err("powershell script failed".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = std::process::Command::new("bash")
            .arg("../scripts/download-uv.sh")
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("download-uv.sh failed".into())
        }
    }
}
