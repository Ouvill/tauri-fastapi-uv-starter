# Download uv binary for Windows (x86_64)
# Usage: pwsh -File scripts/download-uv.ps1

param(
    [string]$Version = "0.6.14"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ResourcesDir = Join-Path $ScriptDir "..\src-tauri\resources"

New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null

$DestPath = Join-Path $ResourcesDir "uv.exe"

if (Test-Path $DestPath) {
    Write-Host "uv.exe already exists at $DestPath"
    & $DestPath --version
    exit 0
}

$Arch = $env:PROCESSOR_ARCHITECTURE
switch ($Arch.ToUpperInvariant()) {
    "ARM64" { $Triple = "aarch64-pc-windows-msvc" }
    default { $Triple = "x86_64-pc-windows-msvc" }
}

$Url = "https://github.com/astral-sh/uv/releases/download/$Version/uv-$Triple.zip"
$TmpZip = [System.IO.Path]::GetTempFileName() + ".zip"
$TmpDir = [System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()

Write-Host "Downloading uv $Version from $Url ..."
Invoke-WebRequest -Uri $Url -OutFile $TmpZip -UseBasicParsing

Write-Host "Extracting..."
Expand-Archive -Path $TmpZip -DestinationPath $TmpDir -Force

# uv archive extracts to uv-x86_64-pc-windows-msvc/uv.exe
$ExtractedExe = Get-ChildItem -Path $TmpDir -Recurse -Filter "uv.exe" | Select-Object -First 1
Copy-Item -Path $ExtractedExe.FullName -Destination $DestPath -Force

Remove-Item $TmpZip -Force
Remove-Item $TmpDir -Recurse -Force

Write-Host "uv installed to $DestPath"
& $DestPath --version
