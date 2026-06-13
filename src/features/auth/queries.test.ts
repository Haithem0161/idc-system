// Phase-02 §2.4 React Query hook tests for the auth + users feature
// surface. Every test runs in both `dir=ltr` and `dir=rtl` per the plan's
// RTL invariant.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
    listenEvent: vi.fn(async () => async () => undefined),
  }
})

import { invoke } from "@/lib/ipc"
import type { UserAdminRecord, AuthLoginResult } from "@/lib/ipc"
import {
  authKeys,
  useAuthRefresh,
  useBootstrapJwtKey,
  useChangePassword,
  useCurrentUser,
  useFirstAdmin,
  useHasAnyUser,
  useLock,
  useLogin,
  useLogout,
  usePinnedJwtKeySha256,
  useUnlock,
  useUser,
  useUserCreate,
  useUserResetPassword,
  useUserSoftDelete,
  useUserUpdate,
  useUsersList,
} from "@/features/auth/queries"
import { useAuthStore } from "@/stores/auth-store"

const directions = [["ltr"], ["rtl"]] as const

function makeWrapper(): {
  wrapper: (props: { children: ReactNode }) => ReturnType<typeof createElement>
  client: QueryClient
} {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, children)
  return { wrapper, client }
}

function userResponseFixture(overrides: Partial<UserAdminRecord> = {}): UserAdminRecord {
  return {
    id: "0190a000-0000-7000-8000-000000000000",
    email: "admin@idc.io",
    name: "Mariam",
    role: "superadmin",
    is_active: true,
    last_login_at: null,
    created_at: "2026-05-14T10:00:00.000Z",
    updated_at: "2026-05-14T10:00:00.000Z",
    entity_id: "tenant-1",
    version: 1,
    ...overrides,
  } as UserAdminRecord
}

function loginResultFixture(mode: "online" | "offline" = "offline"): AuthLoginResult {
  return {
    mode,
    user: userResponseFixture(),
  } as AuthLoginResult
}

// Helpers that cast mocked results to satisfy the strongly-typed invoke<>().
const mockOnce = (value: unknown) => {
  vi.mocked(invoke).mockResolvedValueOnce(value as never)
}

describe.each(directions)("Phase-02 §2.4 auth feature hooks (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    vi.mocked(invoke).mockReset()
    useAuthStore.setState({ state: { kind: "anonymous" } })
  })

  afterEach(() => {
    document.documentElement.dir = ""
  })

  it("authKeys exposes the documented cache keys", () => {
    expect(authKeys.current).toEqual(["auth", "current"])
    expect(authKeys.usersList).toEqual(["users", "list"])
    expect(authKeys.user("abc")).toEqual(["users", "detail", "abc"])
    expect(authKeys.hasAnyUser).toEqual(["users", "has-any"])
  })

  it("useLogin dispatches `auth_login` and pushes the result into the auth store", async () => {
    mockOnce(loginResultFixture("offline"))
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useLogin(), { wrapper })
    result.current.mutate({ email: "admin@idc.io", password: "admin-pass" })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("auth_login", {
      args: { email: "admin@idc.io", password: "admin-pass" },
    })
    const state = useAuthStore.getState().state
    expect(state.kind).toBe("authenticated")
    if (state.kind === "authenticated") {
      expect(state.mode).toBe("offline")
      expect(state.user.email).toBe("admin@idc.io")
    }
  })

  it("useLogin surfaces the IPC error path without authenticating the store", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("NotAuthenticated"))
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useLogin(), { wrapper })
    result.current.mutate({ email: "admin@idc.io", password: "WRONG" })
    await waitFor(() => expect(result.current.isError).toBe(true))
    expect(useAuthStore.getState().state.kind).toBe("anonymous")
  })

  it("useLogout dispatches `auth_logout` and resets the auth store to anonymous", async () => {
    useAuthStore.setState({
      state: {
        kind: "authenticated",
        user: {
          user_id: "u1",
          entity_id: "t1",
          email: "a@b.io",
          name: "A",
          role: "superadmin",
        },
        role: "superadmin",
        mode: "online",
        locked: false,
      },
    })
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useLogout(), { wrapper })
    result.current.mutate()
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("auth_logout")
    expect(useAuthStore.getState().state.kind).toBe("anonymous")
  })

  it("useLock dispatches `auth_lock` and resolves without payload", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useLock(), { wrapper })
    result.current.mutate()
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("auth_lock")
  })

  it("useUnlock dispatches `auth_unlock` with the password arg", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useUnlock(), { wrapper })
    result.current.mutate("admin-pass")
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("auth_unlock", { args: { password: "admin-pass" } })
  })

  it("useFirstAdmin dispatches `users_create_first_admin` and auto-authenticates the store", async () => {
    mockOnce(userResponseFixture())
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useFirstAdmin(), { wrapper })
    result.current.mutate({
      email: "root@idc.io",
      name: "Root",
      password: "rootpass1",
      entity_id: "tenant-1",
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_create_first_admin", {
      args: {
        email: "root@idc.io",
        name: "Root",
        password: "rootpass1",
        entity_id: "tenant-1",
      },
    })
    const state = useAuthStore.getState().state
    expect(state.kind).toBe("authenticated")
    if (state.kind === "authenticated") {
      expect(state.mode).toBe("online")
    }
  })

  it("useUsersList caches under [users, list, includeInactive] and dispatches `users_list`", async () => {
    const fixture = [userResponseFixture()]
    mockOnce(fixture)
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useUsersList(false), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_list", { args: { include_inactive: false } })
    expect(client.getQueryData([...authKeys.usersList, false])).toEqual(fixture)
  })

  it("useUsersList response never carries password_hash", async () => {
    const fixture = [userResponseFixture()]
    mockOnce(fixture)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useUsersList(false), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(JSON.stringify(result.current.data)).not.toContain("password_hash")
  })

  it("useUser short-circuits when id is null (no IPC call)", () => {
    const { wrapper } = makeWrapper()
    renderHook(() => useUser(null), { wrapper })
    expect(invoke).not.toHaveBeenCalled()
  })

  it("useUser dispatches `users_get` with the supplied id and keys under [users, detail, id]", async () => {
    mockOnce(userResponseFixture({ id: "u-1" }))
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useUser("u-1"), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_get", { args: { id: "u-1" } })
    expect(client.getQueryData(authKeys.user("u-1"))).toBeTruthy()
  })

  it("useUserCreate dispatches `users_create` and invalidates the users-list key on success", async () => {
    mockOnce(userResponseFixture({ id: "u-new" }))
    const { wrapper, client } = makeWrapper()
    client.setQueryData([...authKeys.usersList, false], [])
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useUserCreate(), { wrapper })
    result.current.mutate({
      email: "new@idc.io",
      name: "New",
      role: "receptionist",
      password: "newpass-1234",
    })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_create", {
      args: {
        email: "new@idc.io",
        name: "New",
        role: "receptionist",
        password: "newpass-1234",
      },
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: authKeys.usersList })
  })

  it("useUserUpdate invalidates both the list AND the detail cache key for the updated id", async () => {
    mockOnce(userResponseFixture({ id: "u-edit" }))
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useUserUpdate(), { wrapper })
    result.current.mutate({ id: "u-edit", name: "Renamed" })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_update", {
      args: { id: "u-edit", name: "Renamed" },
    })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: authKeys.usersList })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: authKeys.user("u-edit") })
  })

  it("useUserSoftDelete dispatches `users_soft_delete` with id arg and invalidates the list", async () => {
    mockOnce(null)
    const { wrapper, client } = makeWrapper()
    const invalidateSpy = vi.spyOn(client, "invalidateQueries")
    const { result } = renderHook(() => useUserSoftDelete(), { wrapper })
    result.current.mutate("u-del")
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_soft_delete", { args: { id: "u-del" } })
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: authKeys.usersList })
  })

  it("useUserResetPassword dispatches `users_reset_password` with the id + new_password", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useUserResetPassword(), { wrapper })
    result.current.mutate({ id: "u-1", new_password: "rotated-12345" })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(invoke).toHaveBeenCalledWith("users_reset_password", {
      args: { id: "u-1", new_password: "rotated-12345" },
    })
  })

  it("useHasAnyUser returns false when the backend reports no users", async () => {
    mockOnce(false)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useHasAnyUser(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBe(false)
  })

  it("useHasAnyUser returns true when the backend reports existing users", async () => {
    mockOnce(true)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useHasAnyUser(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBe(true)
  })

  it("useHasAnyUser calls the dedicated auth_has_any_user command", async () => {
    mockOnce(false)
    const { wrapper } = makeWrapper()
    renderHook(() => useHasAnyUser(), { wrapper })
    await waitFor(() => expect(invoke).toHaveBeenCalled())
    expect(invoke).toHaveBeenCalledWith("auth_has_any_user")
  })

  // --- DEF-007 G30: useCurrentUser ---------------------------------------
  //
  // Phase-02 §7 advertised a `useCurrentUser` hook that wraps the
  // `auth_current_user` IPC and caches under `authKeys.current` so
  // every component reading the live actor shares a single fetch.
  // The hook lands this slice; these tests pin the contract.

  it("useCurrentUser dispatches `auth_current_user` and caches under authKeys.current", async () => {
    mockOnce({
      user_id: "0190a000-0000-7000-8000-000000000000",
      email: "asma@idc.iq",
      name: "Asma",
      role: "accountant",
      entity_id: "tenant-1",
    })
    const { wrapper, client } = makeWrapper()
    const { result } = renderHook(() => useCurrentUser(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(vi.mocked(invoke).mock.calls[0][0]).toBe("auth_current_user")
    expect(client.getQueryData(authKeys.current)).toBeTruthy()
  })

  it("useCurrentUser returns null when the IPC resolves to null (signed-out state)", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useCurrentUser(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBeNull()
  })

  it("useCurrentUser shares a single fetch across multiple subscribers", async () => {
    mockOnce({
      user_id: "0190a000-0000-7000-8000-000000000000",
      email: "asma@idc.iq",
      name: "Asma",
      role: "accountant",
      entity_id: "tenant-1",
    })
    const { wrapper } = makeWrapper()
    const { result: a } = renderHook(() => useCurrentUser(), { wrapper })
    const { result: b } = renderHook(() => useCurrentUser(), { wrapper })
    await waitFor(() => expect(a.current.isSuccess).toBe(true))
    await waitFor(() => expect(b.current.isSuccess).toBe(true))
    // Only one IPC fired -- the second hook reads from cache.
    expect(vi.mocked(invoke).mock.calls.length).toBe(1)
    // Both hooks observe the same data (same query-key cache).
    expect(a.current.data).toEqual(b.current.data)
  })

  // -----------------------------------------------------------------
  // DEF-007 G01: useAuthRefresh
  // -----------------------------------------------------------------

  it("DEF-007 G01: useAuthRefresh dispatches `auth_refresh` and returns refreshed_at", async () => {
    const refreshed = "2026-05-19T10:00:00.000Z"
    mockOnce({ refreshed_at: refreshed })
    const { wrapper, client } = makeWrapper()
    // Seed authKeys.current so we can assert invalidation flips its
    // staleness flag.
    client.setQueryData(authKeys.current, { user_id: "u", email: "e", entity_id: "t", role: "superadmin" })
    const { result } = renderHook(() => useAuthRefresh(), { wrapper })
    const ret = await result.current.mutateAsync(undefined)
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("auth_refresh")
    expect(ret).toEqual({ refreshed_at: refreshed })
    const stateAfter = client.getQueryState(authKeys.current)
    expect(stateAfter?.isInvalidated).toBe(true)
  })

  it("DEF-007 G01: useAuthRefresh surfaces 401 from the IPC as a mutation error", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ code: "NOT_AUTHENTICATED", message: "expired" })
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useAuthRefresh(), { wrapper })
    await expect(result.current.mutateAsync(undefined)).rejects.toMatchObject({
      code: "NOT_AUTHENTICATED",
    })
  })

  // -----------------------------------------------------------------
  // DEF-007 G31: useChangePassword
  // -----------------------------------------------------------------

  it("DEF-007 G31: useChangePassword dispatches `auth_change_password` with current+new password args", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useChangePassword(), { wrapper })
    await result.current.mutateAsync({
      current_password: "old-pass",
      new_password: "new-pass-12-chars",
    })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("auth_change_password", {
      args: { current_password: "old-pass", new_password: "new-pass-12-chars" },
    })
  })

  it("DEF-007 G31: useChangePassword surfaces OFFLINE_NOT_ALLOWED so the UI can render the right toast", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "OFFLINE_NOT_ALLOWED",
      message: "operation requires online connectivity",
    })
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useChangePassword(), { wrapper })
    await expect(
      result.current.mutateAsync({
        current_password: "old-pass",
        new_password: "new-pass-12-chars",
      }),
    ).rejects.toMatchObject({ code: "OFFLINE_NOT_ALLOWED" })
  })

  // -----------------------------------------------------------------
  // DEF-007 G08 / G21: useBootstrapJwtKey + usePinnedJwtKeySha256
  // -----------------------------------------------------------------

  it("DEF-007 G08: useBootstrapJwtKey dispatches the IPC with no server_url by default", async () => {
    mockOnce({ outcome: { status: "bootstrapped" }, pinned_sha256: "a".repeat(64) })
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useBootstrapJwtKey(), { wrapper })
    const ret = await result.current.mutateAsync(undefined)
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("auth_bootstrap_jwt_key", { args: {} })
    expect(ret.outcome.status).toBe("bootstrapped")
    expect(ret.pinned_sha256).toHaveLength(64)
  })

  it("DEF-007 G08: useBootstrapJwtKey passes server_url through when supplied", async () => {
    mockOnce({ outcome: { status: "already_pinned" }, pinned_sha256: "b".repeat(64) })
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useBootstrapJwtKey(), { wrapper })
    await result.current.mutateAsync({ server_url: "https://sync.idc.io" })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("auth_bootstrap_jwt_key", {
      args: { server_url: "https://sync.idc.io" },
    })
  })

  it("DEF-007 G21: useBootstrapJwtKey surfaces pin_mismatch outcome so the UI can render a hostile-rotation warning", async () => {
    mockOnce({ outcome: { status: "pin_mismatch" }, pinned_sha256: "c".repeat(64) })
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => useBootstrapJwtKey(), { wrapper })
    const ret = await result.current.mutateAsync(undefined)
    expect(ret.outcome.status).toBe("pin_mismatch")
  })

  it("DEF-007 G08: usePinnedJwtKeySha256 returns the lowercase-hex pin digest", async () => {
    mockOnce("a".repeat(64))
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => usePinnedJwtKeySha256(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBe("a".repeat(64))
  })

  it("DEF-007 G08: usePinnedJwtKeySha256 returns null when no pin exists", async () => {
    mockOnce(null)
    const { wrapper } = makeWrapper()
    const { result } = renderHook(() => usePinnedJwtKeySha256(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBeNull()
  })
})
