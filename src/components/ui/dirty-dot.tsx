import { useTranslation } from "react-i18next"

/**
 * Tiny indicator that the row has uncommitted local changes (the `dirty=1`
 * column on every syncable SQLite table). Phase-05 §7.29 + phase-06 §7.12.
 */
export function DirtyDot ({ dirty }: { dirty: boolean }) {
  const { t } = useTranslation()
  const label = dirty
    ? t("common.pending_sync", { defaultValue: "Pending sync" })
    : t("common.synced", { defaultValue: "Synced" })
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={
        "inline-block h-1.5 w-1.5 rounded-full " +
        (dirty ? "bg-crimson" : "bg-ink-4/40")
      }
    />
  )
}
