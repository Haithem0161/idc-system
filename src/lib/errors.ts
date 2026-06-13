import type { TFunction } from "i18next"

import { AppErrorSchema, type AppErrorCode } from "@/lib/schemas/error"

/**
 * Shared translator for Tauri IPC errors.
 *
 * Every `#[tauri::command]` rejects with a serialized `AppError`
 * (`{ code, message }` -- see `src/lib/schemas/error.ts` and
 * `src-tauri/src/error.rs`). The raw `message` is an English Rust string,
 * so showing it directly leaks untranslated internals to the user.
 *
 * `formatIpcError` parses the rejection with `AppErrorSchema`. On a match it
 * looks up the localized copy at `errors:errors.codes.<CODE>` (the structure
 * of `src/i18n/locales/{en,ar}/errors.json`) and returns that. If the lookup
 * is missing it falls back to the raw `message`; if the value does not parse
 * as an `AppError` (e.g. a plain `throw new Error(...)`), it returns
 * `String(err)`.
 *
 * Pass the page's `t` so the result honours the active locale. The function
 * itself is locale-agnostic -- it only forwards to `t`.
 */
export function formatIpcError(err: unknown, t: TFunction): string {
  const parsed = AppErrorSchema.safeParse(err)
  if (parsed.success) {
    return localizeCode(parsed.data.code, parsed.data.message, t)
  }
  // Plain Error throws still carry a usable message; surface it rather than
  // "[object Object]".
  if (err instanceof Error && err.message) return err.message
  return String(err)
}

/**
 * Localizes a known `AppErrorCode`, falling back to `rawMessage` when the
 * code has no locale entry. Exposed separately so call sites that already
 * hold a `{ code, message }` (e.g. the conflict resolver) can reuse it.
 */
export function localizeCode(
  code: AppErrorCode,
  rawMessage: string,
  t: TFunction,
): string {
  // `errors.json` nests every key under a top-level `errors` object, so the
  // resolved path inside the `errors` namespace is `errors.codes.<CODE>`.
  return t(`errors:errors.codes.${code}`, { defaultValue: rawMessage })
}
