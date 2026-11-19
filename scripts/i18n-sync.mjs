// Sync the locale catalogs: every locale file carries every key.
//
// Usage:
//   node scripts/i18n-sync.mjs             sync every locale against en.json
//   node scripts/i18n-sync.mjs --add xx   create locales/xx.json to translate
//
// A key that a locale lacks is added with an empty value. The empty value
// fails the `no_translation_is_empty` test, so an untranslated key cannot
// reach a build unnoticed.

import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = "crates/photrez/locales";
const source = "en";

const args = process.argv.slice(2);
const addIndex = args.indexOf("--add");
const newLocale = addIndex >= 0 ? args[addIndex + 1] : null;

const sourceCatalog = JSON.parse(readFileSync(join(dir, `${source}.json`), "utf8"));
const sourceKeys = Object.keys(sourceCatalog);

const write = (path, entries) => {
  const body = entries.map(([key, value]) => `  ${JSON.stringify(key)}: ${JSON.stringify(value)}`);
  writeFileSync(path, `{\n${body.join(",\n")}\n}\n`, "utf8");
};

if (newLocale) {
  if (!newLocale) {
    console.error("--add needs a locale code, for example --add ja");
    process.exit(1);
  }
  const path = join(dir, `${newLocale}.json`);
  write(path, sourceKeys.map((key) => [key, ""]));
  console.log(`created ${path}: ${sourceKeys.length} keys to translate`);
  process.exit(0);
}

for (const file of readdirSync(dir).filter((name) => name.endsWith(".json") && name !== `${source}.json`)) {
  const path = join(dir, file);
  const catalog = JSON.parse(readFileSync(path, "utf8"));
  const missing = sourceKeys.filter((key) => !(key in catalog));
  const extra = Object.keys(catalog).filter((key) => !(key in sourceCatalog));
  if (missing.length === 0 && extra.length === 0) {
    console.log(`${file}: ${sourceKeys.length} keys, in sync`);
    continue;
  }
  const entries = sourceKeys.map((key) => [key, key in catalog ? catalog[key] : ""]);
  for (const key of extra) entries.push([key, catalog[key]]);
  write(path, entries);
  console.log(`${file}: ${missing.length} keys to translate, ${extra.length} keys not in ${source}.json`);
  for (const key of missing) console.log(`   + ${key}`);
  for (const key of extra) console.log(`   - ${key}`);
}
