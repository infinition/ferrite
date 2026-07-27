@echo off
setlocal EnableDelayedExpansion

rem ==========================================================================
rem  Ferrite task runner.
rem
rem  Every routine action lives here so the same commands run locally and in
rem  CI. Run "make" with no argument for the list.
rem ==========================================================================

cd /d "%~dp0"

set "TARGET=x86_64-pc-windows-msvc"
set "OUT=target\%TARGET%\release\Ferrite.exe"
set "DEBUG_OUT=target\%TARGET%\debug\Ferrite.exe"

if "%~1"=="" goto :help
set "TASK=%~1"
shift

if /i "%TASK%"=="help"    goto :help
if /i "%TASK%"=="dev"     goto :dev
if /i "%TASK%"=="run"     goto :run
if /i "%TASK%"=="serve"   goto :serve
if /i "%TASK%"=="build"   goto :build
if /i "%TASK%"=="check"   goto :check
if /i "%TASK%"=="fmt"     goto :fmt
if /i "%TASK%"=="lint"    goto :lint
if /i "%TASK%"=="i18n"    goto :i18n
if /i "%TASK%"=="icon"    goto :icon
if /i "%TASK%"=="verify"  goto :verify
if /i "%TASK%"=="dist"    goto :dist
if /i "%TASK%"=="release" goto :release
if /i "%TASK%"=="clean"   goto :clean
if /i "%TASK%"=="rules"   goto :rules

echo Unknown task: %TASK%
call :help
exit /b 1

rem ==========================================================================
:help
echo.
echo   Ferrite task runner
echo.
echo   Development
echo     make dev              build in debug and launch, console attached
echo     make run              launch the release build, building it if needed
echo     make serve [port]     launch headless, interface served to the browser
echo.
echo   Quality
echo     make check            fmt, clippy and i18n, the same gates as CI
echo     make fmt              apply formatting
echo     make lint             clippy with warnings denied
echo     make i18n             locale coverage only
echo     make rules            count the detection rules
echo.
echo   Build and ship
echo     make build            release build
echo     make verify           check the icon and version resource of the build
echo     make dist             build, verify, copy to dist\ and refresh the shortcut
echo     make release 1.1.0    bump the version, run every gate, build, tag
echo.
echo   Housekeeping
echo     make icon             regenerate the icons from tools\make_icon.py
echo     make clean            remove target\ and dist\
echo.
goto :eof

rem ==========================================================================
:dev
call :need_cargo || exit /b 1
echo [dev] building debug
cargo build --target %TARGET%
if errorlevel 1 exit /b 1
echo [dev] launching, close the window to return
"%DEBUG_OUT%"
goto :eof

rem ==========================================================================
:run
call :need_cargo || exit /b 1
if not exist "%OUT%" (
    echo [run] no release build yet, building
    call :build || exit /b 1
)
start "" "%OUT%"
echo [run] launched
goto :eof

rem ==========================================================================
:serve
call :need_cargo || exit /b 1
if not exist "%OUT%" call :build || exit /b 1
set "PORT=%~1"
if "%PORT%"=="" set "PORT=7420"
echo [serve] headless on port %PORT%, Ctrl+C to stop
"%OUT%" --headless --port %PORT%
goto :eof

rem ==========================================================================
:build
call :need_cargo || exit /b 1
rem The MSVC target is required: build.rs reaches for the Windows SDK resource
rem compiler to embed the icon and the version resource.
echo [build] cargo build --release --target %TARGET%
cargo build --release --target %TARGET%
if errorlevel 1 exit /b 1
for %%F in ("%OUT%") do set "SIZE=%%~zF"
set /a "MB=!SIZE! / 1048576"
echo [build] %OUT%  (!MB! MB)
goto :eof

rem ==========================================================================
:check
echo [check] formatting
cargo fmt --all -- --check
if errorlevel 1 (
    echo [check] formatting differs, run: make fmt
    exit /b 1
)
echo [check] clippy
cargo clippy --target %TARGET% --all-targets -- -D warnings
if errorlevel 1 exit /b 1
call :i18n || exit /b 1
echo [check] all gates passed
goto :eof

rem ==========================================================================
:fmt
call :need_cargo || exit /b 1
cargo fmt --all
echo [fmt] applied
goto :eof

rem ==========================================================================
:lint
call :need_cargo || exit /b 1
cargo clippy --target %TARGET% --all-targets -- -D warnings
goto :eof

rem ==========================================================================
:i18n
call :need_python || exit /b 1
python tools\check_i18n.py
if errorlevel 1 exit /b 1
goto :eof

rem ==========================================================================
:icon
call :need_python || exit /b 1
python tools\make_icon.py
if errorlevel 1 exit /b 1
echo [icon] rebuild to embed the new icon: make build
goto :eof

rem ==========================================================================
:verify
if not exist "%OUT%" (
    echo [verify] no release build found, run: make build
    exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_release.ps1 -Path "%OUT%"
if errorlevel 1 exit /b 1
goto :eof

rem ==========================================================================
:dist
call :build || exit /b 1
call :verify || exit /b 1
if not exist dist mkdir dist
copy /y "%OUT%" "dist\Ferrite.exe" >nul
echo [dist] dist\Ferrite.exe
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$s = (New-Object -ComObject WScript.Shell).CreateShortcut(\"$env:USERPROFILE\Desktop\Ferrite.lnk\");" ^
  "$s.TargetPath = \"%CD%\dist\Ferrite.exe\";" ^
  "$s.WorkingDirectory = \"%CD%\dist\";" ^
  "$s.IconLocation = \"%CD%\dist\Ferrite.exe,0\";" ^
  "$s.Description = 'Ferrite, workspace cleanup';" ^
  "$s.Save()"
echo [dist] desktop shortcut refreshed
goto :eof

rem ==========================================================================
:release
set "VERSION=%~1"
if "%VERSION%"=="" (
    echo [release] usage: make release 1.1.0
    exit /b 1
)

rem A dirty tree means the tag would not describe what was actually built.
for /f %%S in ('git status --porcelain 2^>nul') do (
    echo [release] working tree is not clean, commit or stash first
    git status --short
    exit /b 1
)

echo [release] setting version to %VERSION%
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$p = 'Cargo.toml';" ^
  "$t = [IO.File]::ReadAllText($p);" ^
  "$t = [regex]::Replace($t, '(?m)^version\s*=\s*\".+?\"', 'version = \"%VERSION%\"', 1);" ^
  "[IO.File]::WriteAllText($p, $t)"
if errorlevel 1 exit /b 1

call :check || exit /b 1
call :build || exit /b 1

powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify_release.ps1 -Path "%OUT%" -ExpectedVersion %VERSION%
if errorlevel 1 exit /b 1

git add Cargo.toml Cargo.lock
git commit -m "Release %VERSION%"
if errorlevel 1 exit /b 1
git tag -a "v%VERSION%" -m "Ferrite %VERSION%"
if errorlevel 1 exit /b 1

echo.
echo [release] tagged v%VERSION%
echo [release] pushing the tag is a manual step:
echo             git push origin main --follow-tags
echo           the release workflow then builds and publishes Ferrite.exe
goto :eof

rem ==========================================================================
:clean
if exist target rmdir /s /q target
if exist dist rmdir /s /q dist
echo [clean] target\ and dist\ removed
goto :eof

rem ==========================================================================
:rules
call :need_python || exit /b 1
python tools\check_i18n.py
goto :eof

rem ==========================================================================
:need_cargo
where cargo >nul 2>&1
if errorlevel 1 (
    echo cargo not found. Install Rust from https://rustup.rs
    exit /b 1
)
exit /b 0

rem ==========================================================================
:need_python
where python >nul 2>&1
if errorlevel 1 (
    echo python not found. It is only needed for the icon and i18n tools.
    exit /b 1
)
exit /b 0
