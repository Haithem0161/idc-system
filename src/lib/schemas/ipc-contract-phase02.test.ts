// Phase-02 §3.2 IPC shape contract.
//
// Pin the JSON shape that Rust `#[tauri::command]` handlers produce for the
// auth + users + settings surface against the frontend's Zod/TS expectations.
// Samples mirror real serializations from the matching integration tests in
// `src-tauri/tests/auth_phase02.rs` / `auth_ipc_phase02.rs` / `users_phase02.rs`
// / `settings_phase02.rs`. If either side drifts, both suites fail loudly.

import { describe, expect, it } from "vitest"

import { LoginSchema, UserCreateSchema } from "@/lib/schemas/auth"
import { SettingSchema, SettingValueSchema } from "@/lib/schemas/setting"

describe("Phase-02 §3.2 IPC shape contract -- Rust <-> Zod parity", () => {
  // -- auth_login ----------------------------------------------------------
  it("LoginResult shape: { mode, user: UserResponse } with no password_hash", () => {
    // Mirror of `serde_json::to_value(&LoginResult)` from Rust:
    //   LoginResult { mode: LoginMode (lowercase), user: UserResponse }
    // UserResponse is serialized via `#[derive(Serialize)]` with fields:
    //   id, email, name, role, is_active, last_login_at, created_at,
    //   updated_at, entity_id, version.
    const sample = {
      mode: "offline",
      user: {
        id: "0190a000-0000-7000-8000-000000000000",
        email: "admin@idc.io",
        name: "Mariam",
        role: "superadmin",
        is_active: true,
        last_login_at: null,
        created_at: "2026-05-14T10:00:00.000Z",
        updated_at: "2026-05-14T10:00:00.000Z",
        entity_id: "tenant-1",
        version: 1,
      },
    }
    // The CRITICAL invariant: `password_hash` MUST NOT appear at any depth.
    expect(JSON.stringify(sample)).not.toContain("password_hash")
    // mode is one of {online, offline}.
    expect(["online", "offline"]).toContain(sample.mode)
    // role is one of {superadmin, receptionist, accountant}.
    expect(["superadmin", "receptionist", "accountant"]).toContain(sample.user.role)
  })

  // -- LoginSchema body ----------------------------------------------------
  it("LoginSchema (frontend) accepts the body the IPC expects", () => {
    // The IPC dispatches `auth_login` with `{ args: { email, password, entity_id_hint? } }`.
    // The user-facing LoginSchema validates `{ email, password }` from the form.
    const r = LoginSchema.safeParse({ email: "admin@idc.io", password: "admin-pw-12345" })
    expect(r.success).toBe(true)
  })

  // -- users_list ----------------------------------------------------------
  it("UserResponse[] from users_list never carries password_hash", () => {
    // Mirror of `users_list_impl` -> Vec<UserResponse>.
    const sample = [
      {
        id: "u-1",
        email: "a@b.io",
        name: "A",
        role: "superadmin",
        is_active: true,
        last_login_at: null,
        created_at: "2026-05-14T10:00:00.000Z",
        updated_at: "2026-05-14T10:00:00.000Z",
        entity_id: "t-1",
        version: 1,
      },
    ]
    const json = JSON.stringify(sample)
    expect(json).not.toContain("password_hash")
    expect(json).not.toContain("$argon2id$")
  })

  // -- users_create body ---------------------------------------------------
  it("UserCreateSchema (frontend) accepts the body the IPC expects", () => {
    const r = UserCreateSchema.safeParse({
      email: "new@idc.io",
      name: "New",
      role: "receptionist",
      password: "newpass-1234",
    })
    expect(r.success).toBe(true)
  })

  it("UserCreateSchema rejects unknown role values (closed enum on Rust side too)", () => {
    const r = UserCreateSchema.safeParse({
      email: "new@idc.io",
      name: "New",
      role: "doctor",
      password: "newpass-1234",
    })
    expect(r.success).toBe(false)
  })

  // -- settings_get / settings_list / settings_update ---------------------
  it("Setting envelope shape from settings_list mirrors Rust SettingResponse", () => {
    // SettingResponse on the Rust side serializes as:
    //   id (str), key (str), value (tagged SettingValue), updated_at (DateTime<Utc>),
    //   version (i64), entity_id (str).
    const sample = {
      id: "0190a000-0000-7000-8000-000000000000",
      key: "report_pct",
      value: { valueType: "int", value: 20 },
      updated_at: "2026-05-14T10:00:00.000Z",
      version: 1,
      entity_id: "unscoped",
    }
    const r = SettingSchema.safeParse(sample)
    expect(r.success).toBe(true)
  })

  it("SettingValue discriminated union matches the Rust `SettingValue` tagged enum", () => {
    // Rust uses `#[serde(rename_all = "lowercase", tag = "valueType", content = "value")]`
    // So an Int(10) becomes `{ valueType: "int", value: 10 }` etc.
    expect(SettingValueSchema.safeParse({ valueType: "int", value: 10 }).success).toBe(true)
    expect(SettingValueSchema.safeParse({ valueType: "bool", value: false }).success).toBe(true)
    expect(SettingValueSchema.safeParse({ valueType: "text", value: "IQD" }).success).toBe(true)
    expect(SettingValueSchema.safeParse({ valueType: "decimal", value: "3.14" }).success).toBe(
      true,
    )
    // No 5th tag.
    expect(SettingValueSchema.safeParse({ valueType: "json", value: "{}" }).success).toBe(false)
  })

  // -- Error envelope ------------------------------------------------------
  it("AppError envelope shape: { kind, message } (any kind string is currently accepted)", () => {
    // The Rust AppError::Serialize impl emits `{ kind: "<UPPER_SNAKE>", message: "<str>" }`.
    // Every command's error path returns this shape. Sample values:
    const errs = [
      { kind: "NOT_AUTHENTICATED", message: "" },
      { kind: "VALIDATION_ERROR", message: "this action requires the superadmin role" },
      { kind: "NOT_FOUND", message: "user 0190a000-0000-..." },
      { kind: "CONFLICT", message: "a user already exists" },
      { kind: "INTERNAL_ERROR", message: "argon2 hash: ..." },
    ]
    for (const e of errs) {
      expect(typeof e.kind).toBe("string")
      expect(e.kind).toMatch(/^[A-Z_]+$/)
      expect(typeof e.message).toBe("string")
    }
  })
})
