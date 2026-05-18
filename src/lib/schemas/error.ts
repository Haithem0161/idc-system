import { z } from "zod"

// Phase-09 §3.2 FIXED final case: the canonical Tauri error envelope.
//
// Every `#[tauri::command]` in the Rust backend returns
// `Result<T, AppError>`. Rust's `impl Serialize for AppError` (in
// `src-tauri/src/error.rs`) emits `{ code, message }` where `code` is
// one of ten UPPER_SNAKE_CASE strings derived from `AppError::code()`.
// This schema MUST stay byte-compatible with that serialization --
// the §3.2 harness asserts the round-trip below.
//
// Adding a new variant on the Rust side without updating this enum
// will fail the static-source diff in `ipc-contract-phase09.test.ts`.
//
// The `code` literal union matches the 10 arms of `AppError::code()`:
//   NotAuthenticated   -> "NOT_AUTHENTICATED"
//   SessionExpired     -> "SESSION_EXPIRED"
//   Validation(_)      -> "VALIDATION_ERROR"
//   Conflict(_)        -> "CONFLICT_PARKED"
//   NotFound(_)        -> "NOT_FOUND"
//   Network(_)         -> "NETWORK_OFFLINE"
//   SyncUnavailable(_) -> "SERVER_UNAVAILABLE"
//   Database(_)        -> "DATABASE_ERROR"
//   Configuration(_)   -> "CONFIGURATION_ERROR"
//   Internal(_)        -> "INTERNAL_ERROR"
//
// Note the project's `.claude/rules/auth.md` mentions a `kind` field in
// some documentation. The Rust serializer uses `code` (see error.rs:68
// `state.serialize_field("code", self.code())?;`). The wire contract is
// `code`; documentation that says `kind` is stale and should be updated
// to match the wire shape. The FIXED final case below pins `code`.

export const APP_ERROR_CODES = [
  "NOT_AUTHENTICATED",
  "SESSION_EXPIRED",
  "VALIDATION_ERROR",
  "CONFLICT_PARKED",
  "NOT_FOUND",
  "NETWORK_OFFLINE",
  "SERVER_UNAVAILABLE",
  "DATABASE_ERROR",
  "CONFIGURATION_ERROR",
  "INTERNAL_ERROR",
] as const

export const AppErrorCodeSchema = z.enum(APP_ERROR_CODES)
export type AppErrorCode = z.infer<typeof AppErrorCodeSchema>

export const AppErrorSchema = z.object({
  code: AppErrorCodeSchema,
  message: z.string(),
})
export type AppError = z.infer<typeof AppErrorSchema>
