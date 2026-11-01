// SPDX-License-Identifier: AGPL-3.0-or-later
// Duplicate-name planning for the engine's layer graph: the numeric-suffix
// next-free-name contract used by the DuplicateLayer arm and structural clone.

/// Strip a trailing numeric suffix so "Layer 1" -> "Layer", "Background 2"
/// -> "Background", and a pure number "123" -> "". Feeds next_duplicate_name.
pub(crate) fn base_name(name: &str) -> String {
    let t = name.trim_end();
    // JS \s (in the previous build) treats NBSP (U+00A0) as whitespace. We widen
    // the trailing strip to ASCII whitespace + NBSP; exotic Unicode spaces
    // beyond NBSP are a deliberate ceiling (full JS \s set not matched).
    let is_ws = |c: char| c.is_ascii_whitespace() || c == '\u{00A0}';
    let chars: Vec<char> = t.chars().collect();
    let mut i = chars.len();
    while i > 0 && chars[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i < chars.len() {
        let mut j = i;
        while j > 0 && is_ws(chars[j - 1]) {
            j -= 1;
        }
        if j == 0 {
            return String::new();
        }
        let mut k = j;
        while k > 0 && is_ws(chars[k - 1]) {
            k -= 1;
        }
        return chars[..k].iter().collect();
    }
    t.to_string()
}

/// Parse leading decimal digits like JS `parseInt(x, 10)`: skip leading ASCII
/// whitespace, optional sign, consume leading ASCII digits, ignore the rest.
/// Returns None when there are no leading digits (JS returns NaN). Uses i128 so
/// huge inputs do not silently fail the way Rust `i64::parse` would on overflow.
fn js_parse_int(s: &str) -> Option<i128> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut negative = false;
    if i < bytes.len() {
        if bytes[i] == b'-' {
            negative = true;
            i += 1;
        } else if bytes[i] == b'+' {
            i += 1;
        }
    }
    let mut value: i128 = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add((bytes[i] - b'0') as i128);
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// Next duplicate name: increment a numeric suffix past any existing sibling.
/// "Layer 1" -> "Layer 2" -> "Layer 3".
// Accepts borrowed name slices so BOTH the document graph (document.rs)
// and the engine structural arms (document/structural.rs) share one
// algorithm - no fork of the numeric-suffix rule.
pub(crate) fn next_duplicate_name(layers: &[&str], layer_name: &str) -> String {
    let base = base_name(layer_name);
    let prefix = format!("{} ", base);
    let mut max_num: i128 = 1;
    for name in layers {
        if let Some(rest) = name.strip_prefix(&prefix) {
            if let Some(num) = js_parse_int(rest) {
                if num > max_num {
                    max_num = num;
                }
            }
        }
    }
    format!("{} {}", base, max_num + 1)
}

#[cfg(test)]
mod tests;
