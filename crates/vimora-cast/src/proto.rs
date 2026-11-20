//! The CASTV2 wire format: a 4-byte big-endian length, then a protobuf `CastMessage`
//! (written by hand: seven fields are all it has).

use anyhow::{bail, Result};

pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// A message with a text (JSON) payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastMessage {
    pub source: String,
    pub destination: String,
    pub namespace: String,
    pub payload: String,
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_str(out: &mut Vec<u8>, field: u8, s: &str) {
    out.push(field << 3 | 2);
    put_varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

impl CastMessage {
    /// Protobuf body (no length prefix).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&[1 << 3, 0]); // protocol_version = CASTV2_1_0
        put_str(&mut b, 2, &self.source);
        put_str(&mut b, 3, &self.destination);
        put_str(&mut b, 4, &self.namespace);
        b.extend_from_slice(&[5 << 3, 0]); // payload_type = STRING
        put_str(&mut b, 6, &self.payload);
        b
    }

    /// With the length prefix, ready to write.
    pub fn frame(&self) -> Vec<u8> {
        let body = self.encode();
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend(body);
        out
    }

    pub fn decode(mut b: &[u8]) -> Result<Self> {
        let mut m = CastMessage {
            source: String::new(),
            destination: String::new(),
            namespace: String::new(),
            payload: String::new(),
        };
        while !b.is_empty() {
            let (tag, rest) = varint(b)?;
            b = rest;
            let (field, wire) = (tag >> 3, tag & 7);
            match wire {
                0 => {
                    let (_, rest) = varint(b)?;
                    b = rest;
                }
                2 => {
                    let (len, rest) = varint(b)?;
                    let len = len as usize;
                    if rest.len() < len {
                        bail!("truncated field");
                    }
                    let (data, rest) = rest.split_at(len);
                    b = rest;
                    let text = || String::from_utf8_lossy(data).into_owned();
                    match field {
                        2 => m.source = text(),
                        3 => m.destination = text(),
                        4 => m.namespace = text(),
                        6 => m.payload = text(),
                        _ => {} // 7: binary payload — not used here
                    }
                }
                _ => bail!("unsupported wire type {wire}"),
            }
        }
        Ok(m)
    }
}

fn varint(b: &[u8]) -> Result<(u64, &[u8])> {
    let mut v = 0u64;
    for (i, byte) in b.iter().enumerate().take(10) {
        v |= ((byte & 0x7f) as u64) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok((v, &b[i + 1..]));
        }
    }
    bail!("bad varint")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let m = CastMessage {
            source: "sender-0".into(),
            destination: "receiver-0".into(),
            namespace: NS_CONNECTION.into(),
            payload: r#"{"type":"CONNECT"}"#.into(),
        };
        let f = m.frame();
        assert_eq!(
            u32::from_be_bytes(f[..4].try_into().unwrap()) as usize,
            f.len() - 4
        );
        assert_eq!(CastMessage::decode(&f[4..]).unwrap(), m);
    }

    #[test]
    fn long_payload_uses_multibyte_length() {
        let m = CastMessage {
            source: "a".into(),
            destination: "b".into(),
            namespace: NS_MEDIA.into(),
            payload: "x".repeat(300),
        };
        assert_eq!(CastMessage::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(CastMessage::decode(&[0x12, 0xff]).is_err());
        assert!(CastMessage::decode(&[0x12, 0x05, b'a']).is_err());
    }
}
