import { useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import {
  useChecksGrid,
  usePatientCreate,
  usePatientSearch,
  useQualifiedOperators,
  useVisitCreateDraft,
  useVisitDiscard,
  useVisitLock,
  useVisitUpdateDraft,
} from "@/features/visits/queries"
import { useCheckSubtypes, useDoctors } from "@/features/catalog/queries"
import type { PatientRecord, VisitRecord } from "@/lib/ipc"

interface DraftState {
  visit: VisitRecord | null
  patient: PatientRecord | null
  doctorId: string | null
  subtypeId: string | null
  dye: boolean
  report: boolean
}

const emptyDraft: DraftState = {
  visit: null,
  patient: null,
  doctorId: null,
  subtypeId: null,
  dye: false,
  report: false,
}

export default function NewVisitPage () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const navigate = useNavigate()
  const params = useParams()
  const slug = params.slug ?? ""

  const { data: cards } = useChecksGrid()
  const checkType = useMemo(
    () => (cards ?? []).find((c) => c.check_type_id === slug),
    [cards, slug]
  )
  const checkTypeId = checkType?.check_type_id ?? null
  const localized =
    (i18n.language === "en" ? checkType?.name_en : checkType?.name_ar) ??
    checkType?.name_ar ??
    "—"

  const [draft, setDraft] = useState<DraftState>(emptyDraft)
  const [patientQuery, setPatientQuery] = useState("")
  const [operatorPickerOpen, setOperatorPickerOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [info, setInfo] = useState<string | null>(null)

  const { data: patients } = usePatientSearch(patientQuery)
  const { data: subtypes } = useCheckSubtypes(
    checkType?.has_subtypes ? (checkTypeId ?? null) : null
  )
  const { data: doctors } = useDoctors({ include_inactive: false })
  const { data: qualifiedOperators } = useQualifiedOperators(
    operatorPickerOpen ? checkTypeId : null
  )

  const patientCreate = usePatientCreate()
  const visitCreate = useVisitCreateDraft()
  const visitUpdate = useVisitUpdateDraft()
  const visitDiscard = useVisitDiscard()
  const visitLock = useVisitLock()

  const lockEnabled =
    Boolean(draft.patient) &&
    Boolean(checkTypeId) &&
    (!checkType?.has_subtypes || Boolean(draft.subtypeId))

  // Apply running total preview from snapshot if visit already drafted.
  const total = useMemo(() => {
    if (!draft.visit?.snapshots) return null
    return draft.visit.snapshots.total_amount_iqd
  }, [draft.visit])

  // Clear the error inline whenever the patient query changes; an effect
  // would only trigger a redundant render.
  function onPatientQueryChange (next: string) {
    setPatientQuery(next)
    setError(null)
    setDraft((d) => ({ ...d, patient: null }))
  }

  async function selectOrCreatePatient (name: string): Promise<PatientRecord | null> {
    const match = patients?.find(
      (p) => p.name.trim().toLowerCase() === name.trim().toLowerCase()
    )
    if (match) return match
    try {
      return await patientCreate.mutateAsync({ name })
    } catch (e) {
      setError(String((e as Error).message ?? e))
      return null
    }
  }

  async function ensureDraft (patient: PatientRecord): Promise<VisitRecord | null> {
    if (!checkTypeId) return null
    if (draft.visit) {
      const updated = await visitUpdate.mutateAsync({
        visit_id: draft.visit.id,
        check_subtype_id: draft.subtypeId,
        doctor_id: draft.doctorId,
        dye: draft.dye,
        report: draft.report,
      })
      return updated
    }
    const created = await visitCreate.mutateAsync({
      patient_id: patient.id,
      check_type_id: checkTypeId,
      check_subtype_id: draft.subtypeId,
      doctor_id: draft.doctorId,
      dye: draft.dye,
      report: draft.report,
    })
    return created
  }

  async function onSaveDraft () {
    setError(null)
    if (!draft.patient) {
      const patient = await selectOrCreatePatient(patientQuery)
      if (!patient) return
      setDraft((d) => ({ ...d, patient }))
      try {
        const visit = await ensureDraft(patient)
        if (visit) setDraft((d) => ({ ...d, visit }))
        setInfo(t("reception.new_visit.saved"))
      } catch (e) {
        setError(String((e as Error).message ?? e))
      }
      return
    }
    try {
      const visit = await ensureDraft(draft.patient)
      if (visit) setDraft((d) => ({ ...d, visit }))
      setInfo(t("reception.new_visit.saved"))
    } catch (e) {
      setError(String((e as Error).message ?? e))
    }
  }

  async function onDiscard () {
    if (!draft.visit) {
      navigate(`/reception/checks/${slug}`)
      return
    }
    try {
      await visitDiscard.mutateAsync({ visit_id: draft.visit.id })
      setInfo(t("reception.new_visit.errors.discarded"))
      navigate(`/reception/checks/${slug}`)
    } catch (e) {
      setError(String((e as Error).message ?? e))
    }
  }

  async function onLockClick () {
    setError(null)
    let patient = draft.patient
    if (!patient) {
      patient = await selectOrCreatePatient(patientQuery)
      if (!patient) return
      setDraft((d) => ({ ...d, patient: patient! }))
    }
    try {
      const visit = await ensureDraft(patient)
      if (!visit) return
      setDraft((d) => ({ ...d, visit }))
      setOperatorPickerOpen(true)
    } catch (e) {
      setError(String((e as Error).message ?? e))
    }
  }

  async function confirmLock (operatorId: string) {
    if (!draft.visit) return
    try {
      const lockResult = await visitLock.mutateAsync({
        visit_id: draft.visit.id,
        operator_id: operatorId,
      })
      setInfo(t("reception.new_visit.errors.locked"))
      setOperatorPickerOpen(false)
      navigate(`/reception/visits/${lockResult.visit.id}`)
    } catch (e) {
      setError(String((e as Error).message ?? e))
    }
  }

  return (
    <div className="space-y-6">
      <Link
        to={`/reception/checks/${slug}`}
        className="text-[11px] uppercase tracking-[0.1em] text-ink-3 hover:text-ink"
      >
        ← {t("reception.new_visit.back_to_workspace")}
      </Link>
      <AdminHeader
        eyebrow={t("reception.eyebrow")}
        title={t("reception.new_visit.title")}
        subtitle={`${localized} · ${t("reception.new_visit.subtitle")}`}
      />
      <ErrorBanner message={error} />
      {info ? (
        <div className="status-pill is-success w-full justify-center">{info}</div>
      ) : null}

      <div className="grid gap-6 lg:grid-cols-3">
        <div className="space-y-5 lg:col-span-2 panel panel-body">
          <FieldLabel label={t("reception.new_visit.patient")}>
            <input
              className="input"
              placeholder={t("reception.new_visit.patient_placeholder")}
              value={draft.patient?.name ?? patientQuery}
              onChange={(e) => onPatientQueryChange(e.target.value)}
              list="patient-search"
            />
            <datalist id="patient-search">
              {(patients ?? []).map((p) => (
                <option key={p.id} value={p.name} />
              ))}
            </datalist>
            <p className="mt-1 text-[11px] text-ink-3">
              {t("reception.new_visit.patient_create_hint")}
            </p>
          </FieldLabel>

          {checkType?.has_subtypes ? (
            <FieldLabel label={t("reception.new_visit.subtype")}>
              <select
                className="input"
                value={draft.subtypeId ?? ""}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    subtypeId: e.target.value || null,
                  }))
                }
              >
                <option value="">{t("reception.new_visit.subtype_required")}</option>
                {(subtypes ?? []).map((s) => (
                  <option key={s.id} value={s.id}>
                    {i18n.language === "en"
                      ? (s.name_en ?? s.name_ar)
                      : s.name_ar}{" "}
                    · {s.price_iqd.toLocaleString()}
                  </option>
                ))}
              </select>
            </FieldLabel>
          ) : null}

          <FieldLabel label={t("reception.new_visit.doctor")}>
            <select
              className="input"
              value={draft.doctorId ?? ""}
              onChange={(e) =>
                setDraft((d) => ({
                  ...d,
                  doctorId: e.target.value || null,
                }))
              }
            >
              <option value="">{t("reception.new_visit.house")}</option>
              {(doctors ?? []).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </FieldLabel>

          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-[13px] text-ink-2">
              <input
                type="checkbox"
                disabled={!checkType?.dye_supported}
                checked={draft.dye}
                onChange={(e) =>
                  setDraft((d) => ({ ...d, dye: e.target.checked }))
                }
              />
              {t("reception.new_visit.dye")}
            </label>
            <label className="flex items-center gap-2 text-[13px] text-ink-2">
              <input
                type="checkbox"
                disabled={!checkType?.report_supported}
                checked={draft.report}
                onChange={(e) =>
                  setDraft((d) => ({ ...d, report: e.target.checked }))
                }
              />
              {t("reception.new_visit.report")}
            </label>
          </div>
        </div>

        <aside className="panel">
          <div className="panel-head">
            <span className="panel-title">
              {t("reception.new_visit.total_label")}
            </span>
          </div>
          <div className="panel-body space-y-3">
            <p className="font-mono text-[28px] font-bold tabular-nums text-ink">
              {total != null ? total.toLocaleString() : "—"}
            </p>
            <div className="flex flex-col gap-2">
              <button
                type="button"
                className="btn btn-primary"
                disabled={!lockEnabled}
                onClick={onLockClick}
              >
                {t("reception.new_visit.lock_and_print")}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                onClick={onSaveDraft}
              >
                {t("reception.new_visit.save_draft")}
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={onDiscard}
              >
                {t("reception.new_visit.discard")}
              </button>
            </div>
          </div>
        </aside>
      </div>

      {operatorPickerOpen ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4">
          <div className="panel max-w-md w-full">
            <div className="panel-head flex items-center justify-between">
              <span className="panel-title">
                {t("reception.new_visit.operator_picker.title")}
              </span>
              <button
                type="button"
                className="text-ink-3 hover:text-ink"
                onClick={() => setOperatorPickerOpen(false)}
              >
                ×
              </button>
            </div>
            <div className="panel-body space-y-3">
              <p className="text-[12px] text-ink-3">
                {t("reception.new_visit.operator_picker.subtitle")}
              </p>
              {(qualifiedOperators ?? []).length === 0 ? (
                <p className="text-[13px] text-crimson">
                  {t("reception.new_visit.operator_picker.no_qualified")}
                </p>
              ) : (
                <ul className="divide-y divide-line">
                  {(qualifiedOperators ?? []).map((op) => (
                    <li key={op.id} className="flex items-center justify-between py-2">
                      <span className="text-[13px] text-ink-2">{op.name}</span>
                      <button
                        type="button"
                        className="btn btn-ink btn-sm"
                        onClick={() => confirmLock(op.id)}
                      >
                        {t("reception.new_visit.operator_picker.confirm")}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}
