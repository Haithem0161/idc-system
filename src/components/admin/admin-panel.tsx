import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

export function AdminHeader ({
  eyebrow,
  title,
  subtitle,
  count,
  actions,
}: {
  eyebrow?: string
  title: string
  subtitle?: string
  count?: number
  actions?: ReactNode
}) {
  const { t } = useTranslation()
  const eyebrowText = eyebrow ?? t("admin.eyebrow", { defaultValue: "Administration" })
  return (
    <header className="flex flex-wrap items-end justify-between gap-3 border-b border-line pb-5">
      <div className="space-y-2">
        <span className="eyebrow">{eyebrowText}</span>
        <h1 className="flex items-center gap-3 text-[28px] font-bold leading-[1.05] tracking-[-0.024em] text-ink">
          {title}
          {typeof count === "number" ? (
            <span className="count-badge text-[11px]">{count}</span>
          ) : null}
        </h1>
        {subtitle ? <p className="text-[13px] text-ink-3">{subtitle}</p> : null}
      </div>
      {actions ? <div className="flex items-center gap-3">{actions}</div> : null}
    </header>
  )
}

export function FieldLabel ({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="field-label">{label}</span>
      {children}
    </label>
  )
}

export function ErrorBanner ({ message }: { message: string | null }) {
  if (!message) return null
  return (
    <div role="alert" className="status-pill is-danger w-full justify-center">
      {message}
    </div>
  )
}

export function EmptyRow ({ colSpan, message }: { colSpan: number; message: string }) {
  return (
    <tr>
      <td colSpan={colSpan} className="py-12 text-center text-[13px] text-ink-3">
        {message}
      </td>
    </tr>
  )
}
