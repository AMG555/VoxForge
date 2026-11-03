@echo off
rem Start VoxForge Speech Synthesis Workstation and launch browser
call "%~dp0scripts\cargo_msvc.bat" run --bin voxforge -- serve --open %*

