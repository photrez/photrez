// ADR 0009 helper contract tests — resolveSelectionRoute (pure function).
//
// Covers the mandated cases: empty, duplicates, all-legacy, all-owned, mixed,
// and facade flag OFF/ON. Pure: no protocol traffic, no mutation, no UI.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  resolveSelectionRoute,
  MIXED_OWNERSHIP_MESSAGE,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

const owned = new Set<string>();
vi.mock("@/engine/document", () => ({
  isFacadeOwnedLayer: (id: string) => localStorage.getItem("photrez.facade") === "1" && owned.has(id),
  hasFacadeOwnedLayers: () => owned.size > 0,
}));

beforeEach(() => localStorage.setItem("photrez.facade", "1"));
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  owned.clear();
  __resetFacadeRegistryForTests();
});

describe("resolveSelectionRoute", () => {
  it("empty selection -> { mode: 'empty' } (silent no-op; never facade)", () => {
    expect(resolveSelectionRoute([])).toEqual({ mode: "empty" });
    // even with flag off
    localStorage.removeItem("photrez.facade");
    expect(resolveSelectionRoute([])).toEqual({ mode: "empty" });
  });

  it("duplicates deduplicated before classification", () => {
    owned.add("a");
    const r = resolveSelectionRoute(["a", "a", "b"]);
    // ["a"(owned), "b"] -> mixed-rejected, deterministic over unique set
    expect(r).toEqual({ mode: "mixed-rejected" });
    // pure duplicates of one owned id collapse to facade with unique ids
    const r2 = resolveSelectionRoute(["a", "a"]);
    expect(r2).toEqual({ mode: "facade", ownedIds: ["a"] });
  });

  it("all legacy -> legacy (flag ON but nothing owned)", () => {
    const r = resolveSelectionRoute(["bg", "other2"]);
    expect(r).toEqual({ mode: "legacy" });
  });

  it("all facade-owned -> facade with unique ownedIds in first-occurrence order", () => {
    owned.add("a");
    owned.add("b");
    const r = resolveSelectionRoute(["b", "a", "b"]);
    expect(r).toEqual({ mode: "facade", ownedIds: ["b", "a"] });
  });

  it("mixed -> mixed-rejected", () => {
    owned.add("a");
    const r = resolveSelectionRoute(["a", "bg"]);
    expect(r).toEqual({ mode: "mixed-rejected" });
  });

  it("facade flag OFF forces legacy even for previously-owned ids", () => {
    owned.add("a"); // stale ownership record from a prior session state
    localStorage.removeItem("photrez.facade");
    const r = resolveSelectionRoute(["a", "b"]);
    expect(r).toEqual({ mode: "legacy" });
  });

  it("exports the single shared UX message constant", () => {
    expect(MIXED_OWNERSHIP_MESSAGE).toContain("mixed selection");
    expect(MIXED_OWNERSHIP_MESSAGE).toContain("one group at a time");
  });
});
