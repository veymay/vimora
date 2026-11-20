//! One cast: connect to a display, start the Default Media Receiver, keep sending the screen.

use crate::proto::*;
use crate::Device;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Notify;
use tokio_rustls::rustls;

/// The Default Media Receiver: shows pictures and plays media from a URL.
const APP_ID: &str = "CC1AD845";
const SENDER: &str = "sender-0";
const RECEIVER: &str = "receiver-0";
/// Pictures are sent at most this often (the display needs time to fetch and show one).
const MIN_GAP: Duration = Duration::from_millis(600);
const POLL: Duration = Duration::from_millis(300);

/// Gives the current screen as a JPEG (None = nothing to show right now).
pub type FrameSource = Arc<dyn Fn() -> Option<Vec<u8>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Connecting,
    Casting,
    /// Ended by the display, the network or an error (short English reason).
    Failed(String),
    /// Ended by the teacher.
    Stopped,
}

/// A running cast. Dropping it does not stop it; call [`Session::stop`].
pub struct Session {
    device: Device,
    state: Arc<Mutex<State>>,
    stop: Arc<StopFlag>,
}

#[derive(Default)]
struct StopFlag {
    on: AtomicBool,
    wake: Notify,
}

impl Session {
    pub fn start(rt: &tokio::runtime::Handle, device: Device, frames: FrameSource) -> Self {
        let state = Arc::new(Mutex::new(State::Connecting));
        let stop = Arc::new(StopFlag::default());
        let (st, sp, dev) = (state.clone(), stop.clone(), device.clone());
        rt.spawn(async move {
            let r = run(&dev, frames, &st, &sp).await;
            let mut s = st.lock().unwrap();
            *s = match r {
                _ if sp.on.load(Ordering::Relaxed) => State::Stopped,
                Ok(()) => State::Stopped,
                Err(e) => {
                    tracing::warn!("cast to {}: {e:#}", dev.name);
                    State::Failed(format!("{e:#}"))
                }
            };
        });
        Self {
            device,
            state,
            stop,
        }
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// Still connecting or casting.
    pub fn alive(&self) -> bool {
        matches!(self.state(), State::Connecting | State::Casting)
    }

    pub fn stop(&self) {
        self.stop.on.store(true, Ordering::Relaxed);
        self.stop.wake.notify_waiters();
    }
}

/// Cast displays present their own certificate (not one a browser would trust), so it is not
/// checked — as every Cast sender does. Nothing secret goes over this link: it carries the
/// screen to a display the teacher picked on the local network.
#[derive(Debug)]
struct AcceptAny(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(provider)))
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

/// The newest picture and its number (the URL changes with it, so the display never reuses an
/// old one).
#[derive(Default)]
struct Latest {
    jpeg: Vec<u8>,
    n: u64,
}

/// Serves the newest picture to whoever asks (the display).
async fn serve_pictures(listener: TcpListener, latest: Arc<Mutex<Latest>>) {
    loop {
        let Ok((mut s, _)) = listener.accept().await else {
            return;
        };
        let latest = latest.clone();
        tokio::spawn(async move {
            // Read the request head (it is not looked at: every path gives the newest picture).
            let mut head = Vec::new();
            let mut buf = [0u8; 1024];
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 8192 {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
            })
            .await;
            let jpeg = latest.lock().unwrap().jpeg.clone();
            let h = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\
                 Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n\
                 Connection: close\r\n\r\n",
                jpeg.len()
            );
            let _ = s.write_all(h.as_bytes()).await;
            let _ = s.write_all(&jpeg).await;
            let _ = s.shutdown().await;
        });
    }
}

/// Our address as the display sees it.
async fn local_ip_towards(dev: &Device) -> Result<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").await?;
    s.connect((dev.addr, 9)).await?;
    Ok(s.local_addr()?.ip())
}

async fn read_frames<R: AsyncReadExt + Unpin>(
    mut r: R,
    tx: tokio::sync::mpsc::Sender<CastMessage>,
) -> Result<()> {
    loop {
        let len = r
            .read_u32()
            .await
            .context("display closed the connection")? as usize;
        if len > 1 << 20 {
            bail!("message too large");
        }
        let mut b = vec![0u8; len];
        r.read_exact(&mut b).await?;
        if let Ok(m) = CastMessage::decode(&b) {
            if tx.send(m).await.is_err() {
                return Ok(());
            }
        }
    }
}

async fn run(
    dev: &Device,
    frames: FrameSource,
    state: &Mutex<State>,
    stop: &StopFlag,
) -> Result<()> {
    let ip = local_ip_towards(dev).await.context("no network route")?;
    let pictures = TcpListener::bind("0.0.0.0:0").await?;
    let port = pictures.local_addr()?.port();
    let latest = Arc::new(Mutex::new(Latest::default()));
    let server = tokio::spawn(serve_pictures(pictures, latest.clone()));
    let result = session_loop(dev, frames, state, stop, ip, port, &latest).await;
    server.abort();
    result
}

async fn session_loop(
    dev: &Device,
    frames: FrameSource,
    state: &Mutex<State>,
    stop: &StopFlag,
    ip: IpAddr,
    port: u16,
    latest: &Mutex<Latest>,
) -> Result<()> {
    let tcp = tokio::time::timeout(
        Duration::from_secs(6),
        TcpStream::connect((dev.addr, dev.port)),
    )
    .await
    .context("display did not answer")??;
    tcp.set_nodelay(true)?;
    let connector = tokio_rustls::TlsConnector::from(tls_config()?);
    let tls = connector
        .connect(
            rustls::pki_types::ServerName::IpAddress(IpAddr::V4(dev.addr).into()),
            tcp,
        )
        .await
        .context("secure connection failed")?;
    let (rd, mut wr) = tokio::io::split(tls);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<CastMessage>(64);
    let reader = tokio::spawn(read_frames(rd, tx));

    let mut req_id = 0u64;

    macro_rules! say {
        ($dst:expr, $ns:expr, $body:expr) => {{
            let m = CastMessage {
                source: SENDER.into(),
                destination: $dst.into(),
                namespace: $ns.into(),
                payload: $body.to_string(),
            };
            wr.write_all(&m.frame())
                .await
                .context("display closed the connection")?;
            wr.flush().await?;
        }};
    }
    let mut next_id = || {
        req_id += 1;
        req_id
    };

    say!(RECEIVER, NS_CONNECTION, json!({"type": "CONNECT"}));
    say!(
        RECEIVER,
        NS_RECEIVER,
        json!({"type": "LAUNCH", "appId": APP_ID, "requestId": next_id()})
    );

    let started = Instant::now();
    let mut transport: Option<(String, String)> = None; // (transportId, sessionId)
    let mut last_hash = 0u64;
    let mut last_load = Instant::now() - MIN_GAP;
    let mut last_ping = Instant::now();
    let mut tick = tokio::time::interval(POLL);
    let outcome: Result<()> = loop {
        tokio::select! {
            _ = stop.wake.notified() => break Ok(()),
            m = rx.recv() => {
                let Some(m) = m else { break Err(anyhow!("display closed the connection")) };
                let v: Value = serde_json::from_str(&m.payload).unwrap_or(Value::Null);
                let kind = v["type"].as_str().unwrap_or("");
                match (m.namespace.as_str(), kind) {
                    (NS_HEARTBEAT, "PING") => {
                        say!(RECEIVER, NS_HEARTBEAT, json!({"type": "PONG"}));
                    }
                    (NS_CONNECTION, "CLOSE") => break Err(anyhow!("the display ended the connection")),
                    (NS_RECEIVER, "RECEIVER_STATUS") => {
                        let app = v["status"]["applications"]
                            .as_array()
                            .and_then(|a| a.iter().find(|a| a["appId"] == APP_ID));
                        match (app, &transport) {
                            (Some(a), None) => {
                                if let (Some(t), Some(s)) =
                                    (a["transportId"].as_str(), a["sessionId"].as_str())
                                {
                                    say!(t, NS_CONNECTION, json!({"type": "CONNECT"}));
                                    transport = Some((t.into(), s.into()));
                                    *state.lock().unwrap() = State::Casting;
                                }
                            }
                            // The receiver app was closed on the display (or replaced).
                            (None, Some(_)) => break Err(anyhow!("the display stopped showing the screen")),
                            _ => {}
                        }
                    }
                    (NS_MEDIA, "LOAD_FAILED") | (NS_MEDIA, "INVALID_REQUEST") => {
                        tracing::warn!("display refused a picture: {}", m.payload);
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if transport.is_none() {
                    if started.elapsed() > Duration::from_secs(15) {
                        break Err(anyhow!("the display did not start the receiver"));
                    }
                    continue;
                }
                if last_ping.elapsed() >= Duration::from_secs(5) {
                    last_ping = Instant::now();
                    say!(RECEIVER, NS_HEARTBEAT, json!({"type": "PING"}));
                }
                if last_load.elapsed() < MIN_GAP {
                    continue;
                }
                let src = frames.clone();
                let jpeg = tokio::task::spawn_blocking(move || src()).await.ok().flatten();
                let Some(jpeg) = jpeg else { continue };
                let mut h = DefaultHasher::new();
                jpeg.hash(&mut h);
                let hash = h.finish();
                if hash == last_hash {
                    continue; // the screen did not change: nothing to send
                }
                last_hash = hash;
                last_load = Instant::now();
                let n = {
                    let mut l = latest.lock().unwrap();
                    l.jpeg = jpeg;
                    l.n += 1;
                    l.n
                };
                let (t, sid) = transport.clone().expect("checked above");
                say!(
                    t,
                    NS_MEDIA,
                    json!({
                        "type": "LOAD",
                        "requestId": next_id(),
                        "sessionId": sid,
                        "autoplay": true,
                        "currentTime": 0,
                        "media": {
                            "contentId": format!("http://{ip}:{port}/f{n}.jpg"),
                            "contentType": "image/jpeg",
                            "streamType": "BUFFERED",
                        },
                    })
                );
            }
        }
    };
    // Leave the display clean: end our receiver app.
    if let Some((_, sid)) = &transport {
        let id = next_id();
        let m = CastMessage {
            source: SENDER.into(),
            destination: RECEIVER.into(),
            namespace: NS_RECEIVER.into(),
            payload: json!({"type": "STOP", "sessionId": sid, "requestId": id}).to_string(),
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), wr.write_all(&m.frame())).await;
        let _ = wr.flush().await;
    }
    reader.abort();
    outcome
}
