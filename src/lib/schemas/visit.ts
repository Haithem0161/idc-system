import { z } from "zod"

export const VisitStatusSchema = z.enum(["draft", "locked", "voided"])
export type VisitStatusLiteral = z.infer<typeof VisitStatusSchema>

// A referring doctor and the doctor-substitute "dalal" mode are mutually
// exclusive: dalal stands in for a doctor (flat 10,000 IQD cut), so a visit can
// carry at most one of them.
const doctorAndDalalExclusive = (data: {
  doctor_id?: string | null
  dalal?: boolean
}): boolean => !(data.dalal === true && data.doctor_id != null)

// A referring representative (مندوب) only exists alongside a real referring
// doctor. It can never ride on a house (no-doctor) or dalal-substitute visit.
const mandoubRequiresDoctor = (data: {
  doctor_id?: string | null
  dalal?: boolean
  mandoub_id?: string | null
}): boolean => !(data.mandoub_id != null && (data.doctor_id == null || data.dalal === true))

// A discount zeroes the referring doctor's cut, so it is only valid with a real
// referring doctor -- never on a house (no-doctor) or dalal-substitute visit.
const discountRequiresDoctor = (data: {
  doctor_id?: string | null
  dalal?: boolean
  discount?: boolean
}): boolean => !(data.discount === true && (data.doctor_id == null || data.dalal === true))

export const VisitCreateDraftSchema = z
  .object({
    patient_id: z.string().uuid(),
    check_type_id: z.string().uuid(),
    check_subtype_id: z.string().uuid().nullable().optional(),
    doctor_id: z.string().uuid().nullable().optional(),
    // Doctor-substitute mode. Mutually exclusive with doctor_id.
    dalal: z.boolean().default(false),
    // Optional referring representative. Only valid with a real referring doctor.
    mandoub_id: z.string().uuid().nullable().optional(),
    dye: z.boolean().default(false),
    report: z.boolean().default(false),
    // Discount: zero the referring doctor's cut. Only valid with a real doctor.
    discount: z.boolean().default(false),
    // Receptionist per-visit price edit. Null/omitted = use the catalog's
    // effective price (subtype/base + doctor override). Feeds the paid-basis
    // doctor-cut math on the backend.
    price_override_iqd: z.number().int().min(0).nullable().optional(),
  })
  .refine(doctorAndDalalExclusive, {
    message: "doctor_and_dalal_exclusive",
    path: ["dalal"],
  })
  .refine(mandoubRequiresDoctor, {
    message: "mandoub_requires_doctor",
    path: ["mandoub_id"],
  })
  .refine(discountRequiresDoctor, {
    message: "discount_requires_doctor",
    path: ["discount"],
  })
export type VisitCreateDraftInput = z.infer<typeof VisitCreateDraftSchema>

export const VisitUpdateDraftSchema = z
  .object({
    visit_id: z.string().uuid(),
    // Reassign the draft to a corrected patient. Omitted = unchanged.
    patient_id: z.string().uuid().optional(),
    check_subtype_id: z.string().uuid().nullable().optional(),
    doctor_id: z.string().uuid().nullable().optional(),
    // Doctor-substitute mode. Mutually exclusive with doctor_id.
    dalal: z.boolean().optional(),
    // Optional referring representative. Only valid with a real referring doctor.
    mandoub_id: z.string().uuid().nullable().optional(),
    dye: z.boolean().optional(),
    report: z.boolean().optional(),
    // Discount: zero the referring doctor's cut. Only valid with a real doctor.
    discount: z.boolean().optional(),
    // Receptionist per-visit price edit. Null clears back to the catalog
    // price; omitted = unchanged. See create-draft note.
    price_override_iqd: z.number().int().min(0).nullable().optional(),
  })
  .refine(doctorAndDalalExclusive, {
    message: "doctor_and_dalal_exclusive",
    path: ["dalal"],
  })
  .refine(mandoubRequiresDoctor, {
    message: "mandoub_requires_doctor",
    path: ["mandoub_id"],
  })
  .refine(discountRequiresDoctor, {
    message: "discount_requires_doctor",
    path: ["discount"],
  })
export type VisitUpdateDraftInput = z.infer<typeof VisitUpdateDraftSchema>

export const VisitLockSchema = z.object({
  visit_id: z.string().uuid(),
  operator_id: z.string().uuid(),
  // Cash actually collected when the receptionist overrides the billed total
  // (patient could not pay in full). Omitted/null = paid in full. Zero is a
  // valid collected amount (waived). Must be a non-negative integer.
  amount_paid_override_iqd: z
    .number()
    .int()
    .min(0, "amount_paid_override_negative")
    .nullable()
    .optional(),
  // The chosen representative cut in IQD: 500 or 1000. Sent only when the draft
  // carries a mandoub. A net-side carve-out, never on the patient total.
  mandoub_cut: z.union([z.literal(500), z.literal(1000)]).optional(),
})
export type VisitLockInput = z.infer<typeof VisitLockSchema>

export const VisitVoidSchema = z.object({
  visit_id: z.string().uuid(),
  reason: z.string().trim().min(5, "void_reason_too_short"),
})
export type VisitVoidInput = z.infer<typeof VisitVoidSchema>
