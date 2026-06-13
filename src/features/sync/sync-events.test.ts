import { describe, expect, it } from "vitest"

import { rootsForEntities } from "./sync-events"

describe("rootsForEntities (sync:applied -> query invalidation)", () => {
  it("maps a single entity to its query root(s)", () => {
    expect(rootsForEntities(["settings"])).toEqual(["settings"])
  })

  it("maps visits to both visits and reports", () => {
    expect(rootsForEntities(["visits"]).sort()).toEqual(["reports", "visits"])
  })

  it("dedupes overlapping roots across entities", () => {
    // check_types and doctors both map to 'catalog' -> one entry.
    expect(rootsForEntities(["check_types", "doctors"])).toEqual(["catalog"])
  })

  it("unions roots across distinct entities", () => {
    const roots = rootsForEntities(["patients", "inventory_items"]).sort()
    expect(roots).toEqual(["inventory", "patients", "visits"])
  })

  it("ignores unknown entities", () => {
    expect(rootsForEntities(["some_future_entity"])).toEqual([])
  })

  it("returns empty for no entities", () => {
    expect(rootsForEntities([])).toEqual([])
  })
})
