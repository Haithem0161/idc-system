import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { invoke, isTauri } from "@/lib/ipc"
import type { AuthLoginResult, UserAdminRecord, UserRoleLiteral } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"

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
    onSuccess: () => setAnonymous(),
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
    queryFn: async () => {
      const list = await invoke("users_list", { args: { include_inactive: true } })
      return list.length > 0
    },
    refetchOnMount: "always",
  })
}
