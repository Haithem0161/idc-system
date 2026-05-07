/**
 * Embedded mode detection and auth utilities for Business OS integration.
 *
 * In embedded mode, the app runs inside Business OS's webview as a plain
 * web page served via HTTP. The Tauri IPC bridge (window.__TAURI_INTERNALS__)
 * is not available, so all backend communication happens via HTTP REST API.
 */

/** Check if running inside a Tauri webview (Tauri IPC bridge present) */
export function isTauriEnvironment(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/**
 * Detect whether the app is running in embedded mode.
 *
 * In standalone Tauri mode, `window.__TAURI_INTERNALS__` is injected by the
 * Tauri runtime. In embedded mode, the frontend is served as a plain web page
 * and this global does not exist.
 *
 * Note: This is a heuristic. The AuthProvider confirms embedded mode by
 * probing `/api/auth` — if the probe fails, it falls back to standalone.
 */
export function isEmbeddedMode(): boolean {
  return !isTauriEnvironment();
}

/**
 * Auth status response from the embedded HTTP API.
 *
 * Field names match the Rust AuthResponse/AuthUserInfo structs
 * (camelCase via serde rename_all).
 */
export interface EmbeddedAuthResponse {
  authenticated: boolean;
  token: string | null;
  expiresAt: number | null;
  user: {
    userId: string;
    entityId: string;
    email: string;
    name: string | null;
    entityRole: string;
    permissions: string[];
  } | null;
}

/**
 * Fetch current auth status from the embedded HTTP API.
 * Returns null if the request fails (server not ready yet, or not in embedded mode).
 */
export async function fetchEmbeddedAuth(): Promise<EmbeddedAuthResponse | null> {
  try {
    const res = await fetch("/api/auth");
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

/**
 * Poll the embedded auth endpoint until authentication is established.
 *
 * Business OS sends the AuthToken via IPC after the app connects. This
 * function polls until that token arrives and the user context is set.
 *
 * @param intervalMs - Polling interval in milliseconds (default: 1000)
 * @param timeoutMs - Maximum wait time in milliseconds (default: 60000)
 * @returns The auth status once authenticated, or null if timed out.
 */
export async function waitForEmbeddedAuth(
  intervalMs = 1000,
  timeoutMs = 60000,
): Promise<EmbeddedAuthResponse | null> {
  const start = Date.now();

  while (Date.now() - start < timeoutMs) {
    const status = await fetchEmbeddedAuth();
    if (status?.authenticated && status.user) {
      return status;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }

  return null;
}

/**
 * Refresh the auth token in embedded mode by fetching from the Rust HTTP server.
 * Returns true if the token was successfully refreshed, false otherwise.
 */
export async function refreshEmbeddedToken(): Promise<boolean> {
  const status = await fetchEmbeddedAuth();

  if (status?.authenticated && status.token) {
    localStorage.setItem("token", status.token);
    return true;
  }
  return false;
}
