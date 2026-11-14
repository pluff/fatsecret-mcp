//! ChaCha20-Poly1305 sealing of self-describing OAuth blobs (codes, tokens, client ids,
//! pending-flow state). The AAD is the blob type, so a blob of one type never opens as another.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

const NONCE_LEN: usize = 12;

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Sealed payloads carry an absolute expiry (unix seconds).
pub(crate) trait Expires {
    fn exp(&self) -> u64;
}

pub(crate) struct Sealer {
    cipher: ChaCha20Poly1305,
}

impl Sealer {
    pub(crate) fn new(secret: &str) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(&Sha256::digest(secret.as_bytes())),
        }
    }

    pub(crate) fn seal<T: Serialize>(&self, typ: &str, value: &T) -> String {
        let plain = serde_json::to_vec(value).unwrap_or_default();
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plain,
                    aad: typ.as_bytes(),
                },
            )
            .unwrap_or_default();
        let mut out = nonce.to_vec();
        out.extend(ct);
        URL_SAFE_NO_PAD.encode(out)
    }

    /// Decrypts and authenticates without checking expiry.
    pub(crate) fn open_raw<T: DeserializeOwned>(&self, typ: &str, token: &str) -> Option<T> {
        let data = URL_SAFE_NO_PAD.decode(token).ok()?;
        if data.len() <= NONCE_LEN {
            return None;
        }
        let (nonce, ct) = data.split_at(NONCE_LEN);
        let plain = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ct,
                    aad: typ.as_bytes(),
                },
            )
            .ok()?;
        serde_json::from_slice(&plain).ok()
    }

    pub(crate) fn open_at<T: DeserializeOwned + Expires>(
        &self,
        typ: &str,
        token: &str,
        now: u64,
    ) -> Option<T> {
        self.open_raw::<T>(typ, token).filter(|v| v.exp() > now)
    }

    pub(crate) fn open<T: DeserializeOwned + Expires>(&self, typ: &str, token: &str) -> Option<T> {
        self.open_at(typ, token, now_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Blob {
        exp: u64,
        v: String,
    }
    impl Expires for Blob {
        fn exp(&self) -> u64 {
            self.exp
        }
    }

    fn blob() -> Blob {
        Blob {
            exp: 1000,
            v: "hello".into(),
        }
    }

    #[test]
    fn roundtrip() {
        let s = Sealer::new("secret");
        let t = s.seal("access", &blob());
        assert_eq!(s.open_at::<Blob>("access", &t, 999), Some(blob()));
        // Plaintext is not visible in the token.
        assert!(!t.contains("hello"));
    }

    #[test]
    fn rejects_wrong_aad_secret_expiry() {
        let s = Sealer::new("secret");
        let t = s.seal("access", &blob());
        assert!(s.open_at::<Blob>("refresh", &t, 999).is_none());
        assert!(
            Sealer::new("other")
                .open_at::<Blob>("access", &t, 999)
                .is_none()
        );
        assert!(s.open_at::<Blob>("access", &t, 1000).is_none());
        assert!(s.open_at::<Blob>("access", &t, 5000).is_none());
    }

    #[test]
    fn rejects_tamper_and_garbage() {
        let s = Sealer::new("secret");
        let t = s.seal("access", &blob());
        let mut bytes = URL_SAFE_NO_PAD.decode(&t).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let tampered = URL_SAFE_NO_PAD.encode(bytes);
        assert!(s.open_at::<Blob>("access", &tampered, 1).is_none());
        for g in ["", "x", "not base64!!", "AAAA", &t[..t.len() - 4]] {
            assert!(s.open_at::<Blob>("access", g, 1).is_none(), "{g}");
        }
    }

    #[test]
    fn nonces_differ() {
        let s = Sealer::new("secret");
        assert_ne!(s.seal("access", &blob()), s.seal("access", &blob()));
    }
}
