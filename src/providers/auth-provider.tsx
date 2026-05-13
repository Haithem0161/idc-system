import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  isEmbeddedMode,
  fetchEmbeddedAuth,
  waitForEmbeddedAuth,
  type EmbeddedAuthResponse,
} from "@/lib/embedded";
import { AuthContext, type AuthContextValue, type AuthUser } from "@/hooks/use-auth";

type AuthPhase =
  | "detecting"
  | "waiting_embedded"
  | "authenticated"
  | "standalone"
  | "error";

/** Map embedded auth user info to AuthUser */
function mapEmbeddedUser(
  embeddedUser: NonNullable<EmbeddedAuthResponse["user"]>,
): AuthUser {
  return {
    userId: embeddedUser.userId,
    entityId: embeddedUser.entityId,
    email: embeddedUser.email,
    name: embeddedUser.name,
    role: embeddedUser.entityRole,
  };
}

/** Store embedded auth token in localStorage for Axios interceptor */
function setEmbeddedAuthData(token: string): void {
  localStorage.setItem("token", token);
}

/** Clear auth data from localStorage */
function clearAuthData(): void {
  localStorage.removeItem("token");
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [isInitialized, setIsInitialized] = useState(false);
  const [authPhase, setAuthPhase] = useState<AuthPhase>("detecting");
  const [token, setToken] = useState<string | null>(null);
  const [user, setUser] = useState<AuthUser | null>(null);

  // Capture embedded mode once on mount (avoids re-evaluation on every render)
  const embedded = useRef(isEmbeddedMode()).current;

  // ---- Initialization: detect mode, probe /api/auth, wait for auth ----
  useEffect(() => {
    const initialize = async () => {
      try {
        if (embedded) {
          // Probe /api/auth to confirm we're genuinely in embedded mode
          // (not just a regular browser where __TAURI_INTERNALS__ is also absent)
          const probe = await fetchEmbeddedAuth();

          if (probe !== null) {
            // Endpoint exists — commit to embedded mode
            setAuthPhase("waiting_embedded");

            if (probe.authenticated && probe.token && probe.user) {
              // Already authenticated (token arrived before frontend loaded)
              setEmbeddedAuthData(probe.token);
              setToken(probe.token);
              setUser(mapEmbeddedUser(probe.user));
              setAuthPhase("authenticated");
              return;
            }

            // Not yet authenticated — poll until Business OS sends token
            const status = await waitForEmbeddedAuth(1000, 60000);

            if (status?.authenticated && status.token && status.user) {
              setEmbeddedAuthData(status.token);
              setToken(status.token);
              setUser(mapEmbeddedUser(status.user));
              setAuthPhase("authenticated");
            } else {
              console.error("[AuthProvider] Embedded auth timed out after 60s");
              setAuthPhase("error");
            }
            return;
          }

          // Probe returned null — /api/auth not reachable.
          // We're in a regular browser, fall through to standalone.
        }

        // STANDALONE MODE (or dev browser fallback)
        setAuthPhase("standalone");

        // Check for existing token in localStorage
        const existingToken = localStorage.getItem("token");
        if (existingToken) {
          setToken(existingToken);
        }
      } catch (error) {
        console.error("[AuthProvider] Auth initialization failed:", error);
        setAuthPhase("error");
      } finally {
        setIsInitialized(true);
      }
    };

    initialize();
  }, [embedded]);

  // ---- Embedded Token Refresh Polling (30s interval) ----
  useEffect(() => {
    if (!embedded || authPhase !== "authenticated") return;

    const pollInterval = setInterval(async () => {
      const status = await fetchEmbeddedAuth();

      if (status?.authenticated && status.token && status.user) {
        // Update token (may have been refreshed by Business OS)
        setEmbeddedAuthData(status.token);
        setToken(status.token);
        setUser(mapEmbeddedUser(status.user));
      } else if (status && !status.authenticated) {
        // Session expired in Business OS — clear and wait for re-auth
        console.warn("[AuthProvider] Embedded session expired, clearing auth");
        clearAuthData();
        setToken(null);
        setUser(null);
        setAuthPhase("waiting_embedded");

        clearInterval(pollInterval);

        // Wait for re-authentication
        const reauth = await waitForEmbeddedAuth(1000, 60000);
        if (reauth?.authenticated && reauth.token && reauth.user) {
          setEmbeddedAuthData(reauth.token);
          setToken(reauth.token);
          setUser(mapEmbeddedUser(reauth.user));
          setAuthPhase("authenticated");
        }
      }
    }, 30000);

    return () => clearInterval(pollInterval);
  }, [embedded, authPhase]);

  // Build context value
  const isEmbedded = embedded && authPhase !== "standalone";

  const value: AuthContextValue = {
    token,
    isAuthenticated: !!token && !!user,
    isEmbedded,
    isLoading: !isInitialized,
    user,
  };

  // Show loading state while initializing auth
  if (!isInitialized) {
    return (
      <AuthContext value={value}>
        <div className="flex items-center justify-center min-h-screen">
          <div className="flex flex-col items-center gap-3">
            <div className="h-8 w-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
            <span className="text-sm text-muted-foreground">
              {authPhase === "waiting_embedded"
                ? "Connecting to Business OS..."
                : "Initializing..."}
            </span>
          </div>
        </div>
      </AuthContext>
    );
  }

  return <AuthContext value={value}>{children}</AuthContext>;
}
