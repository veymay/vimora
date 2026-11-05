//! Mutual challenge/response authentication based on a shared classroom key.
//!
//! Both sides prove knowledge of the key without sending it:
//! * client mac = HMAC(key, "vimora-client" || server_nonce || client_nonce)
//! * server mac = HMAC(key, "vimora-server" || client_nonce || server_nonce)
//!
//! The client refuses to obey a console that cannot prove it knows the key, so a student
//! cannot simply run their own console to lock classmates.

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn nonce() -> Vec<u8> {
    let mut n = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut n);
    n
}

fn mac(key: &str, label: &[u8], a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    m.update(label);
    m.update(a);
    m.update(b);
    m.finalize().into_bytes().to_vec()
}

fn verify(key: &str, label: &[u8], a: &[u8], b: &[u8], tag: &[u8]) -> bool {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    m.update(label);
    m.update(a);
    m.update(b);
    m.verify_slice(tag).is_ok()
}

pub fn client_mac(key: &str, server_nonce: &[u8], client_nonce: &[u8]) -> Vec<u8> {
    mac(key, b"vimora-client", server_nonce, client_nonce)
}

pub fn verify_client(key: &str, server_nonce: &[u8], client_nonce: &[u8], tag: &[u8]) -> bool {
    verify(key, b"vimora-client", server_nonce, client_nonce, tag)
}

pub fn server_mac(key: &str, client_nonce: &[u8], server_nonce: &[u8]) -> Vec<u8> {
    mac(key, b"vimora-server", client_nonce, server_nonce)
}

pub fn verify_server(key: &str, client_nonce: &[u8], server_nonce: &[u8], tag: &[u8]) -> bool {
    verify(key, b"vimora-server", client_nonce, server_nonce, tag)
}

/// MAC over a privileged command. Bound to the session's client nonce and a strictly
/// increasing counter, so it can't be replayed in another session or twice.
pub fn priv_mac(key: &str, client_nonce: &[u8], counter: u64, payload: &[u8]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    m.update(b"vimora-priv");
    m.update(client_nonce);
    m.update(&counter.to_be_bytes());
    m.update(payload);
    m.finalize().into_bytes().to_vec()
}

pub fn verify_priv(
    key: &str,
    client_nonce: &[u8],
    counter: u64,
    payload: &[u8],
    tag: &[u8],
) -> bool {
    let mut m = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    m.update(b"vimora-priv");
    m.update(client_nonce);
    m.update(&counter.to_be_bytes());
    m.update(payload);
    m.verify_slice(tag).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let (s, c) = (nonce(), nonce());
        let cm = client_mac("k", &s, &c);
        assert!(verify_client("k", &s, &c, &cm));
        assert!(!verify_client("wrong", &s, &c, &cm));
        let sm = server_mac("k", &c, &s);
        assert!(verify_server("k", &c, &s, &sm));
        assert!(!verify_server("k", &s, &c, &sm));
        let pm = priv_mac("k", &c, 7, b"cmd");
        assert!(verify_priv("k", &c, 7, b"cmd", &pm));
        assert!(!verify_priv("k", &c, 8, b"cmd", &pm));
        assert!(!verify_priv("k", &s, 7, b"cmd", &pm));
    }
}
