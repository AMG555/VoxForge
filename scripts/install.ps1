# VoxForge Turnkey One-Line Installer for Windows PowerShell
# Usage: irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "AMG555/VoxForge"
$InstallBase = "$env:LOCALAPPDATA\Programs\VoxForge"
$BinDir = "$InstallBase\bin"
$TargetExe = "$BinDir\voxforge.exe"

Write-Host @"
  _    _            ______                 
 | |  | |          |  ____|                
 | |  | | _____  __| |__ ___  _ __ __ _    
 | |  | |/ _ \ \/ /|  __/ _ \| '__/ _` |   
  \ \/ / (_) >  < | | | (_) | | | (_| |   
   \__/ \___/_/\_\|_|  \___/|_|  \__, |   
                                  __/ |   
                                 |___/    
"@ -ForegroundColor Yellow

Write-Host "VoxForge Standalone Workstation Installer for Windows" -ForegroundColor White
Write-Host ""

# 1. Architecture check
$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Arch -ne "X64" -and $Arch -ne "Arm64") {
    Write-Error "Unsupported CPU architecture: $Arch. VoxForge requires x64 or Arm64 Windows."
    exit 1
}

$BinaryName = "voxforge-windows-x86_64.exe"
Write-Host "Detected Architecture: $Arch" -ForegroundColor Cyan

# 2. Ensure install directory exists
if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
}

# 3. Retrieve latest release tag
$LatestTag = "v0.2.0"
try {
    $ReleaseInfo = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{"User-Agent"="VoxForge-Installer"} -ErrorAction SilentlyContinue
    if ($ReleaseInfo -and $ReleaseInfo.tag_name) {
        $LatestTag = $ReleaseInfo.tag_name
    }
} catch {
    # Keep fallback tag
}

$LocalCandidates = @(
    "$PSScriptRoot\..\target\release\voxforge.exe",
    "$PSScriptRoot\..\dist\voxforge.exe",
    "$PSScriptRoot\voxforge.exe",
    ".\target\release\voxforge.exe"
)

$LocalIcoCandidates = @(
    "$PSScriptRoot\..\assets\voxforge.ico",
    "$PSScriptRoot\voxforge.ico",
    ".\assets\voxforge.ico"
)

$FoundLocal = $false
foreach ($Cand in $LocalCandidates) {
    if (Test-Path $Cand) {
        Write-Host "Found local binary at $Cand. Installing directly..." -ForegroundColor Green
        Copy-Item -Path $Cand -Destination $TargetExe -Force
        $FoundLocal = $true
        break
    }
}

if (-not $FoundLocal) {
    $DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$BinaryName"
    Write-Host "Downloading VoxForge $LatestTag..." -ForegroundColor Cyan

    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $TargetExe -UseBasicParsing
        Write-Host "Successfully downloaded to $TargetExe" -ForegroundColor Green
    } catch {
        Write-Warning "Direct release download unavailable. Checking for local cargo installation..."
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            cargo install --git "https://github.com/$Repo.git" voxforge-cli --root $InstallBase
            Write-Host "Successfully built and installed via cargo." -ForegroundColor Green
        } else {
            Write-Error "Failed to download binary and cargo was not found. Please install Rust or check release assets."
            exit 1
        }
    }
}

# 4. Copy icon file alongside exe for reliable shortcut display
$IcoTarget = "$BinDir\voxforge.ico"
foreach ($IcoCand in $LocalIcoCandidates) {
    if (Test-Path $IcoCand) {
        Copy-Item -Path $IcoCand -Destination $IcoTarget -Force
        Write-Host "Icon installed: $IcoTarget" -ForegroundColor Green
        break
    }
}
# Fallback: fetch from GitHub release if not found locally
if (-not (Test-Path $IcoTarget)) {
    try {
        Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$LatestTag/voxforge.ico" -OutFile $IcoTarget -UseBasicParsing -ErrorAction SilentlyContinue
    } catch { }
}

$IconSource = if (Test-Path $IcoTarget) { "$IcoTarget,0" } else { "$TargetExe,0" }

# 5. Persist to User PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($UserPath -notlike "*$BinDir*") {
    Write-Host "Adding $BinDir to User PATH..." -ForegroundColor Cyan
    $NewPath = "$UserPath;$BinDir".Trim(';')
    [Environment]::SetEnvironmentVariable("Path", $NewPath, [EnvironmentVariableTarget]::User)
    $env:PATH = "$env:PATH;$BinDir"
    Write-Host "PATH updated successfully." -ForegroundColor Green
}

# 5. Create Desktop & Start Menu Shortcuts for normal desktop users
try {
    $WshShell = New-Object -ComObject WScript.Shell
    $DesktopPath = [Environment]::GetFolderPath("Desktop")
    $StartMenuPath = [Environment]::GetFolderPath("Programs")

    # Create silent GUI launcher script so non-developers see zero terminal window
    $VbsLauncher = "$BinDir\voxforge-gui.vbs"
    Set-Content -Path $VbsLauncher -Value 'CreateObject("Wscript.Shell").Run "cmd /c voxforge serve --open", 0, False' -Force

    # Desktop Shortcut
    $DesktopShortcut = $WshShell.CreateShortcut("$DesktopPath\VoxForge.lnk")
    $DesktopShortcut.TargetPath = "wscript.exe"
    $DesktopShortcut.Arguments = "`"$VbsLauncher`""
    $DesktopShortcut.WorkingDirectory = "$InstallBase"
    $DesktopShortcut.Description = "VoxForge Neural Speech Studio"
    $DesktopShortcut.IconLocation = $IconSource
    $DesktopShortcut.Save()

    # Start Menu Shortcut
    $StartShortcut = $WshShell.CreateShortcut("$StartMenuPath\VoxForge.lnk")
    $StartShortcut.TargetPath = "wscript.exe"
    $StartShortcut.Arguments = "`"$VbsLauncher`""
    $StartShortcut.WorkingDirectory = "$InstallBase"
    $StartShortcut.Description = "VoxForge Neural Speech Studio"
    $StartShortcut.IconLocation = $IconSource
    $StartShortcut.Save()

    Write-Host "Created Desktop and Start Menu shortcuts." -ForegroundColor Green
} catch {
    # Non-fatal if shortcut creation is restricted
}

# 6. Hardware validation
Write-Host "`nProbing system hardware..." -ForegroundColor Cyan
& $TargetExe hardware

Write-Host @"

==================================================
  VoxForge Standalone Workstation Ready!
==================================================

Desktop App:          Double-click 'VoxForge' on your Desktop
Run via Terminal:     voxforge serve --open
Direct synthesis:     voxforge synth "Hello from VoxForge"
Documentation & API:  http://localhost:8080/docs

"@ -ForegroundColor Green
