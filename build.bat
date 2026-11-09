@echo off
rem Vimora: Windows o'rnatuvchisini yig'ish. Ustiga ikki marta bosing.
rem Natija: dist\vimora-<versiya>-windows-x64-setup.exe   Log: dist\build.log
rem   build.bat upload  -> yana GitHub release'iga yuklaydi
setlocal
cd /d "%~dp0"
title Vimora - Windows build
if not exist "%~dp0dist" mkdir "%~dp0dist"
set "UP="
if /i "%~1"=="upload" set "UP=-Upload"
echo.
echo  Vimora yigilmoqda. Birinchi marta 10-30 daqiqa, keyin 2-5 daqiqa.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { & '%~dp0installer\windows\build.ps1' %UP% 2>&1 | Tee-Object -FilePath '%~dp0dist\build.log'; exit $LASTEXITCODE }"
set RC=%ERRORLEVEL%
echo.
if "%RC%"=="0" (echo  --- TAYYOR: dist papkasini oching ---) else (echo  --- XATO. Log: dist\build.log ---)
echo.
pause
exit /b %RC%
