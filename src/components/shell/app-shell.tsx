import { useEffect } from "react"
import { useTranslation } from "react-i18next"
import { Outlet } from "react-router"

import { invoke, isTauri } from "@/lib/ipc"
import { useDeviceStore } from "@/stores/device-store"
import { useSyncStatusStore } from "@/stores/sync-status-store"
import { useSyncEvents } from "@/features/sync/sync-events"
import { FirstLaunchSetup } from "@/components/setup/first-launch-setup"
import { IdleWatcher } from "@/components/auth/idle-watcher"

import { Breadcrumbs } from "./breadcrumbs"
import { LanguageToggle } from "./language-toggle"
import { RtlBoundary } from "./rtl-boundary"
import { Sidebar } from "./sidebar"
import { SkipToContent } from "./skip-to-content"
import { StatusBar } from "./status-bar"
import { UserMenu } from "./user-menu"

/**
 * Editorial chrome (see .claude/rules/design-system.md §10):
 *   256px sidebar / fluid main / 64px header / 32px status bar.
 * The whole frame sits on `--paper`; only `--surface` cards rise from it.
 */
export function AppShell() {
  const setDevice = useDeviceStore((s) => s.setDevice)

  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    invoke("device_info")
      .then((info) => {
        if (cancelled) return
        // device_info serializes snake_case on the wire; map to the store's
        // camelCase DeviceContext.
        setDevice({ deviceId: info.device_id, appVersion: info.app_version })
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [setDevice])

  useSyncEvents()
  const { t } = useTranslation("common")
  const upgradeRequired = useSyncStatusStore((s) => s.upgradeRequired)

  return (
    <RtlBoundary>
      <SkipToContent />
      <FirstLaunchSetup />
      <IdleWatcher />
      <div className="flex h-screen w-full flex-col bg-paper text-ink">
        {upgradeRequired ? (
          <div
            role="alert"
            className="shrink-0 bg-crimson px-9 py-2 text-[13px] font-medium text-white"
          >
            {t("sync.upgrade_required")}
          </div>
        ) : null}
        <div className="flex flex-1 overflow-hidden">
          <Sidebar />
          <div className="flex min-w-0 flex-1 flex-col">
            <header className="flex h-16 shrink-0 items-center justify-between border-b border-line bg-paper px-9">
              <Breadcrumbs />
              <div className="flex items-center gap-2">
                <LanguageToggle />
                <UserMenu />
              </div>
            </header>
            <main
              id="main-content"
              className="flex-1 overflow-y-auto bg-paper px-9 pt-7 pb-16"
              tabIndex={-1}
            >
              <Outlet />
            </main>
          </div>
        </div>
        <StatusBar />
      </div>
    </RtlBoundary>
  )
}
