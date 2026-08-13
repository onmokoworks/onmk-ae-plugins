@echo off
set "DEST=C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
set "SRC=%~dp0target\release"

if not exist "%DEST%" mkdir "%DEST%"
copy /Y "%SRC%\DepthONNX.aex" "%DEST%\"
for %%F in ("%SRC%\*.dll") do copy /Y "%%F" "%DEST%\" >nul

echo.
echo Installed to: %DEST%
dir "%DEST%"
pause
