// DEF-007 G11: <UserMenu> red-dot threshold + isUserMenuStale pure logic.
//
// Pins both the rendered behavior (avatar shows the 6-px crimson dot at
// the leading corner only when both threshold conditions hold) AND the
// underlying pure predicate so a future refactor that changes one but
// not the other shows up as a test failure.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"
import { createElement } from "react"
import { MemoryRouter } from "react-router"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return {
    ...actual,
    isTauri: vi.fn(() => true),
    invoke: vi.fn(),
    listenEvent: vi.fn(async () => async () => undefined),
  }
})

import {
  isUserMenuStale,
  USER_MENU_STALE_THRESHOLD_MS,
  useSyncStatusStore,
} from "@/stores/sync-status-store"
import { useAuthStore } from "@/stores/auth-store"
import { UserMenu } from "@/components/shell/user-menu"

const directions = [["ltr"], ["rtl"]] as const

function wrapper({ children }: { children: ReactNode }): ReturnType<typeof createElement> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return createElement(
    QueryClientProvider,
    { client },
    createElement(MemoryRouter, null, children),
  )
}

function signIn(): void {
  useAuthStore.setState({
    state: {
      kind: "authenticated",
      user: {
        user_id: "u-1",
        entity_id: "tenant-1",
        email: "asma@idc.iq",
        name: "Asma",
        role: "accountant",
      },
      role: "accountant",
      mode: "online",
      locked: false,
    },
  })
}

function signOut(): void {
  useAuthStore.setState({ state: { kind: "anonymous" } })
}

describe("isUserMenuStale (pure predicate)", () => {
  it("returns false when pendingOps is 0 regardless of time elapsed", () => {
    expect(
      isUserMenuStale({
        pendingOps: 0,
        lastPushedAt: 0,
        now: 60 * 60 * 1000,
      }),
    ).toBe(false)
  })

  it("returns true when pendingOps > 0 AND lastPushedAt is null (no push yet)", () => {
    expect(
      isUserMenuStale({
        pendingOps: 1,
        lastPushedAt: null,
        now: 0,
      }),
    ).toBe(true)
  })

  it("returns false EXACTLY AT the 5-minute boundary (must be strictly greater)", () => {
    const lastPushedAt = 1_000_000
    const now = lastPushedAt + USER_MENU_STALE_THRESHOLD_MS // exactly 5 min
    expect(isUserMenuStale({ pendingOps: 1, lastPushedAt, now })).toBe(false)
  })

  it("returns true at 5min + 1ms (strictly greater than threshold)", () => {
    const lastPushedAt = 1_000_000
    const now = lastPushedAt + USER_MENU_STALE_THRESHOLD_MS + 1
    expect(isUserMenuStale({ pendingOps: 1, lastPushedAt, now })).toBe(true)
  })

  it("returns false at 4min 59s (below threshold)", () => {
    const lastPushedAt = 1_000_000
    const now = lastPushedAt + USER_MENU_STALE_THRESHOLD_MS - 1
    expect(isUserMenuStale({ pendingOps: 1, lastPushedAt, now })).toBe(false)
  })

  it("threshold constant is exactly 5 minutes in ms", () => {
    // This pins the documented contract -- the spec G11 calls out the
    // 5-minute boundary explicitly. A regression that changes the
    // constant must also update §7.13 of the build spec.
    expect(USER_MENU_STALE_THRESHOLD_MS).toBe(300_000)
  })
})

describe.each(directions)("DEF-007 G11 <UserMenu> red-dot threshold (dir=%s)", (dir) => {
  beforeEach(() => {
    document.documentElement.dir = dir
    signIn()
    useSyncStatusStore.setState({
      status: "idle",
      pendingOps: 0,
      lastError: null,
      conflicts: [],
      lastPushedAt: null,
    })
  })

  afterEach(() => {
    document.documentElement.dir = ""
    signOut()
  })

  it("does NOT render the red dot when the outbox is empty even after a long idle gap", () => {
    useSyncStatusStore.setState({
      pendingOps: 0,
      lastPushedAt: Date.now() - 60 * 60 * 1000,
    })
    render(<UserMenu />, { wrapper })
    expect(screen.queryByTestId("user-menu-stale-dot")).toBeNull()
  })

  it("does NOT render the red dot at exactly 4 minutes elapsed (below threshold)", () => {
    useSyncStatusStore.setState({
      pendingOps: 3,
      lastPushedAt: Date.now() - 4 * 60 * 1000,
    })
    render(<UserMenu />, { wrapper })
    expect(screen.queryByTestId("user-menu-stale-dot")).toBeNull()
  })

  it("RENDERS the red dot at 5min01s elapsed with outbox non-empty", () => {
    useSyncStatusStore.setState({
      pendingOps: 1,
      lastPushedAt: Date.now() - (5 * 60 * 1000 + 1000),
    })
    render(<UserMenu />, { wrapper })
    const dot = screen.getByTestId("user-menu-stale-dot")
    expect(dot).toBeInTheDocument()
    // 6px size per design-system §5.4
    expect(dot.style.width).toBe("6px")
    expect(dot.style.height).toBe("6px")
    // Crimson token + ring-paper for contrast against the avatar.
    expect(dot.className).toContain("bg-crimson")
    expect(dot.className).toContain("ring-paper")
  })

  it("renders the red dot when lastPushedAt is null AND outbox non-empty (no push yet this session)", () => {
    useSyncStatusStore.setState({ pendingOps: 1, lastPushedAt: null })
    render(<UserMenu />, { wrapper })
    expect(screen.getByTestId("user-menu-stale-dot")).toBeInTheDocument()
  })

  it("dot positions at the trailing edge (top-end) so it mirrors in RTL", () => {
    useSyncStatusStore.setState({ pendingOps: 1, lastPushedAt: null })
    render(<UserMenu />, { wrapper })
    const dot = screen.getByTestId("user-menu-stale-dot")
    // Tailwind logical end + top utilities (-end-0.5, -top-0.5) mirror
    // automatically when `<html dir="rtl">`.
    expect(dot.className).toContain("-end-0.5")
    expect(dot.className).toContain("-top-0.5")
  })

  it("dot carries an aria-label so screen readers announce the stale state", () => {
    useSyncStatusStore.setState({ pendingOps: 1, lastPushedAt: null })
    render(<UserMenu />, { wrapper })
    const dot = screen.getByTestId("user-menu-stale-dot")
    expect(dot.getAttribute("aria-label")).toBeTruthy()
  })

  it("hides the dot the moment pendingOps drops to 0 even though time has elapsed", () => {
    // First render with stale state -> dot shows.
    useSyncStatusStore.setState({
      pendingOps: 1,
      lastPushedAt: Date.now() - (5 * 60 * 1000 + 1000),
    })
    const { rerender } = render(<UserMenu />, { wrapper })
    expect(screen.getByTestId("user-menu-stale-dot")).toBeInTheDocument()
    // Outbox drains; dot must disappear.
    useSyncStatusStore.setState({ pendingOps: 0 })
    rerender(<UserMenu />)
    expect(screen.queryByTestId("user-menu-stale-dot")).toBeNull()
  })
})
