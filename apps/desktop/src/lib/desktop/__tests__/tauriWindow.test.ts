import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const closeMock = vi.fn().mockResolvedValue(undefined);
const minimizeMock = vi.fn().mockResolvedValue(undefined);
const toggleMaximizeMock = vi.fn().mockResolvedValue(undefined);
const startDraggingMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    close: closeMock,
    minimize: minimizeMock,
    toggleMaximize: toggleMaximizeMock,
    startDragging: startDraggingMock,
  })),
}));

import { runTauriWindowAction, isTauriRuntime } from "../tauriWindow";

describe("runTauriWindowAction", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    delete window.__TAURI_INTERNALS__;
  });

  it("calls getCurrentWindow().close() for 'close' action in Tauri runtime", async () => {
    window.__TAURI_INTERNALS__ = {};
    await runTauriWindowAction("close");
    expect(closeMock).toHaveBeenCalledOnce();
  });

  it("calls getCurrentWindow().minimize() for 'minimize' action", async () => {
    window.__TAURI_INTERNALS__ = {};
    await runTauriWindowAction("minimize");
    expect(minimizeMock).toHaveBeenCalledOnce();
  });

  it("calls getCurrentWindow().toggleMaximize() for 'toggleMaximize' action", async () => {
    window.__TAURI_INTERNALS__ = {};
    await runTauriWindowAction("toggleMaximize");
    expect(toggleMaximizeMock).toHaveBeenCalledOnce();
  });

  it("calls getCurrentWindow().startDragging() for 'startDragging' action", async () => {
    window.__TAURI_INTERNALS__ = {};
    await runTauriWindowAction("startDragging");
    expect(startDraggingMock).toHaveBeenCalledOnce();
  });

  it("does nothing when not in Tauri runtime", async () => {
    // Ensure __TAURI_INTERNALS__ is not set
    delete window.__TAURI_INTERNALS__;
    expect(isTauriRuntime()).toBe(false);
    await runTauriWindowAction("close");
    expect(closeMock).not.toHaveBeenCalled();
  });

  it("does not throw when getCurrentWindow()[action] fails", async () => {
    window.__TAURI_INTERNALS__ = {};
    closeMock.mockRejectedValueOnce(new Error("Tauri error"));
    // Should not throw — caught internally
    await expect(runTauriWindowAction("close")).resolves.toBeUndefined();
  });
});
