@echo off
REM Tylluan - Start kernel
REM Usage: .\tylluan-mcp.bat

cd /d "%~dp0"

rem Bootstrap: crear venv y sincronizar deps en primer arranque
if not exist "%~dp0.venv\Scripts\python.exe" (
    echo [Tylluan] Primera ejecucion: creando entorno virtual Python...
    python -m venv "%~dp0.venv"
)
if not exist "%~dp0.venv\Scripts\python.exe" goto :venv_failed
goto :venv_ok
:venv_failed
echo [Tylluan] ERROR: python no encontrado en PATH. Instala Python 3.12+
pause
exit /b 1
:venv_ok

echo [Tylluan] Sincronizando dependencias Python...
"%~dp0.venv\Scripts\pip" install -e . --quiet --no-warn-script-location --disable-pip-version-check
if errorlevel 1 goto :pip_failed
goto :pip_ok
:pip_failed
echo [Tylluan] ERROR: fallo la instalacion de dependencias
pause
exit /b 1
:pip_ok

REM Copy config if needed
if not exist "tylluan.toml" if exist "tylluan.example.toml" (
    echo No tylluan.toml found - copying from tylluan.example.toml
    copy tylluan.example.toml tylluan.toml
)

REM ---- Kernel staleness check: rebuild if any crates/ source or Cargo.toml/.lock
REM      is newer than the release binary, not only if the binary is missing.
set "REBUILD_KERNEL=0"
if not exist "target\release\tylluan-nexus.exe" set "REBUILD_KERNEL=1"
if "%REBUILD_KERNEL%"=="0" (
    for /f %%i in ('powershell -NoProfile -Command "$exe=(Get-Item 'target\release\tylluan-nexus.exe').LastWriteTime; $src=(Get-ChildItem -Path 'crates','Cargo.toml','Cargo.lock' -Recurse -Include *.rs,*.toml,Cargo.lock -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1).LastWriteTime; if($src -gt $exe){'1'}else{'0'}"') do set "REBUILD_KERNEL=%%i"
)
if "%REBUILD_KERNEL%"=="1" (
    echo [Tylluan] Codigo del kernel cambiado o binario ausente - compilando release...
    cargo build --release -p tylluan-kernel
)

REM ---- Dashboard staleness check: same pattern as the kernel above, flat
REM      (no nested if-blocks -- those are fragile under cmd.exe).
set "DASH_PRESENT=0"
if exist "dashboard\package.json" set "DASH_PRESENT=1"
set "REBUILD_DASH=0"
if "%DASH_PRESENT%"=="1" if not exist "dashboard\dist\index.html" set "REBUILD_DASH=1"
if "%DASH_PRESENT%"=="1" if "%REBUILD_DASH%"=="0" if exist "dashboard\dist\index.html" (
    for /f %%i in ('powershell -NoProfile -Command "$out=(Get-Item 'dashboard\dist\index.html').LastWriteTime; $src=(Get-ChildItem -Path 'dashboard\src','dashboard\package.json' -Recurse -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1).LastWriteTime; if($src -gt $out){'1'}else{'0'}"') do set "REBUILD_DASH=%%i"
)
if "%REBUILD_DASH%"=="1" (
    echo [Tylluan] Codigo del dashboard cambiado o build ausente - compilando...
    pushd dashboard
    call pnpm install
    call pnpm run build
    popd
)

REM Start kernel (foreground)
echo Starting tylluan-nexus...
pushd crates\tylluan-kernel
..\..\target\release\tylluan-nexus.exe
popd
