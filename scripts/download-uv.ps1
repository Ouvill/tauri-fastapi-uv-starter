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

$AssetName = "uv-$Triple.zip"
$BaseUrl = "https://github.com/astral-sh/uv/releases/download/$Version"
$Url = "$BaseUrl/$AssetName"
$ChecksumUrl = "$BaseUrl/SHA256SUMS"
$TmpZip = [System.IO.Path]::GetTempFileName() + ".zip"
$TmpChecksums = [System.IO.Path]::GetTempFileName()
$TmpDir = [System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()

Write-Host "Downloading uv $Version from $Url ..."
Invoke-WebRequest -Uri $Url -OutFile $TmpZip -UseBasicParsing

Write-Host "Downloading checksums from $ChecksumUrl ..."
Invoke-WebRequest -Uri $ChecksumUrl -OutFile $TmpChecksums -UseBasicParsing

$ExpectedLine = Select-String -Path $TmpChecksums -Pattern ("\s" + [regex]::Escape($AssetName) + "$") | Select-Object -First 1
if (-not $ExpectedLine) {
    Remove-Item $TmpZip -Force -ErrorAction SilentlyContinue
    Remove-Item $TmpChecksums -Force -ErrorAction SilentlyContinue
    throw "Could not find checksum entry for $AssetName in SHA256SUMS"
}

$ExpectedHash = ($ExpectedLine.Line -split "\s+")[0].ToLowerInvariant()
$ActualHash = (Get-FileHash -Path $TmpZip -Algorithm SHA256).Hash.ToLowerInvariant()

if ($ExpectedHash -ne $ActualHash) {
    Remove-Item $TmpZip -Force -ErrorAction SilentlyContinue
    Remove-Item $TmpChecksums -Force -ErrorAction SilentlyContinue
    throw "SHA256 mismatch for $AssetName. expected=$ExpectedHash actual=$ActualHash"
}

Write-Host "Checksum verified for $AssetName"
Remove-Item $TmpChecksums -Force

Write-Host "Extracting..."
Expand-Archive -Path $TmpZip -DestinationPath $TmpDir -Force

# uv archive extracts to uv-x86_64-pc-windows-msvc/uv.exe
$ExtractedExe = Get-ChildItem -Path $TmpDir -Recurse -Filter "uv.exe" | Select-Object -First 1
Copy-Item -Path $ExtractedExe.FullName -Destination $DestPath -Force

Remove-Item $TmpZip -Force
Remove-Item $TmpDir -Recurse -Force

Write-Host "uv installed to $DestPath"
& $DestPath --version
