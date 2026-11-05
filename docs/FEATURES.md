# FEATURES.md: Photrez

Current feature matrix for the native Photrez desktop application. Product
architecture lives in `ARCHITECTURE.md`; install and build commands live in
`README.md`. The application this one replaced, and the dated record of the
changeover, are preserved in the separate archived repository
[photrez/photrez-legacy](https://github.com/photrez/photrez-legacy); nothing
there is a current product claim.

Status legend: **Shipped** means reachable in the current native app; **Partial**
means reachable with the named gap; **Not implemented** means absent, either by design or
because it has not been built yet.

## Canvas and document

| Status | Capability |
| --- | --- |
| Shipped | The native window renders the document composited by `photrez-core` through one display pass, with the transparent checkerboard behind the page. |
| Shipped | Open and save layered `.ptz` projects carrying raster, shape and text layers; the current format version is v3 and v1/v2 files still load. A raster layer carries no text or shape metadata in the saved project, so a project written by an earlier release that stored either on a raster layer drops that metadata at the next save. |
| Shipped | Import PNG, JPEG, WebP and BMP images, with dimensions and memory checked against the 16384 px / 1 GiB product bounds before decoding. |
| Shipped | Pan and zoom the camera: wheel zoom anchored at the pointer, Space and middle-drag pan, Fit to Screen (`Ctrl+0`), Actual Size (`Ctrl+1`), `Ctrl+=`/`Ctrl+-`. |
| Shipped | Multiple documents in a tab strip; each tab keeps its own camera; closing the last document shows the welcome card with quick presets. |
| Shipped | Moving a raster layer shows a full-resolution, region-composited live preview while dragging. |
| Not implemented | Adjustment layers and group layers: the production loader refuses them with a typed error; per-layer Basic Adjustments on raster layers are available. |
| Not implemented | PSD import/export. |

## Tools

| Status | Capability |
| --- | --- |
| Shipped | Ten-tool rail: Move (`V`), Selection (`M`), Crop (`C`), Eyedropper (`I`), Brush (`B`), Eraser (`E`), Paint Bucket (`G`), Gradient (`Shift+G`), Shape (`U`), Text (`T`), with an option bar for the active tool. |
| Shipped | Brush and eraser with separate size/hardness/strength plus flow and smoothing, six presets, a calibrated soft-edge profile, `[`/`]` size steps, Shift+`[`/`]` hardness steps, Shift-click straight lines, Shift-drag axis lock, Alt-hold colour sampling, and a right-click brush menu over the canvas. |
| Shipped | Paint Bucket with tolerance, contiguous toggle and selection-aware fill; Gradient with linear/radial types, presets, reverse, and a live distance/angle overlay during the drag. |
| Shipped | Shape tool with ten kinds (rectangle, ellipse, line, triangle, star, block arrow, heart, diamond, speech bubble, hexagon) kept parametric, with fill and stroke popovers for colour and width. |
| Shipped | Eyedropper samples the composited canvas and shows the hex value, with an optional auto-copy preference. |
| Partial | Crop: crop frame with handle resize/move, ratio menu (free, custom, presets, recents, lock shape), Size mode at a 300 PPI base, delete-cropped-pixels and fill-background options, arrow-key nudge and per-crop mini undo. Still open: the advanced group (straighten, guides, classic mode) and its overflow menu. |

## Layers and history

| Status | Capability |
| --- | --- |
| Shipped | Layers panel: add, delete, double-click rename, duplicate, merge down, merge selected, flatten, stamp visible, reorder, visibility eye, lock toggle with a badge on the locked row, opacity, blend mode and live row thumbnails. A dragged row carries its icon tile. |
| Shipped | Twelve blend modes (normal, multiply, screen, overlay, darken, lighten, colour dodge/burn, soft/hard light, difference, exclusion) composite identically on screen, save, print and export. |
| Shipped | Multi-layer selection by Ctrl-toggle and Shift-range in the panel or by marquee drag on empty canvas; batch delete/duplicate/merge and simultaneous canvas drag. |
| Shipped | Background-layer rules: pinned to the bottom, position/rotation locked, delete refused, renaming converts it to a normal layer. |
| Shipped | Undo/redo (`Ctrl+Z` / `Ctrl+Y` / `Ctrl+Shift+Z`) executed by the core history, one entry per stroke or drag gesture; the History panel lists labelled rows and jumps to any point. |
| Shipped | Per-layer Basic Adjustments (brightness, contrast, saturation) whose sliders commit one history entry per drag through the core command. |
| Shipped | The five undeveloped adjustment groups (Tone Curve, HSL, Color Grading, Detail, Lens Corrections) open and close on their header, each independently, and the opened group shows the dashed In Development card with its description. |

## Selection and transform

| Status | Capability |
| --- | --- |
| Shipped | Rectangular and elliptical marquee with Shift (constrain) and Alt (from center), Select All (`Ctrl+A`), Deselect (`Ctrl+D`), Invert (`Ctrl+Shift+I`), painted as marching ants. |
| Shipped | Selection option bar reads the committed selection and edits X/Y/W/H/angle through core history; a rotate pin turns the selection with Shift snapping to 15 degrees; `Ctrl+T` toggles the transform chrome. |
| Shipped | Cut/copy/paste/delete pixels of a selection (`Ctrl+X`/`Ctrl+C`/`Ctrl+V`/`Delete`); paste creates a new layer, and a copied selection can be pasted into another document. |
| Shipped | Move-tool layer drag with snapping to layer and canvas edges/centres plus guide lines, Alt to disable, arrow-key nudge (1 px, 10 px with Shift). |
| Shipped | Layer transform frame with eight resize handles, rotation, flip, reset and a distance/size HUD that follows the pointer at fixed screen scale. |
| Shipped | Opacity-aware hit testing: clicking a transparent corner falls through to the layer beneath; text and shape layers select by their box. |

## Text

| Status | Capability |
| --- | --- |
| Shipped | Text tool (`T`): click starts an empty point-text session; typing is WYSIWYG through the document's own rasterizer; `Ctrl+Enter` commits one core history entry and names the layer from its content; Escape cancels; click-away commits; an empty buffer is discarded. |
| Shipped | Text option bar and Properties typography branch carry font family, size, alignment, colour, and a stroke popover (width, colour, outside/center/inside); line height and tracking appear in edit mode. |
| Partial | Text rendering covers regular point text fill and stroke; italic, underline, weights and area-text boxes are outside the current rasterizer subset. |
| Shipped | Text layers persist as re-editable metadata in `.ptz` and stay parametric across document boundaries. |
| Shipped | Rasterize a text layer from its option bar: the rendered bitmap becomes the layer's own pixel content in one history entry, the layer then composites from those pixels, and it is no longer re-editable as text. Text the rasterizer cannot render, such as underline, strikethrough, uppercase or a bold weight, is refused with a typed error and stays a text layer. |

## Colour

| Status | Capability |
| --- | --- |
| Shipped | Foreground/background swatches on the tool rail with swap. |
| Shipped | Colour picker: saturation/value box and hue strip with pointer capture (a drag released outside keeps tracking), six-digit hex entry, Cancel/Escape/outside-click revert and Enter/OK commit. |
| Shipped | The picker targets the right thing by context: the paint foreground/background, the selected text layer's colour in edit mode, or a shape's fill/stroke colour. |

## Dialogs and export

| Status | Capability |
| --- | --- |
| Shipped | One shared dialog shell: titlebar with a real close button, body, actions footer; Escape and the close button run each dialog's own dismissal. |
| Shipped | New Document dialog with category tabs (Paper, Social Media, Web & Video, Photography), a preset grid, and a Name/Width/Height/Unit/Background form; defaults to 1080x1080 white. |
| Shipped | Unit-aware dimension fields (px/in/cm/mm/pt at 96 DPI) shared by New Document and Resize Canvas, with the aspect lock deriving the linked dimension. |
| Shipped | Resize Canvas dialog with unit selects, Keep-proportions lock and maximum-dimension warning. |
| Shipped | Settings dialog chooses the UI language and persists it; About dialog shows the native stack, version and locale line. |
| Shipped | Save (`Ctrl+S`) and Save As (`Ctrl+Shift+S`) write `.ptz` projects; an imported flat image routes Save to Save As. |
| Shipped | Export dialog (`Ctrl+Alt+E`) writes the composite as PNG, JPEG or WebP with a quality slider for lossy formats; WebP export is currently the lossless path (quality is ignored), a documented deviation. |
| Shipped | Autosave writes dirty documents to the app cache on a 60-second timer, and a startup recovery pass offers them again after an abnormal exit. An archive is skipped when it belongs to the document already open and is no newer than the file on disk; anything uncertain still offers. Discard removes only the archives the prompt listed, so a skipped one survives a Discard aimed at another document, and a row whose document is already open says so instead of opening a second copy. |

## Print

| Status | Capability |
| --- | --- |
| Shipped | Print dialog and inspector (`Ctrl+P`): paper preview with dashed page, Printer Setup / Color Management / Position and Size sections, 10-400% scale track and Show Paper White option. |
| Shipped | On Windows the composite is sent through the platform print service to the printer chosen in the dialog, with the copies, orientation and paper size selected there. |
| Partial | The printer list comes from the system spooler, and a failed enumeration is reported with its reason instead of an empty list. Paper sizes come from a built-in table, and margins are typed rather than read from the driver. |
| Not implemented | Printing on macOS and Linux returns a typed unsupported-platform error instead of spooling. |
| Not implemented | Print-properties (device-mode) UI, the selected printer's own paper list, and its hardware minimum margins. |

## Shell and input

| Status | Capability |
| --- | --- |
| Shipped | Title bar and eight top-level menus that open at fixed origins, dismiss on outside click, and enable/disable rows from live document state; the canvas and layer context menus follow the same gates. |
| Shipped | Right dock with Properties, Adjustments, Layers and History panels plus the Navigator, filling the window at any size. |
| Partial | The Navigator shows the page thumbnail and the viewport frame inside it. The reference also carries a zoom row under the thumbnail: minus and plus buttons, a track from 5 % to 400 %, and the current percent. This app reaches the same zoom from the `View` menu and `Ctrl+=` / `Ctrl+-`, and the status bar already reads the percent, so the capability is present and only the control is missing. Adding the row changes the dock layout, so it waits for approval. |
| Shipped | Status bar reads canvas size, zoom percent, active tool, hint, selected layer and frame timing, marks the autosave state with its colour dot beside the words, and opens the dock's History panel from its History trigger. |
| Shipped | Tool option bars reflow against their real width, and the Move, Selection and Brush bars open an overflow panel holding the controls they fold away. The Move bar's transform cells read the selected layer: X, Y and rotation are editable and commit through core history, W and H show its scaled frame, and a locked layer answers no edit. |
| Not implemented | Crop advanced group: straighten, composition guides and the classic interaction mode. The reference reaches these from a crop overflow menu; neither the modes nor the panel exists here, so the crop bar shows no overflow trigger at all rather than one that opens nothing. |
| Shipped | The right dock's Properties transform rows read the selected layer: X, Y, W, H and R show its frame and commit through core history, Scale shows the percentage that produced that frame, and the link decides whether a size edit moves one axis or both. |
| Shipped | Every track takes the keyboard: the Paint Bucket tolerance, the text stroke, the brush menu's size, hardness and strength, export quality, the three adjustment channels, the print scale and the Properties opacity track answer arrows, Home and End within their own range, each is a tab stop, and any of them focused from the keyboard carries the same inset accent band the option-bar toggles use. |
| Shipped | Every field the app edits is in the tab order and shows the shared inset accent band when the keyboard reaches it: the New Document name and size boxes, the Resize Canvas size fields, the Rename prompt answer, the option bar's brush, text, shape and tolerance rows, the Move bar and selection geometry rows, the Properties transform rows, and the Print dialog's size and copies rows. Keystrokes follow real focus, so a digit typed after Tab lands in the focused row and replaces its seed, and the row the user left keeps the value it typed. |
| Shipped | An open dialog holds the keyboard: Tab cannot walk out to the canvas or the bars behind it, and when the dialog closes focus returns to the element that held it beforehand. Escape closing the dropdown before the dialog was already tested. |
| Partial | Nothing is focused when a dialog opens, so no control shows the focus band until the first Tab. The limit is cosmetic, not blocked: one Space press confirms an open New Document dialog with no prior focus step, and Escape dismisses it, because the dialog's own key context answers both. The reference focuses its primary action on open. Matching that visibly needs a focus handle for a button, which the toolkit does not expose and which the test harness cannot observe, so the change would ship unverifiable. |
| Partial | The Move bar takes no typed size where the reference types one during an active transform session: this app has no transform session to gate the edit on, and the same value is editable in the Properties panel. |
| Partial | The tab strip's Export button opens the Export dialog instead of the reference's quick-export menu, which writes a PNG, JPG or WebP straight to disk without asking for a path. The reference's layout toggle is not mounted here at all: it switches the right dock between side-by-side and stacked, this app ships one dock layout, and a painted control that answers no click is worse than an absent one. |
| Shipped | Document tabs: create from the strip's plus button or the File menu, switch, reorder, and close through a Save/Discard/Cancel prompt when dirty; the strip's Export button opens the Export dialog. |
| Shipped | Tooltips on controls and a toast host for warnings such as locked or missing layers. |
| Shipped | The window reopens at the remembered position, size and maximized state; a record that no longer lands on a connected display falls back to the centered default. |
| Shipped | Image files dropped onto the window open as documents. |
| Not implemented | A screen-reader accessibility tree: the current GPUI runtime exposes no platform accessibility provider. |

## Internationalization

| Status | Capability |
| --- | --- |
| Shipped | English and Indonesian catalogs drive the interface; the language choice persists in settings and startup detects the OS locale. |
| Partial | About 60 rendered strings are written as literals beside the catalog, each carrying a written exemption reason, so they stay in English whatever the language choice. Seven of them are strings this app invented, where the reference had no text to translate. Localizing any of them needs a new catalog key, and the equal-key-set rule below currently forbids adding one. |
| Shipped | The `en` and `id` key sets are held equal by a test, so a key cannot exist in one language and not the other. Deliberately-untranslated literals carry an explicit exemption reason, and a guard rejects new hardcoded UI strings in CI. |
| Not implemented | Languages beyond English and Indonesian. |

## Packaging

| Status | Capability |
| --- | --- |
| Shipped | Tag-triggered release workflow builds Windows, macOS and Linux and attaches `Photrez-<tag>-windows-x64.zip`, `Photrez-<tag>-macos-arm64.tar.gz` and `Photrez-<tag>-linux-x64.tar.gz` (executable plus `LICENSE`, `NOTICE`, `THIRD_PARTY.md`) and a `SHA256SUMS` file to the draft GitHub Release. |
| Shipped | Dev loop commands: `cargo dev`, `cargo xtask dev` (Windows GNU PATH wrapper), `cargo ui`, and the `bacon` check loop, as documented in `README.md`. |
| Not implemented | Code-signed installers and an auto-updater; releases are unsigned portable archives verified via `SHA256SUMS`. |
