# Forza DualSense — one-command Windows installer.
#
# Run this from any PowerShell window:
#
#   iwr -useb https://raw.githubusercontent.com/ksc98/forza-dualsense/main/install.ps1 | iex
#
# Behavior:
#   1. If a GitHub Release is published, download the prebuilt binary.
#   2. Otherwise, install Rust silently and build from source.
#   3. Install to %LOCALAPPDATA%\Programs\ForzaDualSense\.
#   4. Create a Start Menu shortcut named "Forza DualSense".

[CmdletBinding()]
param(
    [string]$Branch = "main",
    [string]$Repo   = "ksc98/forza-dualsense"
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Info($msg) { Write-Host "    $msg" -ForegroundColor DarkGray }
function Done($msg) { Write-Host "OK  $msg" -ForegroundColor Green }
function Die ($msg) { Write-Host "ERR $msg" -ForegroundColor Red; exit 1 }

$installDir = Join-Path $env:LOCALAPPDATA "Programs\ForzaDualSense"
$builtExe   = $null

# -----------------------------------------------------------------
# 1. Try the published release first.
# -----------------------------------------------------------------
Step "Looking for the latest published release"
try {
    $headers = @{ "User-Agent" = "forza-dualsense-installer"; "Accept" = "application/vnd.github+json" }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -match "x86_64-pc-windows-msvc\.zip$" } | Select-Object -First 1
    if ($asset) {
        Info "Found $($release.tag_name) — $($asset.name)"
        $tmp = Join-Path $env:TEMP ("forza-dualsense-rel-" + [guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
        $zip = Join-Path $tmp $asset.name
        Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $zip
        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        $found = Get-ChildItem -Path $tmp -Recurse -Filter "forza-dualsense.exe" | Select-Object -First 1
        if ($found) {
            $builtExe = $found.FullName
            Done "Downloaded prebuilt binary"
        }
    } else {
        Info "No matching asset in latest release — will build from source"
    }
} catch {
    Info "No release available yet — will build from source"
}

# -----------------------------------------------------------------
# 2. Source build fallback.
# -----------------------------------------------------------------
if (-not $builtExe) {
    Step "Checking for Rust toolchain"
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue)
    if (-not $cargo) {
        Info "cargo not found — installing rustup (silent)"
        $rustup = Join-Path $env:TEMP "rustup-init.exe"
        Invoke-WebRequest -UseBasicParsing -Uri "https://win.rustup.rs/x86_64" -OutFile $rustup
        & $rustup -y --default-toolchain stable --profile minimal | Out-Host
        if ($LASTEXITCODE -ne 0) { Die "rustup-init failed" }
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
        $cargo = (Get-Command cargo -ErrorAction SilentlyContinue)
        if (-not $cargo) { Die "cargo still not on PATH after rustup install" }
        Done "Rust installed at $($cargo.Source)"
    } else {
        Done "Rust already installed at $($cargo.Source)"
    }

    Step "Downloading source ($Repo @ $Branch)"
    $work = Join-Path $env:TEMP ("forza-dualsense-src-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    $zipPath = Join-Path $work "src.zip"
    $branchEnc = [Uri]::EscapeUriString($Branch)
    $zipUrl = "https://codeload.github.com/$Repo/zip/refs/heads/$branchEnc"
    Invoke-WebRequest -UseBasicParsing -Uri $zipUrl -OutFile $zipPath
    Expand-Archive -Path $zipPath -DestinationPath $work -Force
    $root = Get-ChildItem -Path $work -Directory | Where-Object { $_.Name -like "forza-dualsense-*" } | Select-Object -First 1
    if (-not $root) { Die "could not locate extracted source root in $work" }
    $projDir = $root.FullName
    if (-not (Test-Path (Join-Path $projDir "Cargo.toml"))) { Die "Cargo.toml missing in extracted source" }
    Done "Source ready at $projDir"

    Step "Building release binary (slow — first build only)"
    Push-Location $projDir
    try {
        & cargo build --release | Out-Host
        if ($LASTEXITCODE -ne 0) { Die "cargo build failed" }
    } finally { Pop-Location }
    $builtExe = Join-Path $projDir "target\release\forza-dualsense.exe"
    if (-not (Test-Path $builtExe)) { Die "expected binary not found at $builtExe" }
    Done "Built $builtExe"
}

# -----------------------------------------------------------------
# 3. Install + shortcut.
# -----------------------------------------------------------------
Step "Installing to $installDir"
New-Item -ItemType Directory -Path $installDir -Force | Out-Null
Copy-Item -Path $builtExe -Destination (Join-Path $installDir "forza-dualsense.exe") -Force
Done "Copied binary to $installDir"

Step "Creating Start Menu shortcut"
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$shortcut = Join-Path $startMenuDir "Forza DualSense.lnk"
$wsh = New-Object -ComObject WScript.Shell
$lnk = $wsh.CreateShortcut($shortcut)
$lnk.TargetPath = Join-Path $installDir "forza-dualsense.exe"
$lnk.WorkingDirectory = $installDir
$lnk.Description = "Adaptive trigger feedback for Forza Horizon on the DualSense controller"
$lnk.IconLocation = (Join-Path $installDir "forza-dualsense.exe") + ",0"
$lnk.Save()
Done "Shortcut: $shortcut"

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  Forza DualSense installed." -ForegroundColor Green
Write-Host "  Open the Start Menu and search for: Forza DualSense" -ForegroundColor Green
Write-Host "  Or run directly:  $installDir\forza-dualsense.exe" -ForegroundColor Green
Write-Host "  Web UI: http://127.0.0.1:5301/ once it's running." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
