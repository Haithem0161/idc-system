import { useMemo, useState } from "react"
import { Link, useParams } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner } from "@/components/admin/admin-panel"
import { StatStrip } from "@/components/accounting/stat-strip"
import { useVisit, useVisitVoid } from "@/features/visits/queries"
import { useMoneyDisplay } from "@/features/settings/queries"
import { resolveLocaleName } from "@/lib/format/locale-name"
import { selectCurrentRole, useAuthStore } from "@/stores/auth-store"
import { formatIpcError } from "@/lib/errors"
import { cn } from "@/lib/utils"
import type { VisitRecord, VisitSnapshotRecord } from "@/lib/ipc"

export default function VisitDetailPage () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const localeShort = i18n.language === "ar" ? "ar" : "en"
  const params = useParams()
  const visitId = params.id ?? ""
  const role = useAuthStore(selectCurrentRole)
  const money = useMoneyDisplay()
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string | null>(null)
  const [voidOpen, setVoidOpen] = useState(false)
  const [voidReason, setVoidReason] = useState("")

  const { data: visit } = useVisit(visitId)
  const voidMutation = useVisitVoid()

  const snap = visit?.snapshots ?? null
  const isLocked = visit?.status === "locked"

  async function onVoid () {
    if (!visitId) return
    if (voidReason.trim().length < 5) {
      setError(t("reception.new_visit.errors.void_too_short"))
      return
    }
    try {
      await voidMutation.mutateAsync({ visit_id: visitId, reason: voidReason })
      setVoidOpen(false)
      setVoidReason("")
      setInfo(t("reception.visit_detail.actions.void"))
    } catch (e) {
      setError(formatIpcError(e, t))
    }
  }

  return (
    <div className="space-y-6">
      <Link
        to="/reception"
        className="text-[11px] uppercase tracking-[0.1em] text-ink-3 hover:text-ink"
      >
        ← {t("reception.visit_detail.back_to_workspace")}
      </Link>
      <AdminHeader
        eyebrow={[
          t("reception.eyebrow"),
          visit?.locked_at
            ? new Date(visit.locked_at).toLocaleDateString(
                localeShort === "ar" ? "ar-IQ" : "en-GB",
                { day: "2-digit", month: "short", year: "numeric" }
              )
            : t(`reception.visit_detail.status_pill.${visit?.status ?? "draft"}`),
        ]
          .filter(Boolean)
          .join(" · ")}
        title={snap?.patient_name ?? t("reception.visit_detail.title")}
        actions={
          isLocked && role === "superadmin" ? (
            <button
              type="button"
              className="btn btn-danger btn-sm"
              onClick={() => setVoidOpen(true)}
            >
              {t("reception.visit_detail.actions.void")}
            </button>
          ) : null
        }
      />
      <ErrorBanner message={error} />
      {info ? (
        <div className="status-pill is-success w-full justify-center">{info}</div>
      ) : null}

      {visit ? <StatusBanner visit={visit} /> : null}

      {snap ? (
        <DetailsTab
          snap={snap}
          discount={visit?.discount ?? false}
          money={money}
          localeShort={localeShort}
        />
      ) : (
        <p className="text-[13px] text-ink-3">
          {t("reception.visit_detail.draft_no_snapshot")}
        </p>
      )}

      {voidOpen ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4">
          <div className="panel max-w-md w-full">
            <div className="panel-head">
              <span className="panel-title">
                {t("reception.visit_detail.void_modal.title")}
              </span>
            </div>
            <div className="panel-body space-y-3">
              <p className="text-[12px] text-ink-3">
                {t("reception.visit_detail.void_modal.subtitle")}
              </p>
              <textarea
                className="input min-h-[96px]"
                placeholder={t("reception.visit_detail.void_modal.reason_placeholder")}
                value={voidReason}
                onChange={(e) => setVoidReason(e.target.value)}
              />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => setVoidOpen(false)}
                >
                  {t("reception.visit_detail.void_modal.cancel")}
                </button>
                <button type="button" className="btn btn-danger" onClick={onVoid}>
                  {t("reception.visit_detail.void_modal.submit")}
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}

/** A prominent state-before-content banner. Voided visits surface the reason. */
function StatusBanner ({ visit }: { visit: VisitRecord }) {
  const { t } = useTranslation(["reception"])
  const tone =
    visit.status === "locked"
      ? "border-success/30 bg-success-soft text-success before:bg-success"
      : visit.status === "voided"
        ? "border-crimson/30 bg-crimson-soft text-crimson before:bg-crimson"
        : "border-info/30 bg-info-soft text-info before:bg-info"
  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border px-4 py-2.5",
        tone
      )}
    >
      <span className="inline-flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.06em] before:h-1.5 before:w-1.5 before:rounded-full before:content-['']">
        {t(`reception.visit_detail.status_pill.${visit.status}`)}
      </span>
      {visit.status === "voided" && visit.void_reason ? (
        <span className="text-[12px] text-ink-2">
          {t("reception.visit_detail.voided_reason", { reason: visit.void_reason })}
        </span>
      ) : null}
    </div>
  )
}

function DetailsTab ({
  snap,
  discount,
  money,
  localeShort,
}: {
  snap: VisitSnapshotRecord
  discount: boolean
  money: ReturnType<typeof useMoneyDisplay>
  localeShort: "ar" | "en"
}) {
  const { t } = useTranslation(["reception", "common"])
  const fmt = money.format

  const checkName = resolveLocaleName(
    { name_ar: snap.check_type_name_ar, name_en: snap.check_type_name_en },
    localeShort
  )
  const subtypeName =
    snap.check_subtype_name_ar || snap.check_subtype_name_en
      ? resolveLocaleName(
          { name_ar: snap.check_subtype_name_ar ?? "", name_en: snap.check_subtype_name_en },
          localeShort
        )
      : null
  const overridden = snap.amount_paid_override_iqd != null
  const collected = overridden ? snap.amount_paid_override_iqd! : snap.total_amount_iqd

  // Clinic net = collected − doctor − operator − report − mandoub. (Inventory
  // consumption is a daily-close figure, not carried on the per-visit snapshot.)
  const net = useMemo(
    () =>
      collected -
      snap.doctor_cut_iqd -
      snap.operator_cut_iqd -
      snap.report_amount_iqd -
      snap.mandoub_cut_iqd,
    [collected, snap]
  )

  const suffix = t("reception.visit_detail.currency_suffix", {
    defaultValue: "IQD",
  })

  return (
    <div className="space-y-5">
      <StatStrip
        items={[
          {
            label: t("reception.visit_detail.kpi.billed"),
            value: fmt(snap.total_amount_iqd),
            unit: suffix,
          },
          ...(overridden
            ? [
                {
                  label: t("reception.visit_detail.kpi.collected"),
                  value: fmt(collected),
                  unit: suffix,
                },
              ]
            : []),
          {
            label: t("reception.visit_detail.kpi.net"),
            value: fmt(net),
            unit: suffix,
          },
        ]}
      />

      <Section title={t("reception.visit_detail.section.visit")}>
        <Row label={t("reception.visit_detail.snapshot.patient")} value={snap.patient_name} />
        <Row
          label={t("reception.visit_detail.snapshot.check")}
          value={
            <span>
              {checkName}
              {subtypeName ? <span className="text-ink-3"> · {subtypeName}</span> : null}
            </span>
          }
        />
        <Row
          label={t("reception.visit_detail.snapshot.doctor")}
          value={
            <span className={cn(!snap.doctor_name && "text-ink-4")}>
              {snap.doctor_name ?? t("reception.new_visit.internal")}
            </span>
          }
        />
        <Row label={t("reception.visit_detail.snapshot.operator")} value={snap.operator_name} />
        {snap.mandoub_name ? (
          <Row
            label={t("reception.visit_detail.snapshot.mandoub")}
            value={
              <span className="inline-flex items-center gap-2">
                {snap.mandoub_name}
                <span className="rounded-full bg-paper-2 px-1.5 py-0.5 font-mono text-[10px] font-semibold tabular-nums text-ink-3">
                  {fmt(snap.mandoub_cut_iqd)} {suffix}
                </span>
              </span>
            }
          />
        ) : null}
        <Row
          label={t("reception.visit_detail.snapshot.dye")}
          value={snap.dye_cost_iqd > 0 ? t("common:common.yes") : t("common:common.no")}
        />
        <Row
          label={t("reception.visit_detail.snapshot.report_flag")}
          value={
            snap.report_amount_iqd > 0 || (snap.report_pct ?? 0) > 0
              ? t("common:common.yes")
              : t("common:common.no")
          }
          last={!discount}
        />
        {discount ? (
          <Row
            label={t("reception.visit_detail.snapshot.discount_flag")}
            value={t("common:common.yes")}
            last
          />
        ) : null}
      </Section>

      {/* The money waterfall as the dark "ink card" focal point (design system
          §5.1): billed at top, carve-outs subtracted, clinic net at the base. */}
      <section className="overflow-hidden rounded-lg bg-ink text-paper">
        <div className="border-b border-paper/12 px-5 py-3">
          <h3 className="text-[11px] font-semibold uppercase tracking-[0.1em] text-paper/70">
            {t("reception.visit_detail.section.money")}
          </h3>
        </div>
        <dl className="divide-y divide-paper/10 px-5">
          <MoneyRow label={t("reception.visit_detail.snapshot.price")} value={fmt(snap.price_iqd)} suffix={suffix} />
          {snap.dye_cost_iqd > 0 ? (
            <MoneyRow label={t("reception.visit_detail.snapshot.dye")} value={`+ ${fmt(snap.dye_cost_iqd)}`} suffix={suffix} />
          ) : null}
          <MoneyRow
            label={t("reception.visit_detail.snapshot.total")}
            value={fmt(snap.total_amount_iqd)}
            suffix={suffix}
            emphasis
          />
          {overridden ? (
            <MoneyRow
              label={t("reception.visit_detail.snapshot.collected")}
              value={fmt(collected)}
              suffix={suffix}
              accent
            />
          ) : null}
          <MoneyRow label={t("reception.visit_detail.snapshot.doctor_cut")} value={`− ${fmt(snap.doctor_cut_iqd)}`} suffix={suffix} muted />
          <MoneyRow label={t("reception.visit_detail.snapshot.operator_cut")} value={`− ${fmt(snap.operator_cut_iqd)}`} suffix={suffix} muted />
          {snap.report_amount_iqd > 0 ? (
            <MoneyRow
              label={t("reception.visit_detail.snapshot.report")}
              value={`− ${fmt(snap.report_amount_iqd)}`}
              suffix={suffix}
              muted
            />
          ) : null}
          {snap.mandoub_cut_iqd > 0 ? (
            <MoneyRow
              label={t("reception.visit_detail.snapshot.mandoub_cut")}
              value={`− ${fmt(snap.mandoub_cut_iqd)}`}
              suffix={suffix}
              muted
            />
          ) : null}
          <MoneyRow
            label={t("reception.visit_detail.snapshot.net")}
            value={fmt(net)}
            suffix={suffix}
            net
          />
        </dl>
      </section>
    </div>
  )
}

function Section ({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-ink-3">
        {title}
      </h3>
      <dl className="overflow-hidden rounded-lg border border-line bg-surface">
        {children}
      </dl>
    </section>
  )
}

function Row ({
  label,
  value,
  last,
}: {
  label: string
  value: React.ReactNode
  last?: boolean
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 px-4 py-3",
        !last && "border-b border-line"
      )}
    >
      <dt className="text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
        {label}
      </dt>
      <dd className="min-w-0 truncate text-end text-[13px] text-ink">{value}</dd>
    </div>
  )
}

function MoneyRow ({
  label,
  value,
  suffix,
  muted,
  emphasis,
  accent,
  net,
}: {
  label: string
  value: string
  suffix: string
  muted?: boolean
  emphasis?: boolean
  accent?: boolean
  net?: boolean
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <dt
        className={cn(
          "text-[12px] uppercase tracking-[0.04em]",
          net ? "font-semibold text-paper" : muted ? "text-paper/55" : "text-paper/80"
        )}
      >
        {label}
      </dt>
      <dd
        className={cn(
          "font-mono tabular-nums",
          net
            ? "text-[18px] font-semibold text-paper"
            : emphasis
              ? "text-[14px] font-semibold text-paper"
              : accent
                ? "text-[14px] font-semibold text-paper"
                : "text-[13px] text-paper/80"
        )}
      >
        {value}
        <span className="ms-1 text-[10px] text-paper/50">{suffix}</span>
      </dd>
    </div>
  )
}
