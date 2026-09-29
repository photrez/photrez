// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Pre-load census readers for window.__photrezPixelCensus() /
 * window.__photrezPixelFlush(), injected by apps/desktop/src-tauri/src/main.rs
 * as a Tauri main-frame initialization script, so every document the webview
 * creates receives them before any application script runs.
 *
 * Known hazard: the webview's very first about:blank document runs no
 * initialization script at all (Tauri's own __TAURI_INTERNALS__ is absent
 * there too) and the dev document only commits about a second later, so a CDP
 * census read that lands on that first blank document finds neither reader.
 *
 * Why injection exists: a CDP census drain can be evaluated before the app
 * module graph finishes loading - on a cold dev launch pixelInvokeCensus.ts
 * executes only ~76s after the window appears, so waiting for the module-load
 * registration in pixelInvokeCensus.ts leaves both globals undefined.
 *
 * Truthfulness: until pixelInvokeCensus.ts executes, reading either global
 * throws PhotrezCensusNotReady instead of answering with an empty snapshot,
 * which would drain as "zero pixel invokes" for a census nobody has recorded
 * yet. The module's registerPixelInvokeCensus() overwrites these readers with
 * the real ones as soon as it loads, and this script never overwrites an
 * installed reader. Six-command counting lives only in pixelInvokeCensus.ts;
 * this script records nothing.
 *
 * Marker: it also sets window.__photrezPixelCensusPreload, the only value a
 * document-start init script can publish, so the census bridge can tell "init
 * script injected" from "init script never ran" after registration has
 * replaced both readers.
 *
 * Injected as text via include_str!() - keep it plain ASCII script, no imports.
 */
(function installPhotrezPixelCensusPreload() {
  if (typeof window.__photrezPixelFlush === "function") return;
  window.__photrezPixelCensusPreload = true;
  function notReady() {
    var error = new Error("PhotrezCensusNotReady: census readers are not registered yet");
    error.name = "PhotrezCensusNotReady";
    throw error;
  }
  window.__photrezPixelCensus = notReady;
  window.__photrezPixelFlush = notReady;
})();
