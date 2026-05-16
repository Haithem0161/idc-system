/**
 * Formatting helpers for `<OnShiftTable>` and `<ShiftHistoryToday>`.
 *
 * `formatShiftDuration` collapses an open/closed shift window into the
 * `"Xh Ym"` label shown in the duration column. Arabic-Indic digits kick in
 * when `arabic_numerals` is enabled in settings.
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

export interface ShiftDurationInput {
  check_in_at: string
  check_out_at: string | null
}

/**
 * Returns `"Xh Ym"` for a closed shift, `"--"` while the shift is still open.
 * Throws when `check_out_at` is earlier than `check_in_at` -- callers
 * upstream (component error boundary or React Query) should surface that.
 */
export function formatShiftDuration (
  shift: ShiftDurationInput,
  opts: { arabicNumerals?: boolean } = {}
): string {
  if (shift.check_out_at == null) return "--"
  const inMs = Date.parse(shift.check_in_at)
  const outMs = Date.parse(shift.check_out_at)
  if (Number.isNaN(inMs) || Number.isNaN(outMs)) {
    throw new Error("formatShiftDuration: invalid timestamp")
  }
  if (outMs < inMs) {
    throw new Error("formatShiftDuration: check_out_at before check_in_at")
  }
  const totalMin = Math.floor((outMs - inMs) / 60_000)
  const hours = Math.floor(totalMin / 60)
  const minutes = totalMin % 60
  const out = `${hours}h ${minutes}m`
  return opts.arabicNumerals ? toArabicDigits(out) : out
}
