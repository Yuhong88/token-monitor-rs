@echo off
cd /d "%~dp0"
if exist "TokenMonitorRS.exe" (
  start "" "TokenMonitorRS.exe"
) else if exist "target\release\token-monitor-rs.exe" (
  start "" "target\release\token-monitor-rs.exe"
) else (
  echo Build not found. Building release binary...
  set "PATH=%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;%PATH%"
  "C:\Users\small\.cargo\bin\cargo.exe" build --release --offline
  if %errorlevel% equ 0 (
    copy /Y "target\release\token-monitor-rs.exe" "TokenMonitorRS.exe" >nul
    start "" "TokenMonitorRS.exe"
  ) else (
    pause
  )
)
