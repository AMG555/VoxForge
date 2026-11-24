@echo off
setlocal
echo.
echo [VoxForge] Starting Local Neural Cloning Sidecar (OmniVoice 646-Lang)...
echo.
where python >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Python 3.10+ not found on PATH.
  pause
  exit /b 1
)

python -c "import omnivoice" >nul 2>nul
if errorlevel 1 (
  echo [INFO] Installing OmniVoice neural runtime first...
  python -m pip install --upgrade omnivoice
  if errorlevel 1 (
    echo [WARN] OmniVoice install failed; falling back to auto-detected backend.
  )
)

python "%~dp0neural_sidecar.py" --port 9000 %*
