# Design Tokens (MVP Baseline)

Token ini menjadi sumber tunggal styling UI Photrez di MVP yang menggunakan Tailwind v4 dan sistem warna OKLCH.

> **Last synced**: 2026-10-18 — Synced with current desktop SolidJS & Tailwind v4 UI implementation. Colors mapped to OKLCH values in `src/styles.css`, layout dimensions re-verified against the shipped shell components and matched to the current docked side-by-side double-dock layout.

## 1) Usage Rules

1. Jangan hardcode nilai warna/spacing/radius di komponen.
2. Semua style baru harus memakai token Tailwind v4 yang dideklarasikan di `@theme inline` atau CSS variables di `:root`.
3. Jika token baru diperlukan, tambahkan di `src/styles.css` terlebih dahulu lalu sinkronkan ke dokumen ini.

## 2) Color Tokens (True Neutral OKLCH Palette)

Sistem warna menggunakan rona abu-abu netral sejati (Zero-Tint Gray) untuk mencegah distorsi atau bias warna saat desainer mengedit gambar. Aksen menggunakan warna hangat **Photon Amber** secara minimal.

```css
:root {
  /* Studio Neutral Grays (Zero-Tint for accurate color perception) */
  --editor-bg: oklch(0.205 0 0);           /* Core app container background */
  --editor-topbar: oklch(0.19 0 0);        /* Titlebar, Document Tabs, Status Bar */
  --editor-panel: oklch(0.235 0 0);        /* Properties, Layers sidebar panel background */
  --editor-canvas: oklch(0.17 0 0);        /* Workspace well behind artboard */
  --editor-field: oklch(0.265 0 0);        /* Recessed fields / input controls background */
  --editor-field-border: oklch(0.34 0 0);  /* Soft borders for input fields */
  --editor-divider: oklch(0.3 0 0);        /* Mechanical dividers between panels (1px border) */
  --editor-toolbar: oklch(0.22 0 0);       /* Left Tool Rail / Options Bar background */
  --editor-row-active: oklch(0.3 0 0);     /* Active layer / list item row */

  /* Typography / Text colors */
  --editor-text: oklch(0.84 0 0);          /* High contrast primary text */
  --editor-text-dim: oklch(0.58 0 0);      /* Secondary labels and meta text */
  --editor-icon: oklch(0.62 0 0);          /* Default state for UI icons */

  /* Signature Accent: Photon Amber */
  --editor-accent: oklch(0.74 0.15 55);    /* Warm amber color for focus, active states, active tab bar */
  --editor-brand: oklch(0.62 0.2 36);      /* Accent brand color for 'pz' logo icon container */
}
```

## 3) Typography Tokens

```css
:root {
  --font-sans: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  
  /* Font size definitions mapped inside CSS/Tailwind rules */
  --font-size-xs: 11px;  /* Micro labels in Transform / Anchor Matrix */
  --font-size-sm: 12px;  /* Core inputs / Properties values / Status Bar */
  --font-size-md: 13px;  /* General text, Titlebar Menu dropdowns, Layer titles */
  --font-size-lg: 14px;  /* Section Header Titles (e.g. 'Properties', 'Layers') */
}
```*Catatan: Keutuhan lebar angka dijaga secara global menggunakan `font-variant-numeric: tabular-nums;` di root. Untuk menghindari ilusi optis di mana teks tab terlihat terlalu besar dibandingkan teks properti statis, seluruh tab (seperti DocumentTabsBar, LayersPanel, dan RightDock) diselaraskan secara optikal menggunakan ukuran `text-[12px] font-medium`.*

## 4) Radius and Border Tokens

Aplikasi menerapkan aturan **Soft & Snappy** di mana radius didasarkan pada proporsi modular:

```css
:root {
  --radius-sm: 2px;                        /* Small items, color swatches */
  --radius-md: 4px;                        /* Inputs, buttons, tabs, dropdowns */
  --radius-lg: 6px;                        /* Outer panels, main container elements */
}
```

*Aturan Docking: Hanya sudut yang menghadap ke arah Canvas (inner corners) yang diberikan `--radius-lg` atau `--radius-md`. Sisi yang menempel pada tepi window (outer corners) wajib tajam (`0px`).*

## 5) Layout Dimension Tokens (Live Desktop Spec)

Dimensi aktual yang berlabuh pada tepi window (docked layout):

| Elemen | Dimensi | Catatan / Class Tailwind |
| --- | --- | --- |
| **AppTitleBar** height | `46px` | Desktop titlebar style, centered doc title, `h-[46px]` |
| **DocumentTabsBar** height | `38px` | `h-[38px] bg-editor-topbar` |
| **OptionBar** height | `44px` | `h-[44px] bg-editor-toolbar` |
| **BottomStatusBar** height | `24px` | `h-[24px] bg-editor-topbar` |
| **LeftToolRail** width | `52px` | `w-[52px] bg-editor-toolbar` |
| Tool rail items size | `36×36 px` | `size-9` (36px), rounded `[5px]` |
| Tool rail icon size | `18px` | `size-[18px]` |
| **RightDock** width (double-dock) | `533px` | `272px` Properties + `260px` Layers + `1px` divider, side-by-side |
| RightDock 2XL screen width | `599px` | `300px` Properties + `298px` Layers + `1px` divider, side-by-side |
| Input fields height | `26px` | Recessed boxes, `h-[26px] bg-editor-field` |

## 6) Scrollbar Tokens (Native-Feel Custom Overlay)

Menggunakan scrollbar 10px kustom yang digambar sebagai content-box dengan border transparan sehingga menghasilkan visualisasi ramping (slim overlay scrollbar):

```css
::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: color-mix(in oklch, var(--editor-field-border), transparent 24%);
  border: 3px solid transparent;
  border-radius: 999px;
  background-clip: content-box;
}
::-webkit-scrollbar-thumb:hover {
  background: color-mix(in oklch, var(--editor-text-dim), transparent 25%);
  border: 3px solid transparent;
  background-clip: content-box;
}
```

## 7) Premium Biphasic Control Sliders

Properties panel menggunakan slider kustom tipis bergaris `3px` dengan handle bulat `10px` yang memiliki border pelindung dan shadow kedalaman:

```tsx
// Implementasi slider horizontal bergradien dan center-tick (seperti Temp/Tint)
export function Slider(props: {
  percent: number;
  gradient?: string;
  centerTick?: boolean;
}) {
  return (
    <div
      class={clsx(
        "relative h-[3px] flex-1 rounded-full",
        props.gradient ? "" : "bg-editor-field-border"
      )}
      style={props.gradient ? { "background-image": props.gradient } : undefined}
    >
      <Show when={props.centerTick}>
        {/* Titik tengah balance untuk biphasic adjustments */}
        <div class="absolute left-1/2 top-1/2 size-[3px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/40" />
      </Show>
      <div
        class="absolute top-1/2 size-[10px] -translate-y-1/2 rounded-full border border-black/40 bg-[#d4d4d4] shadow-[0_1px_2px_rgba(0,0,0,0.5)]"
        style={{ left: `calc(${props.percent}% - 5px)` }}
      />
    </div>
  );
}
```

Gradien warna biphasic yang digunakan saat ini:
- **Temp (Temperature)**: Gradien dari biru (dingin), ke abu-abu tengah, ke kuning/jingga (hangat)
  `linear-gradient(to right, #3b82f6, #6b8db8, #d98a2b)` dengan `centerTick` aktif.
- **Tint**: Gradien dari hijau (green), ke abu-abu tengah, ke magenta/pink
  `linear-gradient(to right, #5aa86a, #2b2b2b 50%, #b25fae)` dengan `centerTick` aktif.

## 8) Premium Diagonal Color Swatch

LeftToolRail memuat swatch warna lingkaran bertumpuk yang memaksimalkan ukuran target klik hingga `38px` (mempet penuh untuk efisiensi ruang), dengan komposisi diagonal dari dua lingkaran yang saling tumpang tindih:

```tsx
// Implementasi swatch lingkaran bertumpuk (offset diagonal)
<div class="relative size-[38px]">
  {/* Background Color (Bottom-Right Circle) */}
  <div
    class="absolute bottom-0 right-0 size-[28px] rounded-full border border-white/20 shadow-md"
    style={{ "background-color": bgColor() }}
  />
  {/* Foreground Color (Top-Left Circle) */}
  <div
    class="absolute top-0 left-0 size-[28px] rounded-full border border-white/30 outline outline-1 outline-black/40 shadow-md z-10"
    style={{ "background-color": fgColor() }}
  />
</div>
```

**Aturan Visual**:
1. Dua lingkaran `size-[28px]` bertumpuk di dalam container `size-[38px]`: background di `bottom-0 right-0`, foreground di `top-0 left-0` dengan `z-10` sehingga menutupi area tengah.
2. Pemisahan diagonal dihasilkan dari offset pemosisian absolut + urutan z-order, bukan `clip-path` atau border masking tambahan.
3. Tombol swap warna mikro berada di sudut kanan-atas (`absolute -top-1.5 -right-1.5 z-20 size-4`, ikon `rotate`) dan hanya tampak saat hover (`scale-0 group-hover:scale-100`).

---

## 9) Change History

| Tanggal | Perubahan | Alasan |
| --- | --- | --- |
| 2026-05-27 | Accent: Studio Indigo → Photon Amber | Identitas visual hardware-tooling yang hangat, sesuai arah branding |
| 2026-05-30 | Synced with SolidJS Tailwind v4 UI | Memperbarui token ke sistem warna OKLCH, radius 6px modular, slider horizontal bergradien, dan layout double-dock side-by-side |
| 2026-05-30 | UI/UX Polish: Diagonal Swatches & Tab Sizing | Implementasi custom diagonal split color swatches penuh 36px, visual tab typography alignment 12px (mengatasi ilusi optis kebesaran), layout tool rail bottom-aligned mt-auto, dan pemangkasan non-MVP icons |
| 2026-10-18 | Sinkronisasi dimensi shell & swatch dengan implementasi saat ini | DocumentTabsBar 44→38px, BottomStatusBar 30→24px, RightDock 560→533px (272+260+1) dan varian 2XL 634→599px (300+298+1), swatch container 36→38px dengan lingkaran 35→28px (implementasi lingkaran bertumpuk, bukan clip-path) |
