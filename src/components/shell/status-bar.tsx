import { useTranslation } from "react-i18next"

import { useDeviceStore } from "@/stores/device-store"
import { SyncPill } from "./sync-pill"

/**
 * 32px editorial status strip: sync pill on the leading side, mono device
 * + build identifier on the trailing side.
 */
export function StatusBar() {
  const { t } = useTranslation()
  const device = useDeviceStore((s) => s.device)

  return (
    <div className="flex h-8 shrink-0 items-center justify-between border-t border-line bg-paper px-6 text-[10.5px] uppercase tracking-[0.08em] text-ink-3">
      <div className="flex items-center gap-3">
        <SyncPill />
      </div>
      <div className="flex items-center gap-5 font-mono normal-case tracking-normal">
        <span className="text-[11px]">
          <span className="text-ink-4">{t("status.build", { defaultValue: "Build" })} </span>
          <span className="text-ink-2">{device?.appVersion ?? "?"}</span>
        </span>
        <span className="hidden text-[11px] md:inline">
          <span className="text-ink-4">{t("status.device", { defaultValue: "Device" })} </span>
          <span className="text-ink-2">{device?.deviceId.slice(0, 8) ?? "?"}</span>
        </span>
      </div>
    </div>
  )
}
