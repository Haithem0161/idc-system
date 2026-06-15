import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { LogIn, LogOut } from "lucide-react"

import { useOperators } from "@/features/catalog/queries"
import { useShiftClockIn, useShiftClockOut } from "@/features/shifts/queries"
import type { OperatorRecord, ShiftWithMetaRecord } from "@/lib/ipc"
import { EmptyRow } from "@/components/admin/admin-panel"
import { formatSince, formatTime } from "@/lib/format/duration"
import { cn } from "@/lib/utils"

export type RosterFilter = "all" | "on" | "off"

interface Props {
  /** Currently open (clocked-in) shifts, used to derive each operator's status. */
  openShifts: ShiftWithMetaRecord[]
  filter: RosterFilter
  onFilterChange: (next: RosterFilter) => void
}

interface RosterRow {
  operator: OperatorRecord
  openShift: ShiftWithMetaRecord | null
}

/**
 * Left pane of the shifts page: the full active-operator roster, status-first.
 *
 * Each row leads with a live ON SHIFT / OFF status, then the check-in time and
 * elapsed time when on shift, and a single context-aware action: Clock in when
 * off, Clock out when on. This is the direct one-click path; the modal picker
 * remains for the conflict / retroactive cases surfaced separately on the page.
 *
 * Status is derived by joining the operator roster against the open shifts
 * (there is no single roster+status endpoint); an operator is ON SHIFT iff they
 * have an open shift.
 */
export function OperatorRosterGrid ({ openShifts, filter, onFilterChange }: Props) {
  const { t } = useTranslation()
  const operators = useOperators({ include_inactive: false })
  const clockIn = useShiftClockIn()
  const clockOut = useShiftClockOut()

  const openByOperator = useMemo(() => {
    const m = new Map<string, ShiftWithMetaRecord>()
    for (const s of openShifts) m.set(s.operator_id, s)
    return m
  }, [openShifts])

  const rows = useMemo<RosterRow[]>(() => {
    const all: RosterRow[] = (operators.data ?? []).map((op) => ({
      operator: op,
      openShift: openByOperator.get(op.id) ?? null,
    }))
    if (filter === "on") return all.filter((r) => r.openShift)
    if (filter === "off") return all.filter((r) => !r.openShift)
    return all
  }, [operators.data, openByOperator, filter])

  const onCount = openShifts.length
  const totalCount = operators.data?.length ?? 0

  return (
    <div className="panel overflow-hidden">
      <div className="panel-head">
        <span className="panel-title">
          {t("reception.shifts.roster.title", { defaultValue: "Operators" })}
        </span>
        <RosterFilterPills
          filter={filter}
          onChange={onFilterChange}
          counts={{ all: totalCount, on: onCount, off: totalCount - onCount }}
        />
      </div>
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("reception.shifts.operator", { defaultValue: "Operator" })}</th>
            <th>{t("reception.shifts.status", { defaultValue: "Status" })}</th>
            <th>{t("reception.shifts.since", { defaultValue: "Since" })}</th>
            <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ operator, openShift }) => {
            const onShift = Boolean(openShift)
            const clockingIn =
              clockIn.isPending && clockIn.variables?.operator_id === operator.id
            const clockingOut =
              clockOut.isPending && clockOut.variables?.shift_id === openShift?.id
            return (
              <tr key={operator.id}>
                <td className="font-medium text-ink">
                  {operator.name}
                  {operator.phone ? (
                    <span className="ms-2 font-mono text-[11px] text-ink-4">
                      {operator.phone}
                    </span>
                  ) : null}
                </td>
                <td>
                  {onShift ? (
                    <span className="status-pill is-success is-live">
                      {t("reception.shifts.status_on", { defaultValue: "On shift" })}
                    </span>
                  ) : (
                    <span className="status-pill">
                      {t("reception.shifts.status_off", { defaultValue: "Off" })}
                    </span>
                  )}
                </td>
                <td className="font-mono text-[12px] text-ink-3">
                  {openShift ? (
                    <span title={formatTime(openShift.check_in_at)}>
                      {formatTime(openShift.check_in_at)}
                      <span className="ms-1.5 text-ink-4">
                        ({formatSince(openShift.check_in_at)})
                      </span>
                    </span>
                  ) : (
                    "—"
                  )}
                </td>
                <td className="text-end">
                  {onShift ? (
                    <button
                      type="button"
                      onClick={() => clockOut.mutate({ shift_id: openShift!.id })}
                      disabled={clockingOut}
                      className="btn btn-ghost btn-sm"
                    >
                      <LogOut className="h-3 w-3" strokeWidth={1.8} />
                      {t("reception.shifts.clock_out", { defaultValue: "Clock out" })}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() =>
                        clockIn.mutate({ operator_id: operator.id, note: null })
                      }
                      disabled={clockingIn}
                      className="btn btn-ink btn-sm"
                    >
                      <LogIn className="h-3 w-3" strokeWidth={1.8} />
                      {t("reception.shifts.clock_in_action", { defaultValue: "Clock in" })}
                    </button>
                  )}
                </td>
              </tr>
            )
          })}
          {rows.length === 0 ? (
            <EmptyRow
              colSpan={4}
              message={
                filter === "on"
                  ? t("reception.shifts.empty_open", {
                      defaultValue: "No operators on shift right now.",
                    })
                  : t("reception.shifts.roster.empty", {
                      defaultValue: "No active operators. Add one in Operators.",
                    })
              }
            />
          ) : null}
        </tbody>
      </table>
    </div>
  )
}

function RosterFilterPills ({
  filter,
  onChange,
  counts,
}: {
  filter: RosterFilter
  onChange: (next: RosterFilter) => void
  counts: { all: number; on: number; off: number }
}) {
  const { t } = useTranslation()
  const options: Array<{ key: RosterFilter; label: string; count: number }> = [
    { key: "all", label: t("reception.shifts.filter_all", { defaultValue: "All" }), count: counts.all },
    { key: "on", label: t("reception.shifts.filter_on", { defaultValue: "On shift" }), count: counts.on },
    { key: "off", label: t("reception.shifts.filter_off", { defaultValue: "Off" }), count: counts.off },
  ]
  return (
    <div className="flex items-center gap-1 rounded-md border border-line bg-paper-2 p-0.5">
      {options.map((o) => {
        const active = o.key === filter
        return (
          <button
            key={o.key}
            type="button"
            onClick={() => onChange(o.key)}
            aria-pressed={active}
            className={cn(
              "inline-flex items-center gap-1.5 rounded px-2.5 py-1 text-[11px] font-semibold transition-colors duration-150",
              active
                ? "bg-surface text-ink shadow-[0_1px_2px_rgba(10,18,48,0.06)]"
                : "text-ink-3 hover:text-ink"
            )}
          >
            {o.label}
            <span className="font-mono text-[10px] text-ink-4">{o.count}</span>
          </button>
        )
      })}
    </div>
  )
}
