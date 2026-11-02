@echo off
title Installing VoxForge Neural Speech Studio
echo ======================================================================
echo   Installing VoxForge to system and creating Desktop & Start Menu icons...
echo ======================================================================
if exist "%~dp0install.ps1" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
) else if exist "%~dp0..\scripts\install.ps1" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\scripts\install.ps1"
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -Command "& { irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.ps1 | iex }"
)
pause
