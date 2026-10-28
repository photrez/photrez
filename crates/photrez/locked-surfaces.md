# Locked-Surface Census

This file is a census output, not a premise. It records, for each locked surface L1-L11, the exact command run against the live tree and the observed result. Each surface becomes a mounted regression test before the port may land. The live tree wins over the plan: drift is recorded verbatim, never silently adapted.

Date: 2026-10-17

## Census table

| # | Surface | Census command | Mounted test asserts |
|---|---|---|---|
| L1 | document tab strip | `Select-String -Path apps/desktop/src/components/editor/shell/DocumentTabsBar.tsx -Pattern 'data-document-tab'` | open app, tab X is the active document tab |
| L2 | inspector dock tabs | `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'INSPECTOR_TABS'` | dock region `properties` \| `adjustments` \| `presets` holds panel Z |
| L3 | layer dock tabs | `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'LAYER_DOCK_TABS'` | dock region `layers` \| `history` holds panel Z |
| L4 | dock column split | `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'w-\[(272\|260)px' -Context 2,2` | InspectorDock owns its column, LayerDock owns its column, the Navigator keeps its fixed bottom slot |
| L5 | tool rail tools | `Select-String -Path apps/desktop/src/components/editor/tools/toolTypes.ts -Pattern 'export type ToolId'` | 10 tools in rail order; nav to each works |
| L6 | tool variant fly-out | `Select-String -Path apps/desktop/src/components/editor/shell/LeftToolRail.tsx -Pattern 'variants'` | selection exposes rect + ellipse; the fly-out sits under the option bar |
| L7 | option bar per tool | `Select-String -Path apps/desktop/src/components/editor/shell/OptionBar.tsx -Pattern '<Show when={activeTool'` | selecting tool T mounts option bar T (and only T) |
| L8 | menu bar, 8 top-level | `Select-String -Path apps/desktop/src/components/editor/shell/AppMenuBar.tsx -Pattern 'app-menu-'` | each menu opens at 224px wide with its top edge at y=36 |
| L9 | right-dock toggle | `Select-String -Path apps/desktop/src/components/editor/shell/AppTitleBar.tsx -Pattern 'isRightDockOpen'` | the title-bar button is the only toggle |
| L10 | dialog entry points | `Select-String -Path apps/desktop/src/components/editor/dialogs/DialogProvider.tsx -Pattern 'new-document\|settings\|about'` | New Document / Settings / About reachable through the command bus |
| L11 | delete-layer confirmation | `Select-String -Path apps/desktop/src/components/editor -Pattern 'DeleteLayerConfirm' -Recurse` | none exists — layer.delete runs with no confirmation |

## Recorded output

### L1 — document tab strip

Command: `Select-String -Path apps/desktop/src/components/editor/shell/DocumentTabsBar.tsx -Pattern 'data-document-tab'`

Observed (3 hits):

```
187: const tabElements = Array.from(tabListEl.querySelectorAll<HTMLElement>("[data-document-tab]"));
420: if (target && target.closest("[data-document-tab]")) return;
516: data-document-tab={tab.id}
```

### L2 — inspector dock tabs

Command: `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'INSPECTOR_TABS'`

Observed (4 hits):

```
60: const INSPECTOR_TABS = ["properties", "adjustments", "presets"] as const;
66: const idx = INSPECTOR_TABS.indexOf(current as typeof INSPECTOR_TABS[number]);
69: ? INSPECTOR_TABS[(idx + 1) % INSPECTOR_TABS.length]
70: : INSPECTOR_TABS[(idx - 1 + INSPECTOR_TABS.length) % INSPECTOR_TABS.length];
```

### L3 — layer dock tabs

Command: `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'LAYER_DOCK_TABS'`

Observed (4 hits):

```
184: const LAYER_DOCK_TABS = ["layers", "history"] as const;
190: const idx = LAYER_DOCK_TABS.indexOf(current as typeof LAYER_DOCK_TABS[number]);
193: ? LAYER_DOCK_TABS[(idx + 1) % LAYER_DOCK_TABS.length]
194: : LAYER_DOCK_TABS[(idx - 1 + LAYER_DOCK_TABS.length) % LAYER_DOCK_TABS.length];
```

### L4 — dock column split

Command (as recorded): `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern 'w-\[(272\|260)px' -Context 2,2`

Observed: **BLOCKED — pattern not a valid regex in PowerShell**. PowerShell Select-String interprets `[` as a character-class opener; the pattern `w-\[(272\|260)px` is parsed as an unterminated `[]` set and rejected (`InvalidRegex: parsing "w-[(272|260)px" - Unterminated [] set`). The `-SimpleMatch` flag also failed for the same reason when supplied alone, because the regex engine is still engaged.

Closest honest variant that matched: `Select-String -Path apps/desktop/src/components/editor/shell/RightDock.tsx -Pattern '272px|260px|300px|298px'`

Observed (4 hits):

```
80:  : "border-b lg:border-b-0 lg:border-r lg:w-[272px] 2xl:w-[300px] lg:flex-none"
204:  : "lg:w-[260px] 2xl:w-[298px] lg:flex-none"
363:  ? "flex flex-col lg:flex-row w-[272px] lg:w-auto lg:shadow-none"
364:  : "flex flex-col w-[272px] lg:w-[272px] 2xl:w-[300px] h-full lg:shadow-none",
```

### L5 — tool rail tools

Command: `Select-String -Path apps/desktop/src/components/editor/tools/toolTypes.ts -Pattern 'export type ToolId'`

Observed (1 hit):

```
1: export type ToolId = "move" | "selection" | "crop" | "eyedropper" | "brush" | "eraser" | "paintBucket" | "gradient" | "shape" | "text";
```

Ten tools: move, selection, crop, eyedropper, brush, eraser, paintBucket, gradient, shape, text.

### L6 — tool variant fly-out

Command: `Select-String -Path apps/desktop/src/components/editor/shell/LeftToolRail.tsx -Pattern 'variants'`

Observed (7 hits):

```
165: if (!tool.variants) return null;
166: return tool.variants.find((v) => v.id === selectionShape()) ?? tool.variants[0];
178: const tooltipContent = () => currentLabel() + (tool.variants ? ` (${t("tools.rightClickOptions", "Right-click for options")})` : "");
186: if (tool.variants) {
199: data-has-variants={tool.variants ? "true" : undefined}
204: {/* Micro-indicator (bottom-right 3px triangle) for tools with sub-variants */}
205: <Show when={tool.variants}>
218: <Show when={popoverTool() === tool.id && tool.variants}>
223: <For each={tool.variants}>
```

### L7 — option bar per tool

Command: `Select-String -Path apps/desktop/src/components/editor/shell/OptionBar.tsx -Pattern '<Show when={activeTool'`

Observed (9 hits):

```
47: <Show when={activeTool() === "selection"}>
51: <Show when={activeTool() === "move"}>
55: <Show when={activeTool() === "crop"}>
59: <Show when={activeTool() === "brush" || activeTool() === "eraser"}>
63: <Show when={activeTool() === "eyedropper"}>
67: <Show when={activeTool() === "paintBucket"}>
71: <Show when={activeTool() === "gradient"}>
75: <Show when={activeTool() === "shape" || (isShapeLayerSelected() && activeTool() !== "move" && activeTool() !== "crop" && activeTool() !== "selection")}>
79: <Show when={activeTool() === "text" || (isTextLayerSelected() && activeTool() !== "move" && activeTool() !== "crop" && activeTool() !== "selection")}>
```

### L8 — menu bar, 8 top-level

Command: `Select-String -Path apps/desktop/src/components/editor/shell/AppMenuBar.tsx -Pattern 'app-menu-'`

Observed (3 hits — the pattern fires at the render sites, not the menu definition):

```
189: const popup = document.getElementById(`app-menu-${menu.toLowerCase()}`);
343: aria-controls={`app-menu-${menu.toLowerCase()}`}
362: id={`app-menu-${m().toLowerCase()}`}
```

Menu definitions on lines 15-21 (read alongside the census command):

```
15: File: "menus.file",
16: Edit: "menus.edit",
17: Image: "menus.image",
18: Layer: "menus.layer",
19: View: "menus.view",
20: Window: "menus.window",
21: Help: "menus.help",
22: Settings: "menus.settings",
```

Eight top-level menus: File, Edit, Image, Layer, View, Window, Help, Settings.

### L9 — right-dock toggle

Command: `Select-String -Path apps/desktop/src/components/editor/shell/AppTitleBar.tsx -Pattern 'isRightDockOpen'`

Observed (4 hits):

```
13: isRightDockOpen: boolean;
163: isRightDockOpen={props.isRightDockOpen}
202: aria-label={props.isRightDockOpen ? "Hide side panels" : "Show side panels"}
206: name={props.isRightDockOpen ? "panel-right-close" : "panel-right-open"}
```

**DRIFT — the recorded census claim says `setRightDockLayout` is NOT on `window.__photrezEditor`.** Live tree contradicts: `EditorContext.tsx` line 629 exposes `setRightDockLayout` (inside `dockValue`, which is spread into `window.__photrezEditor` at line 643). Verified via:

```
620: const dockValue: HistoryDockValue = {
629: setRightDockLayout,
638: (window as unknown as { __photrezEditor: EditorContextValue }).__photrezEditor = {
643: ...dockValue,
```

`setRightDockLayout` IS reachable through `window.__photrezEditor` in dev/test builds (gated by `shouldExposeEditorDebugHandle()` at line 637). The recorded negative assertion is stale.

### L10 — dialog entry points

Command: `Select-String -Path apps/desktop/src/components/editor/dialogs/DialogProvider.tsx -Pattern 'new-document|settings|about'`

Observed (22 hits). Representative:

```
18: import { AboutDialog } from "./AboutDialog";
19: import { SettingsDialog } from "./SettingsDialog";
93: about: () => Promise<void>;
94: settings: () => Promise<void>;
104: | { kind: "new-document"; options: NewDocumentDialogOptions; resolve: (result: NewDocumentResult | null) => void }
105: | { kind: "about"; resolve: () => void }
106: | { kind: "settings"; resolve: () => void };
170: queue.push({ kind: "new-document", options: options ?? {}, resolve });
174: const about = () => new Promise<void>((resolve) => {
175: queue.push({ kind: "about", resolve });
179: const settings = () => new Promise<void>((resolve) => {
180: queue.push({ kind: "settings", resolve });
```

Entry points are `newDocument`, `about`, `settings` functions queued through `DialogProvider` — command bus path.

### L11 — delete-layer confirmation

Command (as recorded): `Select-String -Path apps/desktop/src/components/editor -Pattern 'DeleteLayerConfirm' -Recurse`

Observed: **BLOCKED — Select-String does not accept `-Recurse`** (parameter binding error). Rephrased variant: `Get-ChildItem apps/desktop/src/components/editor -Recurse -File | Select-String -Pattern 'DeleteLayerConfirm'`.

Observed: zero matches for `DeleteLayerConfirm` anywhere under `apps/desktop/src/components/editor`.

Verified `layer.delete` path (no confirmation gate):

```
layer.delete  — useEditorCommands.ts:55, 93, 186, 604
              — AppMenuBar.tsx:43 ("layer.delete": "menus.items.deleteLayer")
              — AppMenuBar.tsx:95 ({ kind: "item"; label: "Delete Layer"; command: "layer.delete" })
```

No `DeleteLayerConfirm` component exists. `layer.delete` runs directly with no confirmation dialog. Matches the recorded assertion: none exists.

## 2026-10-25 G6 mounted closeout

| # | Mounted test | Production surface | RED mutation |
|---|---|---|---|
| L1 | `document_tab_shows_the_active_document` | tab id/name/`aria_selected` from `AppState.document_name`/`document_id` | remove `.test_support()` |
| L2 | `dock_tabs_click_update_selected_tab`, `dock_tab_clicks_mount_only_selected_panel` | inspector tab bar | recorded in A8.3 |
| L3 | same dock-tab tests | layer tab bar | recorded in A8.3 |
| L4 | `dock_columns_and_navigator_match_mounted_geometry` | InspectorDock/LayerDock/Navigator | rename `layer-dock` id |
| L5 | `each_tool_selects_its_option_bar`, `tool_rail_click_updates_active_tool` | ten tool buttons | recorded in A8.2 |
| L6 | `selection_variant_flyout_exposes_rect_and_ellipse` | right-click fly-out, rect/ellipse rows | variant handler never opens |
| L7 | `each_tool_selects_its_option_bar` | option bar per tool | recorded in A8.2/A8.3 |
| L8 | `menu_triggers_open_measured_popups` | `open_menu` overlay at measured origins | trigger never opens |
| L9 | `narrow_title_bar_toggle_controls_the_right_dock`, `right_dock_visibility_follows_state` | `<lg` panel toggle | toggle no-op |
| L10 | `menu_commands_open_the_shell_dialogs` | File/Help/Settings command rows -> `active_dialog` | command does not set dialog |
| L11 | none (no confirmation exists) | none | n/a |

All mutations were restored byte-for-byte before the green run. The suite is `cargo test --workspace --lib nav_regression_tests`; the recorded 13-passed run predates the workspace consolidation and used the then-current `--manifest-path crates/photrez/Cargo.toml` form.

Known limits: L6's fly-out origin is derived from the rail geometry/source classes, not a captured baseline; L8 popup origins are the manifest constants; L10 proves reachability of the framed dialog shells, not their bodies' controls; L1 proves the active tab, not multi-document switching (WF-05). The 74-shot pixel sweep is a separate G3/G5 gate and was not re-run here.

## Open questions

- **L4 regex**: The recorded census command uses regex metacharacters (`[`, `|`, `()`) without PowerShell escaping. On PowerShell + Select-String, `-Pattern 'w-\[(272\|260)px'` is rejected as an invalid regex. The recorded honest variant matches `272px`/`260px` literals directly. If the regression test must use the exact recorded command, it needs `git grep` (which handles ERE) or `-Pattern` with fully escaped `w-\[(272|260)px`.
- **L9 drift**: `setRightDockLayout` IS exposed on `window.__photrezEditor` (dev/test builds). The L9 regression test's assertion ("the title-bar button is the only toggle") still holds — the title-bar button is the only UI toggle — but the parenthetical "setRightDockLayout is not on window.__photrezEditor" is false against the live tree. The test should assert the UI toggle contract, not the window-handle absence.
