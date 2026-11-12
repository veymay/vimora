//! The PC's technical state for the teacher's "Kompyuter holati" drawer: CPU, memory, disks,
//! battery, network and every running process (Task Manager style).
//!
//! Load figures are differences between two samples, so one [`Monitor`] lives for the whole
//! agent run; the teacher asks every couple of seconds while the drawer is open.

use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};
use vimora_core::{BatteryInfo, DiskInfo, ProcInfo, SystemInfo};

/// At most this many processes are reported (the busiest first).
const MAX_PROCS: usize = 400;

struct Monitor {
    sys: System,
    nets: Networks,
    last_net: Instant,
    warmed: bool,
}

static MON: Mutex<Option<Monitor>> = Mutex::new(None);

/// Samples the PC now (blocking: the first call waits a moment for CPU figures).
pub fn snapshot() -> SystemInfo {
    let mut guard = MON.lock().unwrap();
    let m = guard.get_or_insert_with(|| Monitor {
        sys: System::new(),
        nets: Networks::new_with_refreshed_list(),
        last_net: Instant::now(),
        warmed: false,
    });
    if !m.warmed {
        // CPU usage is the change between two refreshes.
        m.sys.refresh_cpu_usage();
        m.sys.refresh_processes(ProcessesToUpdate::All, true);
        std::thread::sleep(
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250)),
        );
        m.warmed = true;
    }
    m.sys.refresh_cpu_usage();
    m.sys.refresh_memory();
    m.sys.refresh_processes(ProcessesToUpdate::All, true);
    m.nets.refresh();
    let secs = m.last_net.elapsed().as_secs_f64().max(0.2);
    m.last_net = Instant::now();
    let (rx, tx) = m.nets.iter().fold((0u64, 0u64), |(r, t), (_, d)| {
        (r + d.received(), t + d.transmitted())
    });

    let cores = m.sys.cpus().len().max(1) as u32;
    let mut processes: Vec<ProcInfo> = m
        .sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcInfo {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            cpu: (p.cpu_usage() / cores as f32).clamp(0.0, 100.0),
            mem: p.memory(),
        })
        .collect();
    processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem.cmp(&a.mem)));
    processes.truncate(MAX_PROCS);

    let disks = Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| d.total_space() > 0 && !d.is_removable())
        .map(|d| DiskInfo {
            mount: d.mount_point().to_string_lossy().into_owned(),
            used: d.total_space().saturating_sub(d.available_space()),
            total: d.total_space(),
        })
        .filter(|d| keep_mount(&d.mount))
        .collect();

    SystemInfo {
        os: System::long_os_version().unwrap_or_default(),
        cpu_name: m
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        cores,
        cpu: m.sys.global_cpu_usage().clamp(0.0, 100.0),
        ram_used: m.sys.used_memory(),
        ram_total: m.sys.total_memory(),
        disks,
        battery: battery(),
        net_rx: (rx as f64 / secs) as u64,
        net_tx: (tx as f64 / secs) as u64,
        uptime_secs: System::uptime(),
        processes,
    }
}

/// Ends a process. Returns its name.
pub fn kill(pid: u32) -> anyhow::Result<String> {
    let mut sys = System::new();
    let spid = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[spid]), true);
    let p = sys
        .process(spid)
        .ok_or_else(|| anyhow::anyhow!("jarayon topilmadi (PID {pid})"))?;
    let name = p.name().to_string_lossy().into_owned();
    if !p.kill() {
        anyhow::bail!("{name} (PID {pid}) ni yopib bo'lmadi: huquq yetmaydi");
    }
    Ok(format!("{name} (PID {pid}) yopildi"))
}

/// Real disks only (Linux lists many pseudo mounts).
fn keep_mount(m: &str) -> bool {
    if cfg!(windows) {
        return true;
    }
    m == "/"
        || m.starts_with("/home")
        || m.starts_with("/media")
        || m.starts_with("/run/media")
        || m.starts_with("/mnt")
}

#[cfg(windows)]
fn battery() -> Option<BatteryInfo> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
        return None;
    }
    // 128 = no system battery, 255 = unknown.
    if s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(BatteryInfo {
        percent: s.BatteryLifePercent,
        charging: s.BatteryFlag & 8 != 0,
        plugged: s.ACLineStatus == 1,
        secs_left: (s.BatteryLifeTime != u32::MAX && s.ACLineStatus != 1)
            .then_some(s.BatteryLifeTime),
    })
}

#[cfg(not(windows))]
fn battery() -> Option<BatteryInfo> {
    let read = |p: &std::path::Path, f: &str| {
        std::fs::read_to_string(p.join(f))
            .ok()
            .map(|s| s.trim().to_string())
    };
    let mut bat = None;
    let mut mains = false;
    for e in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let p = e.path();
        match read(&p, "type").as_deref() {
            Some("Mains") => mains |= read(&p, "online").as_deref() == Some("1"),
            Some("Battery") if bat.is_none() && read(&p, "scope").as_deref() != Some("Device") => {
                bat = Some(p)
            }
            _ => {}
        }
    }
    let p = bat?;
    let percent: u8 = read(&p, "capacity")?.parse().ok()?;
    let status = read(&p, "status").unwrap_or_default();
    let num = |f: &str| read(&p, f).and_then(|v| v.parse::<f64>().ok());
    let secs_left = if status == "Discharging" {
        let now = num("energy_now").or_else(|| num("charge_now"));
        let rate = num("power_now").or_else(|| num("current_now"));
        now.zip(rate)
            .filter(|(_, r)| *r > 0.0)
            .map(|(n, r)| (n / r * 3600.0) as u32)
    } else {
        None
    };
    Some(BatteryInfo {
        percent: percent.min(100),
        charging: status == "Charging",
        plugged: mains || status == "Charging" || status == "Full" || status == "Not charging",
        secs_left,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore] // reads the live machine
    fn snapshot_has_data() {
        let s = super::snapshot();
        assert!(s.ram_total > 0 && !s.processes.is_empty());
        let s2 = super::snapshot();
        assert!(s2.cores > 0);
    }
}
