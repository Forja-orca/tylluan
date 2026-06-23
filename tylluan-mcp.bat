@echo off
REM Tylluan ? Start kernel + proxy
REM Usage: .\tylluan-mcp.bat

cd /d "%~dp0"

REM Activate Python venv if available
if exist ".venv\Scripts\activate.bat" call .venv\Scripts\activate.bat

REM Copy config if needed
if not exist "tylluan.toml" (
    if exist "tylluan.example.toml" (
        echo No tylluan.toml found ? copying from tylluan.example.toml
        copy tylluan.example.toml tylluan.toml
    )
)

REM Build dashboard if needed
if exist "dashboard\package.json" (
    if not exist "dashboard\dist\index.html" (
        echo Building dashboard...
        pushd dashboard
        call npm install
        call npm run build
        popd
    )
)

REM Build kernel if needed
if not exist "target\release\tylluan-nexus.exe" (
    echo Building tylluan-kernel...
    cargo build --release -p tylluan-kernel -p tylluan-proxy
)

REM Start proxy
echo Starting tylluan-proxy on :3030...
start /B target\release\tylluan-proxy.exe

REM Small delay
timeout /t 2 /nobreak >nul

REM Start kernel (foreground)
echo Starting tylluan-nexus...
pushd crates\tylluan-kernel
..\..\target\release\tylluan-nexus.exe
popd