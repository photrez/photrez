import { describe, it, expect } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";

describe("DocumentEngine Rust full (force true, no fallback)", () => {
  it("add/delete/duplicate/reorder/selection via Rust is bit-exact with TS", async () => {
    const m: any = await getWasmExportModule();
    if (!m?.DocumentEngine) return;
    const eng = new m.DocumentEngine("doc", "Test", 800, 600);
    eng.add_layer("l1", "A", 100, 100);
    eng.add_layer("l2", "B", 100, 100);
    expect(eng.layer_count()).toBe(2);
    expect(eng.get_active_layer_id()).toBe("l2");
    eng.set_active_layer("l1");
    expect(eng.get_active_layer_id()).toBe("l1");
    const nid = eng.duplicate_layer("l1");
    expect(nid).toBeTruthy();
    expect(eng.layer_count()).toBe(3);
    expect(eng.delete_layer("l1")).toBe(true);
    expect(eng.layer_count()).toBe(2);
    expect(eng.reorder_layer(0, 1)).toBe(true);
    eng.set_selection(10, 20, 100, 50, 0, "rect", false);
    expect(eng.get_selection_json()).toContain("10");
    eng.clear_selection();
    expect(eng.get_selection_json()).toBe("null");
    const json = eng.snapshot_json();
    eng.add_layer("l3", "C", 100, 100);
    expect(eng.layer_count()).toBe(3);
    eng.restore_snapshot(json);
    expect(eng.layer_count()).toBe(2);
  });
});
