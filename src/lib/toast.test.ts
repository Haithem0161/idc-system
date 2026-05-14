import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { emitToast } from "@/lib/toast"

describe("emitToast", () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    consoleSpy = vi.spyOn(console, "info").mockImplementation(() => {})
  })

  afterEach(() => {
    consoleSpy.mockRestore()
  })

  it("suppresses toasts with cause 'network'", () => {
    emitToast("error", "boom", { cause: "network" })
    expect(consoleSpy).not.toHaveBeenCalled()
  })

  it("suppresses toasts with cause 'offline'", () => {
    emitToast("error", "boom", { cause: "offline" })
    expect(consoleSpy).not.toHaveBeenCalled()
  })

  it("emits toasts with cause 'domain'", () => {
    emitToast("error", "validation failed", { cause: "domain" })
    expect(consoleSpy).toHaveBeenCalledWith("[toast:error] validation failed")
  })

  it("emits toasts when no cause is provided (default behaviour)", () => {
    emitToast("info", "hello")
    expect(consoleSpy).toHaveBeenCalledWith("[toast:info] hello")
  })

  it("preserves the toast kind in the log prefix", () => {
    emitToast("success", "done")
    emitToast("warning", "watch out")
    emitToast("info", "fyi")
    emitToast("error", "nope")
    expect(consoleSpy.mock.calls.map((c: unknown[]) => c[0])).toEqual([
      "[toast:success] done",
      "[toast:warning] watch out",
      "[toast:info] fyi",
      "[toast:error] nope",
    ])
  })
})
