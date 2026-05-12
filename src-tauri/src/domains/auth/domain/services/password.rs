//! Argon2id password hashing helpers.
//!
//! Wrap the `argon2` crate with a typed-error surface so callers do not have
//! to thread `argon2::password_hash::Error` around.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use crate::error::{AppError, AppResult};

/// Hash a plaintext password to a PHC string. Uses Argon2id with default
/// (recommended) parameters.
pub fn hash_password(password: &str) -> AppResult<String> {
    if password.len() < 8 {
        return Err(AppError::Validation(
            "password must be at least 8 characters".into(),
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("argon2 hash: {e}")))?
        .to_string();
    Ok(hash)
}

/// Verify a plaintext password against a stored PHC string. Returns `Ok(())`
/// on success, `Err(AppError::NotAuthenticated)` on mismatch.
pub fn verify_password(password: &str, phc: &str) -> AppResult<()> {
    let parsed =
        PasswordHash::new(phc).map_err(|e| AppError::Internal(format!("argon2 parse: {e}")))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AppError::NotAuthenticated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let phc = hash_password("hunter22").unwrap();
        verify_password("hunter22", &phc).unwrap();
    }

    #[test]
    fn wrong_password_rejects() {
        let phc = hash_password("hunter22").unwrap();
        let err = verify_password("wrongpw1", &phc).unwrap_err();
        assert!(matches!(err, AppError::NotAuthenticated));
    }

    #[test]
    fn short_password_rejects_in_hash() {
        let err = hash_password("short").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
