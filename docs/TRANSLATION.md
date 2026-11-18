# Photrez Translation Guide

Photrez ships English (`en`), Indonesian (`id`), and Banjar (`bjn`). Each locale has one catalog file under `crates/photrez/locales`. The files are embedded at compile time through [rust-i18n](https://github.com/longbridge/rust-i18n).

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
2. Run `node scripts/i18n-sync.mjs`. The script adds the key to every other locale with an empty value and prints which key each locale now owes.
3. Translate the empty value in each locale file. The empty value fails the `no_translation_is_empty` test, so an untranslated key cannot reach a build unnoticed.

## Add a language

1. Run `node scripts/i18n-sync.mjs --add <code>`. The script writes `crates/photrez/locales/<code>.json` with every key and an empty value.
2. Register the locale in `crates/photrez/src/i18n.rs` (`Locale` and startup detection).
3. Add the code to `available-locales` in `crates/photrez/Cargo.toml`.
4. Translate the file, then run `cargo test -p photrez i18n::tests --lib`.

The chosen locale is stored under `photrez.locale`. Unknown locale tags fall back to English.

## Runtime lookups

`crate::i18n::t`, `t_args`, and `t_plural` resolve a key in the active locale, then in English. GPUI takes owned `String` values, and many keys are chosen at runtime, so the code calls these helpers instead of the `rust_i18n::t!` macro. A key that no locale carries renders as the key itself.

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

- `cargo test -p photrez i18n::tests --lib` checks that every locale carries every key, that no value is empty, that the placeholders match across the locales, and that the catalog still matches the shipped release.
- CI runs `cargo test -p photrez i18n::tests::every_literal_lookup_resolves --lib`. That gate checks that every key the code asks for exists in every locale.
- The pre-commit hook runs `scripts/check-public-terminology.sh --i18n` over the staged files under `crates/photrez/src/views` and `crates/photrez/src/components`. It rejects a user-visible string literal that is not a catalog key, unless the line carries `// i18n-exempt: <reason>`.

## Translation rules

- Translate values, not keys or product names.
- Keep punctuation, line breaks, and formatting markers where they carry meaning.
- Keep placeholders and their spelling unchanged.
- Review the strings in the native application at the final display size, especially the longer Indonesian text.
