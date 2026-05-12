/**
 * Format ISO timestamp helpers used by the shifts UI. Locale-aware (uses
 * the browser's `Intl` APIs); RTL is handled at the document level.
 */

export function formatTime (iso: string): string {
  const d = new Date(iso)
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
}

export function formatDuration (startIso: string, endIso: string): string {
  const ms = new Date(endIso).getTime() - new Date(startIso).getTime()
  return humanizeDuration(ms)
}

export function formatSince (startIso: string): string {
  const ms = Date.now() - new Date(startIso).getTime()
  return humanizeDuration(ms)
}

function humanizeDuration (ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—"
  const totalSeconds = Math.floor(ms / 1000)
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  if (hours >= 1) {
    return `${hours}h ${String(minutes).padStart(2, "0")}m`
  }
  return `${minutes}m`
}
