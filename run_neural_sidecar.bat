@echo off
setlocal
echo ========================================================
echo  VoxForge Neural Voice Cloning Sidecar (OmniVoice 646 Langs)
echo  Supports: OmniVoice (646 Langs), CosyVoice 3, Whisper ASR
echo ========================================================

where python >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Python not found in PATH. Please install Python 3.10+
    pause
    exit /b 1
)

echo Starting VoxForge Neural Sidecar on http://127.0.0.1:9000 ...
python scripts\neural_sidecar.py %*
if %ERRORLEVEL% neq 0 (
    echo.
    echo Sidecar exited with error code %ERRORLEVEL%.
    pause
)
