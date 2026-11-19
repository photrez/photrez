# Photrez Translation Guide

Photrez has one catalog file per locale under `crates/photrez/locales`. The file name is the locale code. The files are embedded at compile time through [rust-i18n](https://github.com/longbridge/rust-i18n). The Settings list discovers these catalogs automatically.

## Catalog shape

One file per locale. The file name is the locale code. The keys are flat and dotted. A key is the same string that the code asks for:

```json
{
  "adjustments.basic": "Basic",
  "dialogs.about.tagline": "Native desktop image studio. Lean. Offline. Under 80 MB."
}
```

## Edit a translation

1. Edit the value in the locale file. Keep the key unchanged.
2. Keep the placeholders such as `%{count}` unchanged. Do not translate a placeholder name.
3. Keep every value non-empty.
4. Run `cargo test -p photrez i18n::tests --lib` and include the change in a pull request.

## Add or change a key

1. Add the key to `crates/photrez/locales/en.json` with the English text.
2. Add the same key with a translated value to every other locale file. The key-set and empty-value tests reject a missing or empty translation.
3. Run `cargo test -p photrez --lib i18n::tests`.

## Add a language

1. Copy an existing catalog to `crates/photrez/locales/<code>.json`. Use the locale code as the file name.
2. Add the locale code to `available-locales` in `crates/photrez/Cargo.toml`. This list is metadata; the Settings list discovers locales from the embedded catalogs.
3. Translate `locale.name` and every other value. No Rust locale registration is needed.
4. Run `cargo test -p photrez --lib i18n::tests`.

The Settings list reads embedded app catalogs through
`rust_i18n::available_locales!()`. It uses each catalog's `locale.name`, keeps
English first, and sorts the other locale codes. The Cargo metadata list
configures extraction only; it does not drive runtime discovery.

The chosen locale is stored under `photrez.locale`. Unknown locale tags fall back to English.

## Runtime lookups

Call sites use `rust_i18n::t!` after `use rust_i18n::t;`. The macro resolves a
key in the active locale, then in English. A key that no locale carries renders
as the key itself. GPUI takes owned `String` values, so call sites wrap the
macro's `Cow<str>` result in `String::from(...)`.

`crate::i18n::t_args` and `crate::i18n::t_plural` stay as helpers. The `t!` macro
needs its arguments at compile time, and rust-i18n has no plural support.
Plural lookups choose `{base}_one` only when the count is `1` and `{base}_other`
otherwise. They do not implement locale-specific plural categories.

## Toolkit strings

`gpui-kit` ships its own strings under the `gpui_component` namespace. The app catalog can override them:

```json
{
  "gpui_component.Calendar.month.February": "Februari"
}
```

The key name must match the toolkit catalog exactly. Copy it from the toolkit source at `crates/component/locales/ui.yml`.

The app uses the toolkit's button, icon, scroll, sizing and theme pieces. None of them reads a toolkit string in production code, so the app shows no toolkit string today and no override exists yet. The merge is wired before the toolkit init, so a later override is data only.

## Gates

- `cargo test -p photrez --lib i18n::tests` checks every discovered locale for
  missing keys, empty values, placeholder mismatches, and missing locale names.
  Release-reference comparisons remain limited to the original `en`, `id`, and
  `bjn` catalogs.
- CI runs `cargo test -p photrez i18n::tests::every_literal_lookup_resolves --lib`. That gate checks that every literal key passed to `t_args` or `t_plural` exists in every locale.

## Translation rules

- Translate values, not keys or product names.
- Keep punctuation, line breaks, and formatting markers where they carry meaning.
- Keep placeholders and their spelling unchanged.
- Review the strings in the native application at the final display size, especially the longer Indonesian text.
