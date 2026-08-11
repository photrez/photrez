import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  readFileBytes,
  writeFileBytes,
  saveProject,
  saveProjectBinary,
  saveProjectStreamingBegin,
  saveProjectStreamingWriteLayer,
  saveProjectStreamingEnd,
  saveProjectStreamingCancel,
  loadProject,
  ping,
  listSystemFonts,
} from "@/tauri/native";

// Mock Tauri core invoke (needed by native.ts)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const OK = { ok: true, contract_version: "2.0.0", data: {} };

function errorEnvelope(code: string, message: string) {
  return { ok: false, contract_version: "2.0.0", error: { code, message, details: null } };
}

describe("native.ts response-envelope handling", () => {
  beforeEach(async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(OK);
  });

  describe("readFileBytes (base64 decode)", () => {
    it("decodes the base64 payload back to the original bytes", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const original = new Uint8Array([0, 1, 2, 127, 128, 254, 255, 65, 66, 67]);
      const b64 = Buffer.from(original).toString("base64");
      vi.mocked(invoke).mockResolvedValue({ ...OK, data: { data: b64 } });

      const bytes = await readFileBytes("/x.bin");

      expect(bytes).toEqual(original);
    });

    it("throws with code and message on an error envelope", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue(errorEnvelope("E_PERMISSION_DENIED", "path not trusted"));

      await expect(readFileBytes("/secret.bin")).rejects.toThrow("E_PERMISSION_DENIED: path not trusted");
    });
  });

  describe("writeFileBytes (base64 encode round-trip)", () => {
    it("sends a base64 payload that decodes back to the input bytes", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const data = new Uint8Array([10, 20, 30, 200, 201, 0, 255]);

      await writeFileBytes("/out.bin", data);

      const [, args] = vi.mocked(invoke).mock.calls[0];
      const sentB64 = (args as { data: string }).data;
      expect(Buffer.from(sentB64, "base64")).toEqual(Buffer.from(data));
    });

    it("throws on an error envelope", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue(errorEnvelope("E_VALIDATION", "nope"));

      await expect(writeFileBytes("/out.bin", new Uint8Array([1]))).rejects.toThrow("E_VALIDATION: nope");
    });
  });

  describe("saveProject / saveProjectBinary / loadProject", () => {
    it("saveProject forwards path, document JSON and layers", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await saveProject("/p.ptz", "{\"doc\":1}", { a: "AAAA", b: "BBBB" });
      expect(invoke).toHaveBeenCalledWith("save_project", {
        path: "/p.ptz",
        documentJson: "{\"doc\":1}",
        layers: { a: "AAAA", b: "BBBB" },
      });
    });

    it("saveProjectBinary forwards Uint8Array layers (no string coercion)", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const layerBytes = new Uint8Array([137, 80, 78, 71]);
      await saveProjectBinary("/p.ptz", "{}", { layer1: layerBytes });
      const [, args] = vi.mocked(invoke).mock.calls[0];
      expect((args as { layers: Record<string, Uint8Array> }).layers.layer1).toBe(layerBytes);
    });

    it("loadProject returns the parsed data payload", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue({
        ...OK,
        data: { document_json: "{\"doc\":1}", layers: { a: "AAAA" } },
      });

      const result = await loadProject("/p.ptz");

      expect(result).toEqual({ document_json: "{\"doc\":1}", layers: { a: "AAAA" } });
      expect(invoke).toHaveBeenCalledWith("load_project", { path: "/p.ptz" });
    });
  });

  describe("streaming save commands", () => {
    it("saveProjectStreamingBegin returns the handle_id", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue({ ...OK, data: { handle_id: "h-1" } });

      const handle = await saveProjectStreamingBegin("/p.ptz", "{}");

      expect(handle).toBe("h-1");
    });

    it("saveProjectStreamingWriteLayer sends raw bytes body with handle/layer headers", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const png = new Uint8Array([1, 2, 3]);

      await saveProjectStreamingWriteLayer("h-1", "L1", png);

      const [cmd, body, options] = vi.mocked(invoke).mock.calls[0];
      expect(cmd).toBe("save_project_streaming_write_layer");
      expect(body).toBe(png);
      expect(options).toEqual({ headers: { "handle-id": "h-1", "layer-id": "L1" } });
    });

    it("saveProjectStreamingEnd and Cancel forward the handle", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await saveProjectStreamingEnd("h-1");
      await saveProjectStreamingCancel("h-1");
      expect(invoke).toHaveBeenNthCalledWith(1, "save_project_streaming_end", { handleId: "h-1" });
      expect(invoke).toHaveBeenNthCalledWith(2, "save_project_streaming_cancel", { handleId: "h-1" });
    });

    it("throws on an error envelope (expired session)", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue(errorEnvelope("E_VALIDATION", "session not found or expired"));

      await expect(saveProjectStreamingEnd("dead")).rejects.toThrow("E_VALIDATION: session not found or expired");
    });
  });

  describe("ping", () => {
    it("returns true on ok envelope", async () => {
      await expect(ping()).resolves.toBe(true);
    });

    it("returns false when invoke rejects (backend unavailable)", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValue(new Error("IPC down"));
      await expect(ping()).resolves.toBe(false);
    });
  });

  describe("listSystemFonts", () => {
    it("returns the font list from the native command", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValue({
        ...OK,
        data: { fonts: [{ family: "Arial", styles: ["Regular"] }] },
      });

      const fonts = await listSystemFonts();

      expect(fonts).toEqual([{ family: "Arial", styles: ["Regular"] }]);
      expect(invoke).toHaveBeenCalledWith("list_system_fonts", undefined);
    });

    it("rejects when the native command errors", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValue(errorEnvelope("E_FONT_ENUM", "No system fonts found"));

      await expect(listSystemFonts()).rejects.toThrow("E_FONT_ENUM: No system fonts found");
    });
  });
});
