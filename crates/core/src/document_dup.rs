// SPDX-License-Identifier: AGPL-3.0-or-later
// Duplicate-name planning for the Rust graph mirror. Parity with the TS
// layerOps.nextDuplicateName / layerFactory.duplicateLayerNode contract.

use crate::document::Layer;

/// Strip a trailing numeric suffix so "Layer 1" -> "Layer", "Background 2"
/// -> "Background", and a pure number "123" -> "". Mirrors layerOps.baseName.
pub(crate) fn base_name(name: &str) -> String {
    let t = name.trim_end();
    // JS \s (in the TS baseline) treats NBSP (U+00A0) as whitespace. We widen
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
/// "Layer 1" -> "Layer 2" -> "Layer 3". Mirrors layerOps.nextDuplicateName.
pub(crate) fn next_duplicate_name(layers: &[Layer], layer_name: &str) -> String {
    let base = base_name(layer_name);
    let prefix = format!("{} ", base);
    let mut max_num: i128 = 1;
    for l in layers {
        if let Some(rest) = l.name.strip_prefix(&prefix) {
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
mod tests {
    use super::*;
    use crate::document::Transform2D;

    fn mk(n: &str) -> Layer {
        Layer {
            id: n.to_string(),
            name: n.to_string(),
            layer_type: "raster".to_string(),
            visible: true,
            locked: false,
            opacity: 1.0,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: false,
            basic_adjustment: None,
            blend_mode: "normal".to_string(),
            transform: Transform2D::default(),
            width: 1,
            height: 1,
            shape_params: None,
            text_data: None,
        }
    }

    fn layers(names: &[&str]) -> Vec<Layer> {
        names.iter().map(|n| mk(n)).collect()
    }

    #[test]
    fn base_name_strips_trailing_number() {
        assert_eq!(base_name("Layer 1"), "Layer");
        assert_eq!(base_name("Background 2"), "Background");
        assert_eq!(base_name("Layer"), "Layer");
        assert_eq!(base_name("123"), "");
    }

    #[test]
    fn base_name_strips_nbsp_after_number() {
        // JS \s treats NBSP (U+00A0) as whitespace; Rust must mirror the strip.
        assert_eq!(base_name("Layer 2\u{00A0}"), "Layer");
    }

    #[test]
    fn next_duplicate_name_sequence() {
        let layers = layers(&["Layer 1", "Layer 2"]);
        assert_eq!(next_duplicate_name(&layers, "Layer 1"), "Layer 3");
        assert_eq!(next_duplicate_name(&layers, "Layer 2"), "Layer 3");
    }

    #[test]
    fn next_duplicate_name_trailing_garbage_counts_as_js_parseint() {
        // TS nextDuplicateName uses parseInt(suffix, 10) which parses leading
        // digits and ignores trailing garbage ("2x" -> 2). Rust must match.
        let layers = layers(&["Layer 2x"]);
        assert_eq!(next_duplicate_name(&layers, "Layer"), "Layer 3");
    }

    #[test]
    fn next_duplicate_name_accepts_sign_like_js_parseint() {
        // JS parseInt accepts a leading '+'/'-' sign.
        let layers = layers(&["Layer +2"]);
        assert_eq!(next_duplicate_name(&layers, "Layer"), "Layer 3");
    }

    #[test]
    fn next_duplicate_name_huge_number_does_not_collapse() {
        // i64::parse overflows on 20-digit numbers; Rust must not silently
        // fall back to "Layer 2" like the old parse::<i64> path did. Mirrors
        // JS parseInt leading-digit parse (Rust is exact via i128 saturating).
        let layers = layers(&["Layer 99999999999999999999"]);
        let dup = next_duplicate_name(&layers, "Layer");
        assert_ne!(dup, "Layer 2");
        let suffix = dup.strip_prefix("Layer ").unwrap();
        assert_eq!(suffix, "100000000000000000000");
    }
}
