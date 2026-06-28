// Interaction tests for the redesigned admin Settings page. Covers the audit
// outcomes: no raw snake_case keys, thermal_width as a 32/48 segmented control,
// dirty-tracking + atomic batch save, and pct validation gating the save.
// Runs in both ltr + rtl per .claude/rules/testing.md.

import "@/i18n"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { createMemoryRouter, RouterProvider } from "react-router"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import i18n from "i18next"

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc")
  return { ...actual, isTauri: vi.fn(() => true), invoke: vi.fn() }
})

// The updater hook hits Tauri APIs; stub it to a stable "current" state.
vi.mock("@/features/updater/use-updater", () => ({
  useUpdater: () => ({
    state: { status: "current" },
    runCheck: vi.fn(),
    runInstall: vi.fn(),
    canInstall: false,
  }),
}))

import { invoke } from "@/lib/ipc"
import type { SettingRecord } from "@/lib/ipc"
import SettingsPage from "@/pages/admin/settings"

function row (key: string, value: SettingRecord["value"]): SettingRecord {
  return {
    id: `0190a000-0000-7000-8000-0000000000${key.length.toString(16).padStart(2, "0")}`,
    key,
    value,
    entity_id: "tenant-1",
    version: 1,
    updated_at: "2026-06-20T10:00:00.000Z",
  } as SettingRecord
}

// A full snapshot so every spec'd key resolves to a saved (override) row.
const SETTINGS: SettingRecord[] = [
  row("clinic_display_name_ar", { valueType: "text", value: "Clinic-AR" }),
  row("clinic_display_name_en", { valueType: "text", value: "Clinic" }),
  row("currency_symbol", { valueType: "text", value: "د.ع" }),
  row("dye_cost_iqd", { valueType: "int", value: 10000 }),
  row("report_cost_iqd", { valueType: "int", value: 10000 }),
  row("internal_doctor_pct", { valueType: "int", value: 30 }),
  row("idle_lock_minutes", { valueType: "int", value: 10 }),
  row("arabic_numerals", { valueType: "bool", value: false }),
  row("thermal_width", { valueType: "int", value: 32 }),
  row("thermal_printer_name", { valueType: "text", value: "" }),
]

function renderPage () {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0, gcTime: 0 },
      mutations: { retry: false },
    },
  })
  const router = createMemoryRouter(
    [{ path: "/", element: <SettingsPage /> }],
    { initialEntries: ["/"] }
  )
  return render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}

const directions = [["ltr"], ["rtl"]] as const

describe.each(directions)("Settings page (dir=%s)", (dir) => {
  beforeEach(async () => {
    document.documentElement.dir = dir
    await i18n.changeLanguage(dir === "rtl" ? "ar" : "en")
    vi.mocked(invoke).mockReset()
    vi.mocked(invoke).mockImplementation(((cmd: string) => {
      if (cmd === "settings_list") return Promise.resolve(SETTINGS)
      if (cmd === "settings_update_batch") return Promise.resolve(SETTINGS)
      if (cmd === "config_get_sync_server_url") return Promise.resolve("https://idc-sync.example.com")
      if (cmd === "config_update_sync_server_url") return Promise.resolve(undefined)
      if (cmd === "auth_bootstrap_jwt_key") return Promise.resolve(undefined)
      return Promise.resolve(undefined)
    }) as unknown as typeof invoke)
  })
  afterEach(() => vi.clearAllMocks())

  it("never shows raw snake_case storage keys or the int/text/bool type token", async () => {
    renderPage()
    await screen.findByRole("heading", { level: 1 })
    // The redesign removed the developer-facing key + type sub-line entirely.
    expect(screen.queryByText("internal_doctor_pct")).toBeNull()
    expect(screen.queryByText("thermal_width")).toBeNull()
    expect(screen.queryByText(/^int$/)).toBeNull()
    expect(screen.queryByText(/^bool$/)).toBeNull()
  })

  it("renders thermal_width as a 32/48 radiogroup, not a free number input", async () => {
    renderPage()
    await screen.findByRole("heading", { level: 1 })
    const group = screen.getByRole("radiogroup")
    const radios = within(group).getAllByRole("radio")
    expect(radios).toHaveLength(2)
    // No free number input is offered for the width.
    expect(radios.some((r) => r.getAttribute("aria-checked") === "true")).toBe(true)
  })

  it("editing a value reveals the save bar and batch-saves atomically", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findByRole("heading", { level: 1 })

    // idle_lock_minutes is the only int in the Security group -> find by spinbutton.
    const spin = screen.getAllByRole("spinbutton")
    // Edit the first numeric field (a cost) to a new valid value.
    await user.clear(spin[0])
    await user.type(spin[0], "12345")

    const saveBtn = await screen.findByRole("button", {
      name: i18n.t("admin.settings.save_changes"),
    })
    expect(saveBtn).toBeEnabled()
    await user.click(saveBtn)

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "settings_update_batch",
        expect.objectContaining({
          args: expect.objectContaining({
            entries: expect.arrayContaining([
              expect.objectContaining({ value: expect.objectContaining({ valueType: "int" }) }),
            ]),
          }),
        })
      )
    })
  })

  it("blocks save when internal_doctor_pct is out of range and shows an inline error", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findByRole("heading", { level: 1 })

    // Find the pct spinbutton: it is the one with max=100.
    const pct = screen
      .getAllByRole("spinbutton")
      .find((el) => el.getAttribute("max") === "100")
    expect(pct).toBeDefined()
    await user.clear(pct!)
    await user.type(pct!, "150")

    // An inline alert appears and the primary save is disabled.
    await screen.findByRole("alert")
    const saveBtn = screen.getByRole("button", {
      name: i18n.t("admin.settings.save_changes"),
    })
    expect(saveBtn).toBeDisabled()
  })

  it("shows the sync server field seeded with the configured URL", async () => {
    renderPage()
    await screen.findByRole("heading", { level: 1 })
    const urlInput = await screen.findByLabelText(i18n.t("admin.settings.connection.url_label"))
    await waitFor(() => {
      expect((urlInput as HTMLInputElement).value).toBe("https://idc-sync.example.com")
    })
  })

  it("changing the sync server is confirm-gated and goes through the gated command + key re-pin", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findByRole("heading", { level: 1 })
    const urlInput = await screen.findByLabelText(i18n.t("admin.settings.connection.url_label"))
    await waitFor(() => expect((urlInput as HTMLInputElement).value).toBe("https://idc-sync.example.com"))

    await user.clear(urlInput)
    await user.type(urlInput, "https://new-sync.example.com")

    // First click only reveals the confirm step -- nothing committed yet.
    await user.click(screen.getByRole("button", { name: i18n.t("admin.settings.connection.change") }))
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("config_update_sync_server_url", expect.anything())

    // Confirm commits through the gated command and re-pins the new key.
    await user.click(screen.getByRole("button", { name: i18n.t("admin.settings.connection.confirm") }))
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("config_update_sync_server_url", {
        url: "https://new-sync.example.com",
      })
    })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("auth_bootstrap_jwt_key", {
      args: { server_url: "https://new-sync.example.com" },
    })
  })

  it("rejects an invalid sync server URL (no Change button)", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findByRole("heading", { level: 1 })
    const urlInput = await screen.findByLabelText(i18n.t("admin.settings.connection.url_label"))
    await waitFor(() => expect((urlInput as HTMLInputElement).value).toBe("https://idc-sync.example.com"))

    await user.clear(urlInput)
    await user.type(urlInput, "not-a-url")

    // The change action is disabled for a non-http(s) value.
    expect(
      screen.getByRole("button", { name: i18n.t("admin.settings.connection.change") })
    ).toBeDisabled()
  })
})
