import type { ReactNode } from "react"

import { DateRangePicker } from "@/components/accounting/date-range-picker"
import { IncludeVoidedToggle } from "@/components/accounting/include-voided-toggle"

/**
 * Shared filter toolbar for every accounting view (dashboard + explorer).
 * Holds the persisted date-range preset, the locked/voided toggle, and an
 * optional trailing action slot (typically an Export button). Keeping it in
 * one place means the filter context is identical and visually consistent as
 * the user moves between the dashboard and the explorer.
 */
export function AccountingToolbar ({ actions }: { actions?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-4">
      <DateRangePicker />
      <IncludeVoidedToggle />
      {actions ? <div className="ms-auto flex items-center gap-2">{actions}</div> : null}
    </div>
  )
}
