import { useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "react-router"
import { useTranslation } from "react-i18next"

import { AdminHeader, ErrorBanner, FieldLabel } from "@/components/admin/admin-panel"
import { FeatureToggle } from "@/components/ui/feature-toggle"
import { OperatorPickerDialog } from "@/components/reception/operator-picker-dialog"
import { useDebouncedCallback } from "@/hooks/use-debounced-callback"
import {
  useChecksGrid,
  usePatientCreate,
  usePatientSearch,
  useQualifiedOperators,
  useVisitCreateDraft,
  useVisitLock,
  useVisitUpdateDraft,
} from "@/features/visits/queries"
import { useCheckSubtypes, useDoctors } from "@/features/catalog/queries"
import {
  selectActiveTab,
  useVisitTabsStore,
  type VisitTabForm,
} from "@/stores/visit-tabs-store"
import type { PatientRecord } from "@/lib/ipc"

type SaveStatus = "idle" | "saving" | "saved" | "pending" | "error"

/**
 * Tabbed new-visit editor. Renders the form bound to the currently active
 * tab in `useVisitTabsStore`. Patient name typing is debounced; once a
 * patient row is locked in, every subsequent field change is debounced and
 * persisted via `visits_update_draft`. The single Finish button opens the
 * operator picker and ultimately calls `visits_lock`.
 */
export default function NewVisitTabbedPage () {
  const { t, i18n } = useTranslation(["reception", "common"])
  const navigate = useNavigate()

  const activeTab = useVisitTabsStore(selectActiveTab)
  const updateTabForm = useVisitTabsStore((s) => s.updateTabForm)
  const attachDraft = useVisitTabsStore((s) => s.attachDraft)
  const closeTab = useVisitTabsStore((s) => s.closeTab)

  const { data: cards } = useChecksGrid()
  const checkType = useMemo(
    () => (cards ?? []).find((c) => c.check_type_id === activeTab?.checkTypeId),
    [cards, activeTab?.checkTypeId],
  )
  const localizedCheckName =
    (i18n.language === "en" ? checkType?.name_en : checkType?.name_ar) ??
    checkType?.name_ar ??
    "—"

  const { data: subtypes } = useCheckSubtypes(
    checkType?.has_subtypes ? (activeTab?.checkTypeId ?? null) : null,
  )
  const { data: doctors } = useDoctors({ include_inactive: false })

  const [operatorPickerOpen, setOperatorPickerOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle")
  const [info, setInfo] = useState<string | null>(null)

  const { data: qualifiedOperators } = useQualifiedOperators(
    operatorPickerOpen ? (activeTab?.checkTypeId ?? null) : null,
  )
  const { data: patientMatches } = usePatientSearch(
    activeTab?.form.patientName ?? "",
  )

  const patientCreate = usePatientCreate()
  const visitCreate = useVisitCreateDraft()
  const visitUpdate = useVisitUpdateDraft()
  const visitLock = useVisitLock()

  const tabId = activeTab?.tabId ?? null
  const tabIdRef = useRef<string | null>(tabId)
  useEffect(() => {
    tabIdRef.current = tabId
  }, [tabId])

  // Reset transient UI state when the active tab swaps. Using the
  // "set state during render when previous prop changed" pattern from
  // React's docs avoids the cascading-render warning from an effect.
  const [lastTabId, setLastTabId] = useState<string | null>(tabId)
  if (lastTabId !== tabId) {
    setLastTabId(tabId)
    setSaveStatus("idle")
    setError(null)
    setInfo(null)
  }

  async function flushSave () {
    const id = tabIdRef.current
    if (!id) return
    const tab = useVisitTabsStore.getState().tabs.find((t) => t.tabId === id)
    if (!tab) return
    if (!tab.form.patientId) return // can't persist without a patient
    setSaveStatus("saving")
    try {
      if (!tab.draftVisitId) {
        const visit = await visitCreate.mutateAsync({
          patient_id: tab.form.patientId,
          check_type_id: tab.checkTypeId,
          check_subtype_id: tab.form.subtypeId,
          doctor_id: tab.form.doctorId,
          dye: checkType?.dye_supported ? tab.form.dye : false,
          report: checkType?.report_supported ? tab.form.report : false,
        })
        attachDraft(tab.tabId, visit.id)
      } else {
        await visitUpdate.mutateAsync({
          visit_id: tab.draftVisitId,
          check_subtype_id: tab.form.subtypeId,
          doctor_id: tab.form.doctorId,
          dye: checkType?.dye_supported ? tab.form.dye : false,
          report: checkType?.report_supported ? tab.form.report : false,
        })
      }
      setSaveStatus("saved")
    } catch (e) {
      setSaveStatus("error")
      setError(String((e as Error).message ?? e))
    }
  }

  const scheduleFlush = useDebouncedCallback(() => {
    void flushSave()
  }, 500)

  /**
   * Patch the live tab AND mark autosave pending. The actual IPC fires via
   * the debounced flusher above.
   */
  function patchForm (patch: Partial<VisitTabForm>) {
    if (!activeTab) return
    updateTabForm(activeTab.tabId, patch)
    setSaveStatus("pending")
    scheduleFlush()
  }

  async function resolvePatientFromName (
    name: string,
  ): Promise<PatientRecord | null> {
    const trimmed = name.trim()
    if (trimmed.length === 0) return null
    const match = patientMatches?.find(
      (p) => p.name.trim().toLowerCase() === trimmed.toLowerCase(),
    )
    if (match) return match
    try {
      return await patientCreate.mutateAsync({ name: trimmed })
    } catch (e) {
      setError(String((e as Error).message ?? e))
      return null
    }
  }

  async function handlePatientCommit (name: string) {
    if (!activeTab) return
    const patient = await resolvePatientFromName(name)
    if (!patient) return
    patchForm({ patientId: patient.id, patientName: patient.name })
  }

  async function onFinishClick () {
    if (!activeTab) return
    setError(null)
    // Force any pending debounced save first, then ensure a patient exists.
    scheduleFlush.flush()
    let tab = useVisitTabsStore.getState().tabs.find((t) => t.tabId === activeTab.tabId)
    if (!tab) return
    if (!tab.form.patientId) {
      await handlePatientCommit(tab.form.patientName)
      tab = useVisitTabsStore.getState().tabs.find((t) => t.tabId === activeTab.tabId)
      if (!tab || !tab.form.patientId) {
        setError(t("reception.new_visit.errors.patient_required"))
        return
      }
    }
    // Make sure the draft is materialised and up to date with the latest form state.
    await flushSave()
    tab = useVisitTabsStore.getState().tabs.find((t) => t.tabId === activeTab.tabId)
    if (!tab?.draftVisitId) {
      setError(t("reception.new_visit.errors.lock_failed"))
      return
    }
    setOperatorPickerOpen(true)
  }

  async function confirmFinish (operatorId: string) {
    if (!activeTab) return
    const tab = useVisitTabsStore.getState().tabs.find((t) => t.tabId === activeTab.tabId)
    if (!tab?.draftVisitId) return
    try {
      const result = await visitLock.mutateAsync({
        visit_id: tab.draftVisitId,
        operator_id: operatorId,
      })
      setInfo(t("reception.new_visit.errors.locked"))
      setOperatorPickerOpen(false)
      closeTab(activeTab.tabId)
      navigate(`/reception/visits/${result.visit.id}`)
    } catch (e) {
      setError(String((e as Error).message ?? e))
    }
  }

  if (!activeTab) {
    return (
      <div className="space-y-6 px-9 pb-12 pt-6">
        <AdminHeader
          eyebrow={t("reception.eyebrow")}
          title={t("reception.new_visit.title")}
        />
        <p className="text-[13px] text-ink-3">
          {t("reception.new_visit.no_tab")}
        </p>
      </div>
    )
  }

  const form = activeTab.form
  const lockEnabled =
    form.patientName.trim().length > 0 &&
    (!checkType?.has_subtypes || Boolean(form.subtypeId))

  return (
    <div className="space-y-6 px-9 pb-12 pt-6">
      <AdminHeader
        eyebrow={t("reception.eyebrow")}
        title={t("reception.new_visit.title")}
        subtitle={`${localizedCheckName} · ${t("reception.new_visit.subtitle")}`}
      />
      <ErrorBanner message={error} />
      {info ? (
        <div className="status-pill is-success w-full justify-center">
          {info}
        </div>
      ) : null}

      <div className="grid gap-6 lg:grid-cols-3">
        <div className="space-y-5 lg:col-span-2 panel panel-body">
          <FieldLabel label={t("reception.new_visit.patient")}>
            <input
              className="input"
              placeholder={t("reception.new_visit.patient_placeholder")}
              value={form.patientName}
              onChange={(e) => {
                // Typing invalidates a previously-committed patient.
                patchForm({
                  patientName: e.target.value,
                  patientId: null,
                })
              }}
              onBlur={(e) => {
                const v = e.target.value.trim()
                if (v.length === 0) return
                if (form.patientId) return
                void handlePatientCommit(v)
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault()
                  void handlePatientCommit((e.target as HTMLInputElement).value)
                }
              }}
              list="patient-search"
              data-testid="patient-input"
            />
            <datalist id="patient-search">
              {(patientMatches ?? []).map((p) => (
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
                value={form.subtypeId ?? ""}
                onChange={(e) =>
                  patchForm({ subtypeId: e.target.value || null })
                }
              >
                <option value="">
                  {t("reception.new_visit.subtype_required")}
                </option>
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
              value={form.doctorId ?? ""}
              onChange={(e) => patchForm({ doctorId: e.target.value || null })}
            >
              <option value="">{t("reception.new_visit.house")}</option>
              {(doctors ?? []).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </FieldLabel>

          <div className="flex flex-wrap gap-3">
            <FeatureToggle
              label={t("reception.new_visit.dye")}
              pressed={form.dye}
              onPressedChange={(p) => patchForm({ dye: p })}
              disabled={!checkType?.dye_supported}
              disabledHint={t("reception.new_visit.dye_unsupported")}
            />
            <FeatureToggle
              label={t("reception.new_visit.report")}
              pressed={form.report}
              onPressedChange={(p) => patchForm({ report: p })}
              disabled={!checkType?.report_supported}
              disabledHint={t("reception.new_visit.report_unsupported")}
            />
          </div>
        </div>

        <aside className="panel">
          <div className="panel-head">
            <span className="panel-title">
              {t("reception.new_visit.total_label")}
            </span>
          </div>
          <div className="panel-body space-y-3">
            <p
              className="font-mono text-[28px] font-bold tabular-nums text-ink"
              data-testid="running-total"
            >
              —
            </p>
            <button
              type="button"
              className="btn btn-primary w-full"
              disabled={!lockEnabled || visitLock.isPending}
              onClick={() => void onFinishClick()}
              data-testid="finish-btn"
            >
              {t("reception.new_visit.finish")}
            </button>
            <AutosaveIndicator status={saveStatus} />
          </div>
        </aside>
      </div>

      <OperatorPickerDialog
        open={operatorPickerOpen}
        operators={qualifiedOperators}
        busy={visitLock.isPending}
        onClose={() => setOperatorPickerOpen(false)}
        onPick={(id) => void confirmFinish(id)}
      />
    </div>
  )
}

function AutosaveIndicator ({ status }: { status: SaveStatus }) {
  const { t } = useTranslation(["reception"])
  if (status === "idle") return null
  const label =
    status === "saving"
      ? t("reception.new_visit.autosave.saving")
      : status === "pending"
        ? t("reception.new_visit.autosave.pending")
        : status === "error"
          ? t("reception.new_visit.autosave.error")
          : t("reception.new_visit.autosave.saved")
  const tone =
    status === "error"
      ? "text-crimson"
      : status === "saved"
        ? "text-ink-3"
        : "text-ink-3"
  return (
    <p aria-live="polite" className={`text-[11px] font-medium ${tone}`}>
      {label}
    </p>
  )
}
