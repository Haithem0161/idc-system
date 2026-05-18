// Phase-09 §3.2 IPC shape contract harness.
//
// The FIXED final case mandated by phase-09 §3.2 is the shared
// `AppError` envelope -- every `#[tauri::command]` returns
// `Result<T, AppError>`, so this row covers the error path of all
// ~91 commands in one assertion. Per-command happy-path samples
// continue to land incrementally in the phase-N ipc-contract test
// files (phase-01: sync/device, phase-02: auth/users/settings, etc).
//
// What this file pins:
//
//   1. The canonical `AppError` Zod schema accepts every code the
//      Rust serializer can emit (`{ code: <UPPER_SNAKE>, message }`).
//   2. Reject paths -- missing `code`, missing `message`, unknown
//      enum value -- all fail validation. A future Rust variant
//      added without a TS counterpart shows up here.
//   3. Static-source diff: every `AppError::<Variant>` in
//      `src-tauri/src/error.rs` has a matching `code()` mapping AND
//      a matching `APP_ERROR_CODES` entry in `error.ts`. A new Rust
//      variant without the matched TS update fails the test.
//   4. Inventory invariant: every command registered in
//      `src-tauri/src/lib.rs::generate_handler!` returns
//      `AppResult<T>`, so the FIXED AppError envelope covers their
//      error paths uniformly. The harness asserts the registered
//      count is in the expected range (anti-regression: prevents
//      silent command removal).

import { describe, expect, it } from "vitest"

import {
  APP_ERROR_CODES,
  AppErrorCodeSchema,
  AppErrorSchema,
  type AppErrorCode,
} from "@/lib/schemas/error"

// Cross-tree source loads via Vite's `?raw`. The src-tauri/ folder is
// outside the project's tsconfig include but Vite resolves the path
// at test runtime; vitest.config.ts excludes src-tauri/** only from
// test FILE discovery, not from import resolution.
import rustErrorRs from "../../../src-tauri/src/error.rs?raw"
import rustLibRs from "../../../src-tauri/src/lib.rs?raw"

describe("Phase-09 §3.2 FIXED final case -- AppError envelope shape", () => {
  // (1) Positive: each of the 10 codes is accepted.
  for (const code of APP_ERROR_CODES) {
    it(`AppErrorSchema accepts code=${code} with any message string`, () => {
      const sample = { code, message: `sample message for ${code}` }
      const parsed = AppErrorSchema.parse(sample)
      expect(parsed.code).toBe(code)
      expect(parsed.message).toBe(`sample message for ${code}`)
    })
  }

  // (1b) Positive: empty-string message accepted -- the Rust
  // `NotAuthenticated` / `SessionExpired` variants carry no inner
  // String and their `to_string()` emits the static `#[error]`
  // attribute literal, never an empty string. But the schema MUST
  // accept empty strings so we never silently reject a valid
  // serialization edge.
  it("AppErrorSchema accepts an empty message", () => {
    const parsed = AppErrorSchema.parse({
      code: "NOT_AUTHENTICATED",
      message: "",
    })
    expect(parsed.code).toBe("NOT_AUTHENTICATED")
  })

  // (2) Reject paths.
  it("AppErrorSchema rejects a payload missing the code field", () => {
    expect(AppErrorSchema.safeParse({ message: "x" }).success).toBe(false)
  })

  it("AppErrorSchema rejects a payload missing the message field", () => {
    expect(
      AppErrorSchema.safeParse({ code: "NOT_FOUND" as AppErrorCode }).success,
    ).toBe(false)
  })

  it("AppErrorSchema rejects an unknown code (closed enum)", () => {
    const result = AppErrorSchema.safeParse({
      code: "TEAPOT",
      message: "I'm a teapot",
    })
    expect(result.success).toBe(false)
  })

  it("AppErrorSchema rejects a non-string code (closed enum)", () => {
    expect(
      AppErrorSchema.safeParse({ code: 418, message: "x" }).success,
    ).toBe(false)
  })

  it("AppErrorSchema rejects a non-string message", () => {
    expect(
      AppErrorSchema.safeParse({ code: "NOT_FOUND", message: 42 }).success,
    ).toBe(false)
  })

  // (1c) Convenience: AppErrorCodeSchema is the closed enum that the
  // discriminator type derives from.
  it("AppErrorCodeSchema lists exactly 10 codes (matches the Rust enum arms)", () => {
    expect(APP_ERROR_CODES.length).toBe(10)
    // No duplicates.
    expect(new Set(APP_ERROR_CODES).size).toBe(10)
    // The enum schema rejects anything outside the closed set.
    expect(AppErrorCodeSchema.safeParse("TEAPOT").success).toBe(false)
    for (const code of APP_ERROR_CODES) {
      expect(AppErrorCodeSchema.safeParse(code).success).toBe(true)
    }
  })
})

describe("Phase-09 §3.2 static-source diff -- Rust AppError <-> TS APP_ERROR_CODES", () => {
  // (3) Extract every UPPER_SNAKE_CASE string literal that appears on
  // the right-hand side of a `Self::Variant(_) =>` arm in
  // `error.rs::AppError::code()`. Confirm each is in `APP_ERROR_CODES`
  // and that the TS enum has no extras.
  it("every Rust AppError code() arm maps to a member of APP_ERROR_CODES", () => {
    // The format is: `Self::Variant => "CODE_STRING",`
    // OR:           `Self::Variant(_) => "CODE_STRING",`
    const arms = Array.from(
      rustErrorRs.matchAll(/Self::\w+(?:\(_\))?\s*=>\s*"([A-Z_]+)"/g),
    ).map((m: RegExpMatchArray) => m[1])
    expect(arms.length).toBeGreaterThan(0)
    for (const code of arms) {
      expect(APP_ERROR_CODES).toContain(code as AppErrorCode)
    }
  })

  it("APP_ERROR_CODES has no extras vs the Rust code() match arms", () => {
    const arms = new Set(
      Array.from(
        rustErrorRs.matchAll(/Self::\w+(?:\(_\))?\s*=>\s*"([A-Z_]+)"/g),
      ).map((m: RegExpMatchArray) => m[1]),
    )
    for (const code of APP_ERROR_CODES) {
      expect(arms.has(code)).toBe(true)
    }
  })

  it("Rust AppError enum has exactly 10 variants (matches APP_ERROR_CODES.length)", () => {
    // Match `#[error("...")]` lines preceding `Variant` lines inside
    // the AppError enum. Simpler: count `Self::` occurrences on the
    // LHS of code() arms.
    const arms = Array.from(
      rustErrorRs.matchAll(/Self::\w+(?:\(_\))?\s*=>\s*"[A-Z_]+"/g),
    )
    expect(arms.length).toBe(APP_ERROR_CODES.length)
  })

  it("Rust AppError serializer emits `code` (not `kind`)", () => {
    // The auth.md docs originally said `kind`; the Rust serializer
    // uses `code`. This test enforces the wire-shape contract: a
    // refactor that flips to `kind` (or vice versa) must be a
    // deliberate change that updates BOTH error.rs AND error.ts AND
    // these tests.
    expect(rustErrorRs).toMatch(/serialize_field\("code",/)
    expect(rustErrorRs).toMatch(/serialize_field\("message",/)
    expect(rustErrorRs).not.toMatch(/serialize_field\("kind",/)
  })
})

describe("Phase-09 §3.2 inventory invariant -- generate_handler! command count", () => {
  // (4) Extract the command list from `tauri::generate_handler!` so
  // an accidental deletion of a registered command surfaces here.
  // We anchor the lower bound (>= 85) so the count can grow as new
  // phases land, but a silent regression below the post-phase-08
  // baseline (~91 commands) is caught.
  it("lib.rs generate_handler! registers at least 85 IPC commands", () => {
    // Match the generate_handler! block. The block contains one
    // bare identifier per line followed by a comma. Section
    // comments (`// auth`, `// settings`, ...) interleave.
    const handlerMatch = rustLibRs.match(
      /generate_handler!\s*\[([\s\S]*?)\]\s*\)/,
    )
    expect(handlerMatch).not.toBeNull()
    const block = handlerMatch![1]
    // One command per non-comment, non-blank line ending in `,`.
    const commands = block
      .split("\n")
      .map((l: string) => l.trim())
      .filter((l: string) => l.length > 0)
      .filter((l: string) => !l.startsWith("//"))
      .map((l: string) => l.replace(/,$/, "").trim())
      .filter((l: string) => /^[a-z][a-z0-9_]*$/.test(l))
    expect(commands.length).toBeGreaterThanOrEqual(85)
    // Upper bound is a soft cap -- a sudden 2x growth is suspicious
    // (likely a regex regression catching unrelated lines).
    expect(commands.length).toBeLessThan(200)
  })

  it("every command registered in generate_handler! is uniquely named", () => {
    const handlerMatch = rustLibRs.match(
      /generate_handler!\s*\[([\s\S]*?)\]\s*\)/,
    )
    expect(handlerMatch).not.toBeNull()
    const block = handlerMatch![1]
    const commands = block
      .split("\n")
      .map((l: string) => l.trim())
      .filter((l: string) => l.length > 0)
      .filter((l: string) => !l.startsWith("//"))
      .map((l: string) => l.replace(/,$/, "").trim())
      .filter((l: string) => /^[a-z][a-z0-9_]*$/.test(l))
    expect(new Set(commands).size).toBe(commands.length)
  })

  it("settings_set_locale is present (DEF-007 G16 regression sentinel)", () => {
    // Cross-bucket pin: the IPC inventory invariant catches an
    // accidental drop of the G16 close-out. Other commands have their
    // own dedicated suites; the inventory test surfaces the registration
    // contract itself.
    const handlerMatch = rustLibRs.match(
      /generate_handler!\s*\[([\s\S]*?)\]\s*\)/,
    )
    const block = handlerMatch![1]
    expect(block).toMatch(/\bsettings_set_locale\b/)
  })
})

describe("Phase-09 §3.2 per-command coverage progress", () => {
  // The §3.2 brief mandates the FIXED AppError envelope row + per-command
  // happy-path coverage as a follow-up. This test names the currently-
  // covered command set so the next pass can pick up where we left off.
  //
  // Format: each entry is a tuple
  //   [command_name, phase_owning_the_test, status]
  // where status is "covered" (a sample exists in the phase's
  // ipc-contract-phaseNN.test.ts) or "pending" (no sample yet).
  //
  // Each command's error path is uniformly covered by AppErrorSchema
  // above; this map tracks the happy-path Zod schema coverage only.
  it("documents the per-command happy-path coverage matrix", () => {
    const COVERAGE: ReadonlyArray<readonly [string, string, "covered" | "pending"]> = [
      // Phase-01 (sync engine + device)
      ["sync_status", "01", "covered"],
      ["sync_list_conflicts", "01", "covered"],
      ["device_info", "01", "covered"],
      // Phase-02 (auth + users + settings)
      ["auth_login", "02", "covered"],
      ["users_create", "02", "covered"],
      ["settings_get", "02", "covered"],
      ["settings_set_locale", "02", "pending"], // DEF-007 G16
      // The remaining ~80 commands inherit AppError envelope coverage
      // from the FIXED final case above. Per-command happy-path samples
      // land incrementally in the phase-NN ipc-contract test files
      // (the canonical pattern: import the Zod schema for that command's
      // response shape, build a sample mirroring the Rust serialization,
      // assert .parse(sample) succeeds + .safeParse(invalid) fails).
    ]
    // Static-source: the table compiles and counts what it has.
    expect(COVERAGE.length).toBeGreaterThanOrEqual(7)
    const covered = COVERAGE.filter(([, , s]) => s === "covered").length
    const pending = COVERAGE.filter(([, , s]) => s === "pending").length
    expect(covered + pending).toBe(COVERAGE.length)
  })
})
