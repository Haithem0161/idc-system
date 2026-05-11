import { useEffect } from "react"
import { Outlet } from "react-router"
import { useTranslation } from "react-i18next"

import { invoke, isTauri } from "@/lib/ipc"
import { useDeviceStore } from "@/stores/device-store"
import { useSyncEvents } from "@/features/sync/sync-events"
import { FirstLaunchSetup } from "@/components/setup/first-launch-setup"

import { Breadcrumbs } from "./breadcrumbs"
import { LanguageToggle } from "./language-toggle"
import { Logo } from "./logo"
import { RtlBoundary } from "./rtl-boundary"
import { Sidebar } from "./sidebar"
import { SkipToContent } from "./skip-to-content"
import { StatusBar } from "./status-bar"

/**
 * Top-level application shell:
 *
 *   <SkipToContent />
 *   <RtlBoundary>
 *     <Sidebar />
 *     <header> -- breadcrumbs + language toggle
 *     <main id="main-content"> -- outlet for child routes
 *     <StatusBar /> -- sync pill + version
 *   </RtlBoundary>
 *
 * Mounted at the root of the router; every authenticated page sits inside it.
 */
export function AppShell() {
  const { t } = useTranslation()
  const setDevice = useDeviceStore((s) => s.setDevice)

  // Bootstrap device info once.
  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    invoke("device_info")
      .then((info) => {
        if (cancelled) return
        const deviceId = info.deviceId ?? info.device_id ?? ""
        const appVersion = info.appVersion ?? info.app_version ?? ""
        setDevice({ deviceId, appVersion })
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [setDevice])

  // Wire sync:* events into the store.
  useSyncEvents()

  return (
    <RtlBoundary>
      <SkipToContent />
      <FirstLaunchSetup />
      <div className="flex h-screen w-full flex-col bg-background text-foreground">
        <div className="flex flex-1 overflow-hidden">
          <Sidebar />
          <div className="flex flex-1 flex-col">
            <header className="flex h-14 items-center justify-between border-b border-border bg-background px-4">
              <Breadcrumbs />
              <div className="flex items-center gap-2">
                <Logo size={20} />
                <span className="text-xs text-muted-foreground">
                  {t("app.title", { defaultValue: "IDC" })}
                </span>
                <LanguageToggle />
              </div>
            </header>
            <main
              id="main-content"
              className="flex-1 overflow-y-auto bg-muted/20 p-6"
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
