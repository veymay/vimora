@echo off
rem Vimora: Windows o'rnatuvchisini yig'ib, GitHub release'iga yuklaydi.
rem Ustiga ikki marta bosing. Log: dist\windows-release.log
setlocal
cd /d "%~dp0"
title Vimora - Windows release
set "PS1=%~dp0installer\windows\release-local.ps1"
set "LOG=%~dp0dist\windows-release.log"
if not exist "%PS1%" (
  echo XATO: %PS1% topilmadi.
  echo Bu fayl loyiha papkasining ichida turishi kerak. Boshqa joyga nusxalamang.
  pause
  exit /b 1
)
if not exist "%~dp0dist" mkdir "%~dp0dist"
echo.
echo  Vimora Windows release yigilmoqda. Bu 5-20 daqiqa olishi mumkin.
echo  Log: %LOG%
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { & '%PS1%' 2>&1 | Tee-Object -FilePath '%LOG%'; exit $LASTEXITCODE }"
set RC=%ERRORLEVEL%
echo.
if "%RC%"=="0" (
  echo  --- HAMMASI TAYYOR ---
) else (
  echo  --- XATO BOLDI. Log faylini Claudega korsating: %LOG% ---
)
echo.
pause
exit /b %RC%
