# Vimora: Windows o'rnatuvchisini yig'ib, mavjud GitHub release'iga yuklaydi.
# windows-release.bat orqali ishga tushiriladi (log ham o'sha yerda yoziladi).
# Hech narsa qayta o'rnatilmaydi: faqat YO'Q dasturlar o'rnatiladi.
$ErrorActionPreference = "Continue"
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $root
New-Item -ItemType Directory -Force -Path "$root\dist" | Out-Null

$repo = "veymay/vimora"
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Step($t) { Write-Host ""; Write-Host "=== $t ===" -ForegroundColor Cyan }

# Dasturni PATH'dan yoki odatiy o'rnatilgan joylardan qidiradi; topsa to'liq yo'lini qaytaradi.
function Find-Tool($exe, $dirs) {
    $c = Get-Command $exe -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    foreach ($d in $dirs) {
        if (-not $d) { continue }
        $p = Join-Path $d $exe
        if (Test-Path $p) { return $p }
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

try {
    # --- 1. Versiya -------------------------------------------------------------------------
    Step "Versiya"
    $ver = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
    if (-not $ver) { throw "Cargo.toml dan versiya o'qilmadi." }
    $tag = "v$ver"
    $exeName = "vimora-$ver-windows-x64-setup.exe"
    Write-Host "Versiya: $ver  (release: $tag)"

    # --- 2. Kerakli dasturlar (faqat yo'qlari o'rnatiladi) ---------------------------------------
    Step "Kerakli dasturlar"
    $ghDirs   = @("$env:ProgramFiles\GitHub CLI", "${env:ProgramFiles(x86)}\GitHub CLI", "$env:LOCALAPPDATA\Programs\GitHub CLI")
    $cargoDirs = @("$env:USERPROFILE\.cargo\bin")
    $isccDirs = @("${env:ProgramFiles(x86)}\Inno Setup 6", "$env:ProgramFiles\Inno Setup 6", "$env:LOCALAPPDATA\Programs\Inno Setup 6")

    $gh = Find-Tool "gh.exe" $ghDirs
    if (-not $gh) { Winget-Install "GitHub.cli" $null; $gh = Find-Tool "gh.exe" $ghDirs }
    if (-not $gh) { throw "GitHub CLI (gh) topilmadi. Kompyuterni qayta ishga tushirib, yana urinib ko'ring." }
    Write-Host "gh: $gh"

    $vsw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vc = $null
    if (Test-Path $vsw) {
        $vc = & $vsw -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
    if ($vc) {
        Write-Host "C++ build tools: bor"
    } else {
        Write-Host "C++ build tools yo'q, o'rnatiladi (katta hajm, administrator ruxsati so'raladi)..."
        Winget-Install "Microsoft.VisualStudio.2022.BuildTools" @("--override", "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    }

    $cargo = Find-Tool "cargo.exe" $cargoDirs
    if (-not $cargo) { Winget-Install "Rustlang.Rustup" $null; $cargo = Find-Tool "cargo.exe" $cargoDirs }
    if (-not $cargo) { throw "Rust (cargo) topilmadi. Kompyuterni qayta ishga tushirib, yana urinib ko'ring." }
    $env:Path = "$(Split-Path $cargo);$env:Path"
    Write-Host "cargo: $cargo"
    # Toolchain faqat yo'q bo'lsa yuklanadi (rustup default har safar yangilanish tekshirib yuklaydi).
    $tc = (& rustup toolchain list 2>$null) -join " "
    if ($tc -notmatch "x86_64-pc-windows-msvc") {
        Write-Host "Rust toolchain yuklanmoqda..."
        & rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal
        & rustup default stable-x86_64-pc-windows-msvc
    }

    $iscc = Find-Tool "ISCC.exe" $isccDirs
    if (-not $iscc) { Winget-Install "JRSoftware.InnoSetup" $null; $iscc = Find-Tool "ISCC.exe" $isccDirs }
    if (-not $iscc) { throw "Inno Setup (ISCC.exe) topilmadi." }
    Write-Host "iscc: $iscc"

    # --- 3. GitHub'ga kirish -------------------------------------------------------------------
    Step "GitHub akkaunti"
    & $gh auth status *> $null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "GitHub'ga kirish kerak: brauzer ochiladi, kodni kiriting va ruxsat bering."
        & $gh auth login --hostname github.com --git-protocol https --web
        if ($LASTEXITCODE -ne 0) { throw "GitHub'ga kirib bo'lmadi." }
    } else {
        Write-Host "GitHub: kirilgan"
    }
    & $gh release view $tag --repo $repo --json tagName *> $null
    if ($LASTEXITCODE -ne 0) { throw "GitHub'da $tag release'i topilmadi." }

    # --- 4. Yig'ish ----------------------------------------------------------------------------
    Step "Dastur yig'ilmoqda (cargo build --release)"
    & $cargo build --release -p vimora-server -p vimora-client
    if ($LASTEXITCODE -ne 0) { throw "cargo build xato bilan tugadi (yuqoridagi xabarga qarang)." }

    Step "O'rnatuvchi yaratilmoqda (Inno Setup)"
    & $iscc "/DAppVer=$ver" "$root\installer\windows\vimora.iss"
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
    & $gh release download $tag --repo $repo -p SHA256SUMS -D $tmp
    if ($LASTEXITCODE -ne 0) { throw "SHA256SUMS yuklab olinmadi." }
    $sumsPath = Join-Path $tmp "SHA256SUMS"
    $lines = @(Get-Content $sumsPath | Where-Object { $_ -and ($_ -notmatch "windows-x64-setup\.exe") })
    $lines += "$hash  $exeName"
    [System.IO.File]::WriteAllText($sumsPath, (($lines -join "`n") + "`n"), $utf8)
    Get-Content $sumsPath

    # --- 6. Yuklash ----------------------------------------------------------------------------
    Step "GitHub release'ga yuklanmoqda"
    & $gh release upload $tag $exe $sumsPath --repo $repo --clobber
    if ($LASTEXITCODE -ne 0) { throw "Yuklash xato bilan tugadi." }

    # --- 7. Release izohini yangilash ----------------------------------------------------------
    Step "Release izohi yangilanmoqda"
    $body = (& $gh release view $tag --repo $repo --json body -q .body) -join "`n"
    $news = ""
    $i = $body.IndexOf("### Yangiliklar")
    if ($i -ge 0) { $news = $body.Substring($i) }
    $nl = "`n"
    $notes = "### Yuklab olish" + $nl + $nl +
        "| Kompyuter | Windows | Ubuntu |" + $nl +
        "|---|---|---|" + $nl +
        "| O'qituvchi | ``$exeName`` -> **O'qituvchi** | ``vimora-teacher_${ver}_amd64.deb`` |" + $nl +
        "| O'quvchi | ``$exeName`` -> **O'quvchi** | ``vimora-student_${ver}_amd64.deb`` |" + $nl + $nl +
        "Ubuntu: ``sudo apt install ./vimora-student_${ver}_amd64.deb``" + $nl + $nl +
        "O'rnatilgan Vimora keyingi versiyalarni shu sahifadan o'zi yuklab oladi (``SHA256SUMS`` bilan tekshiriladi)." + $nl + $nl +
        $news + $nl
    $notesFile = Join-Path $tmp "notes.md"
    [System.IO.File]::WriteAllText($notesFile, $notes, $utf8)
    & $gh release edit $tag --repo $repo --notes-file $notesFile
    if ($LASTEXITCODE -ne 0) { throw "Izohni yangilab bo'lmadi." }

    Write-Host ""
    Write-Host "TAYYOR. Release: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
    exit 0
}
catch {
    Write-Host ""
    Write-Host "XATO: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
