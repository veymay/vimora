//! Security and inventory facts for the console's "Kompyuter holati" drawer: antivirus,
//! firewall, UAC, hardware, signed-in users and system services. Collected on request (it
//! takes a second or two).

use vimora_core::{SecurityInfo, ServiceInfo};

#[cfg(windows)]
pub fn collect() -> SecurityInfo {
    const SCRIPT: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$av = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | % { @{n=$_.displayName; s=[int]$_.productState} })
$fw = @(Get-NetFirewallProfile | % { @{n=[string]$_.Name; on=[bool]$_.Enabled} })
$uac = (Get-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System).EnableLUA
$cs = Get-CimInstance Win32_ComputerSystem
$bios = Get-CimInstance Win32_BIOS
$gpu = @(Get-CimInstance Win32_VideoController | % { $_.Name })
$mon = @(Get-CimInstance -Namespace root/wmi WmiMonitorID | % { ($_.UserFriendlyName | ? {$_ -ne 0} | % {[char]$_}) -join '' })
$users = @(Get-CimInstance Win32_Process -Filter "Name='explorer.exe'" | % { $o = Invoke-CimMethod -InputObject $_ -MethodName GetOwner; "$($o.Domain)\$($o.User)" } | Sort-Object -Unique)
$svc = @(Get-Service | % { @{n=$_.Name; d=$_.DisplayName; r=($_.Status -eq 'Running'); s=[string]$_.StartType} })
@{av=$av; fw=$fw; uac=$uac; maker=$cs.Manufacturer; model=$cs.Model; serial=$bios.SerialNumber; gpu=$gpu; mon=$mon; users=$users; svc=$svc} | ConvertTo-Json -Depth 4 -Compress
"#;
    let mut info = SecurityInfo::default();
    let Ok((_, out)) = crate::sh::script(SCRIPT) else {
        return info;
    };
    let Some(v) = out
        .lines()
        .find(|l| l.trim_start().starts_with('{'))
        .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    else {
        return info;
    };
    let s = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let arr = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default()
    };
    for a in arr("av") {
        let state = a.get("s").and_then(|x| x.as_u64()).unwrap_or(0);
        info.antivirus.push((
            a.get("n")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            (state >> 12) & 0xF == 1,
            (state >> 4) & 0xF == 0,
        ));
    }
    for f in arr("fw") {
        info.firewall.push((
            f.get("n")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            f.get("on").and_then(|x| x.as_bool()).unwrap_or(false),
        ));
    }
    info.uac = v.get("uac").and_then(|x| x.as_u64()).map(|u| u == 1);
    for (k, label) in [
        ("maker", "Ishlab chiqaruvchi"),
        ("model", "Model"),
        ("serial", "Seriya raqami"),
    ] {
        let val = s(k);
        if !val.is_empty() {
            info.hardware.push((label.into(), val));
        }
    }
    let join = |k: &str| {
        arr(k)
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .filter(|x| !x.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };
    for (k, label) in [("gpu", "Videokarta"), ("mon", "Monitorlar")] {
        let val = join(k);
        if !val.is_empty() {
            info.hardware.push((label.into(), val));
        }
    }
    info.sessions = arr("users")
        .iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect();
    for sv in arr("svc") {
        let g = |k: &str| sv.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        info.services.push(ServiceInfo {
            name: g("n"),
            display: g("d"),
            running: sv.get("r").and_then(|x| x.as_bool()).unwrap_or(false),
            start: g("s"),
        });
    }
    info
}

#[cfg(not(windows))]
pub fn collect() -> SecurityInfo {
    let run = |p: &str, a: &[&str]| crate::sh::run(p, a).map(|(_, o)| o).unwrap_or_default();
    let mut info = SecurityInfo::default();
    let ufw = run("ufw", &["status"]);
    if !ufw.trim().is_empty() {
        info.firewall
            .push(("ufw".into(), ufw.to_lowercase().contains("status: active")));
    }
    for unit in ["clamav-daemon", "clamav-freshclam"] {
        let st = run("systemctl", &["is-active", unit]);
        if !st.contains("inactive") && !st.trim().is_empty() && !st.contains("could not") {
            info.antivirus
                .push((unit.into(), st.trim() == "active", unit.contains("fresh")));
        }
    }
    let dmi = |f: &str| {
        std::fs::read_to_string(format!("/sys/class/dmi/id/{f}"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    for (f, label) in [
        ("sys_vendor", "Ishlab chiqaruvchi"),
        ("product_name", "Model"),
        ("product_serial", "Seriya raqami"),
    ] {
        let v = dmi(f);
        if !v.is_empty() {
            info.hardware.push((label.into(), v));
        }
    }
    let gpu: Vec<String> = run("lspci", &[])
        .lines()
        .filter(|l| l.contains("VGA") || l.contains("3D controller"))
        .map(|l| l.split(": ").nth(1).unwrap_or(l).to_string())
        .collect();
    if !gpu.is_empty() {
        info.hardware.push(("Videokarta".into(), gpu.join(", ")));
    }
    let mut users: Vec<String> = run("who", &[])
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    users.sort();
    users.dedup();
    info.sessions = users;
    for l in run(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--plain",
            "--no-pager",
        ],
    )
    .lines()
    {
        let mut it = l.split_whitespace();
        let (Some(name), Some(_load), Some(active), Some(_sub)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let display: String = it.collect::<Vec<_>>().join(" ");
        info.services.push(ServiceInfo {
            name: name.trim_end_matches(".service").to_string(),
            display,
            running: active == "active",
            start: String::new(),
        });
    }
    info
}

/// Service: start or stop a system service.
pub fn service_control(name: &str, start: bool) -> anyhow::Result<String> {
    if name.is_empty() || name.contains([';', '&', '|', '"', '\'']) {
        anyhow::bail!("noto'g'ri xizmat nomi");
    }
    #[cfg(windows)]
    let r = crate::sh::run_ok("sc.exe", &[if start { "start" } else { "stop" }, name]);
    #[cfg(not(windows))]
    let r = crate::sh::run_ok("systemctl", &[if start { "start" } else { "stop" }, name]);
    r.map(|_| {
        format!(
            "{name}: {}",
            if start {
                "ishga tushirildi"
            } else {
                "to'xtatildi"
            }
        )
    })
}

/// Service: runs the teacher's command as administrator; returns its output (shortened).
pub fn run_script(code: &str) -> anyhow::Result<String> {
    let (ok, out) = crate::sh::script(code)?;
    let mut text: String = out.trim().chars().take(8000).collect();
    if text.is_empty() {
        text = if ok {
            "(bajarildi, natija bo'sh)".into()
        } else {
            "(xato)".into()
        };
    }
    if ok {
        Ok(text)
    } else {
        anyhow::bail!(text)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore] // reads the live machine
    fn collects_something() {
        let s = super::collect();
        eprintln!(
            "fw={:?} av={:?} hw={:?} users={:?} services={}",
            s.firewall,
            s.antivirus,
            s.hardware,
            s.sessions,
            s.services.len()
        );
        assert!(!s.services.is_empty());
    }
}
