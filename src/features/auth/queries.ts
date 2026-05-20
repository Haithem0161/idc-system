import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import type { AuthLoginResult, UserAdminRecord, UserRoleLiteral } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"
import { useVisitTabsStore } from "@/stores/visit-tabs-store"

export const authKeys = {
  current: ["auth", "current"] as const,
  usersList: ["users", "list"] as const,
  user: (id: string) => ["users", "detail", id] as const,
  hasAnyUser: ["users", "has-any"] as const,
}

export function useLogin () {
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated)
  return useMutation({
    mutationFn: async (input: { email: string; password: string }): Promise<AuthLoginResult> => {
      const result = await invoke("auth_login", {
        args: { email: input.email, password: input.password },
      })
      return result
    },
    onSuccess: (result) => {
      const role = (result.user.role as UserRoleLiteral) ?? "receptionist"
      setAuthenticated({
        user: {
          user_id: result.user.id,
          entity_id: result.user.entity_id,
          email: result.user.email,
          name: result.user.name,
          role: result.user.role,
        },
        role,
        mode: result.mode,
      })
    },
  })
}

export function useLogout () {
  const setAnonymous = useAuthStore((s) => s.setAnonymous)
  return useMutation({
    mutationFn: async () => {
      await invoke("auth_logout")
    },
    onSuccess: () => {
      // Wipe in-progress tabs so the next sign-in (possibly a different
      // receptionist on the same workstation) doesn't inherit them.
      useVisitTabsStore.getState().clearAll()
      setAnonymous()
    },
  })
}

export function useLock () {
  return useMutation({
    mutationFn: async () => {
      await invoke("auth_lock")
    },
  })
}

export function useUnlock () {
  return useMutation({
    mutationFn: async (password: string) => {
      await invoke("auth_unlock", { args: { password } })
    },
  })
}

export function useFirstAdmin () {
  const setAuthenticated = useAuthStore((s) => s.setAuthenticated)
  return useMutation({
    mutationFn: async (input: { email: string; name: string; password: string; entity_id?: string }) => {
      return invoke("users_create_first_admin", { args: input })
    },
    onSuccess: (user) => {
      setAuthenticated({
        user: {
          user_id: user.id,
          entity_id: user.entity_id,
          email: user.email,
          name: user.name,
          role: user.role,
        },
        role: user.role,
        mode: "online",
      })
    },
  })
}

export function useUsersList (includeInactive = false) {
  return useQuery({
    queryKey: [...authKeys.usersList, includeInactive] as const,
    enabled: isTauri(),
    queryFn: () => invoke("users_list", { args: { include_inactive: includeInactive } }),
  })
}

export function useUser (id: string | null) {
  return useQuery({
    queryKey: id ? authKeys.user(id) : ["users", "detail", "none"],
    enabled: !!id && isTauri(),
    queryFn: () => invoke("users_get", { args: { id: id! } }),
  })
}

// DEF-007 G30: useCurrentUser -- React Query hook over the
// `auth_current_user` IPC. Cached under `authKeys.current` so any
// component that needs the live actor's id/email/role/entity can
// subscribe without re-invoking the IPC each render. Returns null
// when no user is signed in (the IPC resolves to `Option<UserContext>`
// on the Rust side).
export function useCurrentUser () {
  return useQuery({
    queryKey: authKeys.current,
    enabled: isTauri(),
    queryFn: () => invoke("auth_current_user"),
  })
}

export function useUserCreate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: {
      email: string
      name: string
      role: UserRoleLiteral
      password: string
    }) => invoke("users_create", { args: input }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: authKeys.usersList })
    },
  })
}

export function useUserUpdate () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: {
      id: string
      email?: string
      name?: string
      role?: UserRoleLiteral
    }) => invoke("users_update", { args: input }),
    onSuccess: (user: UserAdminRecord) => {
      void qc.invalidateQueries({ queryKey: authKeys.usersList })
      void qc.invalidateQueries({ queryKey: authKeys.user(user.id) })
    },
  })
}

export function useUserSoftDelete () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => invoke("users_soft_delete", { args: { id } }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: authKeys.usersList })
    },
  })
}

export function useUserResetPassword () {
  return useMutation({
    mutationFn: (input: { id: string; new_password: string }) =>
      invoke("users_reset_password", { args: input }),
  })
}

export function useHasAnyUser () {
  return useQuery({
    queryKey: authKeys.hasAnyUser,
    enabled: isTauri(),
    queryFn: () => invoke("auth_has_any_user"),
    refetchOnMount: "always",
  })
}

// DEF-007 G01: rotate access + refresh tokens via the server's
// /auth/refresh endpoint. The Rust side caches the new pair in AppState
// and emits `auth:refreshed` so subscribers (`useCurrentUser`,
// `<UserMenu>`'s last-pushed-at clock) can invalidate.
//
// Cache invalidation contract: on success we invalidate `authKeys.current`
// so `useCurrentUser` refetches. We do NOT bust `usersList` -- a token
// rotation has no effect on the admin users panel.
export function useAuthRefresh () {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => invoke("auth_refresh"),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: authKeys.current })
    },
  })
}

// DEF-007 G31: online-required password change. The Rust side returns
// `OFFLINE_NOT_ALLOWED` when no server URL is configured OR the call
// fails to connect -- the UI MUST surface a "go online to change your
// password" toast in that case. We do not invalidate any cache: the
// server response is 204 with no body, and the local cached
// password_hash is rotated by the Rust side directly.
export function useChangePassword () {
  return useMutation({
    mutationFn: (input: { current_password: string; new_password: string }) =>
      invoke("auth_change_password", { args: input }),
  })
}

// DEF-007 G08 / G21: bootstrap the pinned JWT public key. The frontend
// calls this once at first launch (after the user configures the sync
// server URL). Subsequent boots use the pinned bytes directly via
// `auth_jwt_pinned_sha256` for telemetry / drift detection.
export function useBootstrapJwtKey () {
  return useMutation({
    mutationFn: (input?: { server_url?: string }) =>
      invoke("auth_bootstrap_jwt_key", { args: input ?? {} }),
  })
}

export function usePinnedJwtKeySha256 () {
  return useQuery({
    queryKey: ["auth", "jwt-pin"] as const,
    enabled: isTauri(),
    queryFn: () => invoke("auth_jwt_pinned_sha256"),
  })
}
