@echo off
rem Запустить desktop в dev-режиме (Rust + Vite одновременно).
setlocal
cd /d "%~dp0..\apps\desktop"
where pnpm >nul 2>nul && (
    pnpm install || goto :err
    pnpm tauri dev || goto :err
) || (
    npm install || goto :err
    npm run tauri -- dev || goto :err
)
exit /b 0
:err
echo dev failed: %errorlevel%
exit /b %errorlevel%
