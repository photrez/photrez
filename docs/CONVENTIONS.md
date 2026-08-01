# CONVENTIONS.md — Photrez Code Patterns & Domain Knowledge

> Detailed code patterns, API examples, and domain-specific assumptions for working on Photrez subsystems.

---

## 1. SolidJS Patterns (NOT React)

### Signal Access — MUST Call As Function

```tsx
// ❌ WRONG — signal is not a value
const x = count;        // returns accessor function, not value
if (count) { ... }      // always truthy (function reference)

// ✅ CORRECT
const x = count();      // returns current value
if (count()) { ... }    // evaluates actual value
```

### Control Flow Components (NOT .map())

```tsx
// ❌ WRONG — React pattern
{layers.map((layer) => <LayerItem key={layer.id} />)}

// ✅ CORRECT — SolidJS <For>
<For each={layers()}>
    {(layer) => <LayerItem layer={layer} />}
</For>

// ✅ Conditional rendering
<Show when={isVisible()} fallback={<Fallback />}>
    <Content />
</Show>

// ✅ Switch/Match
<Switch>
    <Match when={activeTab() === "layers"}><LayersPanel /></Match>
    <Match when={activeTab() === "history"}><HistoryPanel /></Match>
</Switch>
```

### Store for Complex State

```tsx
import { createStore } from "solid-js/store";

const [state, setState] = createStore({
    document: { width: 800, height: 600 },
    layers: [],
    activeLayerId: null
});

// Update nested property
setState("document", "width", 1920);
setState("layers", layers => [...layers, newLayer]);

// ❌ WRONG — direct mutation won't trigger reactivity
state.document.width = 1920;
```

### Untrack and Batch

```tsx
import { untrack, batch } from "solid-js";

// Read signal without tracking (prevent re-run effect)
createEffect(() => {
    const id = props.id;  // tracked
    const label = untrack(() => props.label);  // NOT tracked
});

// Batch multiple updates (single re-render)
batch(() => {
    setCount(1);
    setName("new");
    setActive(true);
});
```

### Lifecycle

```tsx
import { onMount, onCleanup, createEffect } from "solid-js";

onMount(() => {
    // Runs once after component mount
});

onCleanup(() => {
    // Cleanup on component unmount
});

// Every createEffect with listeners MUST have onCleanup()
createEffect(() => {
    const handler = () => { ... };
    window.addEventListener("resize", handler);
    onCleanup(() => window.removeEventListener("resize", handler));
});
```

---

## 2. Tauri 2 IPC Patterns

### Command Definition (Rust)

```rust
#[tauri::command]
fn my_command(
    param: String,
    state: tauri::State<'_, EditorState>
) -> Result<Value, Value> {
    let mut doc = state.document.lock().unwrap();
    // ... logic
    ok_response(&*doc)
}

// Registration in Builder
tauri::Builder::default()
    .manage(EditorState::new())
    .invoke_handler(tauri::generate_handler![my_command])
    .run(tauri::generate_context!())
```

### Frontend Invoke (SolidJS)

```tsx
import { invoke } from "@tauri-apps/api/core";

const result = await invoke("my_command", { param: "value" });
// result = response envelope { ok, contract_version, data }

// ❌ FORBIDDEN — no Node.js APIs in Tauri
import fs from "fs";         // DOES NOT EXIST
import path from "path";     // DOES NOT EXIST
require("electron");         // DOES NOT EXIST
```

### Tauri vs Electron Key Differences

| Aspect           | Tauri 2                              | Electron (DO NOT USE)                  |
|------------------|--------------------------------------|----------------------------------------|
| Backend          | Rust native                          | Node.js                               |
| IPC              | `invoke()` → `#[tauri::command]`     | `ipcRenderer` → `ipcMain`             |
| File dialog      | `@tauri-apps/plugin-dialog`          | `dialog.showOpenDialog`               |
| State            | `tauri::State<'_, T>` (Rust managed) | Zustand / Redux                       |
| Security         | Permission-based (`capabilities/`)   | `contextIsolation` / `sandbox`        |
| Preload          | None (use `invoke` directly)         | `contextBridge.exposeInMainWorld`      |

---

## 3. History / Undo-Redo Pattern

```rust
// Pattern: snapshot-based
// 1. history.commit(current_state) BEFORE mutation
// 2. Mutate document
// 3. Return response

#[tauri::command]
fn add_layer(name: String, state: tauri::State<'_, EditorState>) -> Result<Value, Value> {
    let mut doc = state.document.lock().unwrap();
    let mut history = state.history.lock().unwrap();

    // Snapshot BEFORE mutation
    history.commit((*doc).clone());

    // Mutate
    doc.add_layer(new_layer);

    ok_response(&*doc)
}
```

- **Max depth**: 50 entries (locked in decision log).
- **Eviction**: FIFO when exceeding 50.
- **Redo branch**: Discarded on new mutation after undo.

---

## 4. GPU Resource Lifecycle

```rust
// GPU resource lifecycle (WebGL2):
// 1. Context acquired once at init; lost-context recovery handled by the renderer
// 2. Texture/Buffer allocated per resource, freed on drop / disposal
// 3. RenderPipeline state is implicit in WebGL2 draw calls

// ❌ FORBIDDEN — leaking GPU resources
let texture = device.create_texture(&desc);
// forgot to drop → GPU memory leak

// ✅ CORRECT — resource dropped when no longer needed
{
    let texture = device.create_texture(&desc);
    // ... use texture
} // automatically dropped here
```

---

## 5. Move Tool Runtime Assumptions

Rules specific to the Move Tool subsystem:

- **Two drag paths**: Canvas path (`input-handler.ts`) handles auto-select and fallback move. Overlay path (`SelectionTransformOverlay.tsx`) handles selected-layer move/resize/rotate handles. Every fix must be verified in both paths.
- **Layer stack**: `engine.getLayers()` returns top-first order (`layers[0]` = visually topmost). `hitTestLayers()` returns the first visible matching layer.
- **Transform geometry**: Move math uses visual rect top-left (`transform.x/y`), positive `scaleX/scaleY` magnitude. Orientation (`flipH/flipV`) only affects texture in shader, not geometry.
- **Rotation convention**: Positive degrees = clockwise. Must be consistent across transform geometry, renderer shader, cursor resolver, and SVG overlay.
- **Snapping**: Uses transformed AABB (axis-aligned bounding box), not true rotated-edge snapping. Canvas edges/centers have higher priority (3 and 2) than layer-to-layer (1).
- **Transient states** are separate and must be cleaned explicitly: `snapLines` signal, HUD (`hudInfo`), `dragState`, `hoverHandle`. Cleaning one does not clean the others.
- **Alt behavior** is context-dependent: canvas move path disables snapping, overlay resize scales from center, brush/eraser switches to eyedropper.
- **Overlay Alt snap**: Overlay move path is consistent with canvas path — Alt disables snapping (`!e.altKey` guard).
- **Keyboard nudge** (`Arrow/Shift+Arrow`): bypasses snapping and HUD, commits history only once per non-repeat burst.
- **Viewport rotation** (`ViewportState.rotation`) exists in type but is not yet supported by Move Tool math. If activated, screen-to-document, cursor, and transform geometry need revision.

---

## 6. Tool Creation Recipe (9-12 wiring steps)

> **Origin:** This pattern was extracted after repeated investigation of "every new tool passes test but fails in frontend". Root cause is always the same: **wiring missed 1+ steps**, usually step 3 (pointer handler) or step 5 (option bar).

### Recipe

```text
1.  Tool type union
    → components/editor/tools/toolTypes.ts

2.  Keyboard shortcut
    → useCanvasKeyboard.ts
    → test: keyboard shortcut registered, no conflict with other tools

3.  Pointer handler in dispatcher        ← MOST COMMONLY FORGOTTEN
    → useCanvasPointerTools.ts (or useCanvasPointerTools dispatcher)
    → without this: tool DOES NOT respond to clicks on canvas
    → test: click on canvas with active tool → engine state changes

4.  Toolbar button
    → AppTitleBar.tsx or components/editor/ToolRail.tsx
    → test: button visible, clickable, switches activeTool signal

5.  Option bar component                  ← OFTEN FORGOTTEN
    → components/editor/<Tool>OptionBar.tsx
    → visible when activeTool() === "tool-name"
    → test: render with activeTool(), assert DOM + signals sync

6.  Cursor behavior
    → CSS or cursor resolver (cursorResolver.ts)
    → test: cursor changes per hover state + drag state

7.  Undo/redo integration                ← OFTEN FORGOTTEN
    → history.commit(engine.snapshot()) BEFORE mutation
    → without this: undo does not revert tool action
    → test: trigger action, undo, verify state restored

8.  Status bar integration (optional)
    → StatusBar.tsx
    → test: status info updates per tool state

9.  EditorContext state (optional)
    → if tool needs additional state, register in EditorContext.tsx
    → test: state signal reactive, cleanup on tool switch

10. Tests (mandatory before merge)
    → Unit tests for logic
    → 1 contract test for state machine
    → 1 CanvasViewport integration test for real pointer chain
    → 1 tool switch round-trip test (A→B→A, no orphan state)

 11. Docs
    → FEATURES.md: status update

12. Verification
    → bun run --filter photrez-desktop test --run
    → bun run build
    → no regression
```

### Common bugs from missed steps

| Missed step | Symptom | Diagnosis |
|---|---|---|
| Step 3 (pointer handler) | "Tool does not respond to click" | `useCanvasPointerTools` dispatcher not updated with `case "newTool"` |
| Step 5 (option bar) | "Option bar does not appear when tool active" | Missing `<Show when={activeTool() === "newTool"}>` render in shell |
| Step 6 (cursor) | "Cursor does not change" | Cursor resolver not updated with case for newTool |
| Step 7 (history) | "Undo does not revert tool action" | Forgot `history.commit()` before mutating engine state |

### Tool switch cleanup (mandatory for ALL tools)

When user switches from tool A to tool B, tool A **must**:
- Clear hover state (`hoverHandle` → null)
- Clear drag state (`dragState` → null)
- Clear session-specific state (e.g., `layerTransformSession`, `modernCropFrame`)
- Restore default cursor

Test pattern: see `CanvasViewport.test.tsx` §"Phase 3 Tool Switch Contracts" for 4 round-trip test examples.

---

## 7. Untracked Paths — Files That Must Not Be Committed

These directories and file patterns are in `.gitignore` and cover internal/dev-only content:

| Category | Ignored paths |
|---|---|
| AI agent session docs | `docs/AI_*.md`, `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` |
| Plans & archive | `docs/plans/`, `docs/archive/` |
| Decision logs | `docs/decisions/` |
| Risk registers | `docs/risk-registers/` |
| Audit reports | `docs/*AUDIT_REPORT.md`, `docs/DEEP_PROJECT_REVIEW.md` |
| Analysis docs | `docs/photrez-brush-*.md` |
| Icon history | `docs/icon-history/` |
| Superpowers | `docs/superpowers/` |

If a new internal-only path is added, update `.gitignore` before committing. If accidentally committed, `git rm --cached <path>` then add to `.gitignore`.

Screenshots in `docs/screenshots/*.png` are explicitly un-ignored (`!docs/screenshots/*.png`) for documentation use; internal dev screenshots go under `docs/archive/` (gitignored).

---

## 8. Test Mock Patterns - When is a Cast Acceptable

Guiding rule: cast in ONE place (helper), keep call sites clean. s unknown as Type in a central helper beats s any at 100 call sites.

### Legit (acceptable)
- { workspace: {} as any } - partial mock; the test does not touch that field. Prefer central cast helpers when the shape repeats.
- (document as any).elementFromPoint - jsdom does not implement elementFromPoint; documented workaround (see useCanvasLayerDrag.test.tsx).
- { button: 0, clientX: 50 } as PointerEvent (partial-field pointer events) - jsdom PointerEvent constructor is limited; production only reads a few fields. Must include every field the code under test reads.
- {} as any passed as an irrelevant DocumentModel snapshot to ealHistory.commit/undo - content never inspected by the assertion.

### Code smell (must use typed helper)
- i.spyOn(EditorContextModule, "useEditor").mockReturnValue(X as any) - use mockUseEditor(X) from src/__tests__/mockUseEditor.ts (never add value-imports of EditorContext to 	est-builders.ts - crashes unit-node env tests).
- Record<string, any> for mock editor signals/values - use Record<string, unknown> for inputs, keep the dynamic signal registry Record<string, any> only where tests index arbitrary keys (marked // ponytail:).
- (c: any) => ... in mock functions - annotate with the production type (e.g. { x: number; y: number } | null for setLastPaintCoords).
- (window as any).__TAURI_INTERNALS__ - no longer needed; typed as __TAURI_INTERNALS__?: unknown via declare global in lib/desktop/tauriWindow.ts.

### Forbidden
- s any in production code (non-test) unless justified and documented. Current production count: 0.
