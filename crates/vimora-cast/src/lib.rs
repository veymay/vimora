//! Show the teacher's screen on a wireless display (Google Cast: Chromecast, many smart boards
//! and TVs), the way Chrome's "Cast…" does.
//!
//! * [`Discovery`] lists the displays on the network (mDNS `_googlecast._tcp`).
//! * [`Session`] connects to one (CASTV2 over TLS), starts the Default Media Receiver and shows
//!   the screen on it as pictures that the display fetches from a small HTTP server of ours. A
//!   picture is sent only when the screen changed.
//!
//! This is the picture mode: smooth video would need a video encoder and Cast's streaming
//! transport; slides, documents and a quiet desktop work, fast video is slower.

pub mod proto;
mod session;

pub use session::{FrameSource, Session, State};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

/// One display found on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Stable id from the display's own announcement.
    pub id: String,
    /// The name its owner gave it ("Sinf 5 doska").
    pub name: String,
    /// "Chromecast", "Google TV Streamer" …
    pub model: String,
    pub addr: Ipv4Addr,
    pub port: u16,
}

/// Keeps a live list of displays (browses in the background while it exists).
pub struct Discovery {
    devices: Arc<Mutex<HashMap<String, Device>>>,
    daemon: Option<mdns_sd::ServiceDaemon>,
}

const SERVICE: &str = "_googlecast._tcp.local.";

impl Discovery {
    pub fn start() -> Self {
        let devices: Arc<Mutex<HashMap<String, Device>>> = Arc::default();
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("cast discovery unavailable: {e}");
                return Self {
                    devices,
                    daemon: None,
                };
            }
        };
        match daemon.browse(SERVICE) {
            Ok(rx) => {
                let list = devices.clone();
                std::thread::spawn(move || {
                    while let Ok(ev) = rx.recv() {
                        match ev {
                            mdns_sd::ServiceEvent::ServiceResolved(info) => {
                                if let Some(d) = device_from(&info) {
                                    list.lock().unwrap().insert(info.get_fullname().into(), d);
                                }
                            }
                            mdns_sd::ServiceEvent::ServiceRemoved(_, full) => {
                                list.lock().unwrap().remove(&full);
                            }
                            _ => {}
                        }
                    }
                });
            }
            Err(e) => tracing::warn!("cast browse failed: {e}"),
        }
        Self {
            devices,
            daemon: Some(daemon),
        }
    }

    /// The displays seen now, sorted by name.
    pub fn devices(&self) -> Vec<Device> {
        let mut v: Vec<Device> = self.devices.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        v
    }

    /// Asks the network again (a display that was off a moment ago).
    pub fn refresh(&self) {
        if let Some(d) = &self.daemon {
            let _ = d.browse(SERVICE);
        }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        if let Some(d) = &self.daemon {
            let _ = d.shutdown();
        }
    }
}

fn device_from(info: &mdns_sd::ServiceInfo) -> Option<Device> {
    let addr = info.get_addresses().iter().find_map(|a| match a {
        IpAddr::V4(v4) => Some(*v4),
        _ => None,
    })?;
    let name = info
        .get_property_val_str("fn")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            info.get_fullname()
                .split('.')
                .next()
                .unwrap_or("Cast")
                .to_string()
        });
    Some(Device {
        id: info
            .get_property_val_str("id")
            .map(str::to_string)
            .unwrap_or_else(|| info.get_fullname().to_string()),
        name,
        model: info.get_property_val_str("md").unwrap_or("").to_string(),
        addr,
        port: info.get_port(),
    })
}
