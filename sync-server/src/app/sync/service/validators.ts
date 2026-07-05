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
  'report_pct',
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
 * equal price + dye, the doctor-cut / internal-pct exclusivity accounting for
 * the built-in `dalal` substitute, the report coherence, and the name
 * snapshots).
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
  // مندوب requires a real referring doctor (the opposite polarity of dalal).
  // Holds in EVERY status -- mirrors the desktop CHECK
  // `mandoub_id IS NULL OR doctor_id IS NOT NULL`.
  if (row.mandoub_id != null && row.doctor_id == null) {
    throw new DomainError(
      'VALIDATION_ERROR',
      'mandoub_id requires a referring doctor (doctor_id)',
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
      'report_amount_snapshot_iqd',
      'doctor_cut_snapshot_iqd',
      'operator_cut_snapshot_iqd',
      'total_amount_iqd_snapshot',
    ]
    // مندوب snapshot keys are part of the locked snapshot but are NULL on a
    // visit with no مندوب, so they are validated for coherence (above) rather
    // than required-non-null here. Their immutability is enforced via the
    // conflict snapshotKeys in the entity store (a post-lock mutation parks).
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
      (row.dye_cost_snapshot_iqd ?? 0)
    if (total !== row.total_amount_iqd_snapshot) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'total_amount_iqd_snapshot must equal price + dye',
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
    // The editable per-visit price override is snapshotted into
    // `price_snapshot_iqd` at lock (validated against the total invariant
    // above), but the raw override itself must still be non-negative when
    // present. Mirrors the desktop `Visit` draft guard.
    if (row.price_override_iqd != null && row.price_override_iqd < 0) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'price_override_iqd must be >= 0',
        422,
        { op_id: opId }
      )
    }
    // `dalal` is a built-in doctor substitute (flat cut). It is mutually
    // exclusive with a referring doctor: a visit cannot route a cut to both.
    if (row.dalal && row.doctor_id != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'dalal and doctor_id are mutually exclusive',
        422,
        { op_id: opId }
      )
    }
    // `discount` zeroes the referring doctor's cut, so it is only valid with a
    // real referring doctor. When the visit is locked with the discount on, the
    // money engine must have already zeroed the doctor cut snapshot.
    if (row.discount) {
      if (row.doctor_id == null) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'discount requires a referring doctor',
          422,
          { op_id: opId }
        )
      }
      if (
        row.status === 'locked' &&
        (row.doctor_cut_snapshot_iqd ?? 0) !== 0
      ) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'doctor_cut_snapshot must be 0 when discount is on',
          422,
          { op_id: opId }
        )
      }
    }
    // House mode is the ONLY mode that carries an internal cut split: no
    // referring doctor and no dalal substitute. In every other mode the cut is
    // determined by the doctor/dalal, so internal_pct_snapshot must be null.
    const isHouse = row.doctor_id == null && !row.dalal
    if (isHouse && row.internal_pct_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot required in house mode (no doctor, no dalal)',
        422,
        { op_id: opId }
      )
    }
    if (!isHouse && row.internal_pct_snapshot != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'internal_pct_snapshot must be null when doctor_id is set or dalal is true',
        422,
        { op_id: opId }
      )
    }
    // Report coherence. The report surcharge is no longer part of the patient
    // bill; it is a payable derived from report_pct. A locked visit must keep
    // its report snapshot consistent with the `report` flag.
    if (!row.report) {
      if ((row.report_amount_snapshot_iqd ?? 0) !== 0) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'report_amount_snapshot_iqd must be 0 when report is false',
          422,
          { op_id: opId }
        )
      }
      if (row.report_pct_snapshot != null || row.reporting_doctor_name_snapshot != null) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'report_pct_snapshot and reporting_doctor_name_snapshot must be absent when report is false',
          422,
          { op_id: opId }
        )
      }
    } else if (row.report_pct_snapshot == null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'report_pct_snapshot required when report is true',
        422,
        { op_id: opId }
      )
    }
    // مندوب snapshot coherence on a locked visit. Mirrors the desktop CHECK:
    // when mandoub_id is set, the cut is 500 or 1000 IQD and the name snapshot
    // is captured; when null, both مندوب snapshots are null. (The doctor-present
    // requirement is enforced above for every status.)
    if (row.mandoub_id != null) {
      if (row.mandoub_cut_snapshot_iqd !== 500 && row.mandoub_cut_snapshot_iqd !== 1000) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'mandoub_cut_snapshot_iqd must be 500 or 1000 on a locked visit with a mandoub',
          422,
          { op_id: opId }
        )
      }
      if (row.mandoub_name_snapshot == null) {
        throw new DomainError(
          'VALIDATION_ERROR',
          'mandoub_name_snapshot required on a locked visit with a mandoub',
          422,
          { op_id: opId }
        )
      }
    } else if (row.mandoub_cut_snapshot_iqd != null || row.mandoub_name_snapshot != null) {
      throw new DomainError(
        'VALIDATION_ERROR',
        'mandoub_cut_snapshot_iqd and mandoub_name_snapshot must be null when mandoub_id is null',
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
