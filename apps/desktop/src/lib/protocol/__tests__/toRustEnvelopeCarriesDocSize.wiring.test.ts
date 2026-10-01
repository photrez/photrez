// The document-size pair must SURVIVE the wire.
//
// THE HOLE THIS FILE EXISTS FOR
//
// `toRustEnvelope` builds a loosely-typed object that `applyCommand` then
// stringifies and ships. Its exhaustiveness guard checks the variant TYPE, not
// its FIELDS, so a field the mapping forgets is dropped with:
//   - no type error (the object is `unknown`),
//   - no runtime error,
//   - no Rust error either, because `#[serde(default)]` turns the absent key
//     into `None` silently.
//
// That is exactly what happened: `docSizeBefore` / `docSizeAfter` were built
// correctly by the crop caller, remapped nowhere, and Rust recorded an External
// entry with no size - so the crop undo emitted no delta and the original bug
// stayed fully intact behind a green suite. Neither the type-check gate nor a
// test that stubs the IPC and FABRICATES the delta can catch this, because both
// bypass the serializer. Only driving the real serializer can.
//
// The Rust field names are snake_case: the command enum's VARIANT names are
// camelCase (serde rename_all) but its FIELD names are not renamed - proven by
// `affected_layer_ids` / `memory_cost_bytes` sitting beside them.

import { describe, it, expect } from "vitest";
import { toRustEnvelope } from "../bridge";
import { CONTRACT_VERSION, type CommandEnvelope } from "../types";

function envelope(command: Record<string, unknown>): CommandEnvelope {
  return {
    contractVersion: CONTRACT_VERSION,
    expectedVersion: undefined,
    docId: "doc-1",
    command,
  } as unknown as CommandEnvelope;
}

/** The serialized wire form, exactly as it reaches Rust. */
function wire(command: Record<string, unknown>): {
  command: Record<string, unknown>;
} {
  return toRustEnvelope(envelope(command)) as { command: Record<string, unknown> };
}

describe("toRustEnvelope carries the host document-size pair", () => {
  it("maps BOTH halves onto the snake_case keys the Rust command declares", () => {
    const out = wire({
      type: "recordExternalTransition",
      label: "Crop Canvas",
      affectedLayerIds: ["bg"],
      adapterId: "ts-external",
      token: "tok",
      memoryCostBytes: 8,
      docSizeBefore: [128, 128],
      docSizeAfter: [13, 13],
    });

    // The names Rust deserializes (command.rs: RecordExternalTransition).
    expect(out.command.doc_size_before).toEqual([128, 128]);
    expect(out.command.doc_size_after).toEqual([13, 13]);
    // The camelCase TS names must NOT leak onto the wire.
    expect(out.command.docSizeBefore).toBeUndefined();
    expect(out.command.docSizeAfter).toBeUndefined();
  });

  it("survives real JSON serialization unchanged (the step that actually ships)", () => {
    const json = JSON.stringify(
      toRustEnvelope(
        envelope({
          type: "recordExternalTransition",
          label: "Crop Canvas",
          affectedLayerIds: ["bg"],
          adapterId: "ts-external",
          token: "tok",
          memoryCostBytes: 8,
          docSizeBefore: [640, 480],
          docSizeAfter: [200, 200.5],
        }),
      ),
    );
    const parsed = JSON.parse(json) as { command: Record<string, unknown> };
    expect(parsed.command.doc_size_before).toEqual([640, 480]);
    // A fractional half must not be truncated or stringified on the way through.
    expect(parsed.command.doc_size_after).toEqual([200, 200.5]);
  });

  it("sends explicit nulls for a size-neutral transition (not absent keys)", () => {
    const out = wire({
      type: "recordExternalTransition",
      label: "Delete Layer",
      affectedLayerIds: ["bg"],
      adapterId: "ts-external",
      token: "tok",
      memoryCostBytes: 8,
      docSizeBefore: null,
      docSizeAfter: null,
    });
    // Absent would deserialize to None just the same, but an explicit null makes
    // the intent visible on the wire instead of relying on serde's default.
    expect(out.command.doc_size_before).toBeNull();
    expect(out.command.doc_size_after).toBeNull();
  });

  it("normalizes a MISSING pair to null rather than dropping the key", () => {
    // A caller that omits the pair entirely (every pre-existing call site) must
    // still produce a well-formed command.
    const out = wire({
      type: "recordExternalTransition",
      label: "Legacy Edit",
      affectedLayerIds: [],
      adapterId: "ts-external",
      token: "tok",
      memoryCostBytes: 0,
    });
    expect("doc_size_before" in out.command).toBe(true);
    expect(out.command.doc_size_before).toBeNull();
    expect(out.command.doc_size_after).toBeNull();
  });

  it("leaves the other arms' wire mapping untouched", () => {
    const out = wire({ type: "applyCrop", x: 1, y: 2, width: 3, height: 4, targetWidth: 5 });
    expect(out.command).toMatchObject({
      type: "applyCrop",
      x: 1,
      y: 2,
      width: 3,
      height: 4,
      target_width: 5,
    });
  });
});