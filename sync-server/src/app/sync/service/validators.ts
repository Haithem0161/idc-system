// Shared domain validators for syncable entities. Used by BOTH the push path
// (SyncPushService) and the conflict-resolution path (ConflictResolveService)
// so a `merged`/`local` payload accepted at conflict resolution passes the same
// invariants as a payload accepted on push -- closing the phase-10 T4 bypass
// where a malformed merge could be persisted without validation.

import { DomainError } from '../../common/errors/domain'
import type {
  SettingSyncRecord,
  VisitSyncRecord,
} from '../infrastructure/memory/store'

/**
 * Settings whose deletion would break money math or core display. A delete
 * (tombstone) for one of these keys is rejected; they may be updated but not
 * removed.
 */
export const PROTECTED_SETTING_KEYS = new Set([
  'dye_cost_iqd',
  'report_cost_iqd',
  'internal_doctor_pct',
  'idle_lock_minutes',
  'arabic_numerals',
  'clinic_display_name_ar',
  'clinic_display_name_en',
  'currency_symbol',
  'thermal_width',
  'thermal_printer_name',
])

/** Validate a settings sync record: shape + the protected-key delete guard. */
export function validateSetting (row: SettingSyncRecord, opId: string): void {
  if (!row.id || !row.key || typeof row.key !== 'string') {
    throw new DomainError(
      'VALIDATION_ERROR',
      'setting missing required fields (id, key)',
      422,
      { op_id: opId }
    )
  }
  if (row.deleted_at && PROTECTED_SETTING_KEYS.has(row.key)) {
    throw new DomainError(
      'VALIDATION_ERROR',
      `${row.key} is a required setting and cannot be deleted`,
      422,
      { op_id: opId }
    )
  }
}

/**
 * Validate a visit sync record: required fields, status enum, and the locked /
 * voided invariants including the financial snapshot integrity (the total must
 * equal price + dye + report, the doctor-cut / internal-pct exclusivity, and
 * the name snapshots).
 */
export function validateVisit (row: VisitSyncRecord, opId: string): void {
  if (!row.patient_id || !row.receptionist_user_id || !row.check_type_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'visit missing required fields',
      422,
      { op_id: opId }
    )
  }
  if (!['draft', 'locked', 'voided'].includes(row.status)) {
    throw new DomainError(
      'VALIDATION_ERROR',
      `visit status invalid: ${row.status}`,
      422,
      { op_id: opId }
    )
  }
  if (row.status === 'locked') {
    if (row.operator_id == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'locked visit must have operator_id',
        422,
        { op_id: opId }
      )
    }
    const snapKeys: Array<keyof VisitSyncRecord> = [
      'price_snapshot_iqd',
      'dye_cost_snapshot_iqd',
      'report_cost_snapshot_iqd',
      'doctor_cut_snapshot_iqd',
      'operator_cut_snapshot_iqd',
      'total_amount_iqd_snapshot',
    ]
    for (const k of snapKeys) {
      if (row[k] == null) {
        throw new DomainError(
          'VALIDATION_ERROR',
          `locked visit missing snapshot field: ${String(k)}`,
          422,
          { op_id: opId }
        )
      }
    }
    const total =
      (row.price_snapshot_iqd ?? 0) +
      (row.dye_cost_snapshot_iqd ?? 0) +
      (row.report_cost_snapshot_iqd ?? 0)
    if (total !== row.total_amount_iqd_snapshot) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'total_amount_iqd_snapshot must equal price + dye + report',
        422,
        { op_id: opId }
      )
    }
    // The collected-cash override is decoupled from the billed total above, but
    // must still be non-negative when present (0 = waived). Mirrors the desktop
    // `Visit::lock` guard so a malformed push cannot land a negative amount.
    if (row.amount_paid_override_iqd != null && row.amount_paid_override_iqd < 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'amount_paid_override_iqd must be >= 0',
        422,
        { op_id: opId }
      )
    }
    if (row.doctor_id == null && row.internal_pct_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot required when doctor_id is null',
        422,
        { op_id: opId }
      )
    }
    if (row.doctor_id != null && row.internal_pct_snapshot != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot must be null when doctor_id is set',
        422,
        { op_id: opId }
      )
    }
    if (row.patient_name_snapshot == null || row.operator_name_snapshot == null || row.check_type_name_ar_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'locked visit missing name snapshot fields',
        422,
        { op_id: opId }
      )
    }
  }
  if (row.status === 'voided') {
    if (!row.voided_by_user_id || row.void_reason == null || row.void_reason.trim().length < 5) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'voided visit requires voided_by_user_id and a >= 5-char void_reason',
        422,
        { op_id: opId }
      )
    }
  }
  if (row.dye && !row.check_type_id) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'visit dye requires check_type_id',
      422,
      { op_id: opId }
    )
  }
}
