import { describe, expect, it } from "vitest"

import { PatientCreateSchema, PatientUpdateSchema } from "./patient"

describe("PatientCreateSchema", () => {
  it("trims leading and trailing whitespace", () => {
    expect(PatientCreateSchema.parse({ name: "   Layla   " })).toEqual({
      name: "Layla",
    })
  })

  it("rejects empty name", () => {
    expect(() => PatientCreateSchema.parse({ name: "" })).toThrow(
      /name_required/
    )
  })

  it("rejects whitespace-only name", () => {
    expect(() => PatientCreateSchema.parse({ name: "    " })).toThrow(
      /name_required/
    )
  })

  it("accepts Arabic name and preserves bytes after trim", () => {
    expect(PatientCreateSchema.parse({ name: "ليلى" })).toEqual({
      name: "ليلى",
    })
  })

  it("accepts mixed-script names", () => {
    expect(PatientCreateSchema.parse({ name: "Layla هاشم" })).toEqual({
      name: "Layla هاشم",
    })
  })

  it("rejects missing name field", () => {
    expect(() => PatientCreateSchema.parse({})).toThrow()
  })
})

describe("PatientUpdateSchema", () => {
  const id = "01913d3a-7c70-7c00-a000-000000000001"

  it("requires both id and name", () => {
    expect(PatientUpdateSchema.parse({ id, name: "Layla" })).toEqual({
      id,
      name: "Layla",
    })
  })

  it("rejects missing id", () => {
    expect(() => PatientUpdateSchema.parse({ name: "Layla" })).toThrow()
  })

  it("rejects non-UUID id", () => {
    expect(() =>
      PatientUpdateSchema.parse({ id: "not-a-uuid", name: "Layla" })
    ).toThrow()
  })

  it("trims the name like the create schema does", () => {
    expect(
      PatientUpdateSchema.parse({ id, name: "  Layla H.  " })
    ).toEqual({ id, name: "Layla H." })
  })
})
