# Forza DualSense — complete uninstaller.
#
# Run from any PowerShell window:
#
#   iwr -useb https://raw.githubusercontent.com/ksc98/forza-dualsense/main/uninstall.ps1 | iex
#
# Removes the installed binary, the Start Menu shortcut, and the
# persisted settings directory. Makes no registry changes (none were
# created at install time).

[CmdletBinding()]
param()

$ErrorActionPreference = "Continue"

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Done($msg) { Write-Host "OK  $msg" -ForegroundColor Green }
function Skip($msg) { Write-Host "    $msg" -ForegroundColor DarkGray }

# 1. Stop any running instance so we can delete the binary on Windows.
Step "Stopping any running Forza DualSense process"
$running = Get-Process -Name forza-dualsense -ErrorAction SilentlyContinue
if ($running) {
    $running | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Done "Stopped $($running.Count) process(es)"
} else {
    Skip "No running process found"
}

# 2. Install directory under %LOCALAPPDATA%.
$installDir = Join-Path $env:LOCALAPPDATA "Programs\ForzaDualSense"
Step "Removing install directory"
if (Test-Path $installDir) {
    Remove-Item -Recurse -Force $installDir
    Done "Removed $installDir"
} else {
    Skip "Not present: $installDir"
}

# 3. Start Menu shortcut.
$shortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Forza DualSense.lnk"
Step "Removing Start Menu shortcut"
if (Test-Path $shortcut) {
    Remove-Item -Force $shortcut
    Done "Removed $shortcut"
} else {
    Skip "Not present: $shortcut"
}

# 4. Persisted settings directory under %APPDATA%.
$configDir = Join-Path $env:APPDATA "forza-dualsense"
Step "Removing settings directory"
if (Test-Path $configDir) {
    Remove-Item -Recurse -Force $configDir
    Done "Removed $configDir"
} else {
    Skip "Not present: $configDir"
}

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "  Forza DualSense uninstalled." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
