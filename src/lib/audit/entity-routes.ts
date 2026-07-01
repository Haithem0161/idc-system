/**
 * Maps an audit-row `entity` to its detail-page route, when one exists.
 * Returns `null` for entities without a detail page (`settings`,
 * `audit_log`, sync-engine internals). Phase-08 §7.7.
 */
export function entityDetailRoute(entity: string, entityId: string): string | null {
  switch (entity) {
    case "users":
      return `/admin/users/${entityId}`
    case "doctors":
      return `/admin/doctors/${entityId}`
    case "operators":
      return `/admin/operators/${entityId}`
    case "mandoubs":
      return `/admin/mandoubs/${entityId}`
    case "check_types":
      return `/admin/check-types/${entityId}`
    case "inventory_items":
      return `/admin/inventory/${entityId}`
    case "visits":
      return `/reception/visits/${entityId}`
    case "patients":
      // No dedicated patient detail page yet -- reuses the visit list filtered
      // by patient. Keep null so the audit row doesn't link to a 404.
      return null
    default:
      return null
  }
}
