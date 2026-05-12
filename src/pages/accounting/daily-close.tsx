import { useState } from "react"
import { useTranslation } from "react-i18next"
import { save } from "@tauri-apps/plugin-dialog"

import {
  useDailyClose,
  useDailyCloseRerun,
  useExportDailyClosePdf,
} from "@/features/reports/queries"
import { formatHours, formatIqd } from "@/lib/format/money"
import type { DailyCloseRecord } from "@/lib/ipc"
import { cn } from "@/lib/utils"

function todayLocal (): string {
  const d = new Date()
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

function yesterdayLocal (): string {
  const d = new Date()
  d.setDate(d.getDate() - 1)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

export default function AccountingDailyClosePage () {
  const { t, i18n } = useTranslation()
  const [date, setDate] = useState<string>(todayLocal())
  const close = useDailyClose(date)
  const rerun = useDailyCloseRerun()
  const exportPdf = useExportDailyClosePdf()
  const locale = i18n.language === "ar" ? "ar-IQ" : "en-GB"

  const onExport = async () => {
    if (!close.data) return
    const hashPrefix = close.data.input_hash.slice(0, 6)
    const slug = `daily-close_${close.data.target_date}_${hashPrefix}.pdf`
    const path = await save({
      defaultPath: slug,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    })
    if (!path) return
    await exportPdf.mutateAsync({ date: close.data.target_date, path })
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="eyebrow">
            {t("accounting.daily_close.eyebrow", { defaultValue: "Daily reconciliation" })}
          </div>
          <h1 className="mt-1 text-[28px] font-bold tracking-tight text-ink">
            {t("accounting.daily_close.title", { defaultValue: "Daily close" })}
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="input h-9 px-2 py-1 text-[12px]"
          />
          <button type="button" onClick={() => setDate(yesterdayLocal())} className="btn btn-ghost btn-sm">
            {t("accounting.daily_close.yesterday", { defaultValue: "Yesterday" })}
          </button>
          <button
            type="button"
            onClick={() => rerun.mutate({ date })}
            disabled={rerun.isPending}
            className="btn btn-ink btn-sm"
          >
            {rerun.isPending
              ? t("accounting.daily_close.running", { defaultValue: "Running…" })
              : t("accounting.daily_close.run_close", { defaultValue: "Run close" })}
          </button>
          <button
            type="button"
            disabled
            title={t("accounting.daily_close.sign_disabled_tooltip", {
              defaultValue: "Available in v0.2",
            })}
            className="btn btn-primary btn-sm opacity-50"
          >
            {t("accounting.daily_close.sign", { defaultValue: "Sign and freeze" })}
          </button>
          <button
            type="button"
            onClick={onExport}
            disabled={!close.data || exportPdf.isPending}
            className="btn btn-ghost btn-sm"
          >
            {exportPdf.isPending
              ? t("accounting.actions.exporting", { defaultValue: "Exporting…" })
              : t("accounting.actions.export_pdf", { defaultValue: "Export PDF" })}
          </button>
        </div>
      </header>

      {close.data ? (
        <DailyCloseBody close={close.data} locale={locale} />
      ) : (
        <div className="h-[300px] animate-pulse rounded-lg bg-paper-2" />
      )}
    </div>
  )
}

function DailyCloseBody ({ close, locale }: { close: DailyCloseRecord; locale: string }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <span className="status-pill is-success">
          {close.target_date} · {close.tz_offset}
        </span>
        <span className="font-mono text-[10px] text-ink-3">
          hash {close.input_hash.slice(0, 6)}
        </span>
        {close.provisional ? (
          <span className="status-pill is-warn">
            {t("accounting.daily_close.provisional", { defaultValue: "Provisional" })} · {close.pending_sync}{" "}
            {t("accounting.daily_close.pending_ops", { defaultValue: "pending" })}
          </span>
        ) : (
          <span className="status-pill">
            {t("accounting.daily_close.synced", { defaultValue: "Fully synced" })}
          </span>
        )}
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
        <Stat
          label={t("accounting.kpi.revenue", { defaultValue: "Revenue" })}
          value={formatIqd(close.total_revenue_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.kpi.doctor_cuts", { defaultValue: "Doctor cuts" })}
          value={formatIqd(close.total_doctor_cuts_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.kpi.operator_cuts", { defaultValue: "Operator cuts" })}
          value={formatIqd(close.total_operator_cuts_iqd, { locale, withSuffix: true })}
        />
        <Stat
          label={t("accounting.kpi.inventory_value", { defaultValue: "Inventory value" })}
          value={formatIqd(close.total_inventory_consumption_value_iqd, {
            locale,
            withSuffix: true,
          })}
        />
      </div>

      <div className="rounded-lg bg-ink p-6 text-paper">
        <div className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-paper/70">
          {t("accounting.kpi.net", { defaultValue: "Net" })}
        </div>
        <div
          className={cn(
            "mt-1 font-mono text-[40px] font-bold tabular-nums",
            close.net_iqd < 0 ? "text-crimson" : "text-paper"
          )}
        >
          {formatIqd(close.net_iqd, { locale, withSuffix: true })}
        </div>
        <div className="mt-2 text-[12px] text-paper/70">
          {t("accounting.daily_close.locked_count", {
            defaultValue: "Locked: {{n}}",
            n: close.locked_count,
          })}{" "}
          ·{" "}
          {t("accounting.daily_close.voided_count", {
            defaultValue: "Voided: {{n}}",
            n: close.voided_count,
          })}{" "}
          ·{" "}
          {t("accounting.daily_close.voided_value", {
            defaultValue: "Voided value: {{value}} IQD",
            value: formatIqd(close.voided_value_iqd, { locale }),
          })}
        </div>
      </div>

      <section className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <BreakdownTable
          title={t("accounting.daily_close.per_doctor", { defaultValue: "Per doctor" })}
          headers={[
            t("accounting.daily_close.col.name", { defaultValue: "Doctor" }),
            t("accounting.daily_close.col.visits", { defaultValue: "Visits" }),
            t("accounting.daily_close.col.revenue", { defaultValue: "Revenue" }),
            t("accounting.daily_close.col.cut", { defaultValue: "Cut" }),
          ]}
          rows={close.per_doctor.map((d) => [
            d.name,
            String(d.visits),
            formatIqd(d.revenue_iqd, { locale }),
            formatIqd(d.doctor_cut_iqd, { locale }),
          ])}
        />
        <BreakdownTable
          title={t("accounting.daily_close.per_operator", { defaultValue: "Per operator" })}
          headers={[
            t("accounting.daily_close.col.name", { defaultValue: "Operator" }),
            t("accounting.daily_close.col.visits", { defaultValue: "Visits" }),
            t("accounting.daily_close.col.cut", { defaultValue: "Cut" }),
            t("accounting.daily_close.col.hours", { defaultValue: "Hours" }),
          ]}
          rows={close.per_operator.map((o) => [
            o.name || o.operator_id,
            String(o.visits),
            formatIqd(o.operator_cut_iqd, { locale }),
            formatHours(o.hours_on_shift_milli),
          ])}
        />
        <BreakdownTable
          title={t("accounting.daily_close.per_check_type", { defaultValue: "Per check type" })}
          headers={[
            t("accounting.daily_close.col.name", { defaultValue: "Check" }),
            t("accounting.daily_close.col.visits", { defaultValue: "Visits" }),
            t("accounting.daily_close.col.revenue", { defaultValue: "Revenue" }),
            t("accounting.daily_close.col.cut", { defaultValue: "Doc + Op" }),
          ]}
          rows={close.per_check_type.map((c) => [
            c.name_en ?? c.name_ar,
            String(c.visits),
            formatIqd(c.revenue_iqd, { locale }),
            formatIqd(c.doctor_cut_iqd + c.operator_cut_iqd, { locale }),
          ])}
        />
      </section>
    </div>
  )
}

function Stat ({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-surface p-4">
      <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-ink-3">{label}</div>
      <div className="mt-1 font-mono text-[20px] tabular-nums text-ink">{value}</div>
    </div>
  )
}

function BreakdownTable ({
  title,
  headers,
  rows,
}: {
  title: string
  headers: string[]
  rows: Array<Array<string>>
}) {
  const { t } = useTranslation()
  return (
    <div className="rounded-lg border border-line bg-surface p-5">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.12em] text-ink-3">{title}</div>
      {rows.length === 0 ? (
        <div className="mt-3 text-[12px] text-ink-3">
          {t("accounting.daily_close.empty", { defaultValue: "—" })}
        </div>
      ) : (
        <table className="mt-3 w-full">
          <thead className="text-[10px] uppercase tracking-[0.1em] text-ink-3">
            <tr>
              {headers.map((h, i) => (
                <th
                  key={i}
                  className={cn("pb-1.5", i === 0 ? "text-start" : "text-end")}
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {rows.map((r, ri) => (
              <tr key={ri} className="text-[12px]">
                {r.map((cell, ci) => (
                  <td
                    key={ci}
                    className={cn(
                      "py-1.5 text-ink-2",
                      ci === 0
                        ? "text-start"
                        : "text-end font-mono tabular-nums"
                    )}
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}
