@echo off
title Uninstalling VoxForge
echo ======================================================================
echo   Uninstalling VoxForge, removing shortcuts, and cleaning PATH...
echo ======================================================================
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\uninstall.ps1" %*
pause
