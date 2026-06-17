import { useEffect, useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus, Trash2, Undo2 } from "lucide-react"

import { AdminHeader, EmptyRow, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import { useMoneyDisplay } from "@/features/settings/queries"
import { useVisitTabsStore } from "@/stores/visit-tabs-store"
import { formatIpcError } from "@/lib/errors"
import { ageFromBirthDate } from "@/features/patients/age"
import {
  usePatientDetail,
  usePatientRestore,
  usePatientSoftDelete,
  usePatientStats,
  usePatientUpdateDemographics,
  usePatientVisits,
} from "@/features/patients/queries"
import { cn } from "@/lib/utils"
import { formatTime } from "@/lib/format/duration"

export default function PatientDetailPage () {
  const { t, i18n } = useTranslation(["patients", "reception", "common"])
  const navigate = useNavigate()
  const { id = "" } = useParams()
  const money = useMoneyDisplay()
  const setPendingPatient = useVisitTabsStore((s) => s.setPendingPatient)

  const detail = usePatientDetail(id)
  const stats = usePatientStats(id)
  const visits = usePatientVisits(id)
  const update = usePatientUpdateDemographics()
  const softDelete = usePatientSoftDelete()
  const restore = usePatientRestore()

  const patient = detail.data
  const archived = patient?.deleted_at != null

  const [error, setError] = useState<string | null>(null)

  function startNewVisit () {
    if (!patient) return
    setPendingPatient({ id: patient.id, name: patient.name })
    navigate("/reception")
  }

  async function onDelete () {
    if (!patient) return
    setError(null)
    try {
      await softDelete.mutateAsync({ id: patient.id })
    } catch (e) {
      setError(formatIpcError(e, t))
    }
  }

  async function onRestore () {
    if (!patient) return
    setError(null)
    try {
      await restore.mutateAsync({ id: patient.id })
    } catch (e) {
      setError(formatIpcError(e, t))
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <Link
        to="/reception/patients"
        className="inline-flex items-center gap-1.5 text-[12px] font-medium text-ink-3 hover:text-ink"
      >
        <ArrowLeft className="h-3.5 w-3.5 rtl:rotate-180" strokeWidth={1.8} />
        {t("patients:detail.back")}
      </Link>

      <AdminHeader
        eyebrow={t("patients:eyebrow")}
        title={patient?.name ?? "—"}
        subtitle={
          patient
            ? [
                patient.phone,
                patient.sex ? t(`patients:sex_${patient.sex}`) : null,
                ageFromBirthDate(patient.birth_date) != null
                  ? t("patients:detail.age_years", {
                      age: ageFromBirthDate(patient.birth_date),
                    })
                  : null,
                patient.file_no
                  ? t("patients:detail.file_no_label", { file: patient.file_no })
                  : null,
              ]
                .filter(Boolean)
                .join("  ·  ")
            : undefined
        }
        actions={
          <>
            {archived ? (
              <button
                type="button"
                onClick={() => void onRestore()}
                disabled={restore.isPending}
                className="btn btn-ghost btn-sm"
              >
                <Undo2 className="h-3.5 w-3.5" strokeWidth={1.8} />
                {t("patients:actions.restore")}
              </button>
            ) : (
              <button
                type="button"
                onClick={startNewVisit}
                className="btn btn-primary btn-sm"
              >
                <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
                {t("patients:actions.new_visit")}
              </button>
            )}
          </>
        }
      />

      {archived ? (
        <div className="status-pill is-warn w-fit">
          {t("patients:detail.archived_banner")}
        </div>
      ) : null}

      <ErrorBanner message={error} />

      {/* Stats strip */}
      <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
        <Stat
          label={t("patients:stats.total_visits")}
          value={stats.data ? money.format(stats.data.total_visits) : "—"}
        />
        <Stat
          label={t("patients:stats.total_spent")}
          value={stats.data ? money.format(stats.data.total_spent_iqd) : "—"}
          suffix={money.currencySymbol}
        />
        <Stat
          label={t("patients:stats.last_visit")}
          value={
            stats.data?.last_visit_at
              ? new Date(stats.data.last_visit_at).toLocaleDateString(
                  i18n.language === "ar" ? "ar" : "en-GB"
                )
              : "—"
          }
        />
        <Stat
          label={t("patients:stats.drafts")}
          value={stats.data ? money.format(stats.data.draft_count) : "—"}
        />
      </div>

      {/* Demographics edit */}
      {patient ? (
        <DemographicsForm
          key={patient.id + patient.version}
          patient={patient}
          disabled={archived || update.isPending}
          onSave={async (vals) => {
            setError(null)
            try {
              await update.mutateAsync({ id: patient.id, ...vals })
            } catch (e) {
              setError(formatIpcError(e, t))
            }
          }}
          onDelete={archived ? undefined : onDelete}
        />
      ) : null}

      {/* Visit history */}
      <div className="panel overflow-hidden">
        <div className="panel-head">
          <span className="panel-title">{t("patients:detail.visits_title")}</span>
          <span className="count-badge ms-2 font-mono">
            {visits.data?.length ?? 0}
          </span>
        </div>
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("patients:detail.col_date")}</th>
              <th>{t("patients:detail.col_check")}</th>
              <th>{t("patients:detail.col_doctor")}</th>
              <th className="text-end">{t("patients:detail.col_total")}</th>
              <th>{t("patients:detail.col_status")}</th>
            </tr>
          </thead>
          <tbody>
            {(visits.data ?? []).map((v) => {
              const when = v.locked_at ?? v.created_at
              const checkName =
                (i18n.language === "en" ? v.check_type_name_en : v.check_type_name_ar) ??
                v.check_type_name_ar ??
                "—"
              return (
                <tr
                  key={v.id}
                  className="cursor-pointer"
                  onClick={() => navigate(`/reception/visits/${v.id}`)}
                >
                  <td className="font-mono text-[12px] text-ink-3">
                    {new Date(when).toLocaleDateString(
                      i18n.language === "ar" ? "ar" : "en-GB"
                    )}
                    <span className="ms-1.5 text-ink-4">{formatTime(when)}</span>
                  </td>
                  <td className="text-[13px] text-ink-2">{checkName}</td>
                  <td className="text-[12px] text-ink-3">{v.doctor_name ?? "—"}</td>
                  <td className="text-end font-mono text-[13px] text-ink-2">
                    {v.total_amount_iqd != null ? money.format(v.total_amount_iqd) : "—"}
                  </td>
                  <td>
                    <span className={cn("status-pill", statusTone(v.status))}>
                      {t(`patients:visit_status_${v.status}`)}
                    </span>
                  </td>
                </tr>
              )
            })}
            {(visits.data ?? []).length === 0 ? (
              <EmptyRow colSpan={5} message={t("patients:detail.visits_empty")} />
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function statusTone (status: string): string {
  if (status === "locked") return "is-success"
  if (status === "voided") return "is-danger"
  return "is-info"
}

function Stat ({ label, value, suffix }: { label: string; value: string; suffix?: string }) {
  return (
    <div className="bg-surface px-4 py-3">
      <div className="text-[10px] font-semibold uppercase tracking-[0.08em] text-ink-3">
        {label}
      </div>
      <div className="mt-1 flex items-baseline gap-1">
        <span className="font-mono text-[22px] font-bold tabular-nums text-ink">
          {value}
        </span>
        {suffix ? (
          <span className="font-mono text-[12px] text-ink-3">{suffix}</span>
        ) : null}
      </div>
    </div>
  )
}

interface DemographicsValues {
  phone: string | null
  sex: "M" | "F" | null
  birth_date: string | null
  file_no: string | null
  notes: string | null
}

function DemographicsForm ({
  patient,
  disabled,
  onSave,
  onDelete,
}: {
  patient: import("@/lib/ipc").PatientRecord
  disabled: boolean
  onSave: (vals: DemographicsValues) => Promise<void>
  onDelete?: () => void
}) {
  const { t } = useTranslation(["patients"])
  const [phone, setPhone] = useState(patient.phone ?? "")
  const [sex, setSex] = useState<"M" | "F" | "">(patient.sex ?? "")
  const [birthDate, setBirthDate] = useState(patient.birth_date ?? "")
  const [fileNo, setFileNo] = useState(patient.file_no ?? "")
  const [notes, setNotes] = useState(patient.notes ?? "")
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    if (!saved) return
    const id = window.setTimeout(() => setSaved(false), 1800)
    return () => window.clearTimeout(id)
  }, [saved])

  const submit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    await onSave({
      phone: phone.trim() || null,
      sex: sex || null,
      birth_date: birthDate.trim() || null,
      file_no: fileNo.trim() || null,
      notes: notes.trim() || null,
    })
    setSaved(true)
  }

  return (
    <form onSubmit={submit} className="panel">
      <div className="panel-head">
        <span className="panel-title">{t("patients:detail.demographics_title")}</span>
        {saved ? (
          <span className="status-pill is-success">{t("patients:detail.saved")}</span>
        ) : null}
      </div>
      <div className="panel-body space-y-4">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <FieldLabel label={t("patients:fields.phone")}>
            <input
              type="tel"
              value={phone}
              onChange={(e) => setPhone(e.target.value)}
              disabled={disabled}
              className="input"
            />
          </FieldLabel>
          <FieldLabel label={t("patients:fields.sex")}>
            <select
              value={sex}
              onChange={(e) => setSex(e.target.value as "M" | "F" | "")}
              disabled={disabled}
              className="input"
            >
              <option value="">{t("patients:fields.sex_unset")}</option>
              <option value="M">{t("patients:sex_M")}</option>
              <option value="F">{t("patients:sex_F")}</option>
            </select>
          </FieldLabel>
          <FieldLabel label={t("patients:fields.birth_date")}>
            <input
              type="date"
              value={birthDate}
              onChange={(e) => setBirthDate(e.target.value)}
              disabled={disabled}
              className="input"
            />
          </FieldLabel>
          <FieldLabel label={t("patients:fields.file_no")}>
            <input
              type="text"
              value={fileNo}
              onChange={(e) => setFileNo(e.target.value)}
              disabled={disabled}
              className="input"
            />
          </FieldLabel>
        </div>
        <FieldLabel label={t("patients:fields.notes")}>
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            disabled={disabled}
            rows={2}
            className="input"
          />
        </FieldLabel>
        <div className="flex items-center justify-between gap-2">
          {onDelete ? (
            <button
              type="button"
              onClick={onDelete}
              className="btn btn-danger btn-sm"
            >
              <Trash2 className="h-3.5 w-3.5" strokeWidth={1.8} />
              {t("patients:actions.delete")}
            </button>
          ) : (
            <span />
          )}
          <button type="submit" disabled={disabled} className="btn btn-primary btn-sm">
            {t("patients:detail.save")}
          </button>
        </div>
      </div>
    </form>
  )
}
