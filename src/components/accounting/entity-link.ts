/**
 * Shared helpers for the accounting explorer: encode the nullable doctor id
 * (house = `doctor_id: null`) into a stable URL segment and back, and resolve
 * the display label for the house ("Internal") row.
 *
 * The house doctor is the clinic itself keeping the full cut on a no-referral
 * visit. On the wire it is `doctor_id: null`; in the URL it is the literal
 * `house` segment so the route is shareable.
 */

export const HOUSE_SEGMENT = "house"

/**
 * Sentinel the reports repo emits as the `by_doctor` group key for the house
 * pseudo-row (`COALESCE(v.doctor_id, '__house__')`). Distinct from the URL
 * segment, so the explorer maps it explicitly when building cross-links.
 */
export const HOUSE_GROUP_KEY = "__house__"

/** Encode a nullable doctor id to a URL segment. */
export function doctorIdToSegment (doctorId: string | null): string {
  return doctorId ?? HOUSE_SEGMENT
}

/** Decode a URL segment back to a nullable doctor id. */
export function segmentToDoctorId (segment: string): string | null {
  return segment === HOUSE_SEGMENT ? null : segment
}

export function isHouseSegment (segment: string | undefined): boolean {
  return segment === HOUSE_SEGMENT
}

/**
 * Map a `by_doctor` report group key to the doctor URL segment. The repo emits
 * `__house__` for no-referral visits; everything else is a real doctor id.
 */
export function doctorGroupKeyToSegment (groupKey: string): string {
  return groupKey === HOUSE_GROUP_KEY ? HOUSE_SEGMENT : groupKey
}

/** True when a `by_doctor` report group key is the house pseudo-row. */
export function isHouseGroupKey (groupKey: string): boolean {
  return groupKey === HOUSE_GROUP_KEY
}
