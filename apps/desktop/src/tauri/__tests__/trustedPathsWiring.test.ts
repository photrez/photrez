import { describe, it, expect, vi, beforeEach } from "vitest";
import { showOpenImageDialog, showSaveDialog, showSaveDialogAllFormats, setTrustedPaths } from "@/tauri/native";

// Mock Tauri plugin-dialog
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
  open: vi.fn(),
}));

// Mock Tauri core invoke (needed by native.ts)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const OK_ENVELOPE = { ok: true, contract_version: "2.0.0", data: { trusted: 1 } };

describe("trusted-path wiring (dialog results must be approved for Rust file-IO)", () => {
  beforeEach(async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(OK_ENVELOPE);
  });

  it("showOpenImageDialog approves every selected path", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(open).mockResolvedValue(["/a.png", "/b.ptz"]);

    const result = await showOpenImageDialog();

    expect(result).toEqual(["/a.png", "/b.ptz"]);
    expect(invoke).toHaveBeenCalledWith("set_trusted_paths", {
      paths: ["/a.png", "/b.ptz"],
    });
  });

  it("showOpenImageDialog approves a single selection too", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(open).mockResolvedValue("/single.png");

    await showOpenImageDialog();

    expect(invoke).toHaveBeenCalledWith("set_trusted_paths", {
      paths: ["/single.png"],
    });
  });

  it("showOpenImageDialog does not approve anything when cancelled", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(open).mockResolvedValue(null);

    const result = await showOpenImageDialog();

    expect(result).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("set_trusted_paths", expect.anything());
  });

  it("showSaveDialogAllFormats approves the chosen path", async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(save).mockResolvedValue("/out/doc.ptz");

    const result = await showSaveDialogAllFormats("doc.ptz");

    expect(result).toBe("/out/doc.ptz");
    expect(invoke).toHaveBeenCalledWith("set_trusted_paths", {
      paths: ["/out/doc.ptz"],
    });
  });

  it("showSaveDialogAllFormats does not approve when cancelled", async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(save).mockResolvedValue(null);

    const result = await showSaveDialogAllFormats("doc.ptz");

    expect(result).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("set_trusted_paths", expect.anything());
  });

  it("showSaveDialog approves the chosen path", async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(save).mockResolvedValue("/out/img.png");

    const result = await showSaveDialog("img.png");

    expect(result).toBe("/out/img.png");
    expect(invoke).toHaveBeenCalledWith("set_trusted_paths", {
      paths: ["/out/img.png"],
    });
  });

  it("setTrustedPaths skips invoke for an empty list", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    await setTrustedPaths([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("setTrustedPaths throws on error envelope (trust rejected)", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockResolvedValue({
      ok: false,
      contract_version: "2.0.0",
      error: { code: "E_VALIDATION", message: "nope", details: null },
    });

    await expect(setTrustedPaths(["/x.png"])).rejects.toThrow("E_VALIDATION: nope");
  });
});
