# Builds Vimora for Windows and produces dist\vimora-<ver>-windows-x64-setup.exe
# Requirements: Rust (rustup, MSVC toolchain) and Inno Setup 6 (iscc.exe in PATH or default location).
$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root

cargo build --release -p vimora-server -p vimora-client

$iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path $iscc)) { throw "Inno Setup 6 topilmadi: https://jrsoftware.org/isdl.php" }

& $iscc "installer\windows\vimora.iss"
Get-ChildItem dist\*.exe
