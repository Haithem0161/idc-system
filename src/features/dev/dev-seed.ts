// Dev-only catalog seeder. Drives the REAL IPC create commands (the same path
// the admin screens use), so every row passes domain validation, gets a proper
// client-generated id, and is enqueued for sync -- unlike raw-SQL seeding, which
// bypassed validation and produced rows the app rejected on read.
//
// Idempotent-ish: skips entities whose name already exists, so re-running tops
// up rather than duplicating.

import { invoke } from "@/lib/ipc"
import type {
  CheckTypeRecord,
  DoctorRecord,
  OperatorRecord,
  InventoryItemRecord,
} from "@/lib/ipc"

export interface SeedResult {
  checkTypes: number
  doctors: number
  operators: number
  inventory: number
  specialties: number
}

const CHECK_TYPES = [
  { name_ar: "أشعة سينية", name_en: "X-Ray", base_price_iqd: 15000, dye_supported: false, report_supported: true },
  { name_ar: "سونار", name_en: "Ultrasound", base_price_iqd: 25000, dye_supported: false, report_supported: true },
  { name_ar: "مفراس", name_en: "CT Scan", base_price_iqd: 75000, dye_supported: true, report_supported: true },
  { name_ar: "رنين", name_en: "MRI", base_price_iqd: 120000, dye_supported: true, report_supported: true },
  { name_ar: "تخطيط صدى القلب", name_en: "Echo", base_price_iqd: 35000, dye_supported: false, report_supported: true },
]

const DOCTORS = [
  { name: "Dr. Ahmed Hassan", specialty: "Radiology", phone: "07700000001" },
  { name: "Dr. Mariam Ali", specialty: "Cardiology", phone: "07700000002" },
  { name: "Dr. Omar Salim", specialty: "Internal Medicine", phone: "07700000003" },
  { name: "Dr. Layla Kareem", specialty: "Pediatrics", phone: "07700000004" },
  { name: "Dr. Yusuf Nabil", specialty: "Neurology", phone: "07700000005" },
]

const OPERATORS = [
  { name: "Hassan Tech", phone: "07710000001", base_cut_per_check_iqd: 2000 },
  { name: "Zainab Tech", phone: "07710000002", base_cut_per_check_iqd: 2500 },
  { name: "Karim Tech", phone: "07710000003", base_cut_per_check_iqd: 2000 },
  { name: "Noor Tech", phone: "07710000004", base_cut_per_check_iqd: 3000 },
]

const INVENTORY = [
  { name_ar: "صبغة وريدية", name_en: "IV Contrast Dye", unit: "vial", low_stock_threshold: 20 },
  { name_ar: "فيلم أشعة", name_en: "X-Ray Film", unit: "sheet", low_stock_threshold: 50 },
  { name_ar: "جل سونار", name_en: "Ultrasound Gel", unit: "bottle", low_stock_threshold: 10 },
  { name_ar: "قفازات", name_en: "Gloves", unit: "box", low_stock_threshold: 15 },
  { name_ar: "حقن", name_en: "Syringes", unit: "unit", low_stock_threshold: 40 },
  { name_ar: "مناديل كحول", name_en: "Alcohol Wipes", unit: "pack", low_stock_threshold: 10 },
  { name_ar: "ورق طباعة", name_en: "Printer Paper", unit: "ream", low_stock_threshold: 5 },
  { name_ar: "كمامات", name_en: "Face Masks", unit: "box", low_stock_threshold: 8 },
]

/**
 * Seed the full catalog through the IPC create commands. Each operator is given
 * specialties for every check type so they appear in the new-visit flow. Skips
 * anything already present (by name) so it is safe to re-run.
 */
export async function seedCatalog (): Promise<SeedResult> {
  const result: SeedResult = { checkTypes: 0, doctors: 0, operators: 0, inventory: 0, specialties: 0 }

  // --- Check types ---
  const existingCts = (await invoke("check_types_list", { args: {} })) as CheckTypeRecord[]
  const ctByName = new Map(existingCts.map((c) => [c.name_en ?? c.name_ar, c]))
  const ctIds: string[] = existingCts.map((c) => c.id)
  for (const ct of CHECK_TYPES) {
    if (ctByName.has(ct.name_en)) continue
    const created = (await invoke("check_types_create", {
      args: { ...ct, has_subtypes: false },
    })) as CheckTypeRecord
    ctIds.push(created.id)
    result.checkTypes++
  }

  // --- Doctors ---
  const existingDocs = (await invoke("doctors_list", { args: { include_inactive: false } })) as DoctorRecord[]
  const docNames = new Set(existingDocs.map((d) => d.name))
  for (const d of DOCTORS) {
    if (docNames.has(d.name)) continue
    await invoke("doctors_create", { args: d })
    result.doctors++
  }

  // --- Operators (+ specialties for every check type) ---
  const existingOps = (await invoke("operators_list", { args: { include_inactive: false } })) as OperatorRecord[]
  const opNames = new Set(existingOps.map((o) => o.name))
  for (const o of OPERATORS) {
    if (opNames.has(o.name)) continue
    const created = (await invoke("operators_create", { args: o })) as OperatorRecord
    result.operators++
    // Let this operator run every seeded check type so they're selectable.
    for (const ctId of ctIds) {
      try {
        await invoke("operator_specialties_upsert", {
          args: { operator_id: created.id, check_type_id: ctId },
        })
        result.specialties++
      } catch {
        // specialty already exists or check type invalid -- non-fatal
      }
    }
  }

  // --- Inventory items ---
  const existingInv = (await invoke("inventory_catalog_list", { args: { include_inactive: false } })) as InventoryItemRecord[]
  const invNames = new Set(existingInv.map((i) => i.name_en ?? i.name_ar))
  for (const i of INVENTORY) {
    if (invNames.has(i.name_en)) continue
    await invoke("inventory_catalog_create", { args: i })
    result.inventory++
  }

  return result
}
