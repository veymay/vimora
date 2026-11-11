# Vimora: Windows o'rnatuvchisini Windows'da yig'ish.
# build.bat (yig'ish) va windows-release.bat (yig'ish + GitHub release'ga yuklash) shuni chaqiradi.
#
#   build.ps1            -> dist\vimora-<ver>-windows-x64-setup.exe
#   build.ps1 -Upload    -> yana mavjud v<ver> release'iga yuklaydi va SHA256SUMS'ni yangilaydi
#
# Yetishmayotgan dasturlar (Rust, C++ build tools, NSIS, gh) winget orqali o'zi o'rnatiladi.
param([switch]$Upload)
$ErrorActionPreference = "Continue"
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $root
New-Item -ItemType Directory -Force -Path "$root\dist" | Out-Null
$repo = "veymay/vimora"
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Step($t) { Write-Host ""; Write-Host "=== $t ===" -ForegroundColor Green }

# PATH'dan yoki odatiy joylardan qidiradi.
function Find-Tool($exe, $dirs) {
    $c = Get-Command $exe -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    foreach ($d in $dirs) {
        if ($d -and (Test-Path (Join-Path $d $exe))) { return (Join-Path $d $exe) }
    }
    return $null
}

function Winget-Install($id, $extra) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "winget topilmadi. '$id' ni qo'lda o'rnating va yana ishga tushiring."
    }
    $wargs = @("install", "-e", "--id", $id, "--accept-package-agreements", "--accept-source-agreements")
    if ($extra) { $wargs += $extra }
    Write-Host "O'rnatilmoqda: $id"
    & winget @wargs
}

# Asks for a tool; installs it with winget once if it is missing.
function Require($exe, $dirs, $id, $extra) {
    $p = Find-Tool $exe $dirs
    if (-not $p) { Winget-Install $id $extra; $p = Find-Tool $exe $dirs }
    if (-not $p) { throw "$exe topilmadi. Kompyuterni qayta ishga tushirib, yana urinib ko'ring." }
    Write-Host "$exe : $p"
    return $p
}

try {
    $ver = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
    if (-not $ver) { throw "Cargo.toml dan versiya o'qilmadi." }
    $exeName = "vimora-$ver-windows-x64-setup.exe"
    $exe = Join-Path $root "dist\$exeName"
    Write-Host "Vimora $ver"

    # --- Vositalar ------------------------------------------------------------------------------
    Step "Vositalar"
    $vsw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vc = $null
    if (Test-Path $vsw) {
        $vc = & $vsw -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
    if ($vc) { Write-Host "C++ build tools: bor" } else {
        Write-Host "C++ build tools o'rnatiladi (katta hajm, administrator ruxsati so'raladi)..."
        Winget-Install "Microsoft.VisualStudio.2022.BuildTools" @("--override", "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    }
    $cargo = Require "cargo.exe" @("$env:USERPROFILE\.cargo\bin") "Rustlang.Rustup" $null
    $env:Path = "$(Split-Path $cargo);$env:Path"
    if (((& rustup toolchain list 2>$null) -join " ") -notmatch "x86_64-pc-windows-msvc") {
        & rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal
        & rustup default stable-x86_64-pc-windows-msvc
    }
    $nsis = Require "makensis.exe" @("${env:ProgramFiles(x86)}\NSIS", "$env:ProgramFiles\NSIS") "NSIS.NSIS" $null

    # --- Yig'ish --------------------------------------------------------------------------------
    Step "Dastur yig'ilmoqda"
    & $cargo build --release -p vimora-server -p vimora-client
    if ($LASTEXITCODE -ne 0) { throw "cargo build xato bilan tugadi (yuqoridagi xabarga qarang)." }

    Step "O'rnatuvchi (NSIS)"
    & $nsis -V2 "-DVERSION=$ver" "-DBINDIR=..\..\target\release" "-DOUTFILE=..\..\dist\$exeName" "$root\installer\windows\vimora.nsi"
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $exe)) { throw "NSIS o'rnatuvchini yarata olmadi." }
    $hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
    [System.IO.File]::WriteAllText("$root\dist\SHA256SUMS", "$hash  $exeName`n", $utf8)
    Write-Host ("Tayyor: dist\{0}  ({1} MB)" -f $exeName, [math]::Round((Get-Item $exe).Length / 1MB, 1)) -ForegroundColor Green

    if (-not $Upload) { exit 0 }

    # --- GitHub release'ga yuklash (windows-release.bat) ----------------------------------------
    Step "GitHub release v$ver"
    $gh = Require "gh.exe" @("$env:ProgramFiles\GitHub CLI", "$env:LOCALAPPDATA\Programs\GitHub CLI") "GitHub.cli" $null
    & $gh auth status *> $null
    if ($LASTEXITCODE -ne 0) {
        & $gh auth login --hostname github.com --git-protocol https --web
        if ($LASTEXITCODE -ne 0) { throw "GitHub'ga kirib bo'lmadi." }
    }
    & $gh release view "v$ver" --repo $repo --json tagName *> $null
    if ($LASTEXITCODE -ne 0) { throw "GitHub'da v$ver release'i topilmadi (avval Ubuntu'da yarating)." }
    # Keep the release's other lines (Ubuntu packages), replace the Windows one.
    $tmp = Join-Path $env:TEMP "vimora-release-$ver"
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    & $gh release download "v$ver" --repo $repo -p SHA256SUMS -D $tmp
    $sums = Join-Path $tmp "SHA256SUMS"
    $lines = @()
    if (Test-Path $sums) { $lines = @(Get-Content $sums | Where-Object { $_ -and ($_ -notmatch "windows-x64-setup\.exe") }) }
    $lines += "$hash  $exeName"
    [System.IO.File]::WriteAllText($sums, (($lines -join "`n") + "`n"), $utf8)
    # Student computers install only a signed list (key: %USERPROFILE%\.config\vimora-release\release.key).
    & $cargo run -q -p vimora-update --example sign -- sign $sums
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$sums.sig")) { throw "SHA256SUMS imzolanmadi: release.key kaliti yo'q (Ubuntu'dagi ~/.config/vimora-release/release.key nusxasini qo'ying)." }
    & $gh release upload "v$ver" $exe $sums "$sums.sig" --repo $repo --clobber
    if ($LASTEXITCODE -ne 0) { throw "Yuklash xato bilan tugadi." }
    Write-Host "Yuklandi: https://github.com/$repo/releases/tag/v$ver" -ForegroundColor Green
    exit 0
}
catch {
    Write-Host ""
    Write-Host "XATO: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
