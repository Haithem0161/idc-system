/**
 * Abbreviated money formatters used by the dashboard leaderboards and the
 * explorer master list, where space is tight and exact figures live in the
 * detail pane.
 */

/** Abbreviate to millions ("18.4M") or thousands ("130k"); else the raw value. */
export function abbreviateIqd (amount: number): string {
  if (Math.abs(amount) >= 1_000_000) return `${(amount / 1_000_000).toFixed(1)}M`
  if (Math.abs(amount) >= 1_000) return `${Math.round(amount / 1_000)}k`
  return String(amount)
}

/** Round to the nearest thousand and suffix "k" -- for per-visit/per-hour rates. */
export function thousandsK (amount: number): string {
  return `${Math.round(amount / 1_000)}k`
}
