// DEV-only upload-recorder unit pins. The recorder mirrors ipcFreqDev:
// window-exposed, capped, DEV-gated, never throws. These tests pin the
// contract the live reupload gesture reads through window.__uploadFreq.
import { describe, it, expect, beforeEach } from "vitest";
import {
  recordUploadFreq,
  resetUploadFreq,
  type UploadFreqEvent,
} from "../uploadFreqDev";

function events(): UploadFreqEvent[] {
  const holder = window as unknown as { __uploadFreq?: UploadFreqEvent[] };
  return Array.isArray(holder.__uploadFreq) ? holder.__uploadFreq : [];
}

beforeEach(() => {
  resetUploadFreq();
});

describe("uploadFreqDev recorder", () => {
  it("records one event per call with layer, bytes, and kind", () => {
    recordUploadFreq("layer-a", 800 * 600 * 4, "full");
    const list = events();
    expect(list).toHaveLength(1);
    expect(list[0].layerId).toBe("layer-a");
    expect(list[0].bytes).toBe(800 * 600 * 4);
    expect(list[0].kind).toBe("full");
    expect(typeof list[0].t).toBe("number");
  });

  it("distinguishes patch from full uploads", () => {
    recordUploadFreq("layer-a", 30 * 40 * 4, "patch");
    expect(events()[0].kind).toBe("patch");
    expect(events()[0].bytes).toBe(30 * 40 * 4);
  });

  it("reset clears without removing the array", () => {
    recordUploadFreq("layer-a", 4, "full");
    resetUploadFreq();
    expect(events()).toHaveLength(0);
    recordUploadFreq("layer-b", 4, "full");
    expect(events()).toHaveLength(1);
  });
});
