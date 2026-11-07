@echo off
rem Vimora: Windows o'rnatuvchisini yig'ib, GitHub release'iga yuklaydi.
rem Shunchaki ustiga ikki marta bosing. Hammasini o'zi qiladi.
setlocal
cd /d "%~dp0"
title Vimora - Windows release
echo.
echo  Vimora Windows release yig'ilmoqda. Bu 5-20 daqiqa olishi mumkin.
echo  (Birinchi marta kerakli dasturlar o'rnatiladi; savol chiqsa "Ha" deng.)
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0installer\windows\release-local.ps1"
set RC=%ERRORLEVEL%
echo.
if "%RC%"=="0" (
  echo  --- HAMMASI TAYYOR ---
) else (
  echo  --- XATO BO'LDI. dist\windows-release.log faylini Claude'ga ko'rsating ---
)
echo.
pause
exit /b %RC%
