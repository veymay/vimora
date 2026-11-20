//! One cast: connect to a display, start its receiver app and show the screen.
//!
//! Two ways, chosen from what the display's app offers:
//!
//! * **streaming** (the app has the `webrtc` namespace — Chrome Mirroring; the boards in
//!   classrooms): `OFFER`/`ANSWER`, then encrypted VP8 over UDP with RTCP sender reports;
//! * **pictures** (a plain Default Media Receiver): the screen as JPEGs the display fetches.
//!
//! "Casting" is shown only when the display itself has answered (RTCP for the stream, a media
//! status for pictures) — never just because messages were sent.

use crate::proto::*;
use crate::stream::{self, EncodedFrame, Keys, Packetizer, VideoOffer};
use crate::{Device, EncoderFactory};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, Notify};
use tokio_rustls::rustls;

/// The receiver app started on the display (Default Media Receiver on most; Chrome Mirroring on
/// the classroom boards).
const APP_ID: &str = "CC1AD845";
const SENDER: &str = "sender-0";
const RECEIVER: &str = "receiver-0";
/// Pictures are sent at most this often (the display needs time to fetch and show one).
const MIN_GAP: Duration = Duration::from_millis(600);
const POLL: Duration = Duration::from_millis(300);
/// Stream size and rate. 720p15 is what a Wi-Fi link and a software encoder carry smoothly.
const STREAM_FPS: u32 = 15;
const STREAM_KBPS: u32 = 3000;
const STREAM_MAX_WIDTH: u32 = 1280;

/// Gives the current screen as a JPEG (None = nothing to show right now).
pub type FrameSource = Arc<dyn Fn() -> Option<Vec<u8>> + Send + Sync>;

/// A raw screen frame (RGBA).
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Gives the current screen as raw pixels (None = nothing right now).
pub type RawSource = Arc<dyn Fn() -> Option<RawFrame> + Send + Sync>;

/// Everything a cast needs from the program: the screen, in both forms, and an encoder.
#[derive(Clone)]
pub struct Sources {
    pub jpeg: FrameSource,
    pub raw: RawSource,
    pub encoder: EncoderFactory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Connecting,
    Casting,
    /// Ended by the display, the network or an error (short English reason).
    Failed(String),
    /// Ended by the teacher.
    Stopped,
}

/// What the display did with what we sent (for the diagnostics and for telling "sent" from
/// "shown").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Frames (stream) or pictures we sent.
    pub sent: u64,
    /// Answers the display sent back (RTCP packets / media statuses).
    pub acked: u64,
    /// Pictures the display refused or failed to load.
    pub failed: u64,
    /// "stream" or "pictures", once known.
    pub mode: String,
    /// The display's last report ("PLAYING", "rtcp" …).
    pub last_status: String,
}

/// A running cast. Dropping it does not stop it; call [`Session::stop`].
pub struct Session {
    device: Device,
    state: Arc<Mutex<State>>,
    stats: Arc<Mutex<Stats>>,
    stop: Arc<StopFlag>,
}

#[derive(Default)]
struct StopFlag {
    on: AtomicBool,
    wake: Notify,
}

impl Session {
    pub fn start(rt: &tokio::runtime::Handle, device: Device, sources: Sources) -> Self {
        let state = Arc::new(Mutex::new(State::Connecting));
        let stats = Arc::new(Mutex::new(Stats::default()));
        let stop = Arc::new(StopFlag::default());
        let (st, sp, dev, sx) = (state.clone(), stop.clone(), device.clone(), stats.clone());
        rt.spawn(async move {
            let r = run(&dev, sources, &st, &sp, &sx).await;
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
            stats,
            stop,
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats.lock().unwrap().clone()
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

fn tls_config() -> Result<(Arc<rustls::ClientConfig>, Arc<crate::tls::CastVerifier>)> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = crate::tls::CastVerifier::new(provider.clone());
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    Ok((Arc::new(cfg), verifier))
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

/// A running stream: the encoder feeder's output and what the display has reported.
struct Stream {
    target: SocketAddr,
    packetizer: Packetizer,
    frames: mpsc::Receiver<EncodedFrame>,
    feeder_stop: Arc<AtomicBool>,
    started: Instant,
    last_sr: Instant,
    last_rtcp: Option<Instant>,
    ssrc: u32,
    /// The display's clock minus ours (seconds), from its reference-time reports. Sender reports
    /// carry NTP time in the display's clock so that frame times make sense to it even when its
    /// wall clock is hours off (as on the classroom boards).
    clock_offset: f64,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.feeder_stop.store(true, Ordering::Relaxed);
    }
}

/// Picks an even stream size (at most 1280 wide) with the screen's proportions.
fn stream_size(w: u32, h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (1280, 720);
    }
    let sw = w.min(STREAM_MAX_WIDTH) & !1;
    let sh = ((h as u64 * sw as u64 / w as u64) as u32) & !1;
    (sw.max(2), sh.max(2))
}

/// A thread that grabs the screen, scales it to the stream size and feeds the encoder; the
/// encoded frames come out on the channel. Frames are never queued up: if the encoder is slow,
/// the screen is simply grabbed less often.
fn spawn_feeder(
    sources: &Sources,
    (w, h): (u32, u32),
    tx: mpsc::Sender<EncodedFrame>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let mut enc = (sources.encoder)(w, h, STREAM_FPS, STREAM_KBPS)?;
    let raw = sources.raw.clone();
    std::thread::spawn(move || {
        let step = Duration::from_millis(1000 / STREAM_FPS as u64);
        while !stop.load(Ordering::Relaxed) {
            let t0 = Instant::now();
            if let Some(f) = raw() {
                let rgba = if (f.width, f.height) == (w, h) {
                    f.rgba
                } else if let Some(img) = image::RgbaImage::from_raw(f.width, f.height, f.rgba) {
                    image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
                        .into_raw()
                } else {
                    continue;
                };
                if enc.push(&rgba).is_err() {
                    break;
                }
            }
            for frame in enc.poll() {
                // The receiving side drops when it cannot keep up; never block here.
                if tx.try_send(frame).is_err() && tx.is_closed() {
                    return;
                }
            }
            std::thread::sleep(step.saturating_sub(t0.elapsed()));
        }
    });
    Ok(())
}

async fn run(
    dev: &Device,
    sources: Sources,
    state: &Mutex<State>,
    stop: &StopFlag,
    stats: &Mutex<Stats>,
) -> Result<()> {
    let ip = local_ip_towards(dev).await.context("no network route")?;
    let pictures = TcpListener::bind("0.0.0.0:0").await?;
    let port = pictures.local_addr()?.port();
    let latest = Arc::new(Mutex::new(Latest::default()));
    let server = tokio::spawn(serve_pictures(pictures, latest.clone()));
    let result = session_loop(
        dev,
        &sources,
        &Shared {
            state,
            stop,
            stats,
            latest: &latest,
        },
        ip,
        port,
    )
    .await;
    server.abort();
    result
}

/// What a running cast shares with its owner and its picture server.
struct Shared<'a> {
    state: &'a Mutex<State>,
    stop: &'a StopFlag,
    stats: &'a Mutex<Stats>,
    latest: &'a Mutex<Latest>,
}

/// Waits for the next encoded frame, or never if there is no stream.
async fn next_frame(stream: &mut Option<Stream>) -> Option<EncodedFrame> {
    match stream {
        Some(s) => s.frames.recv().await,
        None => std::future::pending().await,
    }
}

async fn session_loop(
    dev: &Device,
    sources: &Sources,
    shared: &Shared<'_>,
    ip: IpAddr,
    port: u16,
) -> Result<()> {
    let Shared {
        state,
        stop,
        stats,
        latest,
    } = *shared;
    let frames = sources.jpeg.clone();
    let tcp = tokio::time::timeout(
        Duration::from_secs(6),
        TcpStream::connect((dev.addr, dev.port)),
    )
    .await
    .context("display did not answer")??;
    tcp.set_nodelay(true)?;
    let (cfg, verifier) = tls_config()?;
    let connector = tokio_rustls::TlsConnector::from(cfg);
    let tls = connector
        .connect(
            rustls::pki_types::ServerName::IpAddress(IpAddr::V4(dev.addr).into()),
            tcp,
        )
        .await
        .context("secure connection failed")?;
    if let Some(fp) = verifier.seen_fingerprint() {
        tracing::info!("cast: {} presented certificate {fp}", dev.name);
    }
    let (rd, mut wr) = tokio::io::split(tls);
    let (tx, mut rx) = mpsc::channel::<CastMessage>(64);
    let reader = tokio::spawn(read_frames(rd, tx));
    let udp = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let mut req_id = 0u64;

    macro_rules! say {
        ($dst:expr, $ns:expr, $body:expr) => {{
            let m = CastMessage {
                source: SENDER.into(),
                destination: $dst.into(),
                namespace: $ns.into(),
                payload: $body.to_string(),
            };
            tracing::debug!("cast -> {} {} {}", m.destination, m.namespace, m.payload);
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

    // The screen is opened first: on Wayland this is where the system asks the teacher to allow
    // sharing, which may take a while — the display must not be waiting (and its connection
    // timing out) meanwhile. The stream's size comes from this first look.
    let screen_size = {
        let raw = sources.raw.clone();
        tokio::task::spawn_blocking(move || raw())
            .await
            .ok()
            .flatten()
            .map(|f| stream_size(f.width, f.height))
            .unwrap_or((1280, 720))
    };
    say!(RECEIVER, NS_CONNECTION, json!({"type": "CONNECT"}));
    say!(
        RECEIVER,
        NS_RECEIVER,
        json!({"type": "LAUNCH", "appId": APP_ID, "requestId": next_id()})
    );

    let started = Instant::now();
    let mut transport: Option<(String, String)> = None; // (transportId, sessionId)
                                                        // Streaming: set when the app has the webrtc namespace; `offered` = the OFFER's time.
    let mut streaming = false;
    let mut offered: Option<(Instant, (u32, u32), Keys, u32)> = None;
    let mut stream: Option<Stream> = None;
    let mut udp_buf = vec![0u8; 2048];
    // Pictures.
    let mut last_hash = 0u64;
    let mut last_load = Instant::now() - MIN_GAP;
    let mut first_load: Option<Instant> = None;
    let mut last_ping = Instant::now();
    let mut tick = tokio::time::interval(POLL);
    let outcome: Result<()> = loop {
        // `notify_waiters` is lost if nobody waits at that instant: the flag is the truth.
        if stop.on.load(Ordering::Relaxed) {
            break Ok(());
        }
        tokio::select! {
            _ = stop.wake.notified() => break Ok(()),
            m = rx.recv() => {
                let Some(m) = m else { break Err(anyhow!("display closed the connection")) };
                tracing::debug!("cast <- {} {} {}", m.source, m.namespace, m.payload);
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
                                    streaming = a["namespaces"]
                                        .as_array()
                                        .is_some_and(|n| n.iter().any(|x| x["name"] == stream::NS_WEBRTC));
                                    stats.lock().unwrap().mode =
                                        if streaming { "stream" } else { "pictures" }.into();
                                    if streaming {
                                        let size = screen_size;
                                        let keys = Keys::random();
                                        let ssrc = 1 + (started.elapsed().subsec_nanos() & 0xffff);
                                        let msg = stream::offer(
                                            next_id(),
                                            &VideoOffer {
                                                ssrc,
                                                width: size.0,
                                                height: size.1,
                                                fps: STREAM_FPS,
                                                kbps: STREAM_KBPS,
                                                target_delay_ms: 400,
                                            },
                                            &keys,
                                        );
                                        say!(t, stream::NS_WEBRTC, msg);
                                        offered = Some((Instant::now(), size, keys, ssrc));
                                    }
                                }
                            }
                            // The receiver app was closed on the display (or replaced).
                            (None, Some(_)) => break Err(anyhow!("the display stopped showing the screen")),
                            _ => {}
                        }
                    }
                    (stream::NS_WEBRTC, "ANSWER") => {
                        let Some((_, size, keys, ssrc)) = offered.take() else { continue };
                        let ans = match stream::parse_answer(&v) {
                            Ok(a) => a,
                            Err(e) => break Err(anyhow!("the display refused the stream: {e}")),
                        };
                        let (ftx, frx) = mpsc::channel::<EncodedFrame>(8);
                        let feeder_stop = Arc::new(AtomicBool::new(false));
                        let factory = sources.clone();
                        let fs = feeder_stop.clone();
                        let started_feeder =
                            tokio::task::spawn_blocking(move || spawn_feeder(&factory, size, ftx, fs)).await;
                        match started_feeder {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => break Err(anyhow!("the video encoder did not start: {e:#}")),
                            Err(e) => break Err(anyhow!("{e}")),
                        }
                        stream = Some(Stream {
                            target: SocketAddr::new(IpAddr::V4(dev.addr), ans.udp_port),
                            packetizer: Packetizer::new(ssrc, keys),
                            frames: frx,
                            feeder_stop,
                            started: Instant::now(),
                            last_sr: Instant::now() - Duration::from_secs(1),
                            last_rtcp: None,
                            ssrc,
                            clock_offset: 0.0,
                        });
                    }
                    (NS_MEDIA, "LOAD_FAILED") | (NS_MEDIA, "INVALID_REQUEST") => {
                        tracing::warn!("display refused a picture: {}", m.payload);
                        let mut s = stats.lock().unwrap();
                        s.failed += 1;
                        s.last_status = kind.to_string();
                        if s.failed >= 4 && s.acked == 0 {
                            drop(s);
                            break Err(anyhow!("the display refuses the pictures"));
                        }
                    }
                    (NS_MEDIA, "MEDIA_STATUS") => {
                        let st = &v["status"][0];
                        let player = st["playerState"].as_str().unwrap_or("");
                        let idle = st["idleReason"].as_str().unwrap_or("");
                        let mut s = stats.lock().unwrap();
                        s.acked += 1;
                        s.last_status = if idle.is_empty() {
                            player.to_string()
                        } else {
                            format!("{player}/{idle}")
                        };
                        if idle == "ERROR" {
                            s.failed += 1;
                        } else if s.acked == 1 {
                            // The first answer: the display really shows our pictures now.
                            *state.lock().unwrap() = State::Casting;
                        }
                    }
                    _ => {}
                }
            }
            // What the display sends back on the stream's UDP port: RTCP reports.
            r = udp.recv_from(&mut udp_buf) => {
                if let (Ok((n, from)), Some(s)) = (r, stream.as_mut()) {
                    if from.ip() == s.target.ip() && stream::is_rtcp(&udp_buf[..n]) {
                        tracing::debug!("cast rtcp <- {}", stream::describe_rtcp(&udp_buf[..n]));
                        // The display tells what time it is for it: answer so it can measure the
                        // round trip, and learn how far its clock is from ours.
                        if let Some(r) = stream::parse_rrtr(&udp_buf[..n]) {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default();
                            let ours = now.as_secs_f64() + 2_208_988_800.0;
                            // Half the round trip is not known yet; the error is milliseconds.
                            s.clock_offset = r.as_ntp_f64() - ours;
                            let dlrr = stream::dlrr_packet(s.ssrc, &r, 0);
                            let _ = udp.send_to(&dlrr, s.target).await;
                        }
                        s.last_rtcp = Some(Instant::now());
                        let mut st = stats.lock().unwrap();
                        st.acked += 1;
                        st.last_status = "rtcp".into();
                        if st.acked == 1 {
                            *state.lock().unwrap() = State::Casting;
                        }
                    }
                }
            }
            // A frame came out of the encoder: encrypt, cut into packets, send.
            f = next_frame(&mut stream) => {
                let Some(frame) = f else { break Err(anyhow!("the video encoder stopped")) };
                let s = stream.as_mut().expect("a frame needs a stream");
                let rtp_ts = (s.started.elapsed().as_micros() as u64 * stream::RTP_CLOCK as u64 / 1_000_000) as u32;
                for (i, p) in s.packetizer.packetize(&frame, rtp_ts).iter().enumerate() {
                    let _ = udp.send_to(p, s.target).await;
                    if i % 16 == 15 {
                        tokio::task::yield_now().await;
                    }
                }
                stats.lock().unwrap().sent += 1;
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
                if streaming {
                    if let Some((at, ..)) = &offered {
                        if at.elapsed() > Duration::from_secs(10) {
                            break Err(anyhow!("the display did not answer the stream offer"));
                        }
                    }
                    if let Some(s) = stream.as_mut() {
                        // Sender reports tell the display how RTP time maps to wall time.
                        if s.last_sr.elapsed() >= Duration::from_millis(500) {
                            s.last_sr = Instant::now();
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default();
                            let shifted = now.as_secs_f64() + s.clock_offset;
                            let (ns, nf) = stream::ntp_from_unix_f64(shifted);
                            let rtp_ts = (s.started.elapsed().as_micros() as u64 * stream::RTP_CLOCK as u64 / 1_000_000) as u32;
                            let sr = stream::sender_report(
                                s.ssrc, ns, nf, rtp_ts, s.packetizer.packets, s.packetizer.octets,
                            );
                            let _ = udp.send_to(&sr, s.target).await;
                        }
                        // A display that never reports (or stops reporting) is not receiving:
                        // say so instead of showing a stream that goes nowhere.
                        match s.last_rtcp {
                            None if s.started.elapsed() > Duration::from_secs(10) => {
                                break Err(anyhow!("the display does not report receiving the stream"));
                            }
                            Some(t) if t.elapsed() > Duration::from_secs(10) => {
                                break Err(anyhow!("the display stopped reporting"));
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                // Pictures went out but the display never answered one: it does not take pictures.
                if first_load.is_some_and(|t| t.elapsed() > Duration::from_secs(7))
                    && stats.lock().unwrap().acked == 0
                {
                    break Err(anyhow!(
                        "the display does not show pictures (it accepts only Cast streaming)"
                    ));
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
                stats.lock().unwrap().sent += 1;
                first_load.get_or_insert_with(Instant::now);
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
    drop(stream);
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
