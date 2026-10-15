<p align="center">
  <img src="./logo.svg" width="100" height="100" alt="Photrez Logo" />
</p>

<h1 align="center">
  <strong>Photrez</strong>
</h1>

<p align="center">
  <strong>A fast, lightweight desktop image editor for daily graphic work.</strong><br>
  Layer management, transforms, precision selection, brush tools, and print export — built for creators who want clean, dependable editing without the bloat.
</p>

<p align="center">
  <a href="https://github.com/photrez/photrez/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/photrez/photrez/ci.yml?branch=main&label=ci&style=flat-square"></a>
  <a href="https://github.com/photrez/photrez/releases"><img alt="Latest release" src="https://img.shields.io/github/v/release/photrez/photrez?style=flat-square"></a>
  <a href="https://github.com/photrez/photrez/blob/main/LICENSE"><img alt="License: AGPL-3.0-or-later" src="https://img.shields.io/badge/license-AGPL--3.0--or--later-E15A17?style=flat-square"></a>
  <a href="https://github.com/photrez/photrez/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/photrez/photrez?style=flat-square"></a>
  <a href="https://github.com/photrez/photrez/issues"><img alt="GitHub issues" src="https://img.shields.io/github/issues/photrez/photrez?style=flat-square"></a>
  <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%2F11-0078D6?style=flat-square">
</p>

---

![Photrez Editor Preview](docs/screenshots/layers.png)

<div align="center">

### 📦 Download Photrez for Windows

**[⬇️ Download Photrez v0.1.0 (.exe Setup)](https://github.com/photrez/photrez/releases/download/v0.1.0/Photrez_0.1.0_x64-setup.exe)**
<br>
*Supports Windows 10 & 11 (64-bit)*

</div>

> ℹ️ **SmartScreen Notice:** The installer is currently not code-signed with a commercial certificate. If Windows SmartScreen displays a warning upon launch, click **"More info"** and then **"Run anyway"**.

---

## ✨ Key Features

<table>
  <tr>
    <td width="50%" valign="top">
      <h3>⚡ Lightweight & Fast</h3>
      <p>Starts near-instantly with a compact <b>~5MB installer</b> and minimal <b>~34MB idle RAM</b> usage. No background telemetry or forced cloud accounts.</p>
    </td>
    <td width="50%" valign="top">
      <h3>🎨 Layer & Selection Workflow</h3>
      <p>Full multi-layer management (opacity, visibility, lock, duplicate, merge), marquee selection tools (rectangle & ellipse), and object transforms.</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <h3>🖌️ Calibrated Paint & Vector</h3>
      <p>Smooth round-tip brush and eraser with hardness, flow, and smoothing controls, alongside native vector text and shape layers.</p>
    </td>
    <td width="50%" valign="top">
      <h3>🖨️ Pro Print & High-DPI Export</h3>
      <p>Built-in print preview inspector with paper presets, DPI/PPI scale calculations, and clean exports to PNG, JPEG, and WebP.</p>
    </td>
  </tr>
</table>

---

## 📁 Supported Formats

| Format | Extension | Type | Import | Export | Notes |
| --- | --- | --- | :---: | :---: | --- |
| **Photrez Project** | `.ptz` | Native Project | ✅ | ✅ | Backward-compatible from v0.1.0; current format v3, preserves full layer state & vector text |
| **PNG Image** | `.png` | Raster Bitmap | ✅ | ✅ | Full RGBA alpha channel transparency support |
| **JPEG Image** | `.jpg`, `.jpeg` | Raster Bitmap | ✅ | ✅ | Standard compressed photo format with quality controls |
| **WebP Image** | `.webp` | Raster Bitmap | ✅ | ✅ | Modern web image format (lossless & lossy) |

---

## ⌨️ Essential Keyboard Shortcuts

| Shortcut | Tool / Action | Description |
| --- | --- | --- |
| `V` | **Move & Transform** | Select, drag, scale, and rotate objects |
| `M` / `Shift+M` | **Marquee Selection** | Toggle between Rectangular and Elliptical marquee |
| `B` / `E` | **Brush & Eraser** | Paint strokes or erase bitmap layer pixels |
| `T` / `U` | **Text & Shape** | Add typography layers or draw vector shapes |
| `Ctrl + Z` / `Ctrl + Y` | **Undo / Redo** | History stack navigation |
| `Ctrl + Shift + P` | **Print Inspector** | Open Pro-Suite paper setup & DPI preview |

---

## 🗺️ Release Roadmap

- [x] **`v0.1.0` — Stable MVP** *(released 2026-08-06)* — Complete editing toolkit; `.ptz` is backward-compatible from this release (current format v3).
- [ ] **`v0.2.0` — The Native Update** *(in development)*
  - ✅ **Text layers** — live, re-editable text on the canvas (`T`)
  - ✅ **Shape layers** — rectangle, ellipse, line, arrow, star; editable until rasterized (`U`)
  - ✅ **All 12 blend modes** — preview/export parity verified
  - 🚧 **Native Rust UI** — planned replacement for the Tauri web-view shell. Not yet implemented or shipped; installer/performance claims await measurement.
- [ ] **`v0.3.0` (planned)** — Floating and detachable inspector panels.
- [ ] **`v0.4.0` (planned)** — Multi-image windows.
- [ ] **`v1.0.0` (planned)** — Full cross-platform release (Windows, macOS, Linux).

---

## 💡 Why Photrez?

Photrez is a lightweight desktop image editor for practical digital and print workflows:

- **Local-first editing:** Your work stays on your machine; no cloud account is required.
- **A complete editing toolkit:** Layers, text, shapes, selection, brush tools, transforms, print settings, and export.
- **The Native Update:** v0.1.0 ships on Tauri 2 with the operating system's web-view runtime. The planned 0.2.0 release moves the interface to a native Rust UI; performance and installer claims will follow measurement.
- **Open source:** Licensed under AGPL-3.0-or-later.

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-v2-FFC107?style=flat-square&logo=tauri&logoColor=black" alt="Tauri 2 — v0.1.0 runtime">
  <img src="https://img.shields.io/badge/SolidJS-v1.9-2C4F7C?style=flat-square&logo=solid&logoColor=white" alt="SolidJS — v0.1.0 UI">
  <img src="https://img.shields.io/badge/TypeScript-Strict-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript">
  <img src="https://img.shields.io/badge/WebGL-2.0-990000?style=flat-square&logo=webgl&logoColor=white" alt="WebGL2 — v0.1.0 renderer">
  <img src="https://img.shields.io/badge/Rust-Core-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust core">
  <img src="https://img.shields.io/badge/Tailwind-v4-06B6D4?style=flat-square&logo=tailwindcss&logoColor=white" alt="Tailwind v4 — v0.1.0 UI">
  <img src="https://img.shields.io/badge/Bun-v1.4-FBF0DF?style=flat-square&logo=bun&logoColor=black" alt="Bun v1.4">
</p>
---

## 🛠️ Developer Quick Start

```bash
# 1. Clone repository & install dependencies
bun install

# 2. Launch application in desktop dev mode
bun run tauri dev

# 3. Run complete verification gate (tests + build + audit)
bun run verify
```

---

## ❓ Frequently Asked Questions (FAQ)

<details>
<summary><b>Why is the v0.1.0 installer small (~5MB)?</b></summary>
<p>The v0.1.0 release uses Tauri 2 with the operating system's WebView2 runtime. The planned 0.2.0 native Rust UI uses a different runtime; its installer size will be measured before release.</p>
</details>

<details>
<summary><b>Does Photrez require an internet connection or account?</b></summary>
<p>No. Photrez is 100% local-first software. It runs entirely offline on your computer without cloud accounts, telemetry, or subscriptions.</p>
</details>

<details>
<summary><b>Does Photrez support PSD files?</b></summary>
<p>Not currently. Photrez uses its versioned <code>.ptz</code> project format for layered documents and exports to PNG, JPEG, and WebP. PSD import support is on the long-term roadmap.</p>
</details>

---

## 📚 Documentation

- 🐛 **[Known Issues](KNOWN_ISSUES.md)** — Current limitations & bugs
- 🏗️ **[Architecture](docs/ARCHITECTURE.md)** — WebGL2 & SolidJS design
- 📋 **[Feature Checklist](docs/FEATURES.md)** — Full technical inventory
- 🤝 **[Contributing](CONTRIBUTING.md)** — PR & code guidelines
- 🔒 **[Security Policy](SECURITY.md)** — Vulnerability reporting

---

## 📄 License

Photrez is open-source software licensed under **AGPL-3.0-or-later**. See the [LICENSE](LICENSE) file for details.
