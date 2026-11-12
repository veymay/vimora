//! Length-prefixed bincode framing over any async stream.

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Hard upper bound for a single frame (file transfers included).
pub const MAX_FRAME: usize = 128 * 1024 * 1024;

pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    let body = bincode::serialize(msg)?;
    if body.len() > MAX_FRAME {
        bail!("message too large ({} bytes)", body.len());
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub async fn write_msg<W, T>(w: &mut W, msg: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let buf = encode(msg)?;
    w.write_all(&buf).await?;
    w.flush().await?;
    Ok(())
}

/// A frame arrived intact but its content is not understood (a newer peer's message).
/// The stream is still in sync, so receivers may skip it and carry on.
#[derive(Debug)]
pub struct Undecodable(pub String);

impl std::fmt::Display for Undecodable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "message not understood: {}", self.0)
    }
}

impl std::error::Error for Undecodable {}

pub fn is_undecodable(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Undecodable>().is_some()
}

pub async fn read_msg<R, T>(r: &mut R) -> Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let len = r.read_u32().await.context("connection closed")? as usize;
    if len > MAX_FRAME {
        bail!("frame too large ({len} bytes)");
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    bincode::deserialize(&buf).map_err(|e| anyhow::Error::new(Undecodable(e.to_string())))
}

pub fn encode_beacon(b: &crate::protocol::Beacon) -> Result<Vec<u8>> {
    let mut out = crate::protocol::BEACON_MAGIC.to_vec();
    out.extend(bincode::serialize(b)?);
    Ok(out)
}

/// "Take this computer": sent by a teacher (UDP, to the free computer's discovery port) when
/// the teacher adds a free computer. Carries the computer's key and the teacher's TCP port; the
/// teacher's address is the datagram's source.
const CLAIM_MAGIC: &[u8; 8] = b"VIMCLM01";

pub fn encode_claim(computer_key: &str, tcp_port: u16) -> Vec<u8> {
    let mut out = CLAIM_MAGIC.to_vec();
    out.extend(bincode::serialize(&(computer_key, tcp_port)).unwrap_or_default());
    out
}

pub fn decode_claim(data: &[u8]) -> Option<(String, u16)> {
    if data.len() <= CLAIM_MAGIC.len() || &data[..CLAIM_MAGIC.len()] != CLAIM_MAGIC {
        return None;
    }
    bincode::deserialize(&data[CLAIM_MAGIC.len()..]).ok()
}

/// Where a broadcast must go to reach every computer on the classroom network: the limited
/// broadcast (255.255.255.255) and the broadcast address of each IPv4 network this computer is
/// on (e.g. 192.168.1.255). The limited broadcast alone fails on networks without a default
/// gateway ("network unreachable"), and Windows sends it out one adapter only (often a
/// VirtualBox / VPN one), so computers that are not paired yet never heard the teacher.
pub fn broadcast_targets() -> Vec<std::net::Ipv4Addr> {
    let mut v = vec![std::net::Ipv4Addr::BROADCAST];
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        if let if_addrs::IfAddr::V4(a) = iface.addr {
            if a.ip.is_loopback() || a.ip.is_link_local() {
                continue;
            }
            let b = match a.broadcast {
                Some(b) if !b.is_unspecified() => b,
                _ => std::net::Ipv4Addr::from(u32::from(a.ip) | !u32::from(a.netmask)),
            };
            if b != a.ip && !b.is_unspecified() && !v.contains(&b) {
                v.push(b);
            }
        }
    }
    v
}

/// "Who is waiting to be added?": broadcast by a teacher when the teacher presses "Skanerlash".
/// Computers that belong to no teacher answer at once with `StudentMsg::Free` over TCP to the
/// datagram's source address and the teacher TCP port it carries.
const SCAN_MAGIC: &[u8; 8] = b"VIMSCN01";

pub fn encode_scan(tcp_port: u16) -> Vec<u8> {
    let mut out = SCAN_MAGIC.to_vec();
    out.extend(tcp_port.to_le_bytes());
    out
}

pub fn decode_scan(data: &[u8]) -> Option<u16> {
    if data.len() != SCAN_MAGIC.len() + 2 || &data[..SCAN_MAGIC.len()] != SCAN_MAGIC {
        return None;
    }
    Some(u16::from_le_bytes([data[8], data[9]]))
}

pub fn decode_beacon(data: &[u8]) -> Option<crate::protocol::Beacon> {
    let magic = crate::protocol::BEACON_MAGIC;
    if data.len() <= magic.len() || &data[..magic.len()] != magic {
        return None;
    }
    bincode::deserialize(&data[magic.len()..]).ok()
}

#[cfg(test)]
mod scan_tests {
    #[test]
    fn scan_round_trip() {
        let d = super::encode_scan(47801);
        assert_eq!(super::decode_scan(&d), Some(47801));
        assert_eq!(super::decode_scan(&super::encode_claim("k", 1)), None);
    }
}

#[cfg(test)]
mod bcast_tests {
    #[test]
    fn broadcast_targets_start_with_limited() {
        let t = super::broadcast_targets();
        println!("targets: {t:?}");
        assert_eq!(t[0], std::net::Ipv4Addr::BROADCAST);
    }
}
