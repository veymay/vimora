//! A pretend Cast display (TLS, CASTV2) checks the whole conversation: connect, launch the
//! receiver, load a picture, and that the picture can really be fetched from our HTTP server.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls;
use vimora_cast::proto::*;
use vimora_cast::stream::{self, EncodedFrame, Keys};
use vimora_cast::{Device, RawFrame, Session, Sources, State, VideoEncoder};

fn picture_sources(jpeg: Vec<u8>) -> Sources {
    Sources {
        jpeg: Arc::new(move || Some(jpeg.clone())),
        raw: Arc::new(|| None),
        encoder: Arc::new(|_, _, _, _| anyhow::bail!("no encoder in this test")),
    }
}

async fn read_msg<S: AsyncReadExt + Unpin>(s: &mut S) -> Option<CastMessage> {
    let len = s.read_u32().await.ok()? as usize;
    let mut b = vec![0u8; len];
    s.read_exact(&mut b).await.ok()?;
    CastMessage::decode(&b).ok()
}

async fn write_msg<S: AsyncWriteExt + Unpin>(s: &mut S, dst: &str, ns: &str, body: &str) {
    let m = CastMessage {
        source: "receiver-0".into(),
        destination: dst.into(),
        namespace: ns.into(),
        payload: body.into(),
    };
    s.write_all(&m.frame()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn casts_a_picture_to_a_display() {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der()).unwrap(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (got_tx, got_rx) = tokio::sync::oneshot::channel::<(String, Vec<u8>)>();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(tcp).await.unwrap();
        let mut got_tx = Some(got_tx);
        while let Some(m) = read_msg(&mut tls).await {
            let v: serde_json::Value = serde_json::from_str(&m.payload).unwrap();
            match (m.namespace.as_str(), v["type"].as_str().unwrap_or("")) {
                (NS_RECEIVER, "LAUNCH") => {
                    assert_eq!(v["appId"], "CC1AD845");
                    write_msg(
                        &mut tls,
                        "sender-0",
                        NS_RECEIVER,
                        r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"appId":"CC1AD845","sessionId":"S1","transportId":"web-7"}]}}"#,
                    )
                    .await;
                }
                (NS_MEDIA, "LOAD") => {
                    assert_eq!(m.destination, "web-7");
                    assert_eq!(v["sessionId"], "S1");
                    assert_eq!(v["media"]["contentType"], "image/jpeg");
                    let url = v["media"]["contentId"].as_str().unwrap().to_string();
                    // Fetch the picture like a display would.
                    let rest = url.strip_prefix("http://").unwrap();
                    let (host, path) = rest.split_once('/').unwrap();
                    let mut c = TcpStream::connect(host).await.unwrap();
                    c.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes())
                        .await
                        .unwrap();
                    let mut resp = Vec::new();
                    c.read_to_end(&mut resp).await.unwrap();
                    let at = resp.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                    if let Some(t) = got_tx.take() {
                        let _ = t.send((url, resp[at..].to_vec()));
                    }
                    write_msg(
                        &mut tls,
                        "sender-0",
                        NS_MEDIA,
                        r#"{"type":"MEDIA_STATUS","status":[{"playerState":"PLAYING"}]}"#,
                    )
                    .await;
                }
                (NS_HEARTBEAT, "PING") => {
                    write_msg(&mut tls, "sender-0", NS_HEARTBEAT, r#"{"type":"PONG"}"#).await;
                }
                _ => {}
            }
        }
    });

    let device = Device {
        id: "test".into(),
        name: "Test doska".into(),
        model: "Mock".into(),
        addr: "127.0.0.1".parse().unwrap(),
        port,
    };
    let picture = b"\xff\xd8not-really-a-jpeg\xff\xd9".to_vec();
    let p2 = picture.clone();
    let session = Session::start(
        &tokio::runtime::Handle::current(),
        device,
        picture_sources(p2),
    );

    let (url, body) = tokio::time::timeout(Duration::from_secs(10), got_rx)
        .await
        .expect("no picture was loaded")
        .unwrap();
    assert!(url.ends_with(".jpg"));
    assert_eq!(body, picture);
    // "Casting" is shown only once the display has answered the picture.
    for _ in 0..30 {
        if session.state() == State::Casting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(session.state(), State::Casting);
    assert_eq!(session.stats().acked, 1);

    session.stop();
    for _ in 0..50 {
        if session.state() == State::Stopped {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("session did not stop: {:?}", session.state());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_display_that_ignores_pictures_is_reported_failed_not_connected() {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der()).unwrap(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(tcp).await.unwrap();
        while let Some(m) = read_msg(&mut tls).await {
            // Like the real board: starts the app, then ignores every media request.
            if m.namespace == NS_RECEIVER && m.payload.contains("LAUNCH") {
                write_msg(
                    &mut tls,
                    "sender-0",
                    NS_RECEIVER,
                    r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"appId":"CC1AD845","sessionId":"S","transportId":"t"}]}}"#,
                )
                .await;
            }
        }
    });
    let device = Device {
        id: "mute".into(),
        name: "Mute".into(),
        model: String::new(),
        addr: "127.0.0.1".parse().unwrap(),
        port,
    };
    let session = Session::start(
        &tokio::runtime::Handle::current(),
        device,
        picture_sources(vec![0xff, 0xd8, 0xff, 0xd9]),
    );
    for _ in 0..150 {
        match session.state() {
            State::Failed(why) => {
                assert!(why.contains("does not show pictures"), "{why}");
                return;
            }
            State::Casting => panic!("must never claim to be casting"),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    panic!("never failed: {:?}", session.state());
}

/// Pretends to encode: every pushed frame becomes a 3000-byte "frame" (the first is a key frame).
struct FakeEncoder {
    n: u32,
    ready: Vec<EncodedFrame>,
}

impl VideoEncoder for FakeEncoder {
    fn push(&mut self, _rgba: &[u8]) -> anyhow::Result<()> {
        let data: Vec<u8> = (0..3000u32).map(|i| (i + self.n) as u8).collect();
        self.ready.push(EncodedFrame {
            data,
            key: self.n == 0,
        });
        self.n += 1;
        Ok(())
    }

    fn poll(&mut self) -> Vec<EncodedFrame> {
        std::mem::take(&mut self.ready)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn streams_encrypted_frames_and_shows_casting_only_after_the_display_reports() {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der()).unwrap(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let udp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_port = udp.local_addr().unwrap().port();
    let (keys_tx, keys_rx) = tokio::sync::oneshot::channel::<(Keys, u32)>();

    // The display: starts the mirroring app, answers the offer with its UDP port.
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(tcp).await.unwrap();
        let mut keys_tx = Some(keys_tx);
        while let Some(m) = read_msg(&mut tls).await {
            let v: serde_json::Value = serde_json::from_str(&m.payload).unwrap();
            match (m.namespace.as_str(), v["type"].as_str().unwrap_or("")) {
                (NS_RECEIVER, "LAUNCH") => {
                    write_msg(
                        &mut tls,
                        "sender-0",
                        NS_RECEIVER,
                        r#"{"type":"RECEIVER_STATUS","status":{"applications":[{"appId":"CC1AD845","displayName":"Chrome Mirroring","sessionId":"S1","transportId":"web-1","namespaces":[{"name":"urn:x-cast:com.google.cast.webrtc"},{"name":"urn:x-cast:com.google.cast.media"}]}]}}"#,
                    )
                    .await;
                }
                (stream::NS_WEBRTC, "OFFER") => {
                    assert_eq!(m.destination, "web-1");
                    let s = &v["offer"]["supportedStreams"][0];
                    let unhex = |h: &str| -> [u8; 16] {
                        let mut o = [0u8; 16];
                        for i in 0..16 {
                            o[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap();
                        }
                        o
                    };
                    if let Some(t) = keys_tx.take() {
                        let _ = t.send((
                            Keys {
                                key: unhex(s["aesKey"].as_str().unwrap()),
                                iv_mask: unhex(s["aesIvMask"].as_str().unwrap()),
                            },
                            s["ssrc"].as_u64().unwrap() as u32,
                        ));
                    }
                    let answer = format!(
                        r#"{{"type":"ANSWER","seqNum":{},"result":"ok","answer":{{"castMode":"mirroring","udpPort":{udp_port},"sendIndexes":[0],"ssrcs":[9]}}}}"#,
                        v["seqNum"]
                    );
                    write_msg(&mut tls, "sender-0", stream::NS_WEBRTC, &answer).await;
                }
                _ => {}
            }
        }
    });

    let device = Device {
        id: "board".into(),
        name: "Board".into(),
        model: String::new(),
        addr: "127.0.0.1".parse().unwrap(),
        port,
    };
    let sources = Sources {
        jpeg: Arc::new(|| None),
        raw: Arc::new(|| {
            Some(RawFrame {
                width: 64,
                height: 48,
                rgba: vec![0; 64 * 48 * 4],
            })
        }),
        encoder: Arc::new(|_, _, _, _| {
            Ok(Box::new(FakeEncoder {
                n: 0,
                ready: vec![],
            }))
        }),
    };
    let session = Session::start(&tokio::runtime::Handle::current(), device, sources);

    let (keys, ssrc) = tokio::time::timeout(Duration::from_secs(10), keys_rx)
        .await
        .expect("no offer")
        .unwrap();
    // The first frame arrives as Cast RTP packets; decrypted it is exactly what the encoder made.
    let mut buf = vec![0u8; 2048];
    let mut sender = None;
    let mut got_sr = false;
    let mut payload = vec![];
    let mut key_flag = false;
    loop {
        let (n, from) = tokio::time::timeout(Duration::from_secs(10), udp.recv_from(&mut buf))
            .await
            .expect("no packets")
            .unwrap();
        sender = Some(from);
        let p = &buf[..n];
        if stream::is_rtcp(p) {
            assert_eq!(p[1], 200, "sender report");
            assert_eq!(u32::from_be_bytes(p[4..8].try_into().unwrap()), ssrc);
            got_sr = true;
            continue;
        }
        assert_eq!(u32::from_be_bytes(p[8..12].try_into().unwrap()), ssrc);
        if u16::from_be_bytes([p[14], p[15]]) == 0 {
            payload.clear();
            key_flag = p[12] & 0x80 != 0;
        }
        payload.extend_from_slice(&p[19..]);
        if p[1] & 0x80 != 0 {
            break; // the marker: the frame is complete
        }
    }
    assert!(key_flag, "the first frame is a key frame");
    keys.apply(0, &mut payload);
    let expected: Vec<u8> = (0..3000u32).map(|i| i as u8).collect();
    assert_eq!(payload, expected);
    // Sent is not shown: until the display reports, the session must not claim to be casting.
    assert_ne!(session.state(), State::Casting);
    let _ = got_sr;
    // The display reports (an RTCP receiver report): now it is casting.
    udp.send_to(&[0x81, 201, 0, 1, 0, 0, 0, 9], sender.unwrap())
        .await
        .unwrap();
    for _ in 0..50 {
        if session.state() == State::Casting {
            assert!(session.stats().acked >= 1);
            assert_eq!(session.stats().mode, "stream");
            session.stop();
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("never casting: {:?}", session.state());
}
