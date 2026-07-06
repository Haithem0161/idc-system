import { describe, it, expect } from "vitest"

import { SettingSchema, SettingValueSchema } from "./setting"

describe("SettingValueSchema (discriminated union on `valueType`)", () => {
  it("parses an `int` value", () => {
    const v = SettingValueSchema.parse({ valueType: "int", value: 10_000 })
    expect(v).toEqual({ valueType: "int", value: 10_000 })
  })

  it("rejects non-integer `int` value", () => {
    expect(SettingValueSchema.safeParse({ valueType: "int", value: 1.5 }).success).toBe(false)
    expect(SettingValueSchema.safeParse({ valueType: "int", value: "10" }).success).toBe(false)
  })

  it("parses a `decimal` value as string-form", () => {
    const v = SettingValueSchema.parse({ valueType: "decimal", value: "12500.75" })
    expect(v).toEqual({ valueType: "decimal", value: "12500.75" })
  })

  it("parses a `text` value", () => {
    const v = SettingValueSchema.parse({ valueType: "text", value: "IQD" })
    expect(v.valueType).toBe("text")
  })

  it("parses a `bool` value", () => {
    const v = SettingValueSchema.parse({ valueType: "bool", value: true })
    expect(v).toEqual({ valueType: "bool", value: true })
  })

  it("rejects an unknown valueType", () => {
    expect(
      SettingValueSchema.safeParse({ valueType: "json", value: "{}" }).success,
    ).toBe(false)
  })

  it("rejects a payload whose value type does not match its tag", () => {
    expect(
      SettingValueSchema.safeParse({ valueType: "bool", value: "true" }).success,
    ).toBe(false)
    expect(
      SettingValueSchema.safeParse({ valueType: "int", value: true }).success,
    ).toBe(false)
  })
})

describe("SettingSchema", () => {
  it("parses a full setting row with discriminated value", () => {
    const out = SettingSchema.parse({
      id: "0190a000-0000-7000-8000-000000000000",
      key: "report_pct",
      value: { valueType: "int", value: 20 },
      updated_at: "2026-05-14T10:00:00.000Z",
      version: 1,
      entity_id: "tenant-1",
    })
    expect(out.value.valueType).toBe("int")
    expect(out.key).toBe("report_pct")
  })

  it("rejects when value tag mismatches the actual data type", () => {
    expect(
      SettingSchema.safeParse({
        id: "x",
        key: "arabic_numerals",
        value: { valueType: "bool", value: "true" },
        updated_at: "x",
        version: 1,
        entity_id: "t",
      }).success,
    ).toBe(false)
  })

  it("rejects when version is not an integer", () => {
    expect(
      SettingSchema.safeParse({
        id: "x",
        key: "k",
        value: { valueType: "text", value: "v" },
        updated_at: "x",
        version: 1.5,
        entity_id: "t",
      }).success,
    ).toBe(false)
  })
})
