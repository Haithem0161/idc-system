import { useTranslation } from "react-i18next"
import { LogOut } from "lucide-react"

import { useShiftClockOut } from "@/features/shifts/queries"
import type { ShiftWithMetaRecord } from "@/lib/ipc"
import { EmptyRow } from "@/components/admin/admin-panel"
import { formatSince } from "@/lib/format/duration"

interface Props {
  shifts: ShiftWithMetaRecord[]
}

export function OnShiftTable ({ shifts }: Props) {
  const { t } = useTranslation()
  const clockOut = useShiftClockOut()

  return (
    <div className="panel overflow-hidden">
      <div className="panel-head">
        <span className="panel-title">
          {t("reception.shifts.on_shift", { defaultValue: "On shift" })}
        </span>
        <span className="count-badge ms-2 font-mono">{shifts.length}</span>
      </div>
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("reception.shifts.operator", { defaultValue: "Operator" })}</th>
            <th>{t("reception.shifts.phone", { defaultValue: "Phone" })}</th>
            <th>{t("reception.shifts.since", { defaultValue: "Since" })}</th>
            <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th>
          </tr>
        </thead>
        <tbody>
          {shifts.map((s) => (
            <tr key={s.id}>
              <td className="font-medium text-ink">{s.operator_name}</td>
              <td className="font-mono text-[12px] text-ink-3">{s.operator_phone ?? "—"}</td>
              <td className="font-mono text-[12px]">
                <span className="status-pill is-live">{formatSince(s.check_in_at)}</span>
              </td>
              <td className="text-end">
                <button
                  type="button"
                  onClick={() => clockOut.mutate({ shift_id: s.id })}
                  disabled={clockOut.isPending && clockOut.variables?.shift_id === s.id}
                  className="btn btn-ghost btn-sm"
                >
                  <LogOut className="h-3 w-3" strokeWidth={1.8} />
                  {t("reception.shifts.clock_out", { defaultValue: "Clock out" })}
                </button>
              </td>
            </tr>
          ))}
          {shifts.length === 0 ? (
            <EmptyRow
              colSpan={4}
              message={t("reception.shifts.empty_open", { defaultValue: "No operators on shift right now." })}
            />
          ) : null}
        </tbody>
      </table>
    </div>
  )
}
