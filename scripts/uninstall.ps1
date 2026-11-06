# VoxForge Turnkey Uninstaller for Windows PowerShell
# Usage: irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/uninstall.ps1 | iex
# Or:    .\scripts\uninstall.ps1 [-PurgeData]

[CmdletBinding()]
param(
    [switch]$PurgeData = $false
)

$ErrorActionPreference = "SilentlyContinue"

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

Write-Host "VoxForge Uninstaller for Windows" -ForegroundColor White
Write-Host "=================================" -ForegroundColor White

# 1. Terminate running voxforge instances
Write-Host "Stopping any running VoxForge processes..." -ForegroundColor Cyan
Get-Process -Name "voxforge" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# 2. Paths
$InstallBase = "$env:LOCALAPPDATA\Programs\VoxForge"
$BinDir = "$InstallBase\bin"
$DesktopPath = [Environment]::GetFolderPath("Desktop")
$StartMenuPath = [Environment]::GetFolderPath("Programs")
$DesktopShortcut = "$DesktopPath\VoxForge.lnk"
$StartShortcut = "$StartMenuPath\VoxForge.lnk"

# 3. Remove shortcuts
if (Test-Path $DesktopShortcut) {
    Remove-Item -Path $DesktopShortcut -Force -ErrorAction SilentlyContinue
    Write-Host "Removed Desktop shortcut: $DesktopShortcut" -ForegroundColor Green
}
if (Test-Path $StartShortcut) {
    Remove-Item -Path $StartShortcut -Force -ErrorAction SilentlyContinue
    Write-Host "Removed Start Menu shortcut: $StartShortcut" -ForegroundColor Green
}

# 4. Remove from User PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($UserPath -like "*$BinDir*") {
    Write-Host "Removing $BinDir from User PATH..." -ForegroundColor Cyan
    $PathParts = $UserPath -split ';' | Where-Object { $_ -and $_ -ne $BinDir }
    $NewPath = ($PathParts -join ';').Trim(';')
    [Environment]::SetEnvironmentVariable("Path", $NewPath, [EnvironmentVariableTarget]::User)
    $env:PATH = ($env:PATH -split ';' | Where-Object { $_ -ne $BinDir }) -join ';'
    Write-Host "User PATH cleaned successfully." -ForegroundColor Green
}

# 5. Remove Program files
if (Test-Path $InstallBase) {
    Write-Host "Removing binary and program directory: $InstallBase" -ForegroundColor Cyan
    Remove-Item -Path $InstallBase -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Program directory removed." -ForegroundColor Green
}

# 6. Data directories (models, profiles, cache)
$AppDataDir = "$env:APPDATA\VoxForge"
$HomeVoxforge = "$HOME\.voxforge"

if ($PurgeData) {
    if (Test-Path $AppDataDir) {
        Write-Host "Purging app data: $AppDataDir" -ForegroundColor Cyan
        Remove-Item -Path $AppDataDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $HomeVoxforge) {
        Write-Host "Purging cache data: $HomeVoxforge" -ForegroundColor Cyan
        Remove-Item -Path $HomeVoxforge -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Data directories purged." -ForegroundColor Green
} else {
    if ((Test-Path $AppDataDir) -or (Test-Path $HomeVoxforge)) {
        Write-Host "`nNote: Voice profiles and downloaded model weights kept at:" -ForegroundColor Yellow
        if (Test-Path $AppDataDir) { Write-Host "  - $AppDataDir" -ForegroundColor Gray }
        if (Test-Path $HomeVoxforge) { Write-Host "  - $HomeVoxforge" -ForegroundColor Gray }
        Write-Host "To remove data as well, run with -PurgeData" -ForegroundColor Yellow
    }
}

Write-Host "`nVoxForge has been cleanly uninstalled from your system." -ForegroundColor Green

