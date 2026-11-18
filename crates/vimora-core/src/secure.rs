//! Encrypted, tamper-proof session channel.
//!
//! After the teacher has proven who it is (signed welcome), both sides run an **ephemeral
//! X25519** exchange. The teacher signs its public half *together with the student's* (and both
//! session nonces) with its Ed25519 identity, so nobody in the middle can substitute either half.
//! The shared secret goes through HKDF-SHA256 into one **ChaCha20-Poly1305** key per direction;
//! every record carries a counter nonce, so records cannot be altered, replayed, reordered or
//! dropped without the connection failing.
//!
//! Without this, anyone who can sit between a student PC and the teacher (ARP spoofing in a
//! classroom is enough) could inject commands into the plain stream or watch the screens.
//!
//! The record layer is transparent: [`Reader`] / [`Writer`] wrap the two halves of a TCP stream
//! and behave like plain streams until [`Reader::upgrade`] / [`Writer::upgrade`] switches the
//! encryption on, so [`crate::net::read_msg`] / [`crate::net::write_msg`] work unchanged.

use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use x25519_dalek::{EphemeralSecret, PublicKey};

/// Largest plaintext in one record.
const CHUNK: usize = 16 * 1024;
const TAG: usize = 16;

/// One side's ephemeral key pair.
pub struct Ephemeral {
    secret: EphemeralSecret,
    public: [u8; 32],
}

impl Ephemeral {
    pub fn new() -> Self {
        let secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
        let public = PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    pub fn public(&self) -> [u8; 32] {
        self.public
    }

    /// The two directional ciphers. `teacher` = this side is the teacher.
    pub fn finish(
        self,
        peer: &[u8],
        teacher_pub: &[u8; 32],
        student_pub: &[u8; 32],
        teacher: bool,
    ) -> Option<(Cipher, Cipher)> {
        let peer: [u8; 32] = peer.try_into().ok()?;
        let shared = self.secret.diffie_hellman(&PublicKey::from(peer));
        // A low-order peer key gives an all-zero secret: refuse it.
        if !shared.was_contributory() {
            return None;
        }
        let mut salt = Vec::with_capacity(64);
        salt.extend_from_slice(teacher_pub);
        salt.extend_from_slice(student_pub);
        let hk = Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes());
        let mut okm = [0u8; 64];
        hk.expand(b"vimora-channel-v1", &mut okm).ok()?;
        let t2s = Cipher::new(&okm[..32]);
        let s2t = Cipher::new(&okm[32..]);
        Some(if teacher {
            // (send, receive)
            (t2s, s2t)
        } else {
            (s2t, t2s)
        })
    }
}

impl Default for Ephemeral {
    fn default() -> Self {
        Self::new()
    }
}

/// One direction: a key and a record counter.
pub struct Cipher {
    aead: ChaCha20Poly1305,
    counter: u64,
}

impl Cipher {
    fn new(key: &[u8]) -> Self {
        Self {
            aead: ChaCha20Poly1305::new(Key::from_slice(key)),
            counter: 0,
        }
    }

    fn nonce(&mut self) -> io::Result<Nonce> {
        let c = self.counter;
        self.counter = c
            .checked_add(1)
            .ok_or_else(|| io::Error::other("record counter exhausted"))?;
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&c.to_be_bytes());
        Ok(*Nonce::from_slice(&n))
    }

    fn seal(&mut self, plain: &[u8]) -> io::Result<Vec<u8>> {
        let n = self.nonce()?;
        self.aead
            .encrypt(&n, plain)
            .map_err(|_| io::Error::other("encryption failed"))
    }

    fn open(&mut self, data: &[u8]) -> io::Result<Vec<u8>> {
        let n = self.nonce()?;
        self.aead.decrypt(&n, data).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "record rejected (altered or out of order)",
            )
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Record layer
// ---------------------------------------------------------------------------------------------

/// Read half: plain until upgraded.
pub struct Reader<R> {
    inner: R,
    cipher: Option<Cipher>,
    /// Bytes of the record being collected: 4-byte length, then the ciphertext.
    raw: Vec<u8>,
    plain: Vec<u8>,
    pos: usize,
}

impl<R> Reader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            cipher: None,
            raw: Vec::new(),
            plain: Vec::new(),
            pos: 0,
        }
    }

    /// From now on every byte is an encrypted record.
    pub fn upgrade(&mut self, c: Cipher) {
        self.cipher = Some(c);
    }

    pub fn is_secure(&self) -> bool {
        self.cipher.is_some()
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Reader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.cipher.is_none() {
            return Pin::new(&mut this.inner).poll_read(cx, buf);
        }
        loop {
            if this.pos < this.plain.len() {
                let n = (this.plain.len() - this.pos).min(buf.remaining());
                buf.put_slice(&this.plain[this.pos..this.pos + n]);
                this.pos += n;
                return Poll::Ready(Ok(()));
            }
            // Next record: length first, then the body.
            let need = if this.raw.len() < 4 {
                4
            } else {
                let len = u32::from_be_bytes([this.raw[0], this.raw[1], this.raw[2], this.raw[3]])
                    as usize;
                if !(TAG..=CHUNK + TAG).contains(&len) {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "bad record length",
                    )));
                }
                4 + len
            };
            while this.raw.len() < need {
                let mut tmp = [0u8; 8192];
                let want = (need - this.raw.len()).min(tmp.len());
                let mut rb = ReadBuf::new(&mut tmp[..want]);
                ready!(Pin::new(&mut this.inner).poll_read(cx, &mut rb))?;
                if rb.filled().is_empty() {
                    // Clean end only between records.
                    return if this.raw.is_empty() {
                        Poll::Ready(Ok(()))
                    } else {
                        Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()))
                    };
                }
                this.raw.extend_from_slice(rb.filled());
            }
            if need == 4 {
                continue; // the length is known now: collect the body
            }
            let cipher = this.cipher.as_mut().expect("checked above");
            this.plain = cipher.open(&this.raw[4..])?;
            this.pos = 0;
            this.raw.clear();
        }
    }
}

/// Write half: plain until upgraded.
pub struct Writer<W> {
    inner: W,
    cipher: Option<Cipher>,
    /// A sealed record not fully written yet.
    out: Vec<u8>,
    out_pos: usize,
}

impl<W> Writer<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            cipher: None,
            out: Vec::new(),
            out_pos: 0,
        }
    }

    pub fn upgrade(&mut self, c: Cipher) {
        self.cipher = Some(c);
    }

    pub fn is_secure(&self) -> bool {
        self.cipher.is_some()
    }
}

impl<W: AsyncWrite + Unpin> Writer<W> {
    fn drain(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.out_pos < self.out.len() {
            let n = ready!(Pin::new(&mut self.inner).poll_write(cx, &self.out[self.out_pos..]))?;
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.out_pos += n;
        }
        self.out.clear();
        self.out_pos = 0;
        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Writer<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.cipher.is_none() {
            return Pin::new(&mut this.inner).poll_write(cx, buf);
        }
        ready!(this.drain(cx))?;
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let n = buf.len().min(CHUNK);
        let sealed = this
            .cipher
            .as_mut()
            .expect("checked above")
            .seal(&buf[..n])?;
        this.out
            .extend_from_slice(&(sealed.len() as u32).to_be_bytes());
        this.out.extend_from_slice(&sealed);
        // Accepted; the record goes out now if the socket takes it, else on the next call/flush.
        let _ = this.drain(cx)?;
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.drain(cx))?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.drain(cx))?;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{read_msg, write_msg};
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    fn pair() -> ((Cipher, Cipher), (Cipher, Cipher)) {
        let (t, s) = (Ephemeral::new(), Ephemeral::new());
        let (tp, sp) = (t.public(), s.public());
        let a = t.finish(&sp, &tp, &sp, true).unwrap();
        let b = s.finish(&tp, &tp, &sp, false).unwrap();
        (a, b)
    }

    #[tokio::test]
    async fn messages_roundtrip_both_ways() {
        let ((t_send, t_recv), (s_send, s_recv)) = pair();
        let (a, b) = duplex(1 << 16);
        let (ar, aw) = tokio::io::split(a);
        let (br, bw) = tokio::io::split(b);
        let (mut tr, mut tw) = (Reader::new(ar), Writer::new(aw));
        let (mut sr, mut sw) = (Reader::new(br), Writer::new(bw));
        tr.upgrade(t_recv);
        tw.upgrade(t_send);
        sr.upgrade(s_recv);
        sw.upgrade(s_send);
        // Larger than one record, to cross the chunk boundary.
        let big: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();
        let big2 = big.clone();
        let h = tokio::spawn(async move {
            write_msg(&mut tw, &big2).await.unwrap();
            let back: Vec<u8> = read_msg(&mut tr).await.unwrap();
            back
        });
        let got: Vec<u8> = read_msg(&mut sr).await.unwrap();
        assert_eq!(got, big);
        write_msg(&mut sw, &got).await.unwrap();
        assert_eq!(h.await.unwrap(), big);
    }

    #[tokio::test]
    async fn tampering_is_rejected() {
        let ((t_send, _), (_, s_recv)) = pair();
        let (a, b) = duplex(1 << 16);
        let mut tw = Writer::new(a);
        tw.upgrade(t_send);
        tw.write_all(b"hello").await.unwrap();
        tw.flush().await.unwrap();
        // Flip a byte in transit.
        let mut raw = vec![0u8; 4 + 5 + TAG];
        let mut b = b;
        b.read_exact(&mut raw).await.unwrap();
        raw[6] ^= 1;
        let (x, mut y) = duplex(1 << 16);
        y.write_all(&raw).await.unwrap();
        let mut sr = Reader::new(x);
        sr.upgrade(s_recv);
        let mut out = [0u8; 5];
        assert!(sr.read_exact(&mut out).await.is_err());
    }

    #[tokio::test]
    async fn plain_until_upgraded() {
        let (a, b) = duplex(1024);
        let mut w = Writer::new(a);
        let mut r = Reader::new(b);
        w.write_all(b"abc").await.unwrap();
        w.flush().await.unwrap();
        let mut out = [0u8; 3];
        r.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"abc");
        assert!(!r.is_secure() && !w.is_secure());
    }

    #[test]
    fn different_peers_get_different_keys() {
        let (t, s) = (Ephemeral::new(), Ephemeral::new());
        let (tp, sp) = (t.public(), s.public());
        // A man in the middle with its own key cannot match the real student's cipher.
        let mitm = Ephemeral::new();
        let mp = mitm.public();
        let (mut send, _) = t.finish(&mp, &tp, &sp, true).unwrap();
        let (_, mut recv) = s.finish(&tp, &tp, &sp, false).unwrap();
        let sealed = send.seal(b"x").unwrap();
        assert!(recv.open(&sealed).is_err());
    }

    #[test]
    fn low_order_key_refused() {
        let t = Ephemeral::new();
        let tp = t.public();
        assert!(t.finish(&[0u8; 32], &tp, &tp, true).is_none());
    }
}
