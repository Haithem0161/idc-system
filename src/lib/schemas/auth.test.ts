import { describe, it, expect } from "vitest"

import {
  FirstAdminSchema,
  LoginSchema,
  ResetPasswordSchema,
  UnlockSchema,
  UserCreateSchema,
  UserRoleSchema,
  UserUpdateSchema,
} from "./auth"

describe("UserRoleSchema", () => {
  it("accepts each of the three v1 roles", () => {
    for (const r of ["superadmin", "receptionist", "accountant"] as const) {
      expect(UserRoleSchema.parse(r)).toBe(r)
    }
  })

  it("rejects unknown role value", () => {
    expect(UserRoleSchema.safeParse("shareholder").success).toBe(false)
    expect(UserRoleSchema.safeParse("").success).toBe(false)
  })
})

describe("LoginSchema", () => {
  it("parses a valid email + 8-char password", () => {
    const out = LoginSchema.parse({ email: "asma@idc.io", password: "12345678" })
    expect(out.email).toBe("asma@idc.io")
    expect(out.password).toBe("12345678")
  })

  it("rejects an invalid email format", () => {
    const r = LoginSchema.safeParse({ email: "not-email", password: "12345678" })
    expect(r.success).toBe(false)
    if (!r.success) {
      const issues = r.error.issues
      expect(issues.some((i) => i.path.join(".") === "email")).toBe(true)
    }
  })

  it("rejects password shorter than 8 characters", () => {
    const r = LoginSchema.safeParse({ email: "a@b.io", password: "short" })
    expect(r.success).toBe(false)
    if (!r.success) {
      const issues = r.error.issues
      expect(issues.some((i) => i.path.join(".") === "password")).toBe(true)
    }
  })

  it("rejects when password field is missing", () => {
    const r = LoginSchema.safeParse({ email: "a@b.io" })
    expect(r.success).toBe(false)
  })
})

describe("UnlockSchema", () => {
  it("accepts an 8-char password", () => {
    expect(UnlockSchema.parse({ password: "12345678" }).password).toBe("12345678")
  })

  it("rejects a 7-char password", () => {
    expect(UnlockSchema.safeParse({ password: "1234567" }).success).toBe(false)
  })
})

describe("FirstAdminSchema", () => {
  it("requires email, name (min 1), and password (min 8); entity_id is optional", () => {
    const out = FirstAdminSchema.parse({
      email: "root@idc.io",
      name: "Mariam",
      password: "12345678",
    })
    expect(out.email).toBe("root@idc.io")
    expect(out.entity_id).toBeUndefined()
  })

  it("rejects an empty name", () => {
    expect(
      FirstAdminSchema.safeParse({
        email: "root@idc.io",
        name: "",
        password: "12345678",
      }).success,
    ).toBe(false)
  })

  it("accepts entity_id when provided", () => {
    const out = FirstAdminSchema.parse({
      email: "root@idc.io",
      name: "M",
      password: "12345678",
      entity_id: "tenant-1",
    })
    expect(out.entity_id).toBe("tenant-1")
  })
})

describe("UserCreateSchema", () => {
  it("requires email, name, role (enum), and password (min 8)", () => {
    const out = UserCreateSchema.parse({
      email: "u@x.io",
      name: "User",
      role: "receptionist",
      password: "12345678",
    })
    expect(out.role).toBe("receptionist")
  })

  it("rejects when role is missing", () => {
    expect(
      UserCreateSchema.safeParse({
        email: "u@x.io",
        name: "U",
        password: "12345678",
      }).success,
    ).toBe(false)
  })

  it("rejects when role is not in the closed enum", () => {
    expect(
      UserCreateSchema.safeParse({
        email: "u@x.io",
        name: "U",
        role: "doctor",
        password: "12345678",
      }).success,
    ).toBe(false)
  })
})

describe("UserUpdateSchema", () => {
  it("allows partial updates (all fields optional)", () => {
    expect(UserUpdateSchema.parse({}).email).toBeUndefined()
    expect(UserUpdateSchema.parse({ name: "New" }).name).toBe("New")
  })

  it("rejects empty-string name when provided", () => {
    expect(UserUpdateSchema.safeParse({ name: "" }).success).toBe(false)
  })

  it("rejects invalid email when provided", () => {
    expect(UserUpdateSchema.safeParse({ email: "not-email" }).success).toBe(false)
  })
})

describe("ResetPasswordSchema", () => {
  it("requires new_password (min 8)", () => {
    expect(ResetPasswordSchema.parse({ new_password: "12345678" }).new_password).toBe("12345678")
  })

  it("rejects 7-char new_password", () => {
    expect(ResetPasswordSchema.safeParse({ new_password: "1234567" }).success).toBe(false)
  })

  it("rejects missing new_password", () => {
    expect(ResetPasswordSchema.safeParse({}).success).toBe(false)
  })
})
