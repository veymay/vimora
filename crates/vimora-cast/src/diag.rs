//! Receiver diagnostics: what a display says about itself, read-only (nothing is launched or
//! changed on it). Used for the diagnostic report and for compatibility profiles.

use crate::proto::*;
use crate::{tls, Device};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::rustls;

/// What the display's own setup service (HTTP :8008) tells. The MAC address and network details
/// it also offers are deliberately not kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReceiverInfo {
    pub name: String,
    pub model: String,
    pub manufacturer: String,
    pub cast_revision: String,
    pub system_build: String,
    pub display_supported: Option<bool>,
}

impl ReceiverInfo {
    /// Reads `/setup/eureka_info` (plain HTTP, port 8008; a few seconds at most).
    pub fn fetch(ip: Ipv4Addr) -> Result<Self> {
        let body = http_get(
            ip,
            8008,
            "/setup/eureka_info?params=name,build_info,device_info",
        )?;
        Self::parse(&body)
    }

    pub fn parse(json_text: &str) -> Result<Self> {
        // The answer holds a PEM key with raw line breaks inside a string (invalid JSON): the
        // control characters are turned into spaces before reading it.
        let cleaned: String = json_text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let v: Value = serde_json::from_str(&cleaned).context("not JSON")?;
        let s = |p: &Value| p.as_str().unwrap_or("").to_string();
        let info = Self {
            name: s(&v["name"]),
            model: s(&v["device_info"]["model_name"]),
            manufacturer: s(&v["device_info"]["manufacturer"]),
            cast_revision: s(&v["build_info"]["cast_build_revision"]),
            system_build: s(&v["build_info"]["system_build_number"]),
            display_supported: v["device_info"]["capabilities"]["display_supported"].as_bool(),
        };
        if info == Self::default() {
            bail!("no device information in the answer");
        }
        Ok(info)
    }
}

fn http_get(ip: Ipv4Addr, port: u16, path: &str) -> Result<String> {
    let addr = SocketAddr::from((ip, port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(4)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(
        s,
        "GET {path} HTTP/1.0\r\nHost: {ip}\r\nConnection: close\r\n\r\n"
    )?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    // Some displays keep the connection open after the answer: stop at the announced length (or
    // when it goes quiet), and never take more than a sane amount.
    loop {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !raw.is_empty() =>
            {
                break
            }
            Err(e) => return Err(e.into()),
        }
        if raw.len() > 256 * 1024 || complete(&raw) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("bad answer"))?;
    if !head.starts_with("HTTP/1.") || !head.contains(" 200") {
        bail!("HTTP error: {}", head.lines().next().unwrap_or(""));
    }
    Ok(body.to_string())
}

/// The whole HTTP answer is here (the announced `Content-Length` bytes of body arrived).
fn complete(raw: &[u8]) -> bool {
    let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&raw[..at]).to_ascii_lowercase();
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok());
    len.is_some_and(|n| raw.len() >= at + 4 + n)
}

/// The result of a read-only look at a display's Cast control channel.
#[derive(Debug, Clone, Default)]
pub struct ProbeReport {
    pub device: Option<ReceiverInfo>,
    /// Why the setup service gave nothing (it is optional).
    pub device_error: Option<String>,
    /// SHA-256 of the TLS certificate presented on 8009.
    pub certificate: Option<String>,
    /// Applications running on the display now (appId, displayName).
    pub apps: Vec<(String, String)>,
    pub volume: Option<f64>,
    /// How the control channel failed, if it did.
    pub channel_error: Option<String>,
}

/// Looks at a display: its setup service, its certificate and its receiver status. Nothing is
/// launched; the display shows nothing different afterwards.
pub async fn probe(dev: &Device) -> ProbeReport {
    let mut r = ProbeReport::default();
    let ip = dev.addr;
    match tokio::task::spawn_blocking(move || ReceiverInfo::fetch(ip)).await {
        Ok(Ok(i)) => r.device = Some(i),
        Ok(Err(e)) => r.device_error = Some(format!("{e:#}")),
        Err(e) => r.device_error = Some(e.to_string()),
    }
    match probe_channel(dev, &mut r).await {
        Ok(()) => {}
        Err(e) => r.channel_error = Some(format!("{e:#}")),
    }
    r
}

async fn probe_channel(dev: &Device, r: &mut ProbeReport) -> Result<()> {
    let tcp = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect((dev.addr, dev.port)),
    )
    .await
    .context("the display did not answer")??;
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let verifier = tls::CastVerifier::new(provider.clone());
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let mut s = tokio_rustls::TlsConnector::from(std::sync::Arc::new(cfg))
        .connect(
            rustls::pki_types::ServerName::IpAddress(dev.addr.into()),
            tcp,
        )
        .await
        .context("secure connection failed")?;
    r.certificate = verifier.seen_fingerprint();
    for (ns, body) in [
        (NS_CONNECTION, json!({"type": "CONNECT"})),
        (NS_RECEIVER, json!({"type": "GET_STATUS", "requestId": 1})),
    ] {
        let m = CastMessage {
            source: "sender-0".into(),
            destination: "receiver-0".into(),
            namespace: ns.into(),
            payload: body.to_string(),
        };
        s.write_all(&m.frame()).await?;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline {
        let len = tokio::time::timeout_at(deadline, s.read_u32()).await;
        let Ok(Ok(len)) = len else { break };
        if len > 1 << 20 {
            bail!("message too large");
        }
        let mut b = vec![0u8; len as usize];
        s.read_exact(&mut b).await?;
        let Ok(m) = CastMessage::decode(&b) else {
            continue;
        };
        let v: Value = serde_json::from_str(&m.payload).unwrap_or(Value::Null);
        if v["type"] == "RECEIVER_STATUS" {
            r.apps = v["status"]["applications"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            (
                                x["appId"].as_str().unwrap_or("").to_string(),
                                x["displayName"].as_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            r.volume = v["status"]["volume"]["level"].as_f64();
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_setup_answer() {
        let sample = r#"{ "build_info" : { "cast_build_revision" : "1.36.151708", "system_build_number" : "PPR1.180610.011"},
            "device_info" : {"capabilities" : { "display_supported" : true }, "manufacturer" : "Google Inc.", "model_name" : "Chromecast Ultra", "mac_address":"aa:bb:cc:dd:ee:ff"},
            "name" : "1-XONA" }"#;
        let i = ReceiverInfo::parse(sample).unwrap();
        assert_eq!(i.name, "1-XONA");
        assert_eq!(i.model, "Chromecast Ultra");
        assert_eq!(i.cast_revision, "1.36.151708");
        assert_eq!(i.display_supported, Some(true));
    }

    #[test]
    fn a_raw_line_break_inside_a_string_is_tolerated() {
        let sample = "{ \"name\" : \"X\", \"device_info\": {\"public_key\": \"-----BEGIN\nAAAA\n-----END\", \"model_name\": \"M\"} }";
        let i = ReceiverInfo::parse(sample).unwrap();
        assert_eq!((i.name.as_str(), i.model.as_str()), ("X", "M"));
    }

    #[test]
    fn empty_and_broken_answers_are_errors() {
        assert!(ReceiverInfo::parse("{}").is_err());
        assert!(ReceiverInfo::parse("<html>").is_err());
    }
}
