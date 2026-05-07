---
paths:
  - "**/auth*"
  - "**/plugins/auth*"
  - "**/jwt*"
  - "**/use-auth*"
---

# Authentication Rules

Auth must work **offline-first**. The user can be authenticated locally for the current session even if the network is unreachable; refresh and re-issuance happen when connectivity returns.

## Token Architecture

- **Issuer:** the sync server (or a future shared auth service) signs RS256 JWTs.
- **Verification:** the Tauri Rust backend verifies the JWT using the bundled public key (no network call needed). The frontend trusts what Rust says.
- **Storage:** access token in memory; refresh token in OS-secure storage (Tauri stronghold plugin, or app data dir behind a Rust-only command). NEVER `localStorage` or `sessionStorage`.
- **Lifetime:** access token 15m, refresh token 30d, sliding-window refresh on use.

## JWT Fields

| Field | Purpose |
|-|-|
| `sub` | User ID (UUID). |
| `email` | User email. |
| `status` | `active` / `suspended`. |
| `isSuperadmin` | Superadmin flag. |
| `sessionId` | For server-side revocation. |
| `entityId` | Multi-tenant scope. |
| `holdingId` | Holding company scope. |
| `entityRole` | Role within entity. |
| `iat`, `exp` | Standard. |

## Offline Login Behavior

1. User logs in once while online -- server issues access + refresh tokens; tokens cached in OS-secure storage.
2. App launches offline -- Rust loads cached refresh token, verifies its JWT signature locally, checks `exp`. If still valid, the app boots into "offline-authenticated" mode.
3. App tries `POST /auth/refresh` in the background. Success: rotate tokens. Failure (no network): keep the cached access token; pause sync push to keep operations queued under the right user.
4. Cached access token expires while offline -- Rust derives a "best-effort identity" from the still-valid refresh token (still signed, still verified) and lets the user keep working. Sync push will re-auth on reconnection.
5. Refresh token expires while offline -- the user is locked out. Show a "session expired, please reconnect to log in" screen; preserve outbox so no work is lost.

## Tauri Side

- Plugin `auth.plugin.rs` (or module `auth/`) owns:
  - Public-key bundling (compile-time include or app-data-dir fetch on first run).
  - Token verification (`jsonwebtoken` crate).
  - Secure storage read/write.
  - Refresh task (background `tokio::spawn`).
- IPC commands:
  - `auth_login(email, password) -> Result<UserProfile>` -- only available when online.
  - `auth_logout() -> Result<()>` -- clears tokens and closes session on the server (best-effort).
  - `auth_get_state() -> Result<AuthState>` -- current user, online/offline auth status.
- The frontend NEVER stores or handles raw tokens. All requests go through Rust; embedded mode polls `/api/auth` from the bundled HTTP server.

## Frontend Side

- `AuthProvider` wraps the app. It calls `auth_get_state` on mount and subscribes to a `auth:changed` Tauri event.
- React Query `auth` key holds `{ user, status: 'online' | 'offline' | 'expired' | 'anonymous' }`. UI gates routes on this.
- 401s on the sync HTTP path are handled in Rust (refresh + retry once); 401s on direct frontend axios calls (login form, server-only screens) trigger a logout and redirect to `/login`.
- The login form is React Hook Form + Zod; the password is sent ONLY to the IPC command -- never to the sync server directly from the frontend.

## Sync Server Side

| Flow | Description |
|-|-|
| Register | Create user -> email-verification token -> 24h expiry. |
| Login | Verify password -> check MFA -> create session -> issue tokens. |
| MFA | Temporary token (5m) -> verify TOTP -> full access token. |
| Refresh | Validate refresh token -> rotate tokens -> session check. |
| Logout | Revoke refresh token -> revoke session. |

Server best practices:
- Verify `emailVerifiedAt` before allowing login.
- Reject suspended/deleted users (`status` check).
- Include `sessionId` in JWT for server-side revocation.
- Rotate refresh tokens on every use; old refresh tokens are invalidated immediately.
- Hash refresh tokens before storing (`sha256`).
- Generate tokens with `crypto.randomBytes(32+)`.

## Inter-Service Auth (Future)

If additional services join the sync-server stack, they authenticate to it via `x-service-api-key` header (32+ char random, per-service, stored in `.env`). The Tauri app NEVER carries a service API key.

## Common Pitfalls

- Storing tokens in `localStorage` -- they leak to any compromised webview script. Use OS-secure storage via Rust.
- Not handling clock skew on JWT `exp` -- allow a small leeway (60s) when verifying.
- Re-issuing an access token whose user is now suspended -- refresh MUST re-check status server-side.
- Sending the password to the sync server from the frontend bypasses the Rust audit boundary -- always go through `auth_login` IPC.
