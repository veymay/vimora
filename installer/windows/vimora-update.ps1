# Vimora standalone updater (student computers).
#
# Runs as SYSTEM from the "Vimora Updater" tasks (5 minutes after start-up and every 6 hours).
# It does not depend on the Vimora program: even if a Vimora version could not update itself,
# this script still brings it to the newest release.
#
# Newest release from GitHub (the releases/latest redirect, no API limit) -> compare with the
# installed version -> download installer + SHA256SUMS -> verify -> silent install.
# Respects the teacher's "automatic update off". Log: ProgramData\Vimora\updates\updater.log

$ErrorActionPreference = 'Stop'
$repo = 'veymay/vimora'
$data = Join-Path $env:ProgramData 'Vimora'
$dir = Join-Path $data 'updates'
$log = Join-Path $dir 'updater.log'

function Log($text) {
    try {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $line = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss') + '  ' + $text
        Add-Content -Path $log -Value $line
        # Keep the log small.
        $lines = Get-Content -Path $log
        if ($lines.Count -gt 300) { $lines[-200..-1] | Set-Content -Path $log }
    } catch {}
}

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    # The teacher turned automatic updates off.
    $shield = Join-Path $data 'shield.json'
    if (Test-Path $shield) {
        $s = Get-Content -Raw -Path $shield | ConvertFrom-Json
        if ($s.vimora_manual -eq $true) { exit 0 }
    }

    $installed = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Vimora' -ErrorAction SilentlyContinue).DisplayVersion
    if (-not $installed) { $installed = '0.0.0' }

    # The tag of the newest release, from the redirect of releases/latest.
    $req = [System.Net.HttpWebRequest]::Create("https://github.com/$repo/releases/latest")
    $req.AllowAutoRedirect = $false
    $req.UserAgent = 'vimora-updater'
    $resp = $req.GetResponse()
    $location = $resp.Headers['Location']
    $resp.Close()
    if (-not $location) { Log 'no release found'; exit 0 }
    $tag = $location.Split('/')[-1]
    $latest = $tag.TrimStart('v')

    if ([version]$latest -le [version]$installed) { exit 0 }

    # Vimora's own updater started an install a moment ago: let it finish.
    $pending = Join-Path $dir 'pending.json'
    if ((Test-Path $pending) -and ((Get-Date) - (Get-Item $pending).LastWriteTime).TotalMinutes -lt 20) {
        $p = Get-Content -Raw -Path $pending | ConvertFrom-Json
        if ($p.version -eq $latest) { exit 0 }
    }

    Log "installed $installed, newest $latest: updating"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $name = "vimora-$latest-windows-x64-setup.exe"
    $base = "https://github.com/$repo/releases/download/$tag"
    $sums = Join-Path $dir 'SHA256SUMS.updater'
    Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sums -UseBasicParsing
    $want = $null
    foreach ($l in Get-Content -Path $sums) {
        $parts = $l -split '\s+'
        if ($parts.Count -ge 2 -and $parts[1].TrimStart('*') -eq $name) { $want = $parts[0] }
    }
    if (-not $want) { Log "SHA256SUMS has no $name"; exit 1 }

    $file = Join-Path $dir ("updater-" + $name)
    Invoke-WebRequest -Uri "$base/$name" -OutFile $file -UseBasicParsing
    $got = (Get-FileHash -Path $file -Algorithm SHA256).Hash
    if ($got -ne $want) {
        Log 'downloaded file is damaged (SHA-256 mismatch)'
        Remove-Item -Force $file
        exit 1
    }

    $proc = Start-Process -FilePath $file -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent' -Wait -PassThru
    Log ("installer finished with code " + $proc.ExitCode)
    Remove-Item -Force $file -ErrorAction SilentlyContinue
} catch {
    Log ("error: " + $_.Exception.Message)
    exit 1
}
