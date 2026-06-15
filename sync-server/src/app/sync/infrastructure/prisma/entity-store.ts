import type { Prisma, PrismaClient } from '@prisma/client'

import type { SyncEntityStore } from '../../domain/sync-store'
import type { ChangeRow } from '../../domain/types'
import type {
  CheckSubtypeSyncRecord,
  CheckTypeSyncRecord,
  ConsumptionSyncRecord,
  DoctorPricingSyncRecord,
  DoctorSyncRecord,
  InventoryAdjustmentSyncRecord,
  InventoryItemSyncRecord,
  OperatorShiftSyncRecord,
  OperatorSpecialtySyncRecord,
  OperatorSyncRecord,
  PatientSyncRecord,
  SettingSyncRecord,
  UserSyncRecord,
  VisitSyncRecord,
} from '../memory/store'

/**
 * Prisma-backed `SyncEntityStore` for every syncable model.
 *
 * Authored per phase-09 §3 Sync Server (entity-repo) and §4 (LWW helper).
 *
 * All upserts pass through `lwwUpsert` (additive entities — operator_shifts,
 * inventory_adjustments — get bespoke wrappers that match the additive
 * policy from phase-04 §7.9 and phase-05 §7.36). The `(version, updated_at,
 * origin_device_id)` tiebreak is centralised in `lwwShouldApply` so it never
 * drifts across entities (phase-03 §7.17).
 */
export class PrismaEntityStore implements SyncEntityStore {
  constructor (private readonly prisma: PrismaClient) {}

  // ---- users ---------------------------------------------------------------

  async upsertUser (row: UserSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<UserSyncRecord>(
      row,
      async () => {
        const existing = await this.prisma.user.findUnique({ where: { id: row.id } })
        return existing
          ? {
              id: existing.id,
              version: existing.version,
              updated_at: existing.updatedAt.toISOString(),
              origin_device_id: existing.originDeviceId,
            }
          : null
      },
      async () => {
        // A pushed user update with an unchanged password omits password_hash
        // (or sends null). NEVER coerce that to '' -- doing so would overwrite
        // the stored argon2 hash and lock the user out. Only set passwordHash
        // when the client actually carries a non-empty hash.
        const incomingHash =
          typeof row.password_hash === 'string' && row.password_hash.length > 0
            ? row.password_hash
            : null
        const data = {
          id: row.id,
          email: row.email,
          name: row.name,
          role: row.role,
          isActive: row.is_active,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.user.upsert({
          where: { id: row.id },
          // On create the column is NOT NULL, so a brand-new user with no hash
          // gets ''; they must complete an online login to populate it.
          create: { ...data, passwordHash: incomingHash ?? '' },
          // On update, only touch passwordHash when one was actually sent.
          update: {
            ...data,
            createdAt: undefined,
            ...(incomingHash !== null ? { passwordHash: incomingHash } : {}),
          },
        })
      }
    )
  }

  // ---- settings ------------------------------------------------------------

  async upsertSetting (row: SettingSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<SettingSyncRecord>(
      row,
      async () => {
        const existing = await this.prisma.setting.findUnique({ where: { id: row.id } })
        return existing
          ? {
              id: existing.id,
              version: existing.version,
              updated_at: existing.updatedAt.toISOString(),
              origin_device_id: existing.originDeviceId,
            }
          : null
      },
      async () => {
        const data = {
          id: row.id,
          key: row.key,
          value: row.value,
          valueType: row.value_type,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.setting.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async detectSettingConflict (incoming: SettingSyncRecord): Promise<SettingSyncRecord | null> {
    const existing = await this.prisma.setting.findUnique({ where: { id: incoming.id } })
      ?? await this.prisma.setting.findFirst({
        where: { entityId: incoming.entity_id, key: incoming.key, deletedAt: null },
      })
    if (!existing) return null
    if (existing.id === incoming.id && existing.version === incoming.version) return null
    if (
      existing.version >= incoming.version
      && (existing.value !== incoming.value || existing.valueType !== incoming.value_type)
    ) {
      return {
        id: existing.id,
        key: existing.key,
        value: existing.value,
        value_type: existing.valueType,
        entity_id: existing.entityId,
        version: existing.version,
        updated_at: existing.updatedAt.toISOString(),
        deleted_at: existing.deletedAt ? existing.deletedAt.toISOString() : null,
        origin_device_id: existing.originDeviceId,
      }
    }
    return null
  }

  // ---- catalog -------------------------------------------------------------

  async upsertCheckType (row: CheckTypeSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<CheckTypeSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.checkType, row.id),
      async () => {
        const data: Prisma.CheckTypeUncheckedCreateInput = {
          id: row.id,
          nameAr: row.name_ar,
          nameEn: row.name_en,
          hasSubtypes: row.has_subtypes,
          basePriceIqd: row.base_price_iqd,
          dyeSupported: row.dye_supported,
          reportSupported: row.report_supported,
          sortOrder: row.sort_order,
          isActive: row.is_active,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.checkType.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async getCheckType (id: string): Promise<CheckTypeSyncRecord | null> {
    const row = await this.prisma.checkType.findUnique({ where: { id } })
    if (!row) return null
    return {
      id: row.id,
      name_ar: row.nameAr,
      name_en: row.nameEn,
      has_subtypes: row.hasSubtypes,
      base_price_iqd: row.basePriceIqd,
      dye_supported: row.dyeSupported,
      report_supported: row.reportSupported,
      sort_order: row.sortOrder,
      is_active: row.isActive,
      entity_id: row.entityId,
      version: row.version,
      updated_at: row.updatedAt.toISOString(),
      deleted_at: row.deletedAt ? row.deletedAt.toISOString() : null,
      origin_device_id: row.originDeviceId,
    }
  }

  async upsertCheckSubtype (row: CheckSubtypeSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<CheckSubtypeSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.checkSubtype, row.id),
      async () => {
        const data: Prisma.CheckSubtypeUncheckedCreateInput = {
          id: row.id,
          checkTypeId: row.check_type_id,
          nameAr: row.name_ar,
          nameEn: row.name_en,
          priceIqd: row.price_iqd,
          sortOrder: row.sort_order,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.checkSubtype.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertDoctor (row: DoctorSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<DoctorSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.doctor, row.id),
      async () => {
        const data: Prisma.DoctorUncheckedCreateInput = {
          id: row.id,
          name: row.name,
          specialty: row.specialty,
          phone: row.phone,
          isActive: row.is_active,
          notes: row.notes,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.doctor.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertDoctorPricing (row: DoctorPricingSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<DoctorPricingSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.doctorCheckPricing, row.id),
      async () => {
        const data: Prisma.DoctorCheckPricingUncheckedCreateInput = {
          id: row.id,
          doctorId: row.doctor_id,
          checkTypeId: row.check_type_id,
          checkSubtypeId: row.check_subtype_id,
          priceOverrideIqd: row.price_override_iqd,
          cutKind: row.cut_kind,
          cutValue: row.cut_value,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.doctorCheckPricing.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertOperator (row: OperatorSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<OperatorSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.operator, row.id),
      async () => {
        const data: Prisma.OperatorUncheckedCreateInput = {
          id: row.id,
          name: row.name,
          phone: row.phone,
          baseCutPerCheckIqd: row.base_cut_per_check_iqd,
          isActive: row.is_active,
          notes: row.notes,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.operator.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertOperatorSpecialty (row: OperatorSpecialtySyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<OperatorSpecialtySyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.operatorSpecialty, row.id),
      async () => {
        const data: Prisma.OperatorSpecialtyUncheckedCreateInput = {
          id: row.id,
          operatorId: row.operator_id,
          checkTypeId: row.check_type_id,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.operatorSpecialty.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertInventoryItem (row: InventoryItemSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<InventoryItemSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.inventoryItem, row.id),
      async () => {
        const data: Prisma.InventoryItemUncheckedCreateInput = {
          id: row.id,
          nameAr: row.name_ar,
          nameEn: row.name_en,
          unit: row.unit,
          quantityOnHand: row.quantity_on_hand,
          lowStockThreshold: row.low_stock_threshold,
          isActive: row.is_active,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.inventoryItem.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertConsumption (row: ConsumptionSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<ConsumptionSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.inventoryConsumptionMap, row.id),
      async () => {
        const data: Prisma.InventoryConsumptionMapUncheckedCreateInput = {
          id: row.id,
          checkTypeId: row.check_type_id,
          checkSubtypeId: row.check_subtype_id,
          itemId: row.item_id,
          quantityPerCheck: row.quantity_per_check,
          onDyeOnly: row.on_dye_only,
          createdAt: new Date(row.updated_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.inventoryConsumptionMap.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  // ---- reception -----------------------------------------------------------

  async upsertPatient (row: PatientSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<PatientSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.patient, row.id),
      async () => {
        const data: Prisma.PatientUncheckedCreateInput = {
          id: row.id,
          name: row.name,
          createdAt: new Date(row.created_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.patient.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async upsertVisit (row: VisitSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<VisitSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.visit, row.id),
      async () => {
        const data: Prisma.VisitUncheckedCreateInput = {
          id: row.id,
          patientId: row.patient_id,
          status: row.status,
          receptionistUserId: row.receptionist_user_id,
          checkTypeId: row.check_type_id,
          checkSubtypeId: row.check_subtype_id,
          doctorId: row.doctor_id,
          operatorId: row.operator_id,
          dye: row.dye,
          report: row.report,
          lockedAt: row.locked_at ? new Date(row.locked_at) : null,
          voidedAt: row.voided_at ? new Date(row.voided_at) : null,
          voidedByUserId: row.voided_by_user_id,
          voidReason: row.void_reason,
          priceSnapshotIqd: row.price_snapshot_iqd,
          dyeCostSnapshotIqd: row.dye_cost_snapshot_iqd,
          reportCostSnapshotIqd: row.report_cost_snapshot_iqd,
          doctorCutSnapshotIqd: row.doctor_cut_snapshot_iqd,
          operatorCutSnapshotIqd: row.operator_cut_snapshot_iqd,
          internalPctSnapshot: row.internal_pct_snapshot,
          totalAmountIqdSnapshot: row.total_amount_iqd_snapshot,
          patientNameSnapshot: row.patient_name_snapshot,
          doctorNameSnapshot: row.doctor_name_snapshot,
          operatorNameSnapshot: row.operator_name_snapshot,
          checkTypeNameArSnapshot: row.check_type_name_ar_snapshot,
          checkTypeNameEnSnapshot: row.check_type_name_en_snapshot,
          checkSubtypeNameArSnapshot: row.check_subtype_name_ar_snapshot,
          checkSubtypeNameEnSnapshot: row.check_subtype_name_en_snapshot,
          createdAt: new Date(row.created_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.visit.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  async detectVisitConflict (incoming: VisitSyncRecord): Promise<VisitSyncRecord | null> {
    const existing = await this.prisma.visit.findUnique({ where: { id: incoming.id } })
    if (!existing) return null
    const reified = toVisitSyncRecord(existing)
    const snapshotKeys: (keyof VisitSyncRecord)[] = [
      'status',
      'price_snapshot_iqd',
      'dye_cost_snapshot_iqd',
      'report_cost_snapshot_iqd',
      'doctor_cut_snapshot_iqd',
      'operator_cut_snapshot_iqd',
      'internal_pct_snapshot',
      'total_amount_iqd_snapshot',
    ]
    const snapshotDiffers = snapshotKeys.some((k) => reified[k] !== incoming[k])
    if (incoming.version < reified.version && snapshotDiffers) return reified
    if (incoming.version === reified.version && snapshotDiffers) return reified
    return null
  }

  async upsertInventoryAdjustment (
    row: InventoryAdjustmentSyncRecord
  ): Promise<{ applied: boolean, duplicate: boolean }> {
    const existing = await this.prisma.inventoryAdjustment.findUnique({ where: { id: row.id } })
    if (existing) return { applied: false, duplicate: true }

    await this.prisma.$transaction(async (tx) => {
      await tx.inventoryAdjustment.create({
        data: {
          id: row.id,
          itemId: row.item_id,
          delta: row.delta,
          reason: row.reason,
          visitId: row.visit_id,
          note: row.note,
          byUserId: row.by_user_id,
          createdAt: new Date(row.created_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        },
      })
      // Phase-10 T11: scope the on-hand recompute by tenant. Without the
      // entityId filter a push op referencing another tenant's item_id would
      // read (aggregate) and overwrite that tenant's inventory item. The
      // incoming row's entity_id was already verified against the JWT tenant by
      // the push service (assertTenantMatches), so it is the authoritative
      // tenant here. updateMany (not update) means a cross-tenant item_id
      // matches zero rows -- a safe no-op rather than a cross-tenant write.
      const agg = await tx.inventoryAdjustment.aggregate({
        where: { itemId: row.item_id, entityId: row.entity_id, deletedAt: null },
        _sum: { delta: true },
      })
      const total = agg._sum.delta ?? 0
      await tx.inventoryItem.updateMany({
        where: { id: row.item_id, entityId: row.entity_id },
        data: {
          quantityOnHand: total,
          version: { increment: 1 },
          updatedAt: new Date(),
        },
      })
    })
    return { applied: true, duplicate: false }
  }

  // ---- reports read-side --------------------------------------------------

  async listAllVisits (tenantId: string): Promise<VisitSyncRecord[]> {
    const rows = await this.prisma.visit.findMany({
      where: { entityId: tenantId, deletedAt: null },
    })
    return rows.map((r) => toVisitSyncRecord(r))
  }

  async listAllInventoryAdjustments (
    tenantId: string
  ): Promise<InventoryAdjustmentSyncRecord[]> {
    const rows = await this.prisma.inventoryAdjustment.findMany({
      where: { entityId: tenantId, deletedAt: null },
    })
    return rows.map((r) => toInventoryAdjustmentSyncRecord(r))
  }

  async listAllOperatorShifts (
    tenantId: string
  ): Promise<OperatorShiftSyncRecord[]> {
    const rows = await this.prisma.operatorShift.findMany({
      where: { entityId: tenantId, deletedAt: null },
    })
    return rows.map((r) => toOperatorShiftSyncRecord(r))
  }

  async upsertOperatorShift (row: OperatorShiftSyncRecord): Promise<{ applied: boolean }> {
    return this.lwwUpsert<OperatorShiftSyncRecord>(
      row,
      async () => loadVersionMeta(this.prisma.operatorShift, row.id),
      async () => {
        const data: Prisma.OperatorShiftUncheckedCreateInput = {
          id: row.id,
          operatorId: row.operator_id,
          checkInAt: new Date(row.check_in_at),
          checkOutAt: row.check_out_at ? new Date(row.check_out_at) : null,
          checkInByUserId: row.check_in_by_user_id,
          checkOutByUserId: row.check_out_by_user_id,
          note: row.note,
          createdAt: new Date(row.created_at),
          updatedAt: new Date(row.updated_at),
          deletedAt: row.deleted_at ? new Date(row.deleted_at) : null,
          version: row.version,
          originDeviceId: row.origin_device_id ?? null,
          entityId: row.entity_id,
        }
        await this.prisma.operatorShift.upsert({
          where: { id: row.id },
          create: data,
          update: { ...data, createdAt: undefined },
        })
      }
    )
  }

  // ---- pull aggregation ----------------------------------------------------

  /**
   * Aggregate every syncable entity changed since the pull watermark.
   *
   * `sinceUpdatedAt` is the `at` component of the decoded pull cursor. Pushing
   * it into each `findMany` `where` means a pull only loads rows that changed
   * after the cursor instead of the ENTIRE tenant dataset across 14 tables on
   * every /sync/pull. Combined with a per-entity `take` cap, a single pull can
   * never load an unbounded result set. We use `updatedAt: { gt: ... }`; the
   * caller (`PrismaAuditLogRepo.changesSince`) re-applies the full keyset
   * (`updated_at`, `id`) filter in-memory after the merge, so the strict `gt`
   * boundary never drops a row that shares the cursor's exact timestamp.
   *
   * `orderBy [updatedAt asc, id asc]` makes the capped window the OLDEST
   * unsynced rows, so successive pulls walk the change set forward in cursor
   * order rather than returning an arbitrary slice.
   */
  async collectChanges (tenantId: string, sinceUpdatedAt?: string): Promise<ChangeRow[]> {
    const TAKE_CAP = 1000
    const since = sinceUpdatedAt ? new Date(sinceUpdatedAt) : null
    const sinceWhere = since ? { updatedAt: { gt: since } } : {}
    const orderBy = [{ updatedAt: 'asc' as const }, { id: 'asc' as const }]

    const [
      users, settings, checkTypes, checkSubtypes, doctors, doctorPricings,
      operators, operatorSpecialties, inventoryItems, consumptionMaps,
      operatorShifts, patients, visits, inventoryAdjustments,
    ] = await Promise.all([
      this.prisma.user.findMany({ where: { entityId: tenantId, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.setting.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.checkType.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.checkSubtype.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.doctor.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.doctorCheckPricing.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.operator.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.operatorSpecialty.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.inventoryItem.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.inventoryConsumptionMap.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      // Additive shifts: keep tombstones so other devices see the delete.
      this.prisma.operatorShift.findMany({ where: { entityId: tenantId, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.patient.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      this.prisma.visit.findMany({ where: { entityId: tenantId, deletedAt: null, ...sinceWhere }, orderBy, take: TAKE_CAP }),
      // Additive adjustments: keep tombstones for symmetry.
      this.prisma.inventoryAdjustment.findMany({ where: { entityId: tenantId, ...sinceWhere }, orderBy, take: TAKE_CAP }),
    ])

    const changes: ChangeRow[] = []
    pushChanges(changes, 'users', users, (r) => toUserSyncRecord(r))
    pushChanges(changes, 'settings', settings, (r) => toSettingSyncRecord(r))
    pushChanges(changes, 'check_types', checkTypes, (r) => toCheckTypeSyncRecord(r))
    pushChanges(changes, 'check_subtypes', checkSubtypes, (r) => toCheckSubtypeSyncRecord(r))
    pushChanges(changes, 'doctors', doctors, (r) => toDoctorSyncRecord(r))
    pushChanges(changes, 'doctor_check_pricing', doctorPricings, (r) => toDoctorPricingSyncRecord(r))
    pushChanges(changes, 'operators', operators, (r) => toOperatorSyncRecord(r))
    pushChanges(changes, 'operator_specialties', operatorSpecialties, (r) => toOperatorSpecialtySyncRecord(r))
    pushChanges(changes, 'inventory_items', inventoryItems, (r) => toInventoryItemSyncRecord(r))
    pushChanges(changes, 'inventory_consumption_map', consumptionMaps, (r) => toConsumptionSyncRecord(r))
    pushChanges(changes, 'operator_shifts', operatorShifts, (r) => toOperatorShiftSyncRecord(r))
    pushChanges(changes, 'patients', patients, (r) => toPatientSyncRecord(r))
    pushChanges(changes, 'visits', visits, (r) => toVisitSyncRecord(r))
    pushChanges(changes, 'inventory_adjustments', inventoryAdjustments, (r) => toInventoryAdjustmentSyncRecord(r))
    return changes
  }

  // ---- LWW core ------------------------------------------------------------

  private async lwwUpsert<R extends VersionedRow> (
    incoming: R,
    loadExisting: () => Promise<VersionMeta | null>,
    write: () => Promise<void>
  ): Promise<{ applied: boolean }> {
    const existing = await loadExisting()
    if (!existing) {
      await write()
      return { applied: true }
    }
    if (!lwwShouldApply(existing, incoming)) {
      return { applied: false }
    }
    await write()
    return { applied: true }
  }
}

interface VersionedRow {
  id: string
  version: number
  updated_at: string
  origin_device_id: string | null
}

interface VersionMeta {
  id: string
  version: number
  updated_at: string
  origin_device_id: string | null
}

/**
 * Compare two ISO/RFC3339 timestamps by their instant, not lexicographically.
 * Returns >0 when `a` is later, <0 when earlier, 0 when equal/unparseable.
 *
 * Client timestamps come from chrono (`...T10:00:00.123456789Z`, variable
 * fractional precision) while server timestamps come from
 * `Date.toISOString()` (always millisecond, `...T10:00:00.123Z`). A raw
 * `localeCompare` ranks the higher-precision string BEFORE the truncated one
 * ('4' < 'Z'), inverting LWW for same-millisecond writes. Parsing both to
 * epoch ms first normalizes the precision mismatch.
 */
function compareTimestamps (a: string, b: string): number {
  const ta = Date.parse(a)
  const tb = Date.parse(b)
  if (Number.isNaN(ta) || Number.isNaN(tb)) {
    // Unparseable input: fall back to a stable lexicographic order so the
    // comparison is still deterministic rather than throwing.
    return a < b ? -1 : a > b ? 1 : 0
  }
  return ta - tb
}

function lwwShouldApply (existing: VersionMeta, incoming: VersionedRow): boolean {
  if (incoming.version > existing.version) return true
  if (incoming.version < existing.version) return false
  const cmp = compareTimestamps(incoming.updated_at, existing.updated_at)
  if (cmp > 0) return true
  if (cmp < 0) return false
  const incomingOrigin = incoming.origin_device_id ?? ''
  const existingOrigin = existing.origin_device_id ?? ''
  if (incomingOrigin === '') return false
  if (existingOrigin === '') return true
  return incomingOrigin.localeCompare(existingOrigin) < 0
}

async function loadVersionMeta (
  delegate: {
    findUnique: (args: { where: { id: string } }) => Promise<{
      id: string
      version: number
      updatedAt: Date
      originDeviceId: string | null
    } | null>
  },
  id: string
): Promise<VersionMeta | null> {
  const row = await delegate.findUnique({ where: { id } })
  if (!row) return null
  return {
    id: row.id,
    version: row.version,
    updated_at: row.updatedAt.toISOString(),
    origin_device_id: row.originDeviceId,
  }
}

function pushChanges<T> (
  out: ChangeRow[],
  entity: string,
  rows: T[],
  toSync: (row: T) => { id: string, version: number, updated_at: string }
): void {
  for (const row of rows) {
    const sync = toSync(row)
    out.push({
      entity,
      entity_id: sync.id,
      payload: sync as unknown as Record<string, unknown>,
      updated_at: sync.updated_at,
      version: sync.version,
    })
  }
}

// ---- row → SyncRecord mappers ---------------------------------------------

function isoOrNull (d: Date | null | undefined): string | null {
  return d ? d.toISOString() : null
}

function toUserSyncRecord (r: {
  id: string
  email: string
  name: string
  role: 'superadmin' | 'receptionist' | 'accountant'
  isActive: boolean
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): UserSyncRecord {
  // SECURITY: never include password_hash in the pull payload. The client's
  // users pull-apply intentionally preserves the local hash and never reads
  // this field; shipping it would leak every user's argon2 hash to any
  // authenticated tenant member who pulls.
  return {
    id: r.id,
    email: r.email,
    name: r.name,
    role: r.role,
    is_active: r.isActive,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toSettingSyncRecord (r: {
  id: string
  key: string
  value: string
  valueType: 'int' | 'decimal' | 'text' | 'bool'
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): SettingSyncRecord {
  return {
    id: r.id,
    key: r.key,
    value: r.value,
    value_type: r.valueType,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toCheckTypeSyncRecord (r: {
  id: string
  nameAr: string
  nameEn: string | null
  hasSubtypes: boolean
  basePriceIqd: number | null
  dyeSupported: boolean
  reportSupported: boolean
  sortOrder: number
  isActive: boolean
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): CheckTypeSyncRecord {
  return {
    id: r.id,
    name_ar: r.nameAr,
    name_en: r.nameEn,
    has_subtypes: r.hasSubtypes,
    base_price_iqd: r.basePriceIqd,
    dye_supported: r.dyeSupported,
    report_supported: r.reportSupported,
    sort_order: r.sortOrder,
    is_active: r.isActive,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toCheckSubtypeSyncRecord (r: {
  id: string
  checkTypeId: string
  nameAr: string
  nameEn: string | null
  priceIqd: number
  sortOrder: number
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): CheckSubtypeSyncRecord {
  return {
    id: r.id,
    check_type_id: r.checkTypeId,
    name_ar: r.nameAr,
    name_en: r.nameEn,
    price_iqd: r.priceIqd,
    sort_order: r.sortOrder,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toDoctorSyncRecord (r: {
  id: string
  name: string
  specialty: string | null
  phone: string | null
  isActive: boolean
  notes: string | null
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): DoctorSyncRecord {
  return {
    id: r.id,
    name: r.name,
    specialty: r.specialty,
    phone: r.phone,
    is_active: r.isActive,
    notes: r.notes,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toDoctorPricingSyncRecord (r: {
  id: string
  doctorId: string
  checkTypeId: string
  checkSubtypeId: string | null
  priceOverrideIqd: number | null
  cutKind: 'pct' | 'fixed'
  cutValue: number
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): DoctorPricingSyncRecord {
  return {
    id: r.id,
    doctor_id: r.doctorId,
    check_type_id: r.checkTypeId,
    check_subtype_id: r.checkSubtypeId,
    price_override_iqd: r.priceOverrideIqd,
    cut_kind: r.cutKind,
    cut_value: r.cutValue,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toOperatorSyncRecord (r: {
  id: string
  name: string
  phone: string | null
  baseCutPerCheckIqd: number
  isActive: boolean
  notes: string | null
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): OperatorSyncRecord {
  return {
    id: r.id,
    name: r.name,
    phone: r.phone,
    base_cut_per_check_iqd: r.baseCutPerCheckIqd,
    is_active: r.isActive,
    notes: r.notes,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toOperatorSpecialtySyncRecord (r: {
  id: string
  operatorId: string
  checkTypeId: string
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): OperatorSpecialtySyncRecord {
  return {
    id: r.id,
    operator_id: r.operatorId,
    check_type_id: r.checkTypeId,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toInventoryItemSyncRecord (r: {
  id: string
  nameAr: string
  nameEn: string | null
  unit: string
  quantityOnHand: number
  lowStockThreshold: number
  isActive: boolean
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): InventoryItemSyncRecord {
  return {
    id: r.id,
    name_ar: r.nameAr,
    name_en: r.nameEn,
    unit: r.unit,
    quantity_on_hand: r.quantityOnHand,
    low_stock_threshold: r.lowStockThreshold,
    is_active: r.isActive,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toConsumptionSyncRecord (r: {
  id: string
  checkTypeId: string
  checkSubtypeId: string | null
  itemId: string
  quantityPerCheck: number
  onDyeOnly: boolean
  entityId: string
  version: number
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): ConsumptionSyncRecord {
  return {
    id: r.id,
    check_type_id: r.checkTypeId,
    check_subtype_id: r.checkSubtypeId,
    item_id: r.itemId,
    quantity_per_check: r.quantityPerCheck,
    on_dye_only: r.onDyeOnly,
    entity_id: r.entityId,
    version: r.version,
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toOperatorShiftSyncRecord (r: {
  id: string
  operatorId: string
  checkInAt: Date
  checkOutAt: Date | null
  checkInByUserId: string
  checkOutByUserId: string | null
  note: string | null
  entityId: string
  version: number
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): OperatorShiftSyncRecord {
  return {
    id: r.id,
    operator_id: r.operatorId,
    check_in_at: r.checkInAt.toISOString(),
    check_out_at: isoOrNull(r.checkOutAt),
    check_in_by_user_id: r.checkInByUserId,
    check_out_by_user_id: r.checkOutByUserId,
    note: r.note,
    entity_id: r.entityId,
    version: r.version,
    created_at: r.createdAt.toISOString(),
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toPatientSyncRecord (r: {
  id: string
  name: string
  entityId: string
  version: number
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): PatientSyncRecord {
  return {
    id: r.id,
    name: r.name,
    entity_id: r.entityId,
    version: r.version,
    created_at: r.createdAt.toISOString(),
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toVisitSyncRecord (r: {
  id: string
  patientId: string
  status: 'draft' | 'locked' | 'voided'
  receptionistUserId: string
  checkTypeId: string
  checkSubtypeId: string | null
  doctorId: string | null
  operatorId: string | null
  dye: boolean
  report: boolean
  lockedAt: Date | null
  voidedAt: Date | null
  voidedByUserId: string | null
  voidReason: string | null
  priceSnapshotIqd: number | null
  dyeCostSnapshotIqd: number | null
  reportCostSnapshotIqd: number | null
  doctorCutSnapshotIqd: number | null
  operatorCutSnapshotIqd: number | null
  internalPctSnapshot: number | null
  totalAmountIqdSnapshot: number | null
  patientNameSnapshot: string | null
  doctorNameSnapshot: string | null
  operatorNameSnapshot: string | null
  checkTypeNameArSnapshot: string | null
  checkTypeNameEnSnapshot: string | null
  checkSubtypeNameArSnapshot: string | null
  checkSubtypeNameEnSnapshot: string | null
  entityId: string
  version: number
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): VisitSyncRecord {
  return {
    id: r.id,
    patient_id: r.patientId,
    status: r.status,
    receptionist_user_id: r.receptionistUserId,
    check_type_id: r.checkTypeId,
    check_subtype_id: r.checkSubtypeId,
    doctor_id: r.doctorId,
    operator_id: r.operatorId,
    dye: r.dye,
    report: r.report,
    locked_at: isoOrNull(r.lockedAt),
    voided_at: isoOrNull(r.voidedAt),
    voided_by_user_id: r.voidedByUserId,
    void_reason: r.voidReason,
    price_snapshot_iqd: r.priceSnapshotIqd,
    dye_cost_snapshot_iqd: r.dyeCostSnapshotIqd,
    report_cost_snapshot_iqd: r.reportCostSnapshotIqd,
    doctor_cut_snapshot_iqd: r.doctorCutSnapshotIqd,
    operator_cut_snapshot_iqd: r.operatorCutSnapshotIqd,
    internal_pct_snapshot: r.internalPctSnapshot,
    total_amount_iqd_snapshot: r.totalAmountIqdSnapshot,
    patient_name_snapshot: r.patientNameSnapshot,
    doctor_name_snapshot: r.doctorNameSnapshot,
    operator_name_snapshot: r.operatorNameSnapshot,
    check_type_name_ar_snapshot: r.checkTypeNameArSnapshot,
    check_type_name_en_snapshot: r.checkTypeNameEnSnapshot,
    check_subtype_name_ar_snapshot: r.checkSubtypeNameArSnapshot,
    check_subtype_name_en_snapshot: r.checkSubtypeNameEnSnapshot,
    entity_id: r.entityId,
    version: r.version,
    created_at: r.createdAt.toISOString(),
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}

function toInventoryAdjustmentSyncRecord (r: {
  id: string
  itemId: string
  delta: number
  reason: 'receive' | 'writeoff' | 'count_correction' | 'consume_visit'
  visitId: string | null
  note: string | null
  byUserId: string
  entityId: string
  version: number
  createdAt: Date
  updatedAt: Date
  deletedAt: Date | null
  originDeviceId: string | null
}): InventoryAdjustmentSyncRecord {
  return {
    id: r.id,
    item_id: r.itemId,
    delta: r.delta,
    reason: r.reason,
    visit_id: r.visitId,
    note: r.note,
    by_user_id: r.byUserId,
    entity_id: r.entityId,
    version: r.version,
    created_at: r.createdAt.toISOString(),
    updated_at: r.updatedAt.toISOString(),
    deleted_at: isoOrNull(r.deletedAt),
    origin_device_id: r.originDeviceId,
  }
}
