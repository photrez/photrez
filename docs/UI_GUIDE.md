# UI_GUIDE.md: Photrez UI Design Reference

> Consolidated visual and interaction guidance for the native GPUI application.
> Interface values are defined in `crates/photrez/src/tokens.rs`; the web
> interface sheet that preceded them is not the native styling source and is not
> published with this repository.

---

## 1. Core Philosophy: "Soft & Snappy"

1. **Familiarity & Muscle Memory**: Standard industry layout (Toolbar top, Inspector right, Tool rail left). Users must know how to use the app instantly.
2. **Invisible UI (Zero-Tint Neutrality)**: True neutral gray surfaces (OKLCH, no blue/yellow tint). UI must not distort color perception during image editing.
3. **Soft & Snappy Aesthetic**: Avoid "Mechanical Rigidity" (2px sharp) and generic template styling (wide blurs). Modular radius base `6px` (the code's `dim::RADIUS` is 4px; the two are not reconciled). Modern and friendly, but dense for professional efficiency.
4. **Docked Precision**: Panels dock to window edges. Inner corners rounded (facing canvas), outer corners sharp (touching window). No external margins.
5. **Distinct Identity**: Single accent color **Photon Amber** (`oklch(0.74 0.15 55)`), warm, high contrast on neutral UI.

---

## 2. Layout System (Editor Shell)

### Shell Regions

| Region | Size | Notes |
|---|---|---|
| **AppTitleBar** | H: `46px` | Desktop titlebar style. Menu left, title center, window controls right. Logo "pz" 30px |
| **DocumentTabsBar** | H: `44px` | Active tab: 2px Photon Amber indicator below. Text: `12px font-medium` |
| **OptionBar** | H: `44px` | Tool-specific parameters (brush size, opacity, blend mode) |
| **LeftToolRail** | W: `52px` | 6 MVP tools: Move, Rect Select, Crop, Eyedropper, Brush, Eraser. Icon `18px`, button `36px`. Color swatches pinned to the bottom of the rail |
| **RightDock** | W: `560px` (`634px` at the wide breakpoint) | Side-by-side: Properties `300px`/`336px` + Layers `260px`/`298px` |
| **Canvas Viewport** | Flex fill | Background: `oklch(0.17 0 0)` (darkest) |
| **StatusBar** | H: `30px` | Zoom, dimensions, status hint, connection status |

### Viewport Targets

- Primary: `1366x768` and `1920x1080`
- Minimum: `960x640` (scroll overflow)

### Responsive Rules

- **≤ 1024px window**: RightDock stacks panels vertically (Inspector above Layers)
- **≥ 1024px window**: RightDock side-by-side columns (Inspector left, Layers right)
- RightDock toggle via `Ctrl+Shift+P` (overlay, max `min(92vw, 634px)`)

### Focus/Tab Order

`Titlebar → Left Tool Rail → Option Bar → Canvas → Properties → Layers → Status Bar`

---

## 3. Visual Density & Typography

### Spacing

- Base unit: `4px` (small gap `4px`, standard gap `8px`)
- Panel padding: `12-14px`, applied on both axes
- Compact control height: `26px` (inputs, selects, actions)

### Typography

- Font: `ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`
- **Tabular nums**: MANDATORY for all numeric text: fixed-width digits. Prevents shifting in transform coordinates and percentages
- Base: `13px` (native desktop standard)
- Panel headers: `14px font-semibold`
- Tabs (all): `12px font-medium`. Prevents optical illusion where interactive buttons appear larger than static text
- Sub-labels: `11-12px font-medium`
- Min size: `11px` (never smaller)

### Color & Surfaces

- Canvas: `oklch(0.17 0 0)` (darkest)
- Panels: `oklch(0.235 0 0)`
- Recessed fields: `oklch(0.265 0 0)`, border `oklch(0.34 0 0)`

---

## 4. Component Rules

### Buttons

- Height: `26px` (panel), `28px` (primary action like Export)
- Padding: `12px` standard, `8px` dense
- Radius: `dim::RADIUS` (4px). **FORBIDDEN: a fully rounded, pill-shaped corner radius.**
- Primary state: accent `color::ACCENT`

### Input / Select Fields

- Height: `26px`
- Style: recessed box (`color::FIELD` background, `1px border` using `color::FIELD_BORDER`)
- Focus: border → `color::ACCENT`
- **FORBIDDEN: a monospace face or a bold weight in numeric inputs.** Tabular nums handles alignment.

### Sliders (Biphasic & Monophasic)

- Track: `3px` height, fully rounded ends
- Handle: `10px` circle, `border border-black/40 bg-[#d4d4d4] shadow-[0_1px_2px_rgba(0,0,0,0.5)]`
- Center tick: `3px` circle for biphasic (Temp/Tint)
- Row layout: label `58px` + slider flex + value `28-44px` right-aligned

### Panels (Docked Precision)

- Panels anchor to window edges (no outer margin)
- Separation: `1px divider` using `color::DIVIDER` (not margin)
- Rounding: inner corners take the panel's small radius, outer corners → `0px`
- Header: panel bg, `46px` height, `14px font-semibold`

### LeftToolRail Color Swatches

- Container: `36px` (flush compact style)
- Two overlapping `35px` circles split by a diagonal
- Foreground: the top-left triangle, Background: the bottom-right triangle
- Gap: `1.4px` diagonal via position offset

### Layer List Items

- Height: `50px`, active: `color::ROW_ACTIVE` (`oklch(0.3 0 0)`), hover: white at 3% alpha
- Thumbnail: `34x34px`, corner radius `3px` (`R3`, defined per view module)
- Visibility: eye/sun icon `14px` inside an `18px` box

### Interaction States (ALL interactive components)

1. `default`: flat with subtle borders
2. `hover`: white at 4.5% alpha, or white at 5% alpha
3. `active`: accent background or indicator
4. `focused`: border → `color::ACCENT`
5. `disabled`: dimmed opacity, default cursor
6. `cursor`: the platform arrow. The pointing hand is reserved for hyperlinks;
   the canvas keeps its tool cursors, and panning uses the open/closed hand.

---

### Canvas Cursors

The pointer over the canvas is decided by the active tool, then by the gesture,
then by what is under the tip:

1. Resting: the Move tool keeps the standard arrow, because its cursor is
   decided by what it is about to hit. Brush and Eraser replace the pointer
   with the size ring and hide the system pointer. The Selection, Gradient and
   Shape tools show a crosshair. On Windows the Eyedropper, Paint Bucket and
   Crop tools use their own tool cursors (pipette, bucket and crop marks);
   other platforms keep the crosshair. The Text tool shows the I-beam.
2. Handles: over a crop frame's eight handles the arrow becomes the matching
   resize cursor for that direction (diagonal at the corners, vertical or
   horizontal at the edges), and over a transform frame's rotate handle it
   becomes the rotate cursor. The frame's edges and corners stay live for
   dragging.
3. Gesture: a Move drag shows the closed hand, and holding Alt through the drag
   swaps it to the clone cursor. A pan shows the open hand and closes it while
   the view moves.
4. Refusal: painting with a tool that writes pixels onto a locked or otherwise
   unpaintable layer shows the not-allowed cursor instead of silently dropping
   the stroke.

The brush ring is a separate layered window on Windows and a painted ring
elsewhere. The brush hover hides the system pointer on every platform. The
macOS, X11 and Wayland backends show the pointer again on each mouse move, so
the hover hides it again in the same turn. That call comes from a vendored copy
of the toolkit; the released version keeps the platform method private. The
rotate hover uses sixteen OS cursor directions on Windows and a painted sprite
elsewhere. The draggable interior of a crop frame and a layer move use the
standard arrow and the grab hand.

## 5. Scrollbar and Motion Guidance

### Overlay Scrollbars

- Thin overlay scrollbars (don't push layout)
- Default transparent, opaque on hover
- Keep scrollbar tracks unobtrusive; make the thumb visible on hover without changing layout.

### Motion Rules

- Micro-interactions (hover, focus): **max `100ms`**
- Panel/modal open: **max `150ms`**
- **FORBIDDEN: elastic/bounce animations.** Prefer a short ease-out transition.

---

## 6. UI Copy & Language

### Tone

Clear, direct, friendly-professional. No filler.

### Rules

- UI language: **English** (MVP)
- Action labels: clear verbs (`Open`, `Export`, `Resize`, `Undo`). Max 2 words for toolbar.
- Title case for primary buttons/menus
- Tooltips: max 1 sentence. Shortcuts: `Ctrl+K` format.

### Error Messages

Format: `<what happened>. <what user can do>. (Error: <CODE>)`

Example: `Cannot open selected file. Please choose a valid image format. (Error: E_VALIDATION)`

### Empty States

Always include: (1) what's empty, (2) what to do next.

Example: `No layers yet.` / `Open an image to start editing.`

### Prohibited

- Blaming the user
- Overly technical messages without context
- Inconsistent terms for same action (e.g., mixing `Save` and `Store`)

---

## 7. Key User Flows (MVP)

### Flow A: Open File

`Open → File dialog → Select file → Load to canvas + create layer → Status bar shows dimensions`

Error: invalid file → structured error + retry option. Load fail → document state safe.

### Flow B: Edit Session

`Select tool → Manipulate on canvas → Adjust properties in inspector → Undo/redo as needed`

Error: invalid action → warning (no crash). Command fail → structured error + consistent state.

### Flow C: Export

`Export → Choose format (JPG/PNG/WebP) + quality → Choose location → Process → Success/failure feedback`

Error: write fail → `E_IO`. Invalid settings → `E_VALIDATION`.

### UX Guardrails

- No unnecessary extra steps
- Critical actions must be visible, not hidden
- Mouse-first flow must be clear (shortcuts add speed, not replace)

---

## 8. Review Checklist (Before UI Implementation)

### Visual

- [ ] Layout follows locked shell regions
- [ ] Desktop compact density maintained
- [ ] No hardcoded colors/spacing/radius. Use tokens
- [ ] Icon size/style consistent
- [ ] Active/focus states visible

### Components

- [ ] All interactive elements have 5 states (default/hover/active/focus/disabled)
- [ ] Shared native styling conventions are used consistently
- [ ] Numeric values align without shifting as digits change
- [ ] No outer padding/margin causing window edge gaps
- [ ] Corner radius follows docking rules

### UX Flow

- [ ] Open → Edit → Export works without confusion
- [ ] Error cases have actionable feedback
- [ ] Shortcut-enhanced but mouse-first clear

### Copy

- [ ] Action labels use clear verbs
- [ ] Error messages follow format: problem + action + optional code
- [ ] Consistent terminology across panels

### Performance

- [ ] No heavy unnecessary animations
- [ ] UI transitions lightweight, don't block canvas
- [ ] Panel updates don't block canvas rendering
