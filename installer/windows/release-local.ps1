# Vimora: Windows o'rnatuvchisini yig'ib, mavjud GitHub release'iga yuklaydi.
# Odatda windows-release.bat orqali ishga tushiriladi (ustiga ikki marta bosish).
# Native dasturlar (rustup, gh) stderr'ga yozadi; xatolarni chiqish kodi bo'yicha tekshiramiz.
$ErrorActionPreference = "Continue"
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root
New-Item -ItemType Directory -Force -Path dist | Out-Null
Start-Transcript -Path "$root\dist\windows-release.log" -Force | Out-Null

$repo = "veymay/vimora"
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Step($t) { Write-Host ""; Write-Host "=== $t ===" -ForegroundColor Cyan }
function Has($name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }
function Add-Path($dir) { if ((Test-Path $dir) -and ($env:Path -notlike "*$dir*")) { $env:Path = "$dir;$env:Path" } }
function Refresh-Tools {
    Add-Path "$env:USERPROFILE\.cargo\bin"
    Add-Path "${env:ProgramFiles(x86)}\Inno Setup 6"
    Add-Path "$env:ProgramFiles\Inno Setup 6"
    Add-Path "$env:LOCALAPPDATA\Programs\Inno Setup 6"
    Add-Path "$env:ProgramFiles\GitHub CLI"
}
function Winget-Install($id, $extra) {
    if (-not (Has "winget")) { throw "winget topilmadi. Ushbu dasturni qo'lda o'rnating: $id" }
    $wargs = @("install", "-e", "--id", $id, "--accept-package-agreements", "--accept-source-agreements")
    if ($extra) { $wargs += $extra }
    & winget @wargs
    Refresh-Tools
}

try {
    # --- 1. Versiya -------------------------------------------------------------------------
    Step "Versiya"
    $ver = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
    $tag = "v$ver"
    Write-Host "Versiya: $ver  (release: $tag)"
    $exeName = "vimora-$ver-windows-x64-setup.exe"

    # --- 2. Kerakli dasturlar ----------------------------------------------------------------
    Step "Kerakli dasturlar"
    Refresh-Tools
    if (-not (Has "gh")) { Write-Host "GitHub CLI o'rnatilmoqda..."; Winget-Install "GitHub.cli" $null }
    if (-not (Has "gh")) { throw "GitHub CLI (gh) topilmadi." }

    $vsw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $hasVc = (Test-Path $vsw) -and (& $vsw -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if (-not $hasVc) {
        Write-Host "Visual Studio Build Tools (C++) o'rnatilmoqda (bir necha daqiqa, katta hajm)..."
        Winget-Install "Microsoft.VisualStudio.2022.BuildTools" @("--override", "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    }
    if (-not (Has "cargo")) { Write-Host "Rust o'rnatilmoqda..."; Winget-Install "Rustlang.Rustup" $null }
    if (-not (Has "cargo")) { throw "Rust (cargo) topilmadi." }
    & rustup default stable-msvc 2>&1 | Out-Host

    if (-not (Has "iscc.exe")) { Write-Host "Inno Setup o'rnatilmoqda..."; Winget-Install "JRSoftware.InnoSetup" $null }
    if (-not (Has "iscc.exe")) { throw "Inno Setup (iscc.exe) topilmadi." }

    # --- 3. GitHub'ga kirish -------------------------------------------------------------------
    Step "GitHub akkaunti"
    & gh auth status 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "GitHub'ga kirish kerak: brauzer ochiladi, kodni kiriting va ruxsat bering."
        & gh auth login --hostname github.com --git-protocol https --web
        if ($LASTEXITCODE -ne 0) { throw "GitHub'ga kirib bo'lmadi." }
    }
    & gh release view $tag --repo $repo --json tagName 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "GitHub'da $tag release'i topilmadi. Avval Linux release'ini yarating." }

    # --- 4. Yig'ish ----------------------------------------------------------------------------
    Step "Dastur yig'ilmoqda (cargo build --release)"
    & cargo build --release -p vimora-server -p vimora-client
    if ($LASTEXITCODE -ne 0) { throw "cargo build xato bilan tugadi." }

    Step "O'rnatuvchi yaratilmoqda (Inno Setup)"
    & iscc "/DAppVer=$ver" "installer\windows\vimora.iss"
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup xato bilan tugadi." }
    $exe = Join-Path $root "dist\$exeName"
    if (-not (Test-Path $exe)) { throw "Kutilgan fayl yaratilmadi: $exe" }
    $size = [math]::Round((Get-Item $exe).Length / 1MB, 1)
    $hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
    Write-Host "Fayl: $exeName  ($size MB)"
    Write-Host "SHA256: $hash"

    # --- 5. SHA256SUMS yangilash ---------------------------------------------------------------
    Step "SHA256SUMS yangilanmoqda"
    $tmp = Join-Path $env:TEMP "vimora-release-$ver"
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    & gh release download $tag --repo $repo -p SHA256SUMS -D $tmp
    if ($LASTEXITCODE -ne 0) { throw "SHA256SUMS yuklab olinmadi." }
    $sumsPath = Join-Path $tmp "SHA256SUMS"
    $lines = @(Get-Content $sumsPath | Where-Object { $_ -and ($_ -notmatch "windows-x64-setup\.exe") })
    $lines += "$hash  $exeName"
    [System.IO.File]::WriteAllText($sumsPath, (($lines -join "`n") + "`n"), $utf8)
    Get-Content $sumsPath | Out-Host

    # --- 6. Yuklash ----------------------------------------------------------------------------
    Step "GitHub release'ga yuklanmoqda"
    & gh release upload $tag $exe $sumsPath --repo $repo --clobber
    if ($LASTEXITCODE -ne 0) { throw "Yuklash xato bilan tugadi." }

    # --- 7. Release izohini yangilash ----------------------------------------------------------
    Step "Release izohi yangilanmoqda"
    $body = (& gh release view $tag --repo $repo --json body -q .body) -join "`n"
    $news = ""
    $i = $body.IndexOf("### Yangiliklar")
    if ($i -ge 0) { $news = "`n" + $body.Substring($i) }
    $notes = @"
### Yuklab olish

| Kompyuter | Windows | Ubuntu |
|---|---|---|
| O'qituvchi | ``$exeName`` → **O'qituvchi** | ``vimora-teacher_${ver}_amd64.deb`` |
| O'quvchi | ``$exeName`` → **O'quvchi** | ``vimora-student_${ver}_amd64.deb`` |

Ubuntu: ``sudo apt install ./vimora-student_${ver}_amd64.deb``

O'rnatilgan Vimora keyingi versiyalarni shu sahifadan o'zi yuklab oladi (``SHA256SUMS`` bilan tekshiriladi).
$news
"@
    $notesFile = Join-Path $tmp "notes.md"
    [System.IO.File]::WriteAllText($notesFile, $notes, $utf8)
    & gh release edit $tag --repo $repo --notes-file $notesFile
    if ($LASTEXITCODE -ne 0) { throw "Izohni yangilab bo'lmadi." }

    Write-Host ""
    Write-Host "TAYYOR. Release: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
    $code = 0
}
catch {
    Write-Host ""
    Write-Host "XATO: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "Batafsil: dist\windows-release.log (uni Claude'ga ko'rsating)."
    $code = 1
}
Stop-Transcript | Out-Null
exit $code
