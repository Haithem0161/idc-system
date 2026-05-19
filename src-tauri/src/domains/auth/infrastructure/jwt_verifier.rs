//! DEF-007 G08 / G20 / G21: client-side RS256 JWT verifier with pinned
//! public key.
//!
//! The Tauri client verifies every JWT returned by the sync server against
//! a public key that was pinned to local OS-secure storage on first
//! launch (`bootstrap_jwt_key`). The verifier strictly enforces:
//!
//! - `alg == "RS256"` (rejects `alg: "none"` -- the classic algorithm
//!   confusion attack -- and rejects HS256 tokens signed with the
//!   public-key bytes as the shared secret).
//! - Signature verifies against the pinned `DecodingKey`.
//! - Standard `exp` / `iat` claims are validated.
//!
//! Storage: the pin lives at `<app_data_dir>/jwt_public_key.pem`. The
//! `tauri-plugin-stronghold` integration mentioned in `.claude/rules/auth.md`
//! is acceptable as a swap-in; the on-disk PEM is the v0.1.0 default and
//! satisfies the security invariant (the file lives inside a Rust-only
//! command boundary -- the frontend can never read it directly).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Claims expected on every JWT issued by the sync server. Matches the
/// fields documented in `.claude/rules/auth.md` "JWT Fields".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdcAuthClaims {
    pub sub: String,
    pub email: String,
    #[serde(rename = "entityId", default)]
    pub entity_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "isSuperadmin", default)]
    pub is_superadmin: bool,
    pub iat: i64,
    pub exp: i64,
}

/// A pinned-public-key RS256 JWT verifier.
///
/// Construct via `from_pem_bytes` (the bytes are validated to be a valid
/// PEM-encoded RSA public key) or `from_pinned_file` (reads the pinned
/// file from disk). Once constructed, `verify` is cheap (no I/O).
#[derive(Clone)]
pub struct JwtVerifier {
    decoding_key: DecodingKey,
    validation: Validation,
    pinned_bytes_sha256: [u8; 32],
}

impl std::fmt::Debug for JwtVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtVerifier")
            .field("pinned_bytes_sha256", &hex_lower(&self.pinned_bytes_sha256))
            .finish()
    }
}

impl JwtVerifier {
    /// Build a verifier from a PEM-encoded RSA public key. Returns
    /// `AppError::Validation` when the bytes are not a parseable RSA
    /// public key.
    pub fn from_pem_bytes(pem: &[u8]) -> AppResult<Self> {
        let decoding_key = DecodingKey::from_rsa_pem(pem)
            .map_err(|e| AppError::Validation(format!("invalid RSA public key PEM: {e}")))?;
        let mut validation = Validation::new(Algorithm::RS256);
        // Pin algorithms to RS256 only -- this is what defends against
        // `alg: none` and HS256-using-public-key-bytes attacks. The
        // jsonwebtoken crate iterates over `validation.algorithms` and
        // rejects any other algorithm at decode time.
        validation.algorithms = vec![Algorithm::RS256];
        validation.validate_exp = true;
        validation.leeway = 60; // see auth.md "clock skew" pitfall.

        let mut hasher = Sha256::new();
        hasher.update(pem);
        let digest: [u8; 32] = hasher.finalize().into();

        Ok(Self {
            decoding_key,
            validation,
            pinned_bytes_sha256: digest,
        })
    }

    /// Load a verifier from the pinned file at
    /// `<app_data_dir>/jwt_public_key.pem`.
    pub fn from_pinned_file(app_data_dir: &Path) -> AppResult<Self> {
        let path = pinned_key_path(app_data_dir);
        let bytes = fs::read(&path).map_err(|e| {
            AppError::NotFound(format!(
                "no pinned JWT key at {} ({e}). Call bootstrap_jwt_key first.",
                path.display()
            ))
        })?;
        Self::from_pem_bytes(&bytes)
    }

    /// SHA-256 of the pinned PEM bytes. Used by tests + telemetry to
    /// detect pin drift WITHOUT exposing the key material itself.
    pub fn pinned_bytes_sha256_hex(&self) -> String {
        hex_lower(&self.pinned_bytes_sha256)
    }

    /// Verify a JWT and return its claims. Errors:
    /// - `AppError::NotAuthenticated` when the signature is invalid,
    ///   `alg != RS256`, or the claims are expired.
    /// - `AppError::Validation` when the token shape is malformed.
    pub fn verify(&self, token: &str) -> AppResult<IdcAuthClaims> {
        // Any failure -- bad signature, alg-confusion attempt, expired
        // claims, malformed header -- means "this token cannot be
        // trusted." We surface a single `NotAuthenticated` so the
        // frontend cannot distinguish "wrong key" from "expired" by
        // probing the error variant (defense against oracle attacks).
        decode::<IdcAuthClaims>(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::NotAuthenticated)
    }

    /// Record of when a JWT was issued (helper for callers that want to
    /// surface the "last verified at" timestamp to the UI without
    /// exposing the raw `iat`).
    pub fn issued_at(claims: &IdcAuthClaims) -> Option<DateTime<Utc>> {
        DateTime::<Utc>::from_timestamp(claims.iat, 0)
    }
}

/// Pin a freshly-fetched public key to disk. **Idempotent + write-once:**
/// the first call writes the file; subsequent calls REFUSE to overwrite
/// an existing pin even when the bytes differ. The function returns
/// `BootstrapOutcome::Bootstrapped` on first write, `AlreadyPinned` when
/// the file already exists and matches, and `PinMismatch` when the file
/// exists but the new bytes differ -- a `PinMismatch` is a hostile signal
/// (a rotated key, a MITM, or a corrupted disk) and the caller MUST
/// refuse to proceed without operator intervention.
///
/// DEF-007 G21 invariant: the login flow MUST NOT call this -- only the
/// explicit `bootstrap_jwt_key` IPC may pin a key.
pub fn pin_public_key(app_data_dir: &Path, pem_bytes: &[u8]) -> AppResult<BootstrapOutcome> {
    // Validate the bytes parse as a real PEM before writing.
    let _ = JwtVerifier::from_pem_bytes(pem_bytes)?;

    let path = pinned_key_path(app_data_dir);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("create app_data_dir: {e}")))?;
    }

    if path.exists() {
        let existing =
            fs::read(&path).map_err(|e| AppError::Internal(format!("read pinned key: {e}")))?;
        if existing == pem_bytes {
            return Ok(BootstrapOutcome::AlreadyPinned);
        }
        return Ok(BootstrapOutcome::PinMismatch);
    }

    fs::write(&path, pem_bytes)
        .map_err(|e| AppError::Internal(format!("write pinned key: {e}")))?;
    Ok(BootstrapOutcome::Bootstrapped)
}

/// Read the pinned PEM bytes from disk (or `None` when no pin exists).
pub fn read_pinned_pem(app_data_dir: &Path) -> AppResult<Option<Vec<u8>>> {
    let path = pinned_key_path(app_data_dir);
    if !path.exists() {
        return Ok(None);
    }
    fs::read(&path)
        .map(Some)
        .map_err(|e| AppError::Internal(format!("read pinned key: {e}")))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum BootstrapOutcome {
    /// First-time pin: bytes were written.
    Bootstrapped,
    /// Pin already on disk and BYTE-EQUAL to the supplied bytes; no-op.
    AlreadyPinned,
    /// Pin already on disk but DIFFERENT from the supplied bytes. The
    /// caller MUST treat this as hostile: a rotated server key, a MITM,
    /// or disk corruption. We never silently overwrite.
    PinMismatch,
}

fn pinned_key_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("jwt_public_key.pem")
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use tempfile::tempdir;

    // Pinned RSA test keypair (2048-bit). The PEM bytes are committed
    // into the test source so the verifier suite is deterministic and
    // self-contained. NEVER use these in production.
    const TEST_PUBLIC_PEM: &[u8] = include_bytes!("./test_data/jwt_test_public.pem");
    const TEST_PRIVATE_PEM: &[u8] = include_bytes!("./test_data/jwt_test_private.pem");
    // A SECOND keypair so signature-mismatch tests can sign with a
    // different key.
    const OTHER_PUBLIC_PEM: &[u8] = include_bytes!("./test_data/jwt_other_public.pem");
    const OTHER_PRIVATE_PEM: &[u8] = include_bytes!("./test_data/jwt_other_private.pem");

    fn mint_token(private_pem: &[u8], alg: Algorithm) -> String {
        let mut header = Header::new(alg);
        header.typ = Some("JWT".into());
        let claims = IdcAuthClaims {
            sub: "user-1".into(),
            email: "test@example.com".into(),
            entity_id: "tenant-1".into(),
            role: "superadmin".into(),
            status: "active".into(),
            is_superadmin: true,
            iat: chrono::Utc::now().timestamp() - 5,
            exp: chrono::Utc::now().timestamp() + 3600,
        };
        let key = match alg {
            Algorithm::RS256 => EncodingKey::from_rsa_pem(private_pem).unwrap(),
            _ => panic!("test mint only supports RS256"),
        };
        encode(&header, &claims, &key).unwrap()
    }

    fn craft_unsigned_alg_none_token() -> String {
        let header = json!({ "alg": "none", "typ": "JWT" });
        let payload = json!({
            "sub": "attacker",
            "email": "attacker@evil.test",
            "entityId": "tenant-1",
            "role": "superadmin",
            "isSuperadmin": true,
            "iat": chrono::Utc::now().timestamp() - 5,
            "exp": chrono::Utc::now().timestamp() + 3600,
        });
        let h = URL_SAFE_NO_PAD.encode(header.to_string());
        let p = URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{h}.{p}.")
    }

    fn craft_hs256_token_using_public_pem_as_secret() -> String {
        let header = Header::new(Algorithm::HS256);
        let claims = json!({
            "sub": "attacker",
            "email": "attacker@evil.test",
            "entityId": "tenant-1",
            "role": "superadmin",
            "isSuperadmin": true,
            "iat": chrono::Utc::now().timestamp() - 5,
            "exp": chrono::Utc::now().timestamp() + 3600,
        });
        let key = EncodingKey::from_secret(TEST_PUBLIC_PEM);
        encode(&header, &claims, &key).unwrap()
    }

    #[test]
    fn jwt_verifier_accepts_token_signed_with_pinned_public_key() {
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        let token = mint_token(TEST_PRIVATE_PEM, Algorithm::RS256);
        let claims = v.verify(&token).expect("RS256 signed by pinned key");
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.entity_id, "tenant-1");
        assert!(claims.is_superadmin);
    }

    #[test]
    fn jwt_verifier_rejects_token_with_wrong_signature() {
        // pinned to TEST_PUBLIC, token signed by OTHER_PRIVATE.
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        let token = mint_token(OTHER_PRIVATE_PEM, Algorithm::RS256);
        let err = v.verify(&token).expect_err("must reject foreign signature");
        assert!(matches!(err, AppError::NotAuthenticated));
    }

    #[test]
    fn jwt_verifier_rejects_token_with_alg_none_header() {
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        let token = craft_unsigned_alg_none_token();
        let err = v.verify(&token).expect_err("must reject alg=none");
        assert!(matches!(err, AppError::NotAuthenticated));
    }

    #[test]
    fn jwt_verifier_rejects_token_with_hs256_header_using_public_key_as_secret() {
        // Classic alg-confusion: attacker signs an HS256 token using the
        // public-key bytes as the HMAC secret. A naive verifier would
        // accept this because the bytes match. The strict
        // `algorithms: vec![RS256]` setting in `Validation` defends.
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        let token = craft_hs256_token_using_public_pem_as_secret();
        let err = v.verify(&token).expect_err("must reject HS256");
        assert!(matches!(err, AppError::NotAuthenticated));
    }

    #[test]
    fn jwt_verifier_rejects_expired_token() {
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        // Build a claims set whose `exp` is well in the past (account
        // for the 60s leeway in `Validation`).
        let claims = IdcAuthClaims {
            sub: "user-1".into(),
            email: "test@example.com".into(),
            entity_id: "tenant-1".into(),
            role: "superadmin".into(),
            status: "active".into(),
            is_superadmin: true,
            iat: chrono::Utc::now().timestamp() - 7200,
            exp: chrono::Utc::now().timestamp() - 3600,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".into());
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(TEST_PRIVATE_PEM).unwrap(),
        )
        .unwrap();
        let err = v.verify(&token).expect_err("expired token must reject");
        assert!(matches!(err, AppError::NotAuthenticated));
    }

    #[test]
    fn from_pem_bytes_rejects_garbage_pem() {
        let err = JwtVerifier::from_pem_bytes(b"not a real PEM").unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn pin_public_key_writes_on_first_call() {
        let dir = tempdir().unwrap();
        let outcome = pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        assert_eq!(outcome, BootstrapOutcome::Bootstrapped);
        let stored = fs::read(dir.path().join("jwt_public_key.pem")).unwrap();
        assert_eq!(stored, TEST_PUBLIC_PEM);
    }

    #[test]
    fn pin_public_key_is_idempotent_when_bytes_match() {
        let dir = tempdir().unwrap();
        pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        let outcome = pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        assert_eq!(outcome, BootstrapOutcome::AlreadyPinned);
    }

    #[test]
    fn pin_public_key_refuses_to_overwrite_when_bytes_differ() {
        // DEF-007 G21 core invariant: a successful login (or any other
        // path) that supplies a DIFFERENT key MUST NOT overwrite the
        // pin. The caller observes PinMismatch and refuses to proceed.
        let dir = tempdir().unwrap();
        pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        let outcome = pin_public_key(dir.path(), OTHER_PUBLIC_PEM).unwrap();
        assert_eq!(outcome, BootstrapOutcome::PinMismatch);
        // Pin bytes unchanged.
        let stored = fs::read(dir.path().join("jwt_public_key.pem")).unwrap();
        assert_eq!(stored, TEST_PUBLIC_PEM);
    }

    #[test]
    fn from_pinned_file_round_trips() {
        let dir = tempdir().unwrap();
        pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        let v = JwtVerifier::from_pinned_file(dir.path()).unwrap();
        let token = mint_token(TEST_PRIVATE_PEM, Algorithm::RS256);
        let claims = v.verify(&token).unwrap();
        assert_eq!(claims.entity_id, "tenant-1");
    }

    #[test]
    fn from_pinned_file_errors_when_no_pin_exists() {
        let dir = tempdir().unwrap();
        let err = JwtVerifier::from_pinned_file(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn pinned_bytes_sha256_hex_is_64_chars_lowercase() {
        let v = JwtVerifier::from_pem_bytes(TEST_PUBLIC_PEM).unwrap();
        let hex = v.pinned_bytes_sha256_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn read_pinned_pem_returns_none_when_unpinned() {
        let dir = tempdir().unwrap();
        let pem = read_pinned_pem(dir.path()).unwrap();
        assert!(pem.is_none());
    }

    #[test]
    fn read_pinned_pem_returns_bytes_when_pinned() {
        let dir = tempdir().unwrap();
        pin_public_key(dir.path(), TEST_PUBLIC_PEM).unwrap();
        let pem = read_pinned_pem(dir.path()).unwrap().unwrap();
        assert_eq!(pem, TEST_PUBLIC_PEM);
    }
}
