/**
 * Derive a whole-years age from an ISO `YYYY-MM-DD` birth date. Returns null
 * when the input is missing or unparseable, or when the date is in the future.
 * Uses local time; clinic ages don't need timezone precision.
 */
export function ageFromBirthDate (birthDate: string | null | undefined): number | null {
  if (!birthDate) return null
  const born = new Date(birthDate)
  if (Number.isNaN(born.getTime())) return null
  const now = new Date()
  let age = now.getFullYear() - born.getFullYear()
  const m = now.getMonth() - born.getMonth()
  if (m < 0 || (m === 0 && now.getDate() < born.getDate())) {
    age -= 1
  }
  return age >= 0 ? age : null
}
