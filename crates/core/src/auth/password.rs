//! Hashing password dengan Argon2id (default `argon2` crate).
//!
//! Verifikasi selalu mengembalikan `bool`, tidak pernah membedakan "hash rusak"
//! dari "password salah" kepada pemanggil: perbedaan itu hanya berguna bagi
//! penyerang. Hash yang tidak dapat diparse dicatat ke log dan diperlakukan
//! sebagai gagal.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use tracing::error;

/// Hash password untuk disimpan pada `users.password_hash`.
pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("gagal hash password: {error}"))
}

/// Cocokkan password dengan hash tersimpan.
pub fn verify(password: &str, stored_hash: &str) -> bool {
    let parsed = match PasswordHash::new(stored_hash) {
        Ok(parsed) => parsed,
        Err(error) => {
            error!(error = %error, "password hash tersimpan tidak dapat diparse");
            return false;
        }
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_correct_password() {
        let stored = hash("correct horse battery staple").unwrap();
        assert!(verify("correct horse battery staple", &stored));
    }

    #[test]
    fn rejects_wrong_password() {
        let stored = hash("correct horse battery staple").unwrap();
        assert!(!verify("Correct horse battery staple", &stored));
    }

    #[test]
    fn same_password_hashes_differently() {
        // Salt acak: dua hash identik akan membocorkan user berpassword sama.
        assert_ne!(hash("same").unwrap(), hash("same").unwrap());
    }

    #[test]
    fn malformed_hash_fails_closed() {
        assert!(!verify("anything", "bukan-phc-string"));
    }
}
