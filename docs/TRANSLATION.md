# Menerjemahkan Photrez — TRANSLATION GUIDE

Terima kasih mau membantu menerjemahkan Photrez! Alur ini sengaja dibuat sesederhana
mungkin: **kamu tidak perlu menyentuh kode TypeScript**.

## Langkah cepat

1. Copy folder `apps/desktop/src/i18n/locales/en`, lalu ganti namanya menjadi kode
   bahasa ISO 639-1 (huruf kecil, 2 huruf), misalnya `fr`, `es`, `ar`, `ja`, `de`.
2. Buka `translation.json` di dalam folder baru itu. Terjemahkan **nilai** (sisi
   kanan / value). **Jangan** ubah **key** (sisi kiri) dan **jangan** terjemahkan
   placeholder `{{...}}`.
3. Nama bahasa di switcher muncul **otomatis** lewat `Intl.DisplayNames` — kamu tidak
   perlu mendaftarkan nama bahasa di mana pun.
4. Jalankan: `bun run --filter photrez-desktop test --run src/i18n`. Ada test
   *locale key parity* yang akan menolak (merah) kalau ada key yang terlewat, jadi
   kontribusimu aman dari string yang lupa diterjemahkan.
5. Buat Pull Request. Selesai!

## Format katalog

- Object bersarang = grup, misal `tools.brush` (alat kuas).
- **Plural:** ada dua key, `status.layers_one` (untuk jumlah = 1) dan
  `status.layers_other` (untuk jumlah lainnya). Biarkan keduanya ada; i18next
  memilih otomatis sesuai aturan plural bahasa tersebut.
- **Interpolasi:** token `{{count}}` (dan `{{...}}` lainnya) harus tetap utuh dan
  tidak diterjemahkan — ia diganti angka/teks saat runtime.
- Jangan menghapus key dari `en`; `en` adalah bahasa rujukan utama.

## Menambah bahasa tanpa menyentuh kode

Cukup ada file `locales/<code>/translation.json` yang **lengkap** (semua key sama
dengan `en`), maka aplikasi langsung mendeteksi bahasa baru tersebut dan
menampilkannya di switcher. Tidak ada konstanta bahasa yang perlu diedit.

## Catatan untuk pengembang (bukan penerjemah)

- Deteksi bahasa awal: `i18next-browser-languagedetector` membaca preferensi OS
  (`navigator.language`) lalu menyimpannya ke `localStorage` (`photrez.locale`).
- `src/i18n/index.ts` menggunakan `import.meta.glob` sehingga daftar bahasa
  dibangun otomatis dari isi folder `locales/`.
