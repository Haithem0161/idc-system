import { useTranslation } from "react-i18next"

import type { StockStatusLiteral } from "@/lib/ipc"

/**
 * OK / LOW / NEG pill rendered next to inventory rows. Maps to the editorial
 * design system status-pill variants (phase-06 §3.Frontend `<StockStatusPill>`).
 */
export function StockStatusPill ({ status }: { status: StockStatusLiteral }) {
  const { t } = useTranslation()
  const variantClass =
    status === "ok"
      ? "is-success"
      : status === "low"
        ? "is-warn"
        : "is-danger"
  const label = t(`inventory.list.status_pill.${status}` as const)
  return <span className={`status-pill ${variantClass}`}>{label}</span>
}
