# Menerjemahkan Photrez — TRANSLATION GUIDE

Terima kasih mau membantu menerjemahkan Photrez! Yang perlu kamu tahu duluan:
repo ini untuk sementara punya **dua katalog yang sama persis** — satu untuk
aplikasi lama (web/Tauri) dan satu yang di-embed oleh shell native — dan
keduanya harus tetap identik byte-per-byte. Bahasa yang benar-benar dirilis
saat ini: **`en` (English)** dan **`id` (Bahasa Indonesia)**.

## Katalog ada di dua tempat

| Lokasi | Peran |
| --- | --- |
| `apps/desktop/src/i18n/locales/<kode>/translation.json` | **Rujukan.** Dibaca aplikasi lama; ini yang diedit lebih dulu. |
| `crates/photrez/assets/i18n/<kode>.json` | Salinan yang di-embed shell native saat kompilasi. |

**Aturan besinya: rujukan dulu, salinan kedua.** Ubah `apps/desktop/...`
lebih dulu, lalu salin isinya **utuh** ke `crates/photrez/assets/i18n/`.
Test `catalog_matches_reference` menolak (merah) setiap perbedaan byte, supaya
perubahan tidak bisa "nyangkut" di satu shell saja.

## Mengubah string yang ada

1. Buka `apps/desktop/src/i18n/locales/en/translation.json`. Terjemahkan
   **nilai** (sisi kanan). **Jangan** ubah **key** (sisi kiri), dan **jangan**
   terjemahkan placeholder `{{...}}`.
2. Key harus sama persis antar bahasa — test parity menolak key yang terlewat
   di salah satu sisi.
3. Salin file yang kamu ubah ke `crates/photrez/assets/i18n/<kode>.json`
   (byte-identik dengan rujukan).
4. Jalankan test-nya (lihat bagian Catatan pengembang), lalu buat Pull Request.

## Menambah bahasa baru

Menambah bahasa menyentuh tiga tempat, dan shell native butuh dua baris kode:

1. Copy folder `apps/desktop/src/i18n/locales/en` → nama kode bahasa ISO 639-1
   (huruf kecil, 2 huruf), misal `fr`, `ja`. Aplikasi lama menemukan folder
   baru **otomatis** dan menampilkan nama bahasanya di switcher.
2. Salin `translation.json` yang sudah lengkap ke
   `crates/photrez/assets/i18n/<kode>.json`, lalu daftarkan di
   `crates/photrez/src/i18n/mod.rs`: satu arm baru di enum `Locale`, satu
   baris di tabel `CATALOGS`. Tanpa itu shell native tidak melihat bahasa-mu.
3. Jalankan gate cakupan glyph (lihat bawah). Font yang di-embed cakupannya
   terbatas; gate ini melaporkan tiap code point yang jatuh ke font fallback
   supaya karakter baru tidak diam-diam berubah bentuk. Gate saat ini menutup
   katalog `id`; bahasa baru sebaiknya ikut dicakup dengan pola yang sama.

Kalau bahasa yang kamu tambahkan dirilis, baris bahasa di dialog About shell
native (`ABOUT_LANGUAGES` di `crates/photrez/src/views/dialogs.rs`) dan test
`about_names_the_shipped_locales` ikut berubah di commit yang sama. Itu
memang disengaja: daftar "bahasa yang dirilis" tidak boleh basi.

## Format katalog

- Object bersarang = grup, misal `tools.brush` (alat kuas).
- **Plural:** dua key, `status.layers_one` (jumlah = 1) dan
  `status.layers_other` (lainnya). Biarkan keduanya ada.
- **Interpolasi:** token `{{count}}` (dan `{{...}}` lain) diganti saat
  runtime — jangan diterjemahkan. Shell native menolak placeholder tanpa
  argumen dan argumen tanpa placeholder, jadi jangan menambah `{{...}}` baru
  tanpa call site yang mengisinya.
- Jangan hapus key dari `en`; `en` adalah bahasa rujukan.

## Catatan untuk pengembang (bukan penerjemah)

- **Test yang menjaga katalog native** (`crates/photrez/src/i18n/mod.rs`):
  `catalog_matches_reference` (byte-identik dengan rujukan), `locale_parity`
  (key satu-satu antar bahasa), `placeholder_sets_match_across_locales`, dan
  `indonesian_glyph_coverage` (cakupan font, dijalankan eksplisit dengan
  `--ignored`). Dari folder `crates/photrez`: `cargo test i18n::`. Test
  frontend: `bun run --filter photrez-desktop test:unit`.
- **Deteksi bahasa di shell native:** env `PHOTREZ_LOCALE` (dipakai alat
  capture/parity) → preferensi tersimpan di settings (`photrez.locale`) →
  locale OS → `en`. Tag dinormalisasi ke bagian bahasa: `id-ID`, `id_ID.UTF-8`
  dan `ID` semuanya menjadi `id`; tag tak dikenal jatuh ke `en`.
- **String khusus shell native** (nilai teknologi, nama produk, endonim) tidak
  boleh masuk katalog: katalog terkunci byte ke rujukan. Literal semacam itu
  ditandai `// i18n-exempt: <alasan>` dan dijaga
  `scripts/check-public-terminology.sh --i18n`.
- Aplikasi lama memakai `i18next-browser-languagedetector` + `localStorage`
  (`photrez.locale`); daftar bahasanya dibangun otomatis dari isi folder
  `locales/` lewat `import.meta.glob` — lihat
  `apps/desktop/src/i18n/index.ts`.
