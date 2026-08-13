@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\deploy_ae_plugins.ps1" -DeployTarget SystemMediaCore
:done
pause
