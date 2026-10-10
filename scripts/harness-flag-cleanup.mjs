/*
 * PHOTREZ_FLAGS -> the localStorage entries a live runner seeds before boot.
 *
 * Every runner shares this one parser, so the mapping lives in a single place
 * instead of at three call sites that could drift apart. Values are passed
 * through as written; this module does not decide which keys the product
 * still reads (the retired runtime flags - the history bridge and the old
 * paint kill switch - are refused by nothing here and are simply inert).
 */

const warn = (msg) => console.error(`WARN: ${msg}`);

export function parseHarnessFlags(raw) {
  return String(raw || "")
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .flatMap((kv) => {
      const i = kv.indexOf("=");
      if (i < 0) return [];
      const k = kv.slice(0, i).trim();
      const v = kv.slice(i + 1).trim();
      if (!k) return [];
      return [{ key: `photrez.${k}`, value: v }];
    });
}
