import { useTranslation } from "react-i18next"

import type { ShiftWithMetaRecord } from "@/lib/ipc"
import { EmptyRow } from "@/components/admin/admin-panel"
import { formatTime, formatDuration } from "@/lib/format/duration"

interface Props {
  shifts: ShiftWithMetaRecord[]
  canEdit: boolean
  onEditShift: (id: string) => void
}

export function ShiftHistoryToday ({ shifts, canEdit, onEditShift }: Props) {
  const { t } = useTranslation()

  return (
    <div className="panel overflow-hidden">
      <div className="panel-head">
        <span className="panel-title">
          {t("reception.shifts.today_history", { defaultValue: "Today's shifts" })}
        </span>
        <span className="count-badge ms-2 font-mono">{shifts.length}</span>
      </div>
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("reception.shifts.operator", { defaultValue: "Operator" })}</th>
            <th>{t("reception.shifts.check_in", { defaultValue: "In" })}</th>
            <th>{t("reception.shifts.check_out", { defaultValue: "Out" })}</th>
            <th className="text-end">{t("reception.shifts.duration", { defaultValue: "Duration" })}</th>
            <th className="text-end">{t("reception.shifts.lines_run", { defaultValue: "Lines run" })}</th>
            {canEdit ? <th className="text-end">{t("admin.actions", { defaultValue: "Actions" })}</th> : null}
          </tr>
        </thead>
        <tbody>
          {shifts.map((s) => (
            <tr key={s.id}>
              <td className="font-medium text-ink">{s.operator_name}</td>
              <td className="font-mono text-[12px]">{formatTime(s.check_in_at)}</td>
              <td className="font-mono text-[12px]">
                {s.check_out_at ? (
                  formatTime(s.check_out_at)
                ) : (
                  <span className="status-pill is-info">
                    {t("reception.shifts.open", { defaultValue: "Open" })}
                  </span>
                )}
              </td>
              <td className="text-end font-mono">
                {s.check_out_at ? formatDuration(s.check_in_at, s.check_out_at) : "—"}
              </td>
              <td className="text-end font-mono text-ink-3">0</td>
              {canEdit ? (
                <td className="text-end">
                  <button
                    type="button"
                    onClick={() => onEditShift(s.id)}
                    className="text-[12px] font-medium text-ink-2 underline-offset-4 hover:text-crimson hover:underline"
                  >
                    {t("admin.edit", { defaultValue: "Edit" })}
                  </button>
                </td>
              ) : null}
            </tr>
          ))}
          {shifts.length === 0 ? (
            <EmptyRow
              colSpan={canEdit ? 6 : 5}
              message={t("reception.shifts.empty_today", { defaultValue: "No shifts today." })}
            />
          ) : null}
        </tbody>
      </table>
    </div>
  )
}
