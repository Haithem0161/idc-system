import { useState } from "react"
import { Link, useParams } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner } from "@/components/admin/admin-panel"
import {
  useReceiptReprint,
  useVisit,
  useVisitVoid,
} from "@/features/visits/queries"
import { selectCurrentRole, useAuthStore } from "@/stores/auth-store"

type Tab = "details" | "audit" | "receipts"

export default function VisitDetailPage () {
  const { t } = useTranslation(["reception", "common"])
  const params = useParams()
  const visitId = params.id ?? ""
  const role = useAuthStore(selectCurrentRole)
  const [tab, setTab] = useState<Tab>("details")
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string | null>(null)
  const [voidOpen, setVoidOpen] = useState(false)
  const [voidReason, setVoidReason] = useState("")

  const { data: visit } = useVisit(visitId)
  const voidMutation = useVisitVoid()
  const reprint = useReceiptReprint()

  const snap = visit?.snapshots ?? null

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
      setError(String((e as Error).message ?? e))
    }
  }

  async function onReprint () {
    if (!visitId) return
    try {
      await reprint.mutateAsync({ visit_id: visitId })
      setInfo(t("reception.visit_detail.reprinted"))
    } catch (e) {
      setError(String((e as Error).message ?? e))
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
        eyebrow={t("reception.eyebrow")}
        title={t("reception.visit_detail.title")}
        subtitle={visit?.id}
        actions={
          <div className="flex items-center gap-2">
            {visit?.status === "locked" && role === "superadmin" ? (
              <button
                type="button"
                className="btn btn-danger btn-sm"
                onClick={() => setVoidOpen(true)}
              >
                {t("reception.visit_detail.actions.void")}
              </button>
            ) : null}
            {visit && visit.status !== "draft" ? (
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={onReprint}
              >
                {t("reception.visit_detail.actions.reprint")}
              </button>
            ) : null}
          </div>
        }
      />
      <ErrorBanner message={error} />
      {info ? (
        <div className="status-pill is-success w-full justify-center">{info}</div>
      ) : null}

      <div className="flex gap-2 border-b border-line">
        {(["details", "audit", "receipts"] as const).map((k) => (
          <button
            key={k}
            type="button"
            className={
              tab === k
                ? "border-b-2 border-crimson px-3 pb-2 text-[12px] font-semibold uppercase tracking-[0.08em] text-ink"
                : "border-b-2 border-transparent px-3 pb-2 text-[12px] font-medium uppercase tracking-[0.08em] text-ink-3 hover:text-ink"
            }
            onClick={() => setTab(k)}
          >
            {t(`reception.visit_detail.tabs.${k}`)}
          </button>
        ))}
      </div>

      {tab === "details" ? (
        <div className="panel">
          <div className="panel-head flex items-center justify-between">
            <span className="panel-title">
              {visit?.status
                ? t(`reception.visit_detail.status_pill.${visit.status}`)
                : "—"}
            </span>
            <span
              className={
                visit?.status === "locked"
                  ? "status-pill is-success"
                  : visit?.status === "voided"
                    ? "status-pill is-danger"
                    : "status-pill is-info"
              }
            >
              {visit?.status ?? "—"}
            </span>
          </div>
          <dl className="panel-body grid grid-cols-1 gap-3 md:grid-cols-2">
            <Field label={t("reception.visit_detail.snapshot.patient")} value={snap?.patient_name ?? "—"} />
            <Field label={t("reception.visit_detail.snapshot.doctor")} value={snap?.doctor_name ?? t("reception.new_visit.house")} />
            <Field label={t("reception.visit_detail.snapshot.operator")} value={snap?.operator_name ?? "—"} />
            <Field label={t("reception.visit_detail.snapshot.check")} value={snap?.check_type_name_ar ?? "—"} />
            <Field label={t("reception.visit_detail.snapshot.subtype")} value={snap?.check_subtype_name_ar ?? "—"} />
            <Field label={t("reception.visit_detail.snapshot.price")} value={snap?.price_iqd.toLocaleString() ?? "—"} mono />
            <Field label={t("reception.visit_detail.snapshot.dye")} value={snap?.dye_cost_iqd.toLocaleString() ?? "—"} mono />
            <Field label={t("reception.visit_detail.snapshot.report")} value={snap?.report_cost_iqd.toLocaleString() ?? "—"} mono />
            <Field label={t("reception.visit_detail.snapshot.doctor_cut")} value={snap?.doctor_cut_iqd.toLocaleString() ?? "—"} mono />
            {snap?.internal_pct != null ? (
              <Field label={t("reception.visit_detail.snapshot.internal_pct")} value={`${snap.internal_pct}%`} mono />
            ) : null}
            <Field label={t("reception.visit_detail.snapshot.operator_cut")} value={snap?.operator_cut_iqd.toLocaleString() ?? "—"} mono />
            <Field label={t("reception.visit_detail.snapshot.total")} value={snap?.total_amount_iqd.toLocaleString() ?? "—"} mono />
          </dl>
        </div>
      ) : null}

      {tab === "audit" ? (
        <p className="text-[13px] text-ink-3">
          {t("reception.visit_detail.audit_empty")}
        </p>
      ) : null}

      {tab === "receipts" ? (
        visit && visit.status !== "draft" ? (
          <div className="panel">
            <div className="panel-body space-y-2">
              <p className="text-[13px] text-ink-3">
                {t("reception.visit_detail.receipts.a5_label")} &amp;{" "}
                {t("reception.visit_detail.receipts.thermal_label")}
              </p>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={onReprint}
              >
                {t("reception.visit_detail.actions.reprint")}
              </button>
            </div>
          </div>
        ) : (
          <p className="text-[13px] text-ink-3">
            {t("reception.visit_detail.receipts.empty")}
          </p>
        )
      ) : null}

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
                <button
                  type="button"
                  className="btn btn-danger"
                  onClick={onVoid}
                >
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

function Field ({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <dt className="field-label">{label}</dt>
      <dd className={mono ? "font-mono tabular-nums text-[14px] text-ink" : "text-[14px] text-ink"}>
        {value}
      </dd>
    </div>
  )
}
