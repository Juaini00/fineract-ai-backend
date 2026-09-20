//! Access token (JWT HS256) dan refresh token (opaque).
//!
//! HS256 sah **hanya** selama Jarvis menerbitkan sekaligus memverifikasi
//! (#7). Bila kelak dashboard/SSO yang menerbitkan, wajib pindah ke asimetris:
//! secret simetris bersama berarti tiap layanan dapat mencetak token milik
//! yang lain.
//!
//! Refresh token **bukan** JWT. Ia opaque dan diverifikasi lewat database,
//! karena rotasi dan pencabutan menuntut state — sesuatu yang tidak dimiliki
//! token stateless.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;

/// Isi access token. Nama field mengikuti konvensi JWT supaya perpindahan ke
/// penerbit lain (SSO) tidak mengubah kontrak klien.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User ID.
    pub sub: Uuid,
    /// `auth_sessions.id` — session autentikasi, **bukan** `chat_sessions`.
    pub sid: Uuid,
    pub role: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
}

/// Penerbit/pemverifikasi access token.
#[derive(Clone)]
pub struct TokenIssuer {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    issuer: String,
    audience: String,
    access_ttl_secs: i64,
}

impl std::fmt::Debug for TokenIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Kunci tidak pernah ikut tercetak.
        f.debug_struct("TokenIssuer")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish_non_exhaustive()
    }
}

impl TokenIssuer {
    pub fn new(config: &Config) -> Self {
        Self {
            encoding: EncodingKey::from_secret(config.jwt_access_secret.as_bytes()),
            decoding: DecodingKey::from_secret(config.jwt_access_secret.as_bytes()),
            validation: validation(&config.jwt_issuer, &config.jwt_audience),
            issuer: config.jwt_issuer.clone(),
            audience: config.jwt_audience.clone(),
            access_ttl_secs: config.jwt_access_token_expiry_seconds as i64,
        }
    }

    /// Terbitkan access token beserta detik masa berlakunya.
    pub fn issue(&self, user_id: Uuid, auth_session_id: Uuid, role: &str) -> anyhow::Result<Issued> {
        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            sub: user_id,
            sid: auth_session_id,
            role: role.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now,
            exp: now + self.access_ttl_secs,
        };

        Ok(Issued {
            token: jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)?,
            expires_in_secs: self.access_ttl_secs,
        })
    }

    /// Verifikasi signature, issuer, audience dan kedaluwarsa.
    pub fn verify(&self, token: &str) -> anyhow::Result<Claims> {
        Ok(jsonwebtoken::decode::<Claims>(token, &self.decoding, &self.validation)?.claims)
    }
}

/// Aturan verifikasi access token.
///
/// `leeway` default `jsonwebtoken` adalah 60 detik — seperdelima belas umur
/// token 15 menit, dan toleransi skew sebesar itu tidak pernah diminta. 5 detik
/// cukup untuk beda jam antar host yang ber-NTP.
fn validation(issuer: &str, audience: &str) -> Validation {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);
    // `exp` diwajibkan: token tanpa kedaluwarsa berlaku selamanya.
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    validation.leeway = 5;
    validation
}

/// Access token yang baru diterbitkan.
#[derive(Debug, Clone)]
pub struct Issued {
    pub token: String,
    pub expires_in_secs: i64,
}

/// Refresh token opaque: nilai mentah hanya ada di response/cookie, database
/// menyimpan hash-nya saja (`refresh_tokens.token_hash`).
#[derive(Debug, Clone)]
pub struct RefreshToken {
    raw: String,
}

impl RefreshToken {
    /// 32 byte acak kriptografis, dikodekan hex.
    pub fn generate() -> Self {
        use rand::RngCore;

        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self {
            raw: hex::encode(bytes),
        }
    }

    pub fn from_raw(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }

    /// Nilai mentah. Hanya untuk dikirim ke klien — tidak pernah disimpan,
    /// tidak pernah masuk log.
    pub fn expose(&self) -> &str {
        &self.raw
    }

    /// SHA-256 hex. Cukup untuk token acak 256-bit: tidak ada ruang tebakan
    /// yang membuat hash lambat memberi perlindungan tambahan seperti pada
    /// password buatan manusia.
    pub fn hash(&self) -> String {
        use sha2::{Digest, Sha256};

        hex::encode(Sha256::digest(self.raw.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> TokenIssuer {
        TokenIssuer {
            encoding: EncodingKey::from_secret(b"secret-a"),
            decoding: DecodingKey::from_secret(b"secret-a"),
            validation: validation("jarvis", "jarvis-api"),
            issuer: "jarvis".into(),
            audience: "jarvis-api".into(),
            access_ttl_secs: 900,
        }
    }

    #[test]
    fn issued_token_verifies_and_carries_identity() {
        let issuer = issuer();
        let user = Uuid::new_v4();
        let session = Uuid::new_v4();

        let issued = issuer.issue(user, session, "admin").unwrap();
        let claims = issuer.verify(&issued.token).unwrap();

        assert_eq!(claims.sub, user);
        assert_eq!(claims.sid, session);
        assert_eq!(claims.role, "admin");
        assert_eq!(issued.expires_in_secs, 900);
    }

    #[test]
    fn token_from_another_secret_is_rejected() {
        let issued = issuer().issue(Uuid::new_v4(), Uuid::new_v4(), "admin").unwrap();

        let mut foreign = issuer();
        foreign.decoding = DecodingKey::from_secret(b"secret-b");

        assert!(foreign.verify(&issued.token).is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        let mut expired = issuer();
        expired.access_ttl_secs = -60;

        let issued = expired.issue(Uuid::new_v4(), Uuid::new_v4(), "admin").unwrap();
        assert!(expired.verify(&issued.token).is_err());
    }

    #[test]
    fn wrong_audience_is_rejected() {
        let issued = issuer().issue(Uuid::new_v4(), Uuid::new_v4(), "admin").unwrap();

        let mut other = issuer();
        other.validation.set_audience(&["dashboard"]);

        assert!(other.verify(&issued.token).is_err());
    }

    #[test]
    fn refresh_token_is_random_and_stored_only_as_hash() {
        let first = RefreshToken::generate();
        let second = RefreshToken::generate();

        assert_ne!(first.expose(), second.expose());
        assert_ne!(first.hash(), first.expose());
        assert_eq!(first.hash(), RefreshToken::from_raw(first.expose()).hash());
    }
}
