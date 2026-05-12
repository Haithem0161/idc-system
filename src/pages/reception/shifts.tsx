import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Plus, RefreshCw } from "lucide-react"

import {
  useOpenShifts,
  useShiftHistoryToday,
} from "@/features/shifts/queries"
import { useAuthStore } from "@/stores/auth-store"
import { AdminHeader, ErrorBanner } from "@/components/admin/admin-panel"
import { ClockInDialog } from "@/components/reception/clock-in-dialog"
import { OnShiftTable } from "@/components/reception/on-shift-table"
import { OpenShiftConflictBanner } from "@/components/reception/open-shift-conflict-banner"
import { ResolveOverlappingShifts } from "@/components/reception/resolve-overlapping-shifts"
import { RetroactiveShiftEditor } from "@/components/reception/retroactive-shift-editor"
import { ShiftHistoryToday } from "@/components/reception/shift-history-today"

/**
 * `/reception/shifts` (PRD §7.1.5, phase-04).
 *
 * Two tables stacked vertically:
 *   - On shift (clocked in, awaiting clock-out).
 *   - Today's history (open + closed, by clock-in time).
 *
 * Operator picker for clock-in is filtered to active operators NOT
 * currently on an open shift. Superadmin sees retroactive edit actions.
 */
export default function ShiftsPage () {
  const { t } = useTranslation()
  const role = useAuthStore((s) =>
    s.state.kind === "authenticated" ? s.state.role : null
  )
  const canEdit = role === "superadmin"

  const open = useOpenShifts()
  const history = useShiftHistoryToday()

  const [clockInOpen, setClockInOpen] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [resolvingOperatorId, setResolvingOperatorId] = useState<string | null>(null)

  const loading = open.isLoading || history.isLoading
  const error =
    open.error?.message ?? history.error?.message ?? null

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <AdminHeader
        eyebrow={t("reception.eyebrow", { defaultValue: "Reception" })}
        title={t("reception.shifts.title", { defaultValue: "Operator shifts" })}
        subtitle={t("reception.shifts.subtitle", {
          defaultValue: "Clock operators in and out for the day.",
        })}
        actions={
          <>
            <button
              type="button"
              onClick={() => {
                void open.refetch()
                void history.refetch()
              }}
              className="btn btn-ghost btn-sm"
              aria-label={t("admin.refresh", { defaultValue: "Refresh" })}
            >
              <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.8} />
            </button>
            <button
              type="button"
              onClick={() => setClockInOpen(true)}
              className="btn btn-primary btn-sm"
            >
              <Plus className="h-3.5 w-3.5" strokeWidth={1.8} />
              {t("reception.shifts.actions.clock_in_operator", {
                defaultValue: "Clock in operator",
              })}
            </button>
          </>
        }
      />

      <OpenShiftConflictBanner onResolve={(id) => setResolvingOperatorId(id)} />

      <ErrorBanner message={error} />

      {loading ? (
        <div className="rounded-md border border-line bg-paper-2 p-6 text-center text-[13px] text-ink-3">
          {t("admin.loading", { defaultValue: "Loading..." })}
        </div>
      ) : (
        <>
          <OnShiftTable shifts={open.data ?? []} />
          <ShiftHistoryToday
            shifts={history.data ?? []}
            canEdit={canEdit}
            onEditShift={(id) => setEditingId(id)}
          />
        </>
      )}

      <ClockInDialog open={clockInOpen} onClose={() => setClockInOpen(false)} />
      <RetroactiveShiftEditor
        shiftId={editingId}
        onClose={() => setEditingId(null)}
      />
      <ResolveOverlappingShifts
        operatorId={resolvingOperatorId}
        onClose={() => setResolvingOperatorId(null)}
      />
    </div>
  )
}
