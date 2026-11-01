# Photrez Translation Guide

Photrez currently ships English (`en`) and Indonesian (`id`). Their embedded catalogs are in `crates/photrez/assets/i18n/en.json` and `crates/photrez/assets/i18n/id.json`.

## Edit a translation

1. Edit the values in the relevant catalog. Keep keys unchanged and keep the key sets aligned across both files.
2. Preserve interpolation placeholders such as `{{count}}` exactly; do not translate or remove them.
3. Keep every translated value non-empty. Nested objects group related strings.
4. Run the relevant `photrez` crate tests and include the translation change in a pull request.

## Add a language

Adding a catalog also requires registering its locale and embedded catalog in `crates/photrez/src/i18n.rs`, then extending the catalog and locale tests. Adding a JSON file alone does not make a language available in the application.

The chosen locale is stored under `photrez.locale`. Unknown locale tags fall back to English.

## Translation rules

- Translate values, not keys or product names.
- Preserve punctuation, line breaks, and formatting markers where they carry meaning.
- Keep placeholders and their spelling unchanged.
- Review strings in the native application at the final display size, especially longer Indonesian text.
