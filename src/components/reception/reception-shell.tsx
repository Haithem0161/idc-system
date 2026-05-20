import { useEffect, useRef } from "react"
import { Outlet } from "react-router"

import { invoke } from "@/lib/ipc"
import { useAuthStore } from "@/stores/auth-store"
import { useVisitTabsStore } from "@/stores/visit-tabs-store"
import { VisitTabsStrip } from "./visit-tabs-strip"

/**
 * Reception layout wrapper. Renders the persistent tab strip and reconciles
 * the persisted tab set against the DB on first mount: tabs whose draft is
 * gone (locked elsewhere, voided, discarded) are pruned silently. Tabs that
 * never had a draft (still pre-patient) stay as-is.
 */
export function ReceptionShell () {
  const userId = useAuthStore((s) =>
    s.state.kind === "authenticated" ? s.state.user.user_id : null,
  )
  const claimForUser = useVisitTabsStore((s) => s.claimForUser)
  const tabs = useVisitTabsStore((s) => s.tabs)
  const pruneTabs = useVisitTabsStore((s) => s.pruneTabs)
  const reconciledRef = useRef(false)

  // Claim the store for the current user. If a different user was last to
  // touch it, their tabs are wiped instead of leaking across sign-ins.
  useEffect(() => {
    if (userId) claimForUser(userId)
  }, [userId, claimForUser])

  useEffect(() => {
    if (reconciledRef.current) return
    if (!userId) return
    reconciledRef.current = true
    void (async () => {
      const stale: string[] = []
      for (const tab of tabs) {
        if (!tab.draftVisitId) continue
        try {
          const visit = await invoke("visits_get", {
            args: { visit_id: tab.draftVisitId },
          })
          // Anything that's no longer a draft can't be edited via the tab.
          if (visit.status !== "draft" || visit.deleted_at) {
            stale.push(tab.tabId)
          }
        } catch {
          // Visit gone (404 / sync conflict / etc.) → drop the tab.
          stale.push(tab.tabId)
        }
      }
      if (stale.length > 0) pruneTabs(stale)
    })()
    // The ref + userId guard means we only reconcile once per mount of the
    // shell; `tabs` is intentionally excluded from the dep array so the
    // effect doesn't re-run as we mutate the store inside it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [userId])

  return (
    <div className="flex h-full flex-col">
      <VisitTabsStrip />
      <div className="flex-1 overflow-y-auto">
        <Outlet />
      </div>
    </div>
  )
}
