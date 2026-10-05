@echo off
setlocal
set "PROTOTYPE=%~dp0..\..\outputs\ac_town_prototype.exe"
if not exist "%PROTOTYPE%" (
    echo Prototype not found. Run build_x86.bat first.
    exit /b 1
)
"%PROTOTYPE%" %*
if errorlevel 1 exit /b 1
start "" "%~dp0..\..\outputs\ac_town_preview.html"
