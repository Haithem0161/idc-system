/**
 * Formatting helpers for reception visit views.
 *
 * `formatVisitTotal` renders an IQD amount; Arabic-Indic digits kick in
 * when `arabic_numerals` is enabled in settings.
 *
 * `computeRunningTotal` is the TS port of the canonical Rust `money_math`
 * routine. It is read-only and never touches IPC; the lock path runs the
 * authoritative Rust implementation inside the lock transaction. Drift
 * between the two ports is caught by canonical-input parity tests on both
 * sides.
 */

const ARABIC_DIGITS: Record<string, string> = {
  "0": "٠",
  "1": "١",
  "2": "٢",
  "3": "٣",
  "4": "٤",
  "5": "٥",
  "6": "٦",
  "7": "٧",
  "8": "٨",
  "9": "٩",
}

function toArabicDigits (s: string): string {
  let out = ""
  for (const ch of s) {
    out += ARABIC_DIGITS[ch] ?? ch
  }
  return out
}

/**
 * Returns the integer amount as a digit-only string, optionally rendered
 * in Arabic-Indic digits. Receipts and tables tag numeric columns with
 * `font-feature-settings: 'tnum'`; this function never injects a thousands
 * separator so the tabular alignment stays exact.
 */
export function formatVisitTotal (
  amount: number,
  opts: { arabicNumerals?: boolean } = {}
): string {
  if (!Number.isFinite(amount)) {
    throw new Error("formatVisitTotal: amount must be a finite number")
  }
  if (!Number.isInteger(amount)) {
    throw new Error("formatVisitTotal: amount must be an integer IQD value")
  }
  const ascii = String(amount)
  return opts.arabicNumerals ? toArabicDigits(ascii) : ascii
}

/** Flat doctor-substitute cut applied when the "dalal" option is chosen. */
export const DALAL_DOCTOR_CUT_IQD = 10_000

export interface MoneyMathInputs {
  base_price_iqd: number
  subtype_price_iqd?: number | null
  doctor_pricing?: {
    cut_kind: "pct" | "fixed"
    cut_value: number
    price_override_iqd?: number | null
  } | null
  operator_base_cut_iqd: number
  dye: boolean
  dye_supported: boolean
  dye_cost_iqd: number
  report: boolean
  /**
   * Percentage of the post-doctor-cut price paid to the internal reporting
   * doctor. Applied only when `report` is true. NOT charged to the patient.
   */
  report_pct: number
  internal_doctor_pct: number
  /**
   * Doctor-substitute mode: a flat 10,000 IQD doctor cut, mutually exclusive
   * with a referring doctor (`doctor_pricing`). When true, the house
   * internal_pct path is skipped.
   */
  dalal: boolean
}

export interface MoneyMathSnapshot {
  price_iqd: number
  dye_cost_iqd: number
  doctor_cut_iqd: number
  operator_cut_iqd: number
  internal_pct: number | null
  /**
   * Internal reporting-doctor share, carved out of the clinic net. Zero when
   * `report` is off. NEVER part of `patient_total_iqd`.
   */
  report_amount_iqd: number
  /** What the patient actually pays: price + dye. Excludes the report. */
  patient_total_iqd: number
}

/**
 * Pure TS port of the Rust `money_math::compute`. Throws when invariants
 * the Rust side enforces are violated (e.g. dye flagged but check type
 * does not support dye). Always returns integer IQD amounts.
 *
 * Patient total = price + dye. The report is an internal figure: a percentage
 * of the post-doctor-cut price paid to the reporting doctor out of net, never
 * added to what the patient pays.
 */
export function computeRunningTotal (
  inputs: MoneyMathInputs
): MoneyMathSnapshot {
  if (inputs.dye && !inputs.dye_supported) {
    throw new Error("computeRunningTotal: check type does not support dye")
  }
  const base =
    inputs.subtype_price_iqd != null
      ? inputs.subtype_price_iqd
      : inputs.base_price_iqd
  if (!Number.isInteger(base)) {
    throw new Error("computeRunningTotal: base price must be an integer")
  }
  const price =
    inputs.doctor_pricing?.price_override_iqd != null
      ? inputs.doctor_pricing.price_override_iqd
      : base
  const dyeCost = inputs.dye ? inputs.dye_cost_iqd : 0
  let doctorCut: number
  let internalPct: number | null
  if (inputs.dalal) {
    // Doctor-substitute mode: flat cut, no house percentage.
    doctorCut = DALAL_DOCTOR_CUT_IQD
    internalPct = null
  } else if (inputs.doctor_pricing == null) {
    if (inputs.internal_doctor_pct < 0 || inputs.internal_doctor_pct > 100) {
      throw new Error(
        "computeRunningTotal: internal_doctor_pct must be in 0..=100"
      )
    }
    doctorCut = Math.floor((price * inputs.internal_doctor_pct) / 100)
    internalPct = inputs.internal_doctor_pct
  } else if (inputs.doctor_pricing.cut_kind === "pct") {
    if (
      inputs.doctor_pricing.cut_value < 0 ||
      inputs.doctor_pricing.cut_value > 100
    ) {
      throw new Error(
        "computeRunningTotal: doctor cut percentage must be in 0..=100"
      )
    }
    doctorCut = Math.floor((price * inputs.doctor_pricing.cut_value) / 100)
    internalPct = null
  } else {
    doctorCut = Math.max(0, inputs.doctor_pricing.cut_value)
    internalPct = null
  }
  if (inputs.report_pct < 0 || inputs.report_pct > 100) {
    throw new Error("computeRunningTotal: report_pct must be in 0..=100")
  }
  const reportAmount = inputs.report
    ? Math.floor(((price - doctorCut) * inputs.report_pct) / 100)
    : 0
  const patientTotal = price + dyeCost
  return {
    price_iqd: price,
    dye_cost_iqd: dyeCost,
    doctor_cut_iqd: doctorCut,
    operator_cut_iqd: inputs.operator_base_cut_iqd,
    internal_pct: internalPct,
    report_amount_iqd: reportAmount,
    patient_total_iqd: patientTotal,
  }
}
