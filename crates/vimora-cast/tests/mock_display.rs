//! A pretend Cast display (TLS, CASTV2) checks the whole conversation: connect, launch the
//! receiver, load a picture, and that the picture can really be fetched from our HTTP server.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls;
use vimora_cast::proto::*;
use vimora_cast::{Device, Session, State};

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
        Arc::new(move || Some(p2.clone())),
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
        Arc::new(|| Some(vec![0xff, 0xd8, 0xff, 0xd9])),
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
