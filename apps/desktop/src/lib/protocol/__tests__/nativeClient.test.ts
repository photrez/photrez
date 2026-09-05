// Native-authority client tests. The client MUST drive the native protocol
// commands through the RAW `invoke()` transport (not the invokeApi `{ok,error}`
// wrapper). On a Rust `Err(String)`, Tauri v2 `invoke()` REJECTS with the bare
// `"CODE: message"` string; the client must surface that rejection unchanged
// (mock-fidelity: we do not assume a resolved `{ok:false}` envelope).

import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { nativeProtocol } from "../nativeClient";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("nativeClient raw invoke transport", () => {
  it("protocol_apply_command_native calls raw invoke with command + camelCase args", async () => {
    invokeMock.mockResolvedValue("\"ok\"");
    const out = await nativeProtocol.protocol_apply_command_native("{\"a\":1}", "doc-a");
    expect(out).toBe("\"ok\"");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("protocol_apply_command_native", {
      envelopeJson: "{\"a\":1}",
      docId: "doc-a",
    });
  });

  it("protocol_snapshot_native calls raw invoke with the command name + docId", async () => {
    invokeMock.mockResolvedValue("{\"version\":0,\"layers\":[]}");
    const out = await nativeProtocol.protocol_snapshot_native("doc-a");
    expect(out).toBe("{\"version\":0,\"layers\":[]}");
    expect(invokeMock).toHaveBeenCalledWith("protocol_snapshot_native", { docId: "doc-a" });
  });

  it("protocol_history_cursor_commit_native passes typed seq/direction args", async () => {
    invokeMock.mockResolvedValue("{}");
    await nativeProtocol.protocol_history_cursor_commit_native("doc-a", 3, "undo");
    expect(invokeMock).toHaveBeenCalledWith("protocol_history_cursor_commit_native", {
      docId: "doc-a",
      seq: 3,
      direction: "undo",
    });
  });

  it("normalizes empty docId to the reserved 'default' key", async () => {
    invokeMock.mockResolvedValue("{}");
    await nativeProtocol.protocol_seed_native("{}", "");
    expect(invokeMock).toHaveBeenCalledWith("protocol_seed_native", {
      payloadJson: "{}",
      docId: "default",
    });
  });

  // Mock-fidelity: a Rust Err(String) makes invoke() REJECT, not resolve with
  // {ok:false}. The client must propagate the bare "CODE: message" string.
  it("surfaces a rejected invoke with a bare 'CODE: message' string (error path)", async () => {
    invokeMock.mockRejectedValue("E_NOT_OPEN: document not open: default");
    await expect(nativeProtocol.protocol_snapshot_native("default")).rejects.toBe(
      "E_NOT_OPEN: document not open: default",
    );
  });
});
