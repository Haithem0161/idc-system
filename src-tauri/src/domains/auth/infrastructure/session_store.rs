//! Persistent session store: keeps a signed-in session alive across app
//! restarts so the user is not forced to log in every launch.
//!
//! The session blob is written to `<app_data_dir>/session.json`, the same
//! Rust-only command boundary the pinned JWT public key uses
//! (`jwt_verifier.rs`). The frontend/webview can never read this file; only
//! Tauri commands touch it. `auth.md` blesses this on-disk approach as
//! satisfying the secure-storage invariant (the `tauri-plugin-stronghold`
//! vault is an acceptable later swap-in behind the same API).
//!
//! What is stored:
//! - `refresh_token` -- the opaque 30-day secret used to rotate sessions.
//! - `access_token` + `access_expires_at` -- the last RS256 JWT, so an offline
//!   restart can re-establish identity by re-verifying it against the pinned
//!   key without any network round-trip.
//! - the resolved `user` context (id / entity / email / name / role).
//!
//! On restore the access token is ALWAYS re-verified against the pinned key
//! (caller's responsibility) -- the file is trusted for transport, never for
//! authority.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::state::UserContext;

/// The on-disk shape. Versioned so a future field change can migrate or
/// discard an incompatible blob instead of failing to parse on launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSession {
    /// Schema version of this blob. Bump when the shape changes.
    #[serde(default = "default_version")]
    pub version: u32,
    pub refresh_token: String,
    /// The last access JWT. `None` is tolerated (e.g. an offline-only login
    /// that never received one), in which case restore relies on refresh.
    #[serde(default)]
    pub access_token: Option<String>,
    /// Unix-seconds expiry of `access_token`, mirrored from the JWT `exp`.
    #[serde(default)]
    pub access_expires_at: Option<i64>,
    /// Whether the session was locked (idle auto-lock) when persisted. Carried
    /// across restarts so restarting the app cannot bypass the lock screen --
    /// the session restores locked and the user must re-enter their password to
    /// unlock. Defaults to `false` for blobs written before this field existed.
    #[serde(default)]
    pub locked: bool,
    pub user: UserContext,
}

fn default_version() -> u32 {
    1
}

const CURRENT_VERSION: u32 = 1;

fn session_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("session.json")
}

impl PersistedSession {
    pub fn new(
        refresh_token: String,
        access_token: Option<String>,
        access_expires_at: Option<i64>,
        locked: bool,
        user: UserContext,
    ) -> Self {
        Self {
            version: CURRENT_VERSION,
            refresh_token,
            access_token,
            access_expires_at,
            locked,
            user,
        }
    }
}

/// Persist a session to `<app_data_dir>/session.json`, replacing any existing
/// blob. Best-effort: the directory is created if missing. The file is written
/// atomically (write to a temp sibling, then rename) so a crash mid-write can
/// never leave a half-written, unparseable session that would strand login.
pub fn save_session(app_data_dir: &Path, session: &PersistedSession) -> AppResult<()> {
    fs::create_dir_all(app_data_dir)
        .map_err(|e| AppError::Internal(format!("create app_data_dir: {e}")))?;
    let path = session_path(app_data_dir);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(session)
        .map_err(|e| AppError::Internal(format!("serialize session: {e}")))?;
    fs::write(&tmp, &json).map_err(|e| AppError::Internal(format!("write session tmp: {e}")))?;
    fs::rename(&tmp, &path).map_err(|e| AppError::Internal(format!("rename session: {e}")))?;
    Ok(())
}

/// Load the persisted session, or `None` when no session file exists. A file
/// that exists but cannot be parsed (corruption / an incompatible older
/// version) is treated as "no session" and removed, so a bad blob degrades to
/// a normal login instead of bricking launch.
pub fn load_session(app_data_dir: &Path) -> AppResult<Option<PersistedSession>> {
    let path = session_path(app_data_dir);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Internal(format!("read session: {e}"))),
    };
    match serde_json::from_slice::<PersistedSession>(&bytes) {
        Ok(s) if s.version == CURRENT_VERSION => Ok(Some(s)),
        // Unknown version or unparseable: discard and fall back to login.
        _ => {
            let _ = fs::remove_file(&path);
            Ok(None)
        }
    }
}

/// Remove the persisted session (logout, or any path that clears auth). Absence
/// of the file is success, not an error.
pub fn clear_session(app_data_dir: &Path) -> AppResult<()> {
    let path = session_path(app_data_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Internal(format!("remove session: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx() -> UserContext {
        UserContext {
            user_id: "01900000-0000-7000-8000-000000000001".into(),
            entity_id: "tenant-1".into(),
            email: "asma@example.com".into(),
            name: Some("Asma".into()),
            role: "accountant".into(),
        }
    }

    fn sample() -> PersistedSession {
        PersistedSession::new(
            "refresh-abc".into(),
            Some("access.jwt.here".into()),
            Some(1_900_000_000),
            false,
            ctx(),
        )
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = tempdir().unwrap();
        assert!(load_session(dir.path()).unwrap().is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        save_session(dir.path(), &sample()).unwrap();
        let loaded = load_session(dir.path()).unwrap().expect("session present");
        assert_eq!(loaded.refresh_token, "refresh-abc");
        assert_eq!(loaded.access_token.as_deref(), Some("access.jwt.here"));
        assert_eq!(loaded.access_expires_at, Some(1_900_000_000));
        assert!(!loaded.locked);
        assert_eq!(loaded.user.email, "asma@example.com");
        assert_eq!(loaded.user.role, "accountant");
    }

    #[test]
    fn locked_flag_round_trips() {
        let dir = tempdir().unwrap();
        let session = PersistedSession::new(
            "r".into(),
            Some("a".into()),
            Some(1),
            true, // locked
            ctx(),
        );
        save_session(dir.path(), &session).unwrap();
        let loaded = load_session(dir.path()).unwrap().unwrap();
        assert!(loaded.locked, "locked state must survive a save/load cycle");
    }

    #[test]
    fn locked_defaults_false_for_older_blob_without_the_field() {
        let dir = tempdir().unwrap();
        // A v1 blob written before `locked` existed omits the key entirely.
        let blob = serde_json::json!({
            "version": 1,
            "refresh_token": "r",
            "access_token": "a",
            "access_expires_at": 1,
            "user": {
                "user_id": "u", "entity_id": "t", "email": "e",
                "name": null, "role": "accountant"
            }
        });
        fs::write(
            dir.path().join("session.json"),
            serde_json::to_vec(&blob).unwrap(),
        )
        .unwrap();
        let loaded = load_session(dir.path()).unwrap().unwrap();
        assert!(!loaded.locked);
    }

    #[test]
    fn save_overwrites_previous_session() {
        let dir = tempdir().unwrap();
        save_session(dir.path(), &sample()).unwrap();
        let mut next = sample();
        next.refresh_token = "refresh-rotated".into();
        save_session(dir.path(), &next).unwrap();
        let loaded = load_session(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.refresh_token, "refresh-rotated");
    }

    #[test]
    fn clear_removes_session_and_is_idempotent() {
        let dir = tempdir().unwrap();
        save_session(dir.path(), &sample()).unwrap();
        clear_session(dir.path()).unwrap();
        assert!(load_session(dir.path()).unwrap().is_none());
        // second clear is a no-op success
        clear_session(dir.path()).unwrap();
    }

    #[test]
    fn corrupt_file_is_discarded_and_treated_as_no_session() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("session.json"), b"{ not valid json").unwrap();
        assert!(load_session(dir.path()).unwrap().is_none());
        // the corrupt file was removed
        assert!(!dir.path().join("session.json").exists());
    }

    #[test]
    fn unknown_version_is_discarded() {
        let dir = tempdir().unwrap();
        let blob = serde_json::json!({
            "version": 999,
            "refresh_token": "x",
            "user": {
                "user_id": "u", "entity_id": "t", "email": "e",
                "name": null, "role": "accountant"
            }
        });
        fs::write(
            dir.path().join("session.json"),
            serde_json::to_vec(&blob).unwrap(),
        )
        .unwrap();
        assert!(load_session(dir.path()).unwrap().is_none());
    }
}
