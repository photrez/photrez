// SPDX-License-Identifier: AGPL-3.0-or-later
// Text edit overlay — the live typing surface for a text edit session.
//
// Plan §7.92: a <textarea> positioned over the canvas at doc→screen coords,
// synced with zoom/pan; transparent background, accent border. While editing
// the canvas keeps rendering the live raster (debounced re-raster on every
// input), so the result is WYSIWYG without a placeholder (research R1).
//
// Session contract (Task 6):
//   - Mounts while textEditSession() is non-null (new temp layer or re-edit).
//   - Every input re-rasterizes the layer live (debounced 50ms) via
//     engine.updateTextData → renderer.uploadImage.
//   - Ctrl/Cmd+Enter commits the session (one undo step); Escape cancels.
//   - IME-safe: composition events do not push/commit until compositionend,
//     and Enter during composition inserts the IME candidate, not a commit.
import { Show, createEffect, createMemo, createSignal, onCleanup, untrack, type JSX } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { commitTextSession, cancelTextSession, setPendingTextFlush } from "./canvas/pointerTools/textTool";
import type { TextData } from "@/engine/textTypes";

/** Debounce window for live re-raster (plan risk R4: memory churn). */
const RERASTER_DEBOUNCE_MS = 50;

export function TextEditOverlay() {
  const {
    workspace,
    renderer,
    scheduler,
    zoom,
    pan,
    layers,
    textEditSession,
    setTextEditSession,
  } = useEditor();

  let textareaRef: HTMLTextAreaElement | undefined;
  let pushTimer: ReturnType<typeof setTimeout> | undefined;
  let composing = false;
  let prevSessionKey: string | null = null;
  const [value, setValue] = createSignal("");

  const session = () => textEditSession();
  const layer = createMemo(() => {
    const s = session();
    if (!s) return null;
    // Read through the reactive layers() signal (same pattern as ShapeOptionBar)
    // so the binding updates immediately on engine change.
    const found = layers().find((l) => l.id === s.layerId);
    return found && found.type === "text" && found.textData ? found : null;
  });
  const textData = createMemo<TextData | null>(() => layer()?.textData ?? null);

  const editorForSession = () => ({ workspace, textEditSession: session, setTextEditSession, scheduler });

  // External value sync: when the session/layer changes (new session, undo,
  // option-bar edit), adopt the layer's content into the textarea. Focus +
  // select-all happen ONLY on session start (double-click re-edit UX, R3) —
  // never on keystrokes, or typing would keep re-selecting all text.
  createEffect(() => {
    const s = session();
    const td = textData();
    if (!s) {
      // Session closed: drop the registered flush (so a stale overlay never
      // pushes after commit) and reset the session key so re-editing the SAME
      // layer in a later session still gets focus+select-all.
      setPendingTextFlush(null);
      prevSessionKey = null;
      return;
    }
    if (!td) return;
    // Adopt the engine content when it differs from the textarea. Reading
    // value() UNTRACKED is essential: a tracked read would make this effect
    // re-run on every keystroke and clobber the typed text back to the (still
    // debounced-stale) engine content. This effect must react only to session
    // and layer changes — never to our own typing.
    untrack(() => {
      if (td.content !== value()) setValue(td.content);
    });
    if (prevSessionKey !== s.layerId) {
      prevSessionKey = s.layerId;
      if (textareaRef) {
        textareaRef.focus();
        textareaRef.select();
      }
    }
    // Register the pending-content flush so external commits (click-away,
    // tool switch) see the latest typed text before empty-commit cleanup runs.
    setPendingTextFlush(() => {
      if (pushTimer) {
        clearTimeout(pushTimer);
        pushTimer = undefined;
      }
      pushContent(value());
    });
  });

  onCleanup(() => {
    if (pushTimer) clearTimeout(pushTimer);
  });

  const pushContent = (content: string) => {
    const s = session();
    if (!s) return;
    const engine = workspace.getActiveEngine();
    if (!engine) return;
    const l = engine.getLayer(s.layerId);
    if (!l || l.type !== "text" || !l.textData) return;
    engine.updateTextData(s.layerId, { ...l.textData, content });
    const bitmap = engine.getLayerImageBitmap(s.layerId);
    if (bitmap) renderer?.uploadImage(s.layerId, bitmap);
    scheduler.requestRender();
  };

  const schedulePush = (content: string) => {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      pushTimer = undefined;
      pushContent(content);
    }, RERASTER_DEBOUNCE_MS);
  };

  // Doc→screen positioning (same math as the artboard border: pan + doc*zoom).
  const overlayStyle = createMemo<JSX.CSSProperties>(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return {};
    const z = zoom();
    const p = pan();
    return {
      position: "absolute",
      left: `${p.x + s.docX * z}px`,
      top: `${p.y + s.docY * z}px`,
      width: s.boxMode === "area" && s.boxWidth > 0 ? `${s.boxWidth * z}px` : "auto",
      minWidth: "160px",
      "font-family": `"${td.fontFamily}", sans-serif`,
      "font-size": `${td.fontSize * z}px`,
      "font-weight": td.fontWeight,
      "font-style": td.fontStyle,
      color: td.color,
      "text-align": td.align,
      "line-height": td.lineHeight,
      "letter-spacing": `${td.letterSpacing * z}px`,
      background: "transparent",
      resize: "none",
      overflow: "hidden",
      border: "1px solid var(--editor-accent, #E15A17)",
      outline: "none",
      padding: "2px",
      "caret-color": "var(--editor-accent, #E15A17)",
      "white-space": "pre-wrap",
      "word-break": "break-word",
      "pointer-events": "auto",
      "z-index": 60,
    };
  });

  return (
    <Show when={session()}>
      <textarea
        ref={textareaRef}
        data-text-edit-overlay
        aria-label="Edit text"
        value={value()}
        spellcheck={false}
        onPointerDown={(e) => e.stopPropagation()}
        onPointerUp={(e) => e.stopPropagation()}
        onPointerMove={(e) => e.stopPropagation()}
        onPointerCancel={(e) => e.stopPropagation()}
        onLostPointerCapture={(e) => e.stopPropagation()}
        onCompositionStart={() => { composing = true; }}
        onCompositionEnd={(e) => {
          composing = false;
          const v = e.currentTarget.value;
          setValue(v);
          schedulePush(v);
        }}
        onInput={(e) => {
          const v = e.currentTarget.value;
          setValue(v);
          // During an IME composition the intermediate text is the browser's
          // candidate preview — push only the settled value at compositionend.
          if (!composing) schedulePush(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cancelTextSession(editorForSession());
            return;
          }
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !composing) {
            e.preventDefault();
            e.stopPropagation();
            // Flush any pending debounce so the committed layer matches what
            // the user sees (empty-commit cleanup then sees real content).
            if (pushTimer) {
              clearTimeout(pushTimer);
              pushTimer = undefined;
            }
            pushContent(value());
            commitTextSession(editorForSession());
          }
        }}
        style={overlayStyle()}
      />
    </Show>
  );
}
