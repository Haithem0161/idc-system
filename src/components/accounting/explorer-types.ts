/**
 * Shared types for the accounting explorer. The master list is generic over a
 * normalized `MasterRow`; each entity adapter maps its earnings/report record
 * into this shape so the list, search, and selection logic stay in one place.
 */

export type ExplorerEntity = "visits" | "doctors" | "operators" | "checks"

export const EXPLORER_ENTITIES: ExplorerEntity[] = [
  "visits",
  "doctors",
  "operators",
  "checks",
]

export function isExplorerEntity (value: string | undefined): value is ExplorerEntity {
  return (
    value === "visits" ||
    value === "doctors" ||
    value === "operators" ||
    value === "checks"
  )
}

export interface MasterRow {
  /** URL segment id for this row (house doctor -> "house"; visit -> visit_id). */
  id: string
  /** Display name (already locale-resolved / house-labeled by the adapter). */
  name: string
  /** Secondary line under the name. */
  sub: string
  /** Pre-formatted primary metric shown on the trailing side. */
  primary: string
  /** Pre-formatted secondary metric under the primary. */
  secondary: string
  /** Lowercased text used for client-side search matching. */
  searchText: string
  /** Render the name in the muted house tone. */
  house?: boolean
}
