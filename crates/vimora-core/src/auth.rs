//! Zero-configuration pairing: no key or port to type in.
//!
//! * The teacher creates an **Ed25519 identity** on first start and keeps the secret half.
//! * A student PC **pins** the public key of the first teacher it connects to (trust on first
//!   use) and from then on obeys only that teacher — every session and every privileged command
//!   must carry a valid signature from it.
//!
//! So installing the agent is enough for it to appear in the teacher, while a student who
//! starts their own teacher on the same network still can't control anyone's PC.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// A teacher's public key (what student PCs pin).
pub type TeacherKey = [u8; 32];

pub fn nonce() -> Vec<u8> {
    let mut n = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut n);
    n
}

/// The teacher's secret identity.
pub struct Identity(SigningKey);

impl Identity {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        Self(SigningKey::from_bytes(&seed))
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let b = from_hex(s)?;
        let seed: [u8; 32] = b.try_into().ok()?;
        Some(Self(SigningKey::from_bytes(&seed)))
    }

    pub fn to_hex(&self) -> String {
        to_hex(&self.0.to_bytes())
    }

    pub fn public(&self) -> TeacherKey {
        self.0.verifying_key().to_bytes()
    }

    fn sign(&self, parts: &[&[u8]]) -> Vec<u8> {
        self.0.sign(&message(parts)).to_bytes().to_vec()
    }

    /// Proves to a student PC that this session comes from the pinned teacher.
    pub fn sign_welcome(&self, student_nonce: &[u8], teacher_nonce: &[u8]) -> Vec<u8> {
        self.sign(&[b"vimora-welcome", student_nonce, teacher_nonce])
    }

    /// Signs a privileged command, bound to the session nonce and an increasing counter
    /// (no replay into another session, no replay at all).
    pub fn sign_priv(&self, student_nonce: &[u8], counter: u64, payload: &[u8]) -> Vec<u8> {
        self.sign(&[
            b"vimora-priv",
            student_nonce,
            &counter.to_be_bytes(),
            payload,
        ])
    }
}

fn message(parts: &[&[u8]]) -> Vec<u8> {
    // Length-prefix every part so different splits can't produce the same message.
    let mut m = Vec::new();
    for p in parts {
        m.extend_from_slice(&(p.len() as u32).to_be_bytes());
        m.extend_from_slice(p);
    }
    m
}

fn verify(key: &TeacherKey, parts: &[&[u8]], sig: &[u8]) -> bool {
    let (Ok(vk), Ok(sig)) = (VerifyingKey::from_bytes(key), Signature::from_slice(sig)) else {
        return false;
    };
    vk.verify(&message(parts), &sig).is_ok()
}

pub fn verify_welcome(
    key: &TeacherKey,
    student_nonce: &[u8],
    teacher_nonce: &[u8],
    sig: &[u8],
) -> bool {
    verify(key, &[b"vimora-welcome", student_nonce, teacher_nonce], sig)
}

pub fn verify_priv(
    key: &TeacherKey,
    student_nonce: &[u8],
    counter: u64,
    payload: &[u8],
    sig: &[u8],
) -> bool {
    verify(
        key,
        &[
            b"vimora-priv",
            student_nonce,
            &counter.to_be_bytes(),
            payload,
        ],
        sig,
    )
}

/// Short human-readable fingerprint, e.g. "3F2A-91C0-7B44".
pub fn fingerprint(key: &TeacherKey) -> String {
    let h = Sha256::digest(key);
    let hex = to_hex(&h[..6]).to_uppercase();
    format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn key_from_hex(s: &str) -> Option<TeacherKey> {
    from_hex(s)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures() {
        let id = Identity::generate();
        let pk = id.public();
        let (c, s) = (nonce(), nonce());
        let w = id.sign_welcome(&c, &s);
        assert!(verify_welcome(&pk, &c, &s, &w));
        assert!(!verify_welcome(&pk, &s, &c, &w));
        let other = Identity::generate().public();
        assert!(!verify_welcome(&other, &c, &s, &w));

        let p = id.sign_priv(&c, 7, b"cmd");
        assert!(verify_priv(&pk, &c, 7, b"cmd", &p));
        assert!(!verify_priv(&pk, &c, 8, b"cmd", &p));
        assert!(!verify_priv(&pk, &s, 7, b"cmd", &p));
        assert!(!verify_priv(&pk, &c, 7, b"cmd!", &p));
    }

    #[test]
    fn identity_roundtrip() {
        let id = Identity::generate();
        let back = Identity::from_hex(&id.to_hex()).unwrap();
        assert_eq!(id.public(), back.public());
        assert_eq!(key_from_hex(&to_hex(&id.public())), Some(id.public()));
        assert_eq!(fingerprint(&id.public()).len(), 14);
    }
}
