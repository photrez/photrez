import { readFileSync, writeFileSync } from "fs";

const newVersion = process.argv[2];
if (!newVersion) {
  console.error("Usage: bun run version:bump <version>");
  console.error("Example: bun run version:bump 0.1.0-alpha.2");
  process.exit(1);
}

// Validate SemVer pattern (MAJOR.MINOR.PATCH[-stage.N][+build])
const semverPattern = /^\d+\.\d+\.\d+(-[a-z0-9]+(\.[a-z0-9]+)*)?(\+[a-z0-9.]+)?$/i;
if (!semverPattern.test(newVersion)) {
  console.error(`Invalid version format: ${newVersion}`);
  console.error("Expected: MAJOR.MINOR.PATCH[-stage.N]");
  process.exit(1);
}

// Root package.json has no "version" field — only bump files that carry it.
const files = [
  "apps/desktop/package.json",
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/tauri.conf.json",
];

for (const f of files) {
  let content = readFileSync(f, "utf-8");
  let oldVersion: string;
  if (f.endsWith(".json")) {
    const json = JSON.parse(content);
    if (!json.version) {
      console.error(`No "version" field in ${f}`);
      process.exit(1);
    }
    oldVersion = json.version;
    json.version = newVersion;
    content = JSON.stringify(json, null, 2) + "\n";
  } else {
    const match = content.match(/^version = "([^"]+)"/m);
    if (!match) {
      console.error(`No version field in ${f}`);
      process.exit(1);
    }
    oldVersion = match[1];
    content = content.replace(/^version = ".*"/m, `version = "${newVersion}"`);
  }
  writeFileSync(f, content);
  console.log(`✓ ${f}: ${oldVersion} → ${newVersion}`);
}

console.log(`\n✓ All version files bumped to ${newVersion}`);
console.log(`Next steps:`);
console.log(`  1. Update CHANGELOG.md with new section`);
console.log(`  2. Update KNOWN_ISSUES.md if needed`);
console.log(`  3. git add -A && git commit -m "chore(release): v${newVersion}"`);
console.log(`  4. git tag v${newVersion}`);
console.log(`  5. git push origin main --tags`);
