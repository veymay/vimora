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

pub fn decode_beacon(data: &[u8]) -> Option<crate::protocol::Beacon> {
    let magic = crate::protocol::BEACON_MAGIC;
    if data.len() <= magic.len() || &data[..magic.len()] != magic {
        return None;
    }
    bincode::deserialize(&data[magic.len()..]).ok()
}
