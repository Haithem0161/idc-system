/**
 * Resolve a bilingual entity's display name based on the active locale
 * (phase-03 §7.16).
 *
 * The fallback is always `name_ar`. When the active locale is `en` and
 * `name_en` is non-null + non-empty, prefer `name_en`.
 */
export function resolveLocaleName (
  entity: { name_ar: string; name_en: string | null },
  locale: "ar" | "en",
): string {
  if (locale === "en" && entity.name_en && entity.name_en.trim().length > 0) {
    return entity.name_en
  }
  return entity.name_ar
}
