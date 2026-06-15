import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { LogIn, LogOut } from "lucide-react"

import { useUsersList } from "@/features/auth/queries"
import type { ShiftWithMetaRecord, UserAdminRecord } from "@/lib/ipc"
import { formatTime } from "@/lib/format/duration"
import { cn } from "@/lib/utils"

interface Props {
  /** Today's shifts (open + closed), each exploded into its clock events. */
  shifts: ShiftWithMetaRecord[]
}

type EventKind = "in" | "out"

interface ShiftEvent {
  id: string
  kind: EventKind
  /** ISO timestamp of the event, used for ordering and display. */
  at: string
  operatorName: string
  byUserId: string
}

/**
 * Right pane of the shifts page: a chronological feed of today's clock
 * activity, newest first. Every shift contributes a clock-in event and, when
 * closed, a clock-out event -- so the feed reads like a running log of who came
 * on and off shift through the day, and by which staff member.
 *
 * "By <user>" is resolved from the users list when available; if the lookup is
 * empty (e.g. a role that can't list users), the feed still renders the event
 * without the actor rather than failing.
 */
export function ShiftActivityFeed ({ shifts }: Props) {
  const { t } = useTranslation()
  const users = useUsersList(true)

  const userNames = useMemo(() => {
    const m = new Map<string, string>()
    for (const u of (users.data ?? []) as UserAdminRecord[]) {
      m.set(u.id, u.name ?? u.email)
    }
    return m
  }, [users.data])

  const events = useMemo<ShiftEvent[]>(() => {
    const out: ShiftEvent[] = []
    for (const s of shifts) {
      out.push({
        id: `${s.id}:in`,
        kind: "in",
        at: s.check_in_at,
        operatorName: s.operator_name,
        byUserId: s.check_in_by_user_id,
      })
      if (s.check_out_at) {
        out.push({
          id: `${s.id}:out`,
          kind: "out",
          at: s.check_out_at,
          operatorName: s.operator_name,
          byUserId: s.check_out_by_user_id ?? s.check_in_by_user_id,
        })
      }
    }
    // Newest first; stable for equal timestamps.
    return out.sort((a, b) => b.at.localeCompare(a.at))
  }, [shifts])

  return (
    <div className="panel flex h-full flex-col overflow-hidden">
      <div className="panel-head">
        <span className="panel-title">
          {t("reception.shifts.activity.title", { defaultValue: "Activity" })}
        </span>
        <span className="count-badge ms-2 font-mono">{events.length}</span>
      </div>
      <div className="panel-body flex-1 overflow-y-auto">
        {events.length === 0 ? (
          <p className="py-8 text-center text-[13px] text-ink-3">
            {t("reception.shifts.activity.empty", {
              defaultValue: "No clock activity today.",
            })}
          </p>
        ) : (
          <ol className="relative space-y-4 ps-5" data-testid="activity-feed">
            {/* The vertical timeline rail. */}
            <span
              aria-hidden
              className="absolute inset-y-1 start-[5px] w-px bg-line"
            />
            {events.map((ev) => {
              const by = userNames.get(ev.byUserId)
              return (
                <li key={ev.id} className="relative flex items-start gap-3">
                  <span
                    aria-hidden
                    className={cn(
                      "absolute -start-5 mt-1 flex h-[18px] w-[18px] items-center justify-center rounded-full border bg-surface",
                      ev.kind === "in"
                        ? "border-success/40 text-success"
                        : "border-ink-4/40 text-ink-3"
                    )}
                  >
                    {ev.kind === "in" ? (
                      <LogIn className="h-2.5 w-2.5" strokeWidth={2} />
                    ) : (
                      <LogOut className="h-2.5 w-2.5" strokeWidth={2} />
                    )}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate text-[13px] font-medium text-ink">
                        {ev.operatorName}
                      </span>
                      <span className="shrink-0 font-mono text-[11px] text-ink-3">
                        {formatTime(ev.at)}
                      </span>
                    </div>
                    <p className="mt-0.5 text-[12px] text-ink-3">
                      {ev.kind === "in"
                        ? t("reception.shifts.activity.clocked_in", {
                            defaultValue: "Clocked in",
                          })
                        : t("reception.shifts.activity.clocked_out", {
                            defaultValue: "Clocked out",
                          })}
                      {by ? (
                        <span className="text-ink-4">
                          {" · "}
                          {t("reception.shifts.activity.by", {
                            defaultValue: "by {{name}}",
                            name: by,
                          })}
                        </span>
                      ) : null}
                    </p>
                  </div>
                </li>
              )
            })}
          </ol>
        )}
      </div>
    </div>
  )
}
