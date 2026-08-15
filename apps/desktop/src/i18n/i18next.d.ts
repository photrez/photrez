import "i18next";

// Configure i18next's TypeScript defaults for Photrez. We intentionally do NOT
// type `resources` here: i18next's plural keys use `_one`/`_other` suffixes in
// the catalog while call sites use the base key (`status.layers`), so a strict
// `resources` type would reject valid plural calls. Missing-key safety for
// translators is enforced at runtime by the locale parity test instead.
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
  }
}
