import { describe, it, expect } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";

describe("DocumentEngine Rust SSOT wiring", () => {
  it("addLayer via Rust creates a layer and sets active", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return; // wasm not available in headless — skip (handled by cargo test)
    const eng = new m.DocumentEngine("doc1", "Test", 800, 600);
    eng.add_layer("l1", "Layer 1", 100, 100);
    expect(eng.layer_count()).toBe(1);
    expect(eng.get_active_layer_id()).toBe("l1");
  });

  it("setActiveLayer via Rust", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("d", "n", 100, 100);
    eng.add_layer("l1", "A", 100, 100);
    eng.add_layer("l2", "B", 100, 100);
    expect(eng.set_active_layer("l1")).toBe(true);
    expect(eng.get_active_layer_id()).toBe("l1");
  });

  it("delete via Rust", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("d", "n", 100, 100);
    eng.add_layer("l1", "A", 100, 100);
    eng.add_layer("l2", "B", 100, 100);
    expect(eng.delete_layer("l1")).toBe(true);
    expect(eng.layer_count()).toBe(1);
    expect(eng.get_active_layer_id()).toBe("l2");
  });

  it("duplicate via Rust", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("d", "n", 100, 100);
    eng.add_layer("l1", "A", 100, 100);
    const nid = eng.duplicate_layer("l1");
    expect(nid).toBeTruthy();
    expect(eng.layer_count()).toBe(2);
    expect(eng.get_active_layer_id()).toBe(nid);
  });

  it("reorder via Rust", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("d", "n", 100, 100);
    eng.add_layer("l1", "A", 100, 100);
    eng.add_layer("l2", "B", 100, 100);
    eng.add_layer("l3", "C", 100, 100);
    expect(eng.reorder_layer(0, 2)).toBe(true);
    expect(eng.layer_count()).toBe(3);
  });

  it("selection via Rust", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("d", "n", 100, 100);
    expect(eng.get_selection_json()).toBe("null");
    eng.set_selection(10, 20, 100, 50, 0, "rect", false);
    expect(eng.get_selection_json()).toContain("10");
    eng.clear_selection();
    expect(eng.get_selection_json()).toBe("null");
  });
});
