/**
 * Toast policy.
 *
 * Phase-1 ships no third-party toast library; this module documents the
 * "no phantom toasts" rule (PRD §10.8): network failures absorbed by the
 * sync engine are routed to the `<SyncPill>` state, not toasted. Domain
 * mutations that fail in the UI may surface a toast in subsequent phases,
 * but they MUST filter on `Error.cause`:
 *
 *   if ((err as { cause?: unknown }).cause === "network" || "offline") return
 *
 * This helper centralises that gating so later phases can swap in any
 * implementation (sonner, react-hot-toast) without rewriting filters.
 */

export type ToastKind = "info" | "success" | "warning" | "error"

export interface ToastOptions {
  cause?: "network" | "offline" | "domain"
}

export function emitToast (
  kind: ToastKind,
  message: string,
  options: ToastOptions = {}
): void {
  if (options.cause === "network" || options.cause === "offline") {
    // Suppressed: surfaced via <SyncPill> instead.
    return
  }
  // Phase-1: log only. Phase-2 wires a real toaster.
  // eslint-disable-next-line no-console
  console.info(`[toast:${kind}] ${message}`)
}
