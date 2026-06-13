import { afterEach, describe, expect, it, vi } from "vitest"

import { checkForUpdate } from "./updater"

// The plugin packages call into the Tauri host at runtime; mock them so the
// pure decision logic in checkForUpdate() can be exercised in jsdom.
const checkMock = vi.fn()
const relaunchMock = vi.fn()
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => checkMock() }))
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunchMock() }))

const isTauriMock = vi.fn()
vi.mock("@/lib/ipc", () => ({ isTauri: () => isTauriMock() }))

afterEach(() => {
  vi.clearAllMocks()
})

describe("checkForUpdate", () => {
  it("reports unsupported outside Tauri without calling check()", async () => {
    isTauriMock.mockReturnValue(false)
    const result = await checkForUpdate()
    expect(result.kind).toBe("unsupported")
    expect(checkMock).not.toHaveBeenCalled()
  })

  it("maps a missing update to current", async () => {
    isTauriMock.mockReturnValue(true)
    checkMock.mockResolvedValue(null)
    const result = await checkForUpdate()
    expect(result.kind).toBe("current")
  })

  it("swallows the placeholder-host DNS failure as unsupported", async () => {
    isTauriMock.mockReturnValue(true)
    checkMock.mockRejectedValue(new Error("error sending request: RELEASES_HOST_TODO.invalid"))
    const result = await checkForUpdate()
    expect(result.kind).toBe("unsupported")
  })

  it("re-throws a real error from a configured endpoint", async () => {
    isTauriMock.mockReturnValue(true)
    checkMock.mockRejectedValue(new Error("signature verification failed"))
    await expect(checkForUpdate()).rejects.toThrow("signature")
  })

  it("exposes version + an install thunk that relaunches", async () => {
    isTauriMock.mockReturnValue(true)
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined)
    checkMock.mockResolvedValue({ version: "0.2.0", downloadAndInstall })
    const result = await checkForUpdate()
    expect(result.kind).toBe("available")
    if (result.kind !== "available") return
    expect(result.version).toBe("0.2.0")
    await result.install()
    expect(downloadAndInstall).toHaveBeenCalledOnce()
    expect(relaunchMock).toHaveBeenCalledOnce()
  })
})
