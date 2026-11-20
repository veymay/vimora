//! Wi-Fi Display (Miracast) information elements, as a receiver advertises them in Wi-Fi
//! Direct discovery (`wfd_subelems`).

use anyhow::{bail, Result};

/// WFD Device Information subelement (id 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub role: Role,
    /// The receiver accepts a session right now.
    pub session_available: bool,
    /// RTSP control port the source must connect to (7236 is the usual one).
    pub control_port: u16,
    /// Maximum throughput the receiver claims, Mbit/s.
    pub max_throughput_mbps: u16,
    pub coupled_sink_support: bool,
    pub service_discovery: bool,
    pub preferred_connectivity_tdls: bool,
    pub content_protection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    PrimarySink,
    SecondarySink,
    DualRole,
}

/// Decodes the hex subelement string (`"00000600111c440032"`: id, length, 6 bytes of data).
pub fn parse_device_info(hex: &str) -> Result<DeviceInfo> {
    let hex = hex.trim();
    if !hex.is_ascii() || !hex.len().is_multiple_of(2) {
        bail!("not a hex string");
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<_, _>>()?;
    // A list of subelements: id (1 byte), length (2 bytes, big endian), data.
    let mut b = &bytes[..];
    while b.len() >= 3 {
        let (id, len) = (b[0], u16::from_be_bytes([b[1], b[2]]) as usize);
        if b.len() < 3 + len {
            bail!("truncated subelement");
        }
        let data = &b[3..3 + len];
        if id == 0 {
            if len != 6 {
                bail!("device information must be 6 bytes, got {len}");
            }
            let info = u16::from_be_bytes([data[0], data[1]]);
            return Ok(DeviceInfo {
                role: match info & 0x3 {
                    0 => Role::Source,
                    1 => Role::PrimarySink,
                    2 => Role::SecondarySink,
                    _ => Role::DualRole,
                },
                session_available: (info >> 4) & 0x3 == 1,
                control_port: u16::from_be_bytes([data[2], data[3]]),
                max_throughput_mbps: u16::from_be_bytes([data[4], data[5]]),
                coupled_sink_support: info & 0x4 != 0 || info & 0x8 != 0,
                service_discovery: info & 0x40 != 0,
                preferred_connectivity_tdls: info & 0x80 != 0,
                content_protection: info & 0x100 != 0,
            });
        }
        b = &b[3 + len..];
    }
    bail!("no device information subelement")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the developer's Hisense board advertised in a real Wi-Fi Direct scan.
    #[test]
    fn the_real_board_is_a_primary_sink_on_port_7236() {
        let i = parse_device_info("00000600111c440032").unwrap();
        assert_eq!(i.role, Role::PrimarySink);
        assert!(i.session_available);
        assert_eq!(i.control_port, 7236);
        assert_eq!(i.max_throughput_mbps, 50);
        assert!(!i.content_protection);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(parse_device_info("").is_err());
        assert!(parse_device_info("zz").is_err());
        assert!(parse_device_info("0000").is_err());
        assert!(parse_device_info("000006001100").is_err()); // truncated
        assert!(parse_device_info("00000200 11".replace(' ', "").as_str()).is_err());
        // wrong length
    }
}
