// SPDX-License-Identifier: AGPL-3.0-or-later
// Duplicate-name planning for the engine's layer graph: the numbered
// copy-suffix contract used by the DuplicateLayer arm and structural clone.

/// Strip one trailing copy suffix so "Layer 1 copy" and "Layer 1 copy 2"
/// both map back to "Layer 1". Any other name is its own stem.
fn copy_stem(name: &str) -> &str {
    let trimmed = name.trim_end();
    if let Some(stripped) = trimmed.strip_suffix(" copy") {
        return stripped;
    }
    if let Some(pos) = trimmed.rfind(" copy ") {
        let suffix = &trimmed[pos + " copy ".len()..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return &trimmed[..pos];
        }
    }
    trimmed
}

/// Next duplicate name: "{stem} copy", then "{stem} copy 2", and so on.
/// "Layer 1" -> "Layer 1 copy" -> "Layer 1 copy 2".
pub(crate) fn next_duplicate_name(layers: &[&str], layer_name: &str) -> String {
    let stem = copy_stem(layer_name);
    let first = format!("{stem} copy");
    if !layers.contains(&first.as_str()) {
        return first;
    }
    let mut number = 2;
    loop {
        let candidate = format!("{stem} copy {number}");
        if !layers.contains(&candidate.as_str()) {
            return candidate;
        }
        number += 1;
    }
}

#[cfg(test)]
mod tests;
