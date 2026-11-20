//! TLS for the Cast control channel (port 8009), narrowly scoped.
//!
//! Real receivers (Chromecast Ultra and clones, smart boards) present a **legacy X.509 version 1
//! certificate**, signed with SHA-1, valid for only a couple of days and shared by many
//! devices. A normal verifier (webpki) rejects such a certificate with `UnsupportedCertVersion`
//! before looking at anything else, and no public CA signs these anyway: Cast does not use the
//! web PKI for its TLS certificate.
//!
//! What this verifier does instead — and only for the Cast control channel, only towards a
//! display the teacher picked from the discovery list:
//!
//! 1. the certificate must be well-formed X.509 (version 1, 2 or 3; every field present, nothing
//!    trailing) — garbage, truncated or alien versions are refused;
//! 2. the TLS handshake signature must verify against *that certificate's* public key (so the
//!    peer really holds the key it presented, and the handshake cannot be replayed);
//! 3. nothing else is trusted: there is no CA check, so the display's identity is **not**
//!    proven. A hostile device on the same network could pose as the display and receive the
//!    pictures. (Chrome closes that gap with Cast "device authentication"; see
//!    `docs/SCREENSHARE.md`.)
//!
//! Every other TLS connection in Vimora keeps full verification; this module is not used by them.

use std::sync::Arc;
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::SignatureScheme;

#[derive(Debug, PartialEq, Eq)]
pub enum CertError {
    Truncated,
    BadStructure(&'static str),
    UnsupportedVersion(u8),
}

/// What is read from a certificate.
#[derive(Debug, PartialEq, Eq)]
pub struct CertInfo {
    /// 1, 2 or 3.
    pub version: u8,
    /// `subjectPublicKey` bits (an RSAPublicKey or an EC point), without the BIT STRING header.
    pub key: Vec<u8>,
    /// OID bytes of the key algorithm (rsaEncryption, id-ecPublicKey, id-Ed25519).
    pub key_alg: Vec<u8>,
}

/// One DER element: (tag, content, rest).
fn tlv(b: &[u8]) -> Result<(u8, &[u8], &[u8]), CertError> {
    let (&tag, rest) = b.split_first().ok_or(CertError::Truncated)?;
    let (&l0, rest) = rest.split_first().ok_or(CertError::Truncated)?;
    let (len, rest) = if l0 < 0x80 {
        (l0 as usize, rest)
    } else {
        let n = (l0 & 0x7f) as usize;
        if n == 0 || n > 4 {
            return Err(CertError::BadStructure("length"));
        }
        if rest.len() < n {
            return Err(CertError::Truncated);
        }
        let len = rest[..n].iter().fold(0usize, |a, &x| (a << 8) | x as usize);
        (len, &rest[n..])
    };
    if rest.len() < len {
        return Err(CertError::Truncated);
    }
    Ok((tag, &rest[..len], &rest[len..]))
}

fn expect<'a>(b: &'a [u8], tag: u8, what: &'static str) -> Result<(&'a [u8], &'a [u8]), CertError> {
    let (t, c, rest) = tlv(b)?;
    if t != tag {
        return Err(CertError::BadStructure(what));
    }
    Ok((c, rest))
}

/// Reads the parts of an X.509 certificate that the handshake check needs.
pub fn parse_cert(der: &[u8]) -> Result<CertInfo, CertError> {
    let (cert, trailing) = expect(der, 0x30, "certificate")?;
    if !trailing.is_empty() {
        return Err(CertError::BadStructure("trailing data"));
    }
    let (tbs, rest) = expect(cert, 0x30, "tbsCertificate")?;
    let (_sig_alg, rest) = expect(rest, 0x30, "signatureAlgorithm")?;
    let (sig, rest) = expect(rest, 0x03, "signature")?;
    if !rest.is_empty() || sig.is_empty() {
        return Err(CertError::BadStructure("signature"));
    }
    // tbsCertificate
    let mut t = tbs;
    let mut version = 1u8;
    if t.first() == Some(&0xa0) {
        let (v, rest) = expect(t, 0xa0, "version")?;
        let (n, _) = expect(v, 0x02, "version number")?;
        let n = *n.last().ok_or(CertError::BadStructure("version number"))?;
        if n > 2 || tlv(v)?.1.len() != 1 {
            return Err(CertError::UnsupportedVersion(n.saturating_add(1)));
        }
        version = n + 1;
        t = rest;
    }
    let (_serial, t) = expect(t, 0x02, "serialNumber")?;
    let (_alg, t) = expect(t, 0x30, "signature")?;
    let (_issuer, t) = expect(t, 0x30, "issuer")?;
    let (_validity, t) = expect(t, 0x30, "validity")?;
    let (_subject, t) = expect(t, 0x30, "subject")?;
    let (spki, _extensions) = expect(t, 0x30, "subjectPublicKeyInfo")?;
    let (alg, rest) = expect(spki, 0x30, "key algorithm")?;
    let (oid, _) = expect(alg, 0x06, "key algorithm oid")?;
    let (bits, rest) = expect(rest, 0x03, "subjectPublicKey")?;
    if !rest.is_empty() || bits.len() < 2 || bits[0] != 0 {
        return Err(CertError::BadStructure("subjectPublicKey"));
    }
    Ok(CertInfo {
        version,
        key: bits[1..].to_vec(),
        key_alg: oid.to_vec(),
    })
}

/// SHA-256 of the certificate, for the diagnostics ("A1:B2:…").
pub fn fingerprint(der: &[u8]) -> String {
    use ring::digest;
    digest::digest(&digest::SHA256, der)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn algorithm(s: SignatureScheme) -> Option<&'static dyn ring::signature::VerificationAlgorithm> {
    use ring::signature as sg;
    Some(match s {
        SignatureScheme::RSA_PSS_SHA256 => &sg::RSA_PSS_2048_8192_SHA256,
        SignatureScheme::RSA_PSS_SHA384 => &sg::RSA_PSS_2048_8192_SHA384,
        SignatureScheme::RSA_PSS_SHA512 => &sg::RSA_PSS_2048_8192_SHA512,
        SignatureScheme::RSA_PKCS1_SHA256 => &sg::RSA_PKCS1_2048_8192_SHA256,
        SignatureScheme::RSA_PKCS1_SHA384 => &sg::RSA_PKCS1_2048_8192_SHA384,
        SignatureScheme::RSA_PKCS1_SHA512 => &sg::RSA_PKCS1_2048_8192_SHA512,
        SignatureScheme::ECDSA_NISTP256_SHA256 => &sg::ECDSA_P256_SHA256_ASN1,
        SignatureScheme::ECDSA_NISTP384_SHA384 => &sg::ECDSA_P384_SHA384_ASN1,
        SignatureScheme::ED25519 => &sg::ED25519,
        _ => return None,
    })
}

/// Checks that `signature` over `message` was made by the key inside `cert`.
fn check_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    scheme: SignatureScheme,
    signature: &[u8],
) -> Result<(), rustls::Error> {
    use rustls::{CertificateError, Error};
    let info =
        parse_cert(cert).map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    let alg = algorithm(scheme).ok_or_else(|| {
        Error::PeerMisbehaved(rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme)
    })?;
    ring::signature::UnparsedPublicKey::new(alg, &info.key)
        .verify(message, signature)
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadSignature))
}

/// The Cast control channel's verifier (see the module notes for exactly what it checks).
#[derive(Debug)]
pub struct CastVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    /// SHA-256 of the certificate the display presented (for the diagnostics).
    seen: std::sync::Mutex<Option<String>>,
}

impl CastVerifier {
    pub fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self {
            provider,
            seen: Default::default(),
        })
    }

    /// Fingerprint of the certificate seen in the last handshake.
    pub fn seen_fingerprint(&self) -> Option<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl rustls::client::danger::ServerCertVerifier for CastVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        parse_cert(end_entity).map_err(|e| {
            tracing::warn!("cast: refused the display's certificate: {e:?}");
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;
        *self.seen.lock().unwrap() = Some(fingerprint(end_entity));
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        check_signature(message, cert, dss.scheme, dss.signature())?;
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        check_signature(message, cert, dss.scheme, dss.signature())?;
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_v1() -> Vec<u8> {
        let hex = include_str!("../tests/fixtures/legacy_v1.der.hex").trim();
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn real_receiver_certificate_is_version_1_and_accepted() {
        let info = parse_cert(&real_v1()).unwrap();
        assert_eq!(info.version, 1);
        // rsaEncryption
        assert_eq!(
            info.key_alg,
            [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01]
        );
        assert!(info.key.len() > 256); // an RSA-2048 key
    }

    #[test]
    fn modern_certificate_is_version_3() {
        let c = rcgen::generate_simple_self_signed(vec!["x".into()]).unwrap();
        assert_eq!(parse_cert(c.cert.der()).unwrap().version, 3);
    }

    #[test]
    fn truncated_garbage_and_trailing_data_are_refused() {
        let der = real_v1();
        for cut in [0, 1, 3, 10, 100, der.len() - 1] {
            assert!(parse_cert(&der[..cut]).is_err(), "cut at {cut}");
        }
        assert!(parse_cert(b"not a certificate").is_err());
        assert!(parse_cert(&[0x30, 0x00]).is_err());
        let mut more = der.clone();
        more.push(0);
        assert_eq!(
            parse_cert(&more),
            Err(CertError::BadStructure("trailing data"))
        );
    }

    #[test]
    fn unknown_certificate_version_is_refused() {
        // Version [0] INTEGER 5 (a "version 6"): find the A0 03 02 01 02 of a v3 cert and bump it.
        let c = rcgen::generate_simple_self_signed(vec!["x".into()]).unwrap();
        let mut der = c.cert.der().to_vec();
        let at = der
            .windows(5)
            .position(|w| w == [0xa0, 0x03, 0x02, 0x01, 0x02])
            .expect("version field");
        der[at + 4] = 5;
        assert_eq!(parse_cert(&der), Err(CertError::UnsupportedVersion(6)));
    }

    #[test]
    fn signature_must_come_from_the_certificates_key() {
        // A real ECDSA signature by the certificate's own key verifies; a signature by another
        // key (and a changed message) does not.
        use ring::rand::SystemRandom;
        use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
        let c = rcgen::generate_simple_self_signed(vec!["x".into()]).unwrap();
        let rng = SystemRandom::new();
        let pair = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            &c.key_pair.serialize_der(),
            &rng,
        )
        .unwrap();
        let sig = pair.sign(&rng, b"handshake").unwrap();
        let der = CertificateDer::from(c.cert.der().to_vec());
        let scheme = SignatureScheme::ECDSA_NISTP256_SHA256;
        assert!(check_signature(b"handshake", &der, scheme, sig.as_ref()).is_ok());
        assert!(check_signature(b"changed", &der, scheme, sig.as_ref()).is_err());
        let other = rcgen::generate_simple_self_signed(vec!["y".into()]).unwrap();
        let other_der = CertificateDer::from(other.cert.der().to_vec());
        assert!(check_signature(b"handshake", &other_der, scheme, sig.as_ref()).is_err());
        // A scheme that was not offered is a protocol violation, not a pass.
        assert!(check_signature(b"handshake", &der, SignatureScheme::RSA_PKCS1_SHA1, &[]).is_err());
    }
}
