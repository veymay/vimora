//! Offline unlock code for the lock screen.
//!
//! The teacher sets a short numeric code; the teacher sends the student PCs only a salted,
//! iterated SHA-256 hash of it. The lock screen checks what a student types against that hash,
//! so the screen can still be opened when the teacher or the network is gone.

use sha2::{Digest, Sha256};

const ROUNDS: u32 = 60_000;

/// Hash of `code` under `salt` (iterated so guessing a short code costs real time).
pub fn hash(salt: &[u8], code: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(code.as_bytes());
    let mut out = h.finalize().to_vec();
    for _ in 0..ROUNDS {
        let mut h = Sha256::new();
        h.update(&out);
        h.update(salt);
        out = h.finalize().to_vec();
    }
    out
}

pub fn verify(salt: &[u8], hash_wanted: &[u8], code: &str) -> bool {
    !hash_wanted.is_empty() && hash(salt, code) == hash_wanted
}

/// A code is 4–8 digits.
pub fn valid(code: &str) -> bool {
    (4..=8).contains(&code.len()) && code.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let salt = b"salty";
        let h = hash(salt, "1234");
        assert!(verify(salt, &h, "1234"));
        assert!(!verify(salt, &h, "1235"));
        assert!(!verify(b"other", &h, "1234"));
        assert!(!verify(salt, &[], "1234"));
        assert!(valid("0420") && valid("12345678") && !valid("123") && !valid("12a4"));
    }
}
