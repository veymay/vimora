//! Device control: USB drives (allow / read-only / block), CD/DVD, printer, camera, microphone
//! and Bluetooth. Runs in the Vimora service (SYSTEM / root); the settings stay in force with
//! no console connected and survive restarts (and Muzlatish: everything written is reported in
//! [`touched`] so it is committed through the write filter).
//!
//! Sound is muted by the agent in the user's session (see `agent::tasks`), not here.
//!
//! * **Windows** — "Removable Storage Access" policies (USB, CD/DVD), the Print Spooler and
//!   Bluetooth services, the camera policy and the privacy switches for camera / microphone.
//! * **Ubuntu** — kernel modules kept from loading (`/etc/modprobe.d`), a udev rule that makes
//!   USB disks read-only, CUPS and the Bluetooth service, `rfkill`.

use anyhow::Result;
use vimora_core::{DevicePolicy, UsbMode};

/// Applies the policy; returns a summary for the teacher.
pub fn apply(p: &DevicePolicy) -> Result<String> {
    imp::apply(p)?;
    let mut off = vec![];
    match p.usb {
        UsbMode::Allow => {}
        UsbMode::ReadOnly => off.push("USB (faqat o'qish)"),
        UsbMode::Block => off.push("USB"),
    }
    for (on, name) in [
        (p.cd, "CD/DVD"),
        (p.printer, "printer"),
        (p.camera, "kamera"),
        (p.microphone, "mikrofon"),
        (p.bluetooth, "Bluetooth"),
        (p.sound_off, "ovoz"),
    ] {
        if on {
            off.push(name);
        }
    }
    Ok(if off.is_empty() {
        "Qurilmalar: hammasiga ruxsat".into()
    } else {
        format!("Qurilmalar cheklandi: {}", off.join(", "))
    })
}

/// What [`apply`] writes (for commits while the PC is frozen).
pub fn touched() -> crate::sh::Touched {
    imp::touched()
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::sh;

    /// Removable Storage Access policy classes.
    const REMOVABLE: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\RemovableStorageDevices\{53f5630d-b6bf-11d0-94f2-00a0c91efb8b}";
    const CDROM: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\RemovableStorageDevices\{53f56308-b6bf-11d0-94f2-00a0c91efb8b}";
    const CAMERA_POLICY: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Camera";
    const CONSENT: &str =
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";

    fn reg_dword(key: &str, name: &str, v: u32) -> Result<()> {
        sh::run_ok(
            "reg",
            &[
                "add",
                key,
                "/v",
                name,
                "/t",
                "REG_DWORD",
                "/d",
                &v.to_string(),
                "/f",
            ],
        )
        .map(|_| ())
    }

    fn reg_sz(key: &str, name: &str, v: &str) -> Result<()> {
        sh::run_ok(
            "reg",
            &["add", key, "/v", name, "/t", "REG_SZ", "/d", v, "/f"],
        )
        .map(|_| ())
    }

    /// Starts or stops a service and sets how it starts.
    fn service(name: &str, on: bool) {
        let mode = if on { "auto" } else { "disabled" };
        let _ = sh::run("sc.exe", &["config", name, "start=", mode]);
        let _ = sh::run("sc.exe", &[if on { "start" } else { "stop" }, name]);
    }

    pub fn apply(p: &DevicePolicy) -> Result<()> {
        let (read, write) = match p.usb {
            UsbMode::Allow => (0, 0),
            UsbMode::ReadOnly => (0, 1),
            UsbMode::Block => (1, 1),
        };
        reg_dword(REMOVABLE, "Deny_Read", read)?;
        reg_dword(REMOVABLE, "Deny_Write", write)?;
        // Block also keeps the USB storage driver from starting.
        reg_dword(
            r"HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR",
            "Start",
            if p.usb == UsbMode::Block { 4 } else { 3 },
        )?;
        reg_dword(CDROM, "Deny_Read", p.cd as u32)?;
        reg_dword(CDROM, "Deny_Write", p.cd as u32)?;
        reg_dword(CAMERA_POLICY, "AllowCamera", (!p.camera) as u32)?;
        let consent = |on: bool| if on { "Allow" } else { "Deny" };
        reg_sz(&format!(r"{CONSENT}\webcam"), "Value", consent(!p.camera))?;
        reg_sz(
            &format!(r"{CONSENT}\microphone"),
            "Value",
            consent(!p.microphone),
        )?;
        service("Spooler", !p.printer);
        service("bthserv", !p.bluetooth);
        Ok(())
    }

    pub fn touched() -> sh::Touched {
        let mut registry = vec![];
        for k in [REMOVABLE, CDROM] {
            for v in ["Deny_Read", "Deny_Write"] {
                registry.push((k.to_string(), v.to_string()));
            }
        }
        registry.push((CAMERA_POLICY.into(), "AllowCamera".into()));
        registry.push((
            r"HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR".into(),
            "Start".into(),
        ));
        for svc in ["Spooler", "bthserv"] {
            registry.push((
                format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{svc}"),
                "Start".into(),
            ));
        }
        for c in ["webcam", "microphone"] {
            registry.push((format!(r"{CONSENT}\{c}"), "Value".into()));
        }
        sh::Touched {
            files: vec![],
            registry,
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use crate::sh;
    use std::path::{Path, PathBuf};

    const MODPROBE: &str = "/etc/modprobe.d/vimora-devices.conf";
    const UDEV: &str = "/etc/udev/rules.d/80-vimora-usb-readonly.rules";

    fn write_or_remove(path: &str, text: Option<String>) -> Result<()> {
        match text {
            Some(t) => std::fs::write(path, t)?,
            None => {
                if Path::new(path).exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }
        Ok(())
    }

    fn systemd(unit: &str, on: bool) {
        let _ = sh::run(
            "systemctl",
            &[if on { "enable" } else { "disable" }, "--now", unit],
        );
    }

    pub fn apply(p: &DevicePolicy) -> Result<()> {
        // Modules that must not load (and are unloaded now when possible).
        let mut blocked: Vec<&str> = vec![];
        if p.usb == UsbMode::Block {
            blocked.extend(["usb_storage", "uas"]);
        }
        if p.cd {
            blocked.push("sr_mod");
        }
        if p.camera {
            blocked.push("uvcvideo");
        }
        let conf = (!blocked.is_empty()).then(|| {
            let mut t = String::from("# Vimora: devices blocked by the teacher\n");
            for m in &blocked {
                t.push_str(&format!("install {m} /bin/false\nblacklist {m}\n"));
            }
            t
        });
        write_or_remove(MODPROBE, conf)?;
        for m in &blocked {
            let _ = sh::run("modprobe", &["-r", m]);
        }
        let ro = (p.usb == UsbMode::ReadOnly).then(|| {
            "# Vimora: USB disks are read-only\n\
             ACTION==\"add|change\", SUBSYSTEM==\"block\", ENV{ID_BUS}==\"usb\", RUN+=\"/sbin/blockdev --setro /dev/%k\"\n"
                .to_string()
        });
        write_or_remove(UDEV, ro)?;
        let _ = sh::run("udevadm", &["control", "--reload"]);
        systemd("cups.service", !p.printer);
        systemd("bluetooth.service", !p.bluetooth);
        let _ = sh::run(
            "rfkill",
            &[if p.bluetooth { "block" } else { "unblock" }, "bluetooth"],
        );
        Ok(())
    }

    pub fn touched() -> sh::Touched {
        sh::Touched {
            files: [MODPROBE, UDEV].iter().map(PathBuf::from).collect(),
            registry: vec![],
        }
    }
}
