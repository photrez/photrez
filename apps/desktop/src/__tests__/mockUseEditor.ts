// Mock helper for `useEditor()` — lives in its own file (NOT test-builders.ts)
// because it value-imports EditorContext, whose chain (DialogProvider →
// DesktopDialog → solid-js/web) crashes in `unit-node` environment tests
// that import test-builders type-only. Keeping it separate means only jsdom
// component tests pay the import cost.
//
// Keep this helper separate; merge into test-builders if node-env tests ever
// stop importing test-builders.

import { vi } from "vitest";
import * as EditorContextModule from "../components/editor/shell/EditorContext";
import type { EditorContextValue } from "../components/editor/shell/EditorContext";

/**
 * Spies on `useEditor()` and returns a partial context mock. Encapsulates the
 * spy setup + cast so call sites need no `as any`.
 *
 * Usage:
 *   const spy = mockUseEditor({ activeTool: () => "brush", fgColor: () => "#fff" });
 *   // ... test body ...
 *   spy.mockRestore();  // or vi.restoreAllMocks() in afterEach
 */
export function mockUseEditor(value: Record<string, unknown>) {
  return vi
    .spyOn(EditorContextModule, "useEditor")
    .mockReturnValue(value as unknown as EditorContextValue);
}
