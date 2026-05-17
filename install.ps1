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
# 1. Decide between published release and source build.
#
#    Strategy: read the version in Cargo.toml on the target branch and
#    look for a release tagged exactly v<that>. Use it if present,
#    otherwise build from source. Deliberately *not* "fetch latest by
#    semver" — the repo has historical calendar-version tags (v2026.x.y)
#    that lexically outrank legitimate 1.x.y releases.
# -----------------------------------------------------------------
$headers = @{ "User-Agent" = "forza-dualsense-installer"; "Accept" = "application/vnd.github+json" }

function Get-BranchVersion {
    param([string]$Repo, [string]$Branch)
    try {
        $cargoUrl = "https://raw.githubusercontent.com/$Repo/$Branch/Cargo.toml"
        $cargo = Invoke-WebRequest -UseBasicParsing -Uri $cargoUrl -ErrorAction Stop
        if ($cargo.Content -match '(?m)^version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    } catch { }
    return $null
}

$branchVersion = Get-BranchVersion -Repo $Repo -Branch $Branch
if (-not $branchVersion) { Die "Could not read version from $Repo/$Branch/Cargo.toml" }
$targetTag = "v$branchVersion"
Step "Looking for release $targetTag"

$release = $null
$asset = $null
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$targetTag" -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -match "x86_64-pc-windows-msvc\.zip$" } | Select-Object -First 1
} catch {
    $status = $null
    try { $status = $_.Exception.Response.StatusCode.value__ } catch { }
    if ($status -eq 404) {
        Info "No release tagged $targetTag yet — building from source"
    } elseif ($status -eq 403) {
        Info "GitHub API rate-limited from this network — building from source"
    } else {
        Info "Could not query $targetTag ($($_.Exception.Message)) — building from source"
    }
}

if ($asset) {
    Step "Downloading $targetTag — $($asset.name)"
    $tmp = Join-Path $env:TEMP ("forza-dualsense-rel-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    $zip = Join-Path $tmp $asset.name
    Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $found = Get-ChildItem -Path $tmp -Recurse -Filter "forza-dualsense.exe" | Select-Object -First 1
    if ($found) {
        $builtExe = $found.FullName
        Done "Downloaded prebuilt binary"
    } else {
        Info "Release zip did not contain forza-dualsense.exe — falling back to source build"
    }
} elseif ($release) {
    Info "Release $targetTag has no Windows asset — building from source"
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

# Windows holds a lock on a running .exe, so the copy will fail if
# Forza DualSense is currently open. Stop any running instance first.
$running = Get-Process -Name forza-dualsense -ErrorAction SilentlyContinue
if ($running) {
    Info "Stopping $($running.Count) running instance(s) so the binary can be replaced"
    $running | Stop-Process -Force
    # Give Windows a moment to release the file handle.
    for ($i = 0; $i -lt 10; $i++) {
        Start-Sleep -Milliseconds 200
        if (-not (Get-Process -Name forza-dualsense -ErrorAction SilentlyContinue)) { break }
    }
}

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

# -----------------------------------------------------------------
# 4. Friendly hostname: map http://forza.dualsense:5301 to localhost
#    via the hosts file. Needs admin to write to system32. We try it
#    and fall back to printing the manual command if we can't.
# -----------------------------------------------------------------
Step "Mapping http://forza.dualsense:5301 -> 127.0.0.1"
$hostsPath  = Join-Path $env:SystemRoot "System32\drivers\etc\hosts"
$hostsLine  = "127.0.0.1`tforza.dualsense  # forza-dualsense"
$hostsAdded = $false
if (Test-Path $hostsPath) {
    $existing = Get-Content $hostsPath -ErrorAction SilentlyContinue
    if ($existing | Where-Object { $_ -match '^\s*[^#].*\bforza\.dualsense\b' }) {
        Info "hosts entry already present"
        $hostsAdded = $true
    } else {
        try {
            Add-Content -Path $hostsPath -Value "`n$hostsLine" -ErrorAction Stop
            Done "Added hosts entry: $hostsLine"
            $hostsAdded = $true
        } catch {
            Info "Not running as admin — can't edit $hostsPath"
            Info "To enable the friendly URL, run this once in an elevated PowerShell:"
            Info "  Add-Content -Path '$hostsPath' -Value `"`n$hostsLine`""
        }
    }
} else {
    Info "hosts file not found at $hostsPath — skipping friendly URL"
}

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  Forza DualSense installed." -ForegroundColor Green
Write-Host "  Open the Start Menu and search for: Forza DualSense" -ForegroundColor Green
Write-Host "  Or run directly:  $installDir\forza-dualsense.exe" -ForegroundColor Green
if ($hostsAdded) {
    Write-Host "  Web UI: http://forza.dualsense:5301/  (or http://127.0.0.1:5301/)" -ForegroundColor Green
} else {
    Write-Host "  Web UI: http://127.0.0.1:5301/  (run installer as admin to enable forza.dualsense)" -ForegroundColor Green
}
Write-Host "================================================================" -ForegroundColor Green
