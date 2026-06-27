@echo off
REM Tylluan - Start kernel
REM Usage: .\tylluan-mcp.bat

cd /d "%~dp0"

rem ?? Bootstrap: crear venv y sincronizar deps en primer arranque ??????????????
if not exist "%~dp0.venv\Scripts\python.exe" (
    echo [Tylluan] Primera ejecucion: creando entorno virtual Python...
    python -m venv "%~dp0.venv"
    if errorlevel 1 (
        echo [Tylluan] ERROR: python no encontrado en PATH. Instala Python 3.12+
        pause
        exit /b 1
    )
)
echo [Tylluan] Sincronizando dependencias Python...
"%~dp0.venv\Scripts\pip" install -e "%~dp0" --quiet --no-warn-script-location
if errorlevel 1 (
    echo [Tylluan] ERROR: fallo la instalacion de dependencias
    pause
    exit /b 1
)
rem ?????????????????????????????????????????????????????????????????????????????

REM Copy config if needed
if not exist "tylluan.toml" (
    if exist "tylluan.example.toml" (
        echo No tylluan.toml found - copying from tylluan.example.toml
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
    cargo build --release -p tylluan-kernel
)

REM Start kernel (foreground)
echo Starting tylluan-nexus...
pushd crates\tylluan-kernel
..\..\target\release\tylluan-nexus.exe
popd
