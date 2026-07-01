import type {
  CheckSubtypeSyncRecord,
  CheckTypeSyncRecord,
  ConsumptionSyncRecord,
  DailyCloseSyncRecord,
  DoctorPricingSyncRecord,
  DoctorSyncRecord,
  InventoryAdjustmentSyncRecord,
  InventoryItemSyncRecord,
  MandoubSyncRecord,
  OperatorShiftSyncRecord,
  OperatorSpecialtySyncRecord,
  OperatorSyncRecord,
  PatientSyncRecord,
  SettingSyncRecord,
  UserSyncRecord,
  VisitSyncRecord,
} from '../infrastructure/memory/store'

/**
 * Entity-level write port consumed by `SyncPushService`.
 *
 * Both `MemorySyncStore` (test bootstrap) and the Prisma-backed
 * `PrismaEntityStore` (production) implement this contract.
 *
 * Authored per phase-09 §3 Sync Server (Infrastructure additions).
 */
export interface SyncEntityStore {
  upsertUser (row: UserSyncRecord): Promise<{ applied: boolean }>
  upsertSetting (row: SettingSyncRecord): Promise<{ applied: boolean }>
  detectSettingConflict (incoming: SettingSyncRecord): Promise<SettingSyncRecord | null> | SettingSyncRecord | null

  upsertCheckType (row: CheckTypeSyncRecord): Promise<{ applied: boolean }>
  upsertCheckSubtype (row: CheckSubtypeSyncRecord): Promise<{ applied: boolean }>
  upsertDoctor (row: DoctorSyncRecord): Promise<{ applied: boolean }>
  upsertDoctorPricing (row: DoctorPricingSyncRecord): Promise<{ applied: boolean }>
  upsertOperator (row: OperatorSyncRecord): Promise<{ applied: boolean }>
  upsertMandoub (row: MandoubSyncRecord): Promise<{ applied: boolean }>
  upsertOperatorSpecialty (row: OperatorSpecialtySyncRecord): Promise<{ applied: boolean }>
  upsertInventoryItem (row: InventoryItemSyncRecord): Promise<{ applied: boolean }>
  upsertConsumption (row: ConsumptionSyncRecord): Promise<{ applied: boolean }>

  upsertPatient (row: PatientSyncRecord): Promise<{ applied: boolean }>
  upsertVisit (row: VisitSyncRecord): Promise<{ applied: boolean }>
  detectVisitConflict (incoming: VisitSyncRecord): Promise<VisitSyncRecord | null> | VisitSyncRecord | null

  upsertInventoryAdjustment (row: InventoryAdjustmentSyncRecord): Promise<{ applied: boolean, duplicate: boolean }>
  upsertOperatorShift (row: OperatorShiftSyncRecord): Promise<{ applied: boolean }>

  /**
   * Signed & frozen daily close. LWW (version-gated): the freeze is version 1;
   * a superadmin reopen is version 2 of the same id.
   */
  upsertDailyClose (row: DailyCloseSyncRecord): Promise<{ applied: boolean }>

  /**
   * Used by push-service validators to pre-check parent invariants
   * (subtype/dye-supported) without a full upsert.
   */
  getCheckType (id: string): Promise<CheckTypeSyncRecord | null> | CheckTypeSyncRecord | null

  // ---- Reports read-side (phase-09 follow-up: reports-service port) -------
  /**
   * All non-deleted visits for the tenant. Reports filter in-memory; the
   * dataset is bounded by the v0.1.0 retention window (90 days at most).
   */
  listAllVisits (tenantId: string): Promise<VisitSyncRecord[]>

  /**
   * All non-deleted inventory adjustments for the tenant. Daily-close
   * needs `consume_visit` rows in a window; reports do the filter.
   */
  listAllInventoryAdjustments (tenantId: string): Promise<InventoryAdjustmentSyncRecord[]>

  /**
   * All operator shifts for the tenant. Daily-close computes hours-on-
   * shift overlap; reports do the filter.
   */
  listAllOperatorShifts (tenantId: string): Promise<OperatorShiftSyncRecord[]>
}
