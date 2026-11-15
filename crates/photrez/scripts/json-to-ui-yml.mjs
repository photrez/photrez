// One-shot: merge en/id/bjn JSON into locales/ui.yml (_version: 2).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const assetDir = path.join(here, "..", "assets", "i18n");
const outPath = path.join(here, "..", "locales", "ui.yml");

const locales = ["en", "id", "bjn"];
const catalogs = Object.fromEntries(
  locales.map((loc) => [
    loc,
    JSON.parse(fs.readFileSync(path.join(assetDir, `${loc}.json`), "utf8")),
  ]),
);

function convertPlaceholders(text) {
  return text.replaceAll(/\{\{(\s*[^}]+\s*)\}\}/g, (_, name) => `%{${name.trim()}}`);
}

function mergeNode(enNode, paths) {
  if (typeof enNode === "string") {
    const leaf = { en: convertPlaceholders(enNode) };
    for (const loc of ["id", "bjn"]) {
      let node = catalogs[loc];
      for (const seg of paths) {
        node = node?.[seg];
      }
      if (typeof node !== "string") {
        throw new Error(`missing ${loc} at ${paths.join(".")}`);
      }
      leaf[loc] = convertPlaceholders(node);
    }
    return leaf;
  }
  const out = {};
  for (const key of Object.keys(enNode).sort()) {
    out[key] = mergeNode(enNode[key], [...paths, key]);
  }
  return out;
}

const tree = { _version: 2, ...mergeNode(catalogs.en, []) };
fs.mkdirSync(path.dirname(outPath), { recursive: true });

function isLocaleLeaf(value) {
  return (
    value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    Object.keys(value).every((k) => locales.includes(k)) &&
    Object.values(value).every((v) => typeof v === "string")
  );
}

function emitYaml(value, indent = 0) {
  const pad = "  ".repeat(indent);
  if (typeof value === "string") {
    const escaped = value
      .replaceAll("\\", "\\\\")
      .replaceAll('"', '\\"')
      .replaceAll("\n", "\\n");
    return `"${escaped}"`;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("unexpected yaml value");
  }
  const lines = [];
  for (const [key, child] of Object.entries(value)) {
    if (typeof child === "string" || typeof child === "number") {
      lines.push(`${pad}${key}: ${emitYaml(String(child), indent)}`);
    } else if (isLocaleLeaf(child)) {
      lines.push(`${pad}${key}:`);
      for (const loc of locales) {
        lines.push(`${pad}  ${loc}: ${emitYaml(child[loc], indent + 1)}`);
      }
    } else {
      lines.push(`${pad}${key}:`);
      lines.push(emitYaml(child, indent + 1));
    }
  }
  return lines.join("\n");
}

fs.writeFileSync(outPath, emitYaml(tree) + "\n", "utf8");
console.log(`wrote ${outPath}`);
