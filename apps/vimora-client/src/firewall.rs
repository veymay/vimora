//! Firewall rules for the access policy: internet off (the classroom network keeps working)
//! and blocked outgoing ports. Runs in the Vimora service.
//!
//! * **Windows** — Windows Defender Firewall rules named "Vimora …" (`netsh advfirewall`).
//! * **Ubuntu** — an nftables table `inet vimora`.

use anyhow::Result;

/// Public address ranges: everything except private networks, loopback and link-local, so
/// "internet off" never cuts the console off.
const PUBLIC: &str = "1.0.0.0-9.255.255.255,11.0.0.0-126.255.255.255,128.0.0.0-169.253.255.255,169.255.0.0-172.15.255.255,172.32.0.0-192.167.255.255,192.169.0.0-223.255.255.255";

pub fn apply(internet_off: bool, ports: &[u16]) -> Result<String> {
    imp::apply(internet_off, ports)?;
    let mut parts = vec![];
    if internet_off {
        parts.push("internet o'chirildi (mahalliy tarmoq ishlaydi)".to_string());
    }
    if !ports.is_empty() {
        parts.push(format!("{} ta port yopildi", ports.len()));
    }
    Ok(if parts.is_empty() {
        "Tarmoq: cheklov yo'q".into()
    } else {
        format!("Tarmoq: {}", parts.join(", "))
    })
}

fn port_list(ports: &[u16]) -> String {
    ports
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::sh;

    fn delete(name: &str) {
        let _ = sh::run(
            "netsh",
            &[
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                &format!("name={name}"),
            ],
        );
    }

    fn add(args: &[&str]) -> Result<()> {
        let mut all = vec!["advfirewall", "firewall", "add", "rule"];
        all.extend_from_slice(args);
        sh::run_ok("netsh", &all).map(|_| ())
    }

    pub fn apply(internet_off: bool, ports: &[u16]) -> Result<()> {
        delete("Vimora Internet");
        delete("Vimora Ports");
        if internet_off {
            let ip = format!("remoteip={PUBLIC}");
            add(&["name=Vimora Internet", "dir=out", "action=block", &ip])?;
        }
        if !ports.is_empty() {
            let list = format!("remoteport={}", port_list(ports));
            for proto in ["protocol=TCP", "protocol=UDP"] {
                add(&["name=Vimora Ports", "dir=out", "action=block", proto, &list])?;
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use crate::sh;

    pub fn apply(internet_off: bool, ports: &[u16]) -> Result<()> {
        let _ = sh::run("nft", &["delete", "table", "inet", "vimora"]);
        if !internet_off && ports.is_empty() {
            return Ok(());
        }
        let mut rules = String::from(
            "table inet vimora {\n  chain out {\n    type filter hook output priority 0; policy accept;\n",
        );
        if !ports.is_empty() {
            let list = port_list(ports);
            rules.push_str(&format!(
                "    tcp dport {{ {list} }} drop\n    udp dport {{ {list} }} drop\n"
            ));
        }
        if internet_off {
            let ranges = PUBLIC.replace(',', ", ");
            rules.push_str(&format!("    ip daddr {{ {ranges} }} drop\n    meta nfproto ipv6 ip6 daddr != {{ ::1, fe80::/10, fc00::/7 }} drop\n"));
        }
        rules.push_str("  }\n}\n");
        let path = std::env::temp_dir().join("vimora-nft.conf");
        std::fs::write(&path, rules)?;
        sh::run_ok("nft", &["-f", &path.to_string_lossy()])?;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }
}
