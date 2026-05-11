import { useTranslation } from "react-i18next"

import { useDeviceStore } from "@/stores/device-store"
import { SyncPill } from "./sync-pill"

/**
 * Footer status strip: sync pill + last-synced + build version.
 */
export function StatusBar() {
  const { t } = useTranslation()
  const device = useDeviceStore((s) => s.device)

  return (
    <div className="flex h-9 items-center justify-between border-t border-border bg-background px-4 text-xs text-muted-foreground">
      <div className="flex items-center gap-3">
        <SyncPill />
      </div>
      <div className="flex items-center gap-4">
        <span>
          {t("status.build", { defaultValue: "Build" })}: {device?.appVersion ?? "?"}
        </span>
        <span className="hidden md:inline">
          {t("status.device", { defaultValue: "Device" })}: {device?.deviceId.slice(0, 8) ?? "?"}
        </span>
      </div>
    </div>
  )
}
