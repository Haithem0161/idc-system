/**
 * `/accounting/visits/:id` -- read-only Visit Detail for accountants
 * (phase-07 §7.13). Re-uses the reception visit-detail component which
 * already gates Edit / Void on the visit status and the actor role, so
 * accountants see Print + tabs but no destructive controls.
 */
import VisitDetailPage from "@/pages/reception/visit-detail"

export default function AccountingVisitDrillPage () {
  return <VisitDetailPage />
}
