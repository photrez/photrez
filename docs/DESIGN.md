---
name: Photrez
description: A precise, lightweight Windows image editor built as a compact digital workbench.
colors:
  workbench-bg: "oklch(0.205 0 0)"
  chrome-topbar: "oklch(0.19 0 0)"
  panel-surface: "oklch(0.235 0 0)"
  canvas-well: "oklch(0.17 0 0)"
  control-surface: "oklch(0.265 0 0)"
  control-border: "oklch(0.34 0 0)"
  structural-divider: "oklch(0.3 0 0)"
  toolbar-surface: "oklch(0.22 0 0)"
  text-primary: "oklch(0.84 0 0)"
  text-secondary: "oklch(0.62 0 0)"
  text-muted: "oklch(0.58 0 0)"
  photon-amber: "oklch(0.74 0.15 55)"
  brand-orange: "oklch(0.62 0.2 36)"
  state-danger: "#EF4444"
  state-success: "#10B981"
  state-warning: "#F59E0B"
typography:
  title:
    fontFamily: "Inter, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.4
    letterSpacing: "normal"
  body:
    fontFamily: "Inter, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  label:
    fontFamily: "Inter, Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "0.01em"
  micro:
    fontFamily: "Inter, Segoe UI, system-ui, sans-serif"
    fontSize: "10px"
    fontWeight: 500
    lineHeight: 1.2
    letterSpacing: "0.04em"
  data:
    fontFamily: "ui-monospace, SFMono-Regular, Consolas, monospace"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.25
    letterSpacing: "normal"
rounded:
  structural: "0px"
  tight: "2px"
  control: "4px"
  elevated: "6px"
  circular: "9999px"
spacing:
  hairline: "1px"
  xs: "2px"
  sm: "4px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  xxl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.photon-amber}"
    textColor: "{colors.chrome-topbar}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
    height: "28px"
  button-secondary:
    backgroundColor: "{colors.control-surface}"
    textColor: "{colors.text-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
    height: "28px"
  icon-button:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.control}"
    size: "28px"
  compact-field:
    backgroundColor: "{colors.control-surface}"
    textColor: "{colors.text-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "4px 8px"
    height: "24px"
  menu-surface:
    backgroundColor: "{colors.panel-surface}"
    textColor: "{colors.text-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.elevated}"
    padding: "4px 0"
  dialog-surface:
    backgroundColor: "{colors.panel-surface}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.elevated}"
    width: "390px"
  tab-bar:
    backgroundColor: "{colors.chrome-topbar}"
    textColor: "{colors.text-primary}"
    typography: "{typography.label}"
    activeIndicator: "{colors.photon-amber}"
    height: "38px"
  accordion-header:
    backgroundColor: "{colors.chrome-topbar}"
    textColor: "{colors.text-primary}"
    typography: "{typography.title}"
    icon: "micro-chevron-svg"
    height: "38px"
---

# Design System: Photrez

## 1. Overview

**Creative North Star: "Precision Workbench"**

Photrez should feel like a finely made instrument sitting on a Windows desktop: compact, immediate, and mechanically predictable. The interface is dark because image work benefits from a quiet surround, but it is not theatrical. Neutral surfaces recede so the canvas, pixels, guides, and active controls remain visually authoritative.

Every part earns its footprint. Panels align to hard structural edges, controls use modest curvature, and elevated surfaces lift only when the interaction requires them. Familiar desktop conventions are mandatory. Menus should read like menus, dialogs like desktop dialogs, and numeric fields like tools rather than web forms.

The system explicitly rejects generic glassmorphism, overused blur, deep floating SaaS shadows, bloated all-in-one-suite chrome, and web-centric patterns that feel misplaced in a native desktop application.

**Key Characteristics:**

- Dense without becoming cramped.
- Neutral enough for color-accurate image work.
- One warm accent used for state, focus, and decisive actions.
- Structural geometry communicates hierarchy.
- Keyboard, pointer, and native menu paths feel equivalent.
- Motion is brief, functional, and nearly invisible.

**The Canvas Authority Rule.** Chrome must never compete with the image. If a control attracts attention while idle, reduce its contrast or remove decoration.

**The Desktop Contract Rule.** Standard Windows behaviors are not optional styling references. Focus order, Escape, Enter, context menus, window controls, and destructive defaults must behave as experienced desktop users expect.

## 2. Colors

The palette is a zero-tint graphite workbench with Photon Amber reserved for meaningful interaction.

### Primary

- **Photon Amber:** The operational accent. Use it for active tools, focus rings, selected tabs, progress indicators, and decisive primary actions.
- **Brand Orange:** A deeper, more chromatic identity color used primarily by the `pz` application mark. It must not replace Photon Amber in general control states.

### Tertiary

- **State Danger:** Use only for irreversible or destructive meaning, including the Windows close-button hover and explicit error states.
- **State Success:** Use only for confirmed completion or valid operational status.
- **State Warning:** Use only when an action needs attention but remains recoverable.

### Neutral

- **Workbench Background:** The application frame and deepest persistent surface.
- **Chrome Topbar:** The title bar and status chrome, slightly darker than the workspace.
- **Panel Surface:** Docked inspector panels, menus, dialogs, and other working surfaces.
- **Canvas Well:** The quietest, darkest neutral around the document.
- **Control Surface:** Inputs, segmented controls, compact buttons, and selectable fields.
- **Control Border:** Interactive boundaries that must remain legible at rest.
- **Structural Divider:** One-pixel separation between permanent regions.
- **Text Primary:** Active labels and high-priority values.
- **Text Secondary:** Icons and supporting labels.
- **Text Muted:** Metadata, shortcuts, units, and unavailable context.

**The One Warm Signal Rule.** Photon Amber is the only routine accent and should occupy less than ten percent of a screen. Its rarity carries meaning.

**The Zero-Tint Rule.** Persistent editor neutrals remain chroma-zero. Never introduce blue, purple, or green tint into chrome that surrounds color-critical artwork.

**The Semantic Color Rule.** Red, green, and warning amber never decorate. They communicate an explicit state or consequence.

## 3. Typography

**Display Font:** Not used in application UI.

**Body Font:** Inter with Segoe UI and system-ui fallbacks.

**Label/Mono Font:** The body stack for controls; ui-monospace, SFMono-Regular, and Consolas for transient dimensions and technical values.

**Character:** A single compact sans-serif family keeps the tool native, calm, and fast to scan. Weight and contrast establish hierarchy; decorative type never does.

### Hierarchy

- **Title** (600, 14px, 1.4): Dialog titles and rare high-priority surface headings.
- **Body** (400, 12px, 1.5): Dialog copy, menus, status content, and readable explanations. Prose is capped near 70 characters per line.
- **Label** (500, 11px, 1.3): Field labels, panel controls, tabs, and compact buttons.
- **Micro** (500, 10px, 1.2): Units, shortcuts, metadata, and uppercase section labels. Uppercase is limited to short grouping labels.
- **Data** (500, 11px, 1.25): Dimensions, coordinates, percentages, transform values, and transient HUD feedback. Use tabular numerals whenever values update in place.

**The Twelve-Pixel Center Rule.** Twelve pixels is the visual center of Photrez UI. Move smaller for metadata and larger only for genuine hierarchy.

**The Stable Width Rule.** Numeric readouts use tabular numerals or monospace so changing values never make nearby controls jitter.

## 4. Elevation

Photrez is flat by default. Permanent structure is expressed through tonal layers and one-pixel dividers. Shadows belong only to temporary surfaces that overlap work, such as menus, dialogs, tooltips, and the rendered document against the canvas well.

### Shadow Vocabulary

- **Mechanical Low** (`0 1px 2px rgba(0, 0, 0, 0.4)`): Small raised controls or thumbs when tonal contrast is insufficient.
- **Overlay Medium** (`0 8px 24px rgba(0, 0, 0, 0.45)`): Menus and compact popovers.
- **Dialog Lift** (`0 18px 50px rgba(0, 0, 0, 0.55)`): Modal dialogs only.
- **Canvas Depth** (`0 8px 40px rgba(0, 0, 0, 0.65)`): The document plane above the canvas well.

**The Structural First Rule.** Try a tonal step or one-pixel divider before adding a shadow.

**The Temporary Lift Rule.** If a surface does not overlap another surface, it does not receive a floating shadow.

## 5. Components

Components are compact, familiar, and complete across default, hover, focus, active, and disabled states.

### Buttons

- **Shape:** Compact rectangular controls with restrained corners (4px). Tool-rail buttons may use 2px; brand and elevated surfaces may use 6px.
- **Primary:** Photon Amber background, dark neutral text, minimum 28px height, and 12px horizontal padding. Reserve it for the single decisive action in a local workflow.
- **Secondary:** Control-surface background, control border, and primary text. This is the default dialog and toolbar action.
- **Icon:** 28 by 28px minimum in compact chrome. Window controls are 44 by 46px and remain square to the title bar.
- **Hover / Focus:** Hover changes one tonal step. Keyboard focus uses a clearly visible Photon Amber outline or ring. Active state may inset or deepen, but never bounce or scale.
- **Disabled:** Retain geometry, remove pointer response, and lower text opacity to roughly 35 percent.

### Cards / Containers

- **Corner Style:** Permanent panels use square structural edges. Menus, dialogs, and popovers use 6px corners.
- **Background:** Permanent surfaces use Workbench, Topbar, Panel, and Toolbar layers according to depth.
- **Shadow Strategy:** Flat for docked panels; overlay shadows only for temporary surfaces.
- **Border:** One-pixel structural dividers. Avoid boxed subsections when spacing and alignment already communicate grouping.
- **Internal Padding:** Dense controls use 4 to 8px; panel sections use 12 to 16px; dialog content may use 16 to 20px.

### Inputs / Fields

- **Style:** 24 to 30px high, control-surface background, one-pixel control border, 3 to 4px corners, and 11 to 12px text.
- **Focus:** Photon Amber border or focus ring without glow.
- **Units:** Place short units inside the field row in muted text. Keep editable values visually dominant.
- **Error / Disabled:** Error uses semantic danger on the boundary and adjacent message. Disabled values remain readable but clearly inactive.

### Navigation

- **Title Bar:** Fixed 46px application chrome with a 30px brand mark, compact menu triggers, centered document title, and standard Windows controls.
- **Menus:** 28px rows, 12px labels, 10 to 11px shortcut hints, 210px minimum width, 6px overlay radius, and separators only between meaningful command groups.
- **Tabs:** Active state combines stronger text with a restrained Photon Amber indicator. Inactive tabs remain neutral.
- **Tool Rail:** Dense icon controls with visible hover, active, and keyboard focus. The current tool may use Photon Amber on the icon or a minimal state marker, never a decorative card.
- **Responsive Treatment:** At narrower widths, dock structure may collapse or stack. Typography and control density stay fixed.

### Dialogs

- **Character:** A compact desktop decision surface, not a web card. Typical width is 320 to 420px.
- **Structure:** Title region, concise body, one-pixel separators, and a right-aligned action row. Avoid large empty headers, illustrations, and promotional copy.
- **Actions:** Cancel is the safe default focus for destructive confirmation. Enter activates the explicit default action; Escape cancels and restores prior focus.
- **Danger:** Destructive meaning is communicated by copy and the confirm control. Do not flood the dialog red.
- **Backdrop:** Use a plain dark scrim. Blur and glass effects are forbidden.

### Context Menus

- **Character:** Immediate and native-like, with no opening choreography.
- **Behavior:** Clamp to viewport, focus the first enabled item, support Arrow keys, Home, End, Escape, and restore focus to the invoking control.
- **Danger:** Keep destructive rows neutral at rest; reveal the warning emphasis on hover and keyboard focus.

### Status and HUD Feedback

- **Character:** Compact, technical, and transient.
- **Typography:** Use micro or data typography with tabular numerals.
- **Position:** Keep feedback near the operation without covering handles, artwork details, or the pointer target.

## 6. Do's and Don'ts

### Do:

- **Do** preserve zero-chroma graphite around the canvas so the editor does not bias color judgment.
- **Do** use Photon Amber only for active, focused, selected, progressing, or decisive states.
- **Do** keep common controls between 24 and 30px high and maintain a minimum 24 by 24px desktop pointer target.
- **Do** use one-pixel dividers and alignment before creating another container.
- **Do** make standard keyboard behavior complete: visible focus, Enter, Escape, arrows, Tab order, and focus restoration.
- **Do** keep dialogs concise and desktop-like, with safe destructive defaults.
- **Do** reuse the same icon, button, input, menu, and focus vocabulary throughout the application.
- **Do** use 75 to 150ms state transitions only when they clarify feedback.

### Don't:

- **Don't** use "AI Slop": generic glassmorphism, overused blurs, deep floating SaaS shadows, gradient text, or decorative glow.
- **Don't** imitate bloated "all-in-one" design suites with stacked toolbars, redundant controls, or excessive chrome.
- **Don't** introduce web-centric UI patterns that feel out of place in a native desktop app.
- **Don't** use colored side-stripe borders as decoration. A one-pixel active indicator is allowed only when it directly communicates selection.
- **Don't** place display fonts in UI labels, buttons, menus, or data.
- **Don't** use heavy color or full-saturation accents on inactive states.
- **Don't** animate layout, bounce controls, scale buttons on click, or choreograph application load.
- **Don't** make permanent panels float with large shadows or rounded card shells.
- **Don't** use a modal when inline editing, a menu, or an existing tool surface can complete the task more directly.
- **Don't** redesign standard Windows controls or behaviors merely for visual novelty.
