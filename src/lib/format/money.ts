/**
 * IQD money formatter. Honors the `arabic_numerals` setting by switching to
 * Arabic-Indic digits when enabled. Always tabular-friendly: no fractional
 * digits (IQD is an integer currency in v1).
 */
export function formatIqd (
  amount: number,
  opts: { arabicNumerals?: boolean; locale?: string; withSuffix?: boolean } = {}
): string {
  const locale = opts.locale ?? "en-GB"
  const grouped = Math.trunc(amount).toLocaleString(locale, {
    maximumFractionDigits: 0,
  })
  const out = opts.arabicNumerals ? toArabicDigits(grouped) : grouped
  return opts.withSuffix ? `${out} IQD` : out
}

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

/** Format a permille (parts-per-thousand) trend delta as `+X.X%`. */
export function formatPermille (
  permille: number,
  opts: { arabicNumerals?: boolean } = {}
): string {
  if (permille === 0) return opts.arabicNumerals ? "٠.٠٪" : "0.0%"
  const sign = permille > 0 ? "+" : "-"
  const abs = Math.abs(permille)
  const whole = Math.floor(abs / 10)
  const frac = abs % 10
  const out = `${sign}${whole}.${frac}%`
  return opts.arabicNumerals ? toArabicDigits(out) : out
}

/** Hours-on-shift formatter: 3_600_000 milli => `1.0h`. */
export function formatHours (
  milli: number,
  opts: { arabicNumerals?: boolean } = {}
): string {
  if (milli <= 0) return opts.arabicNumerals ? "٠.٠h" : "0.0h"
  const hours = milli / 3_600_000
  const out = `${hours.toFixed(1)}h`
  return opts.arabicNumerals ? toArabicDigits(out) : out
}
