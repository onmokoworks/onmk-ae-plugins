@echo off
REM Copy the freshly-built Rust .aex and ONNX Runtime DLLs into the AE plug-in
REM directory. Re-run after every rebuild.
REM
REM The install target is either:
REM   %DEPTH_ONNX_AE_DIR% (set it if your AE lives somewhere non-default), or
REM   C:\Program Files\Adobe\Adobe After Effects 2026\Support Files\Plug-ins\Effects
REM
REM You almost certainly need to run this in an Administrator cmd — the target
REM lives under Program Files.

setlocal enableextensions enabledelayedexpansion

set PLUGIN_NAME=DepthONNX
set REPO_ROOT=%~dp0..
if defined CARGO_TARGET_DIR (
    set "BUILT_DIR=%CARGO_TARGET_DIR%\release"
) else (
    set "BUILT_DIR=%REPO_ROOT%\target\release"
)

if not defined DEPTH_ONNX_AE_DIR (
    if defined DEPTHANYTHING_AE_DIR (
        set "DEPTH_ONNX_AE_DIR=%DEPTHANYTHING_AE_DIR%"
    ) else (
        set "DEPTH_ONNX_AE_DIR=C:\Program Files\Adobe\Adobe After Effects 2026\Support Files\Plug-ins\Effects"
    )
)

set "INSTALL_DIR=%DEPTH_ONNX_AE_DIR%\%PLUGIN_NAME%"

if not exist "%BUILT_DIR%\%PLUGIN_NAME%.aex" (
    echo [install_dev] error: %BUILT_DIR%\%PLUGIN_NAME%.aex not found
    echo [install_dev]        build it first:
    echo   set AESDK_ROOT=C:\path\to\AfterEffectsSDK
    echo   set CARGO_TARGET_DIR=%REPO_ROOT%\target
    echo   cd crates\depth-onnx-ae
    echo   just release
    exit /b 1
)

echo [install_dev] target: "%INSTALL_DIR%"
if not exist "%INSTALL_DIR%" (
    mkdir "%INSTALL_DIR%" || (echo [install_dev] mkdir failed - run as Administrator & exit /b 1)
)

copy /Y "%BUILT_DIR%\%PLUGIN_NAME%.aex" "%INSTALL_DIR%\" || exit /b 1
for %%F in ("%BUILT_DIR%\*.dll") do copy /Y "%%F" "%INSTALL_DIR%\" >nul

if not exist "%INSTALL_DIR%\models" (
    mkdir "%INSTALL_DIR%\models" || (echo [install_dev] mkdir failed - run as Administrator & exit /b 1)
)
if exist "%REPO_ROOT%\model\depth_anything_v2_small\manifest.json" (
    if not exist "%INSTALL_DIR%\models\depth_anything_v2_small" mkdir "%INSTALL_DIR%\models\depth_anything_v2_small"
    copy /Y "%REPO_ROOT%\model\depth_anything_v2_small\manifest.json" "%INSTALL_DIR%\models\depth_anything_v2_small\" >nul
)

echo.
echo Installed to "%INSTALL_DIR%"
echo Quit After Effects if running, then relaunch and look under
echo Effects ^> Depth ^> Depth ONNX.

endlocal
