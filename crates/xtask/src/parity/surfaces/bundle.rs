// Reading one evidence bundle and saying what is wrong with it: the JSON files inside it,
// the provenance stamps, the captured PNG geometry, and the content labels that tie a diff
// back to the images it claims to compare.

use std::collections::BTreeMap;
use std::path::Path;

use super::*;

pub(super) fn bundle_errors(
    id: &str,
    bundle: &Path,
    oracle_commit: &str,
    build_id: &str,
) -> Vec<String> {
    let mut errors = Vec::new();
    if !bundle.is_dir() {
        return vec![format!(
            "{id}: VERIFIED without an evidence bundle at {}",
            bundle.display()
        )];
    }
    if oracle_commit.is_empty() || build_id.is_empty() {
        errors.push(format!(
            "{id}: VERIFIED requires pinned --oracle-commit and --build-id"
        ));
    }

    if let Ok(entries) = std::fs::read_dir(bundle) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".json")
                && !matches!(
                    name.as_str(),
                    "manifest.json" | "diff.json" | "states.json" | "perf.json"
                )
            {
                errors.push(format!("{id}: unexpected evidence schema `{name}`"));
            }
        }
    }

    let manifest = read_json(bundle, "manifest.json", id, &mut errors);
    let diff = read_json(bundle, "diff.json", id, &mut errors);
    let states = read_json(bundle, "states.json", id, &mut errors);
    let perf = read_json(bundle, "perf.json", id, &mut errors);

    for (name, value) in [
        ("manifest.json", manifest.as_ref()),
        ("diff.json", diff.as_ref()),
        ("states.json", states.as_ref()),
        ("perf.json", perf.as_ref()),
    ] {
        if let Some(value) = value {
            match str_at(value, "surface") {
                Some(surface) if surface == id => {}
                Some(surface) => errors.push(format!("{id}: `{name}` names surface `{surface}`")),
                None => errors.push(format!("{id}: `{name}` lacks a non-empty `surface`")),
            }
        }
    }

    let mut captures: Vec<(&'static str, String)> = Vec::new();
    if let Some(manifest) = &manifest {
        for key in ["oracle_commit", "build_id", "fixture", "locale"] {
            if str_at(manifest, key).is_none() {
                errors.push(format!("{id}: manifest `{key}` is empty or missing"));
            }
        }
        if !num_at(manifest, "dpi").is_some_and(|dpi| dpi > 0.0) {
            errors.push(format!("{id}: manifest `dpi` must be a positive number"));
        }
        let physical = array_at(manifest, "physical_size")
            .and_then(|size| {
                size.iter()
                    .map(|v| v.as_f64())
                    .collect::<Option<Vec<f64>>>()
            })
            .filter(|size| size.len() == 2 && size.iter().all(|v| *v > 0.0 && v.fract() == 0.0));
        if physical.is_none() {
            errors.push(format!(
                "{id}: manifest `physical_size` must be [width, height]"
            ));
        }
        for key in ["oracle_capture", "native_capture"] {
            match str_at(manifest, key) {
                Some(name) => captures.push((key, name.to_string())),
                None => errors.push(format!("{id}: manifest `{key}` is empty or missing")),
            }
        }
        if !same_commit(str_at(manifest, "oracle_commit"), oracle_commit) {
            errors.push(format!(
                "{id}: manifest oracle commit does not identify `{oracle_commit}`"
            ));
        }
        if str_at(manifest, "build_id") != Some(build_id) {
            errors.push(format!("{id}: manifest build is not `{build_id}`"));
        }
        if let Some(size) = physical {
            for (key, name) in &captures {
                let path = bundle.join(name);
                match png_dims(&path) {
                    Some((width, height))
                        if f64::from(width) == size[0] && f64::from(height) == size[1] => {}
                    Some((width, height)) => errors.push(format!(
                        "{id}: `{key}` is {width}x{height}, manifest says {}x{}",
                        size[0], size[1]
                    )),
                    None => errors.push(format!(
                        "{id}: `{key}` is missing, empty or not a valid PNG"
                    )),
                }
            }
        }
    }

    let mut state_ids: Vec<String> = Vec::new();
    if let Some(states) = &states {
        let list = array_at(states, "states").cloned().unwrap_or_default();
        if list.is_empty() {
            errors.push(format!("{id}: `states.json` has no state entries"));
        }
        for entry in &list {
            let state = str_at(entry, "id").unwrap_or("");
            if state.is_empty() {
                errors.push(format!("{id}: `states.json` entry lacks an `id`"));
                continue;
            }
            state_ids.push(state.to_string());
            match str_at(entry, "result") {
                Some("PASS") => {}
                Some(other) => {
                    errors.push(format!("{id}: state `{state}` is `{other}`, expected PASS"))
                }
                None => errors.push(format!("{id}: state `{state}` lacks a `result`")),
            }
            match str_at(entry, "capture") {
                Some(capture) if bundle.join(capture).is_file() => {}
                Some(capture) => errors.push(format!(
                    "{id}: state `{state}` capture `{capture}` does not exist"
                )),
                None => errors.push(format!("{id}: state `{state}` lacks a `capture`")),
            }
            match str_at(entry, "test") {
                Some(test) if interaction_mentions(bundle, test) => {}
                Some(test) => errors.push(format!(
                    "{id}: state `{state}` test `{test}` has no interaction receipt"
                )),
                None => errors.push(format!("{id}: state `{state}` lacks a `test`")),
            }
        }
    }
    match std::fs::read_to_string(bundle.join("interaction.txt")) {
        Ok(text) if text.trim().is_empty() => {
            errors.push(format!("{id}: `interaction.txt` is empty"))
        }
        Ok(text) => {
            for token in ["RED", "GREEN"] {
                if !text.contains(token) {
                    errors.push(format!(
                        "{id}: `interaction.txt` lacks the `{token}` receipt"
                    ));
                }
            }
        }
        Err(_) => errors.push(format!("{id}: `interaction.txt` is missing")),
    }

    if let Some(diff) = &diff {
        if bool_at(diff, "pass") != Some(true) {
            errors.push(format!("{id}: `diff.json` does not record `pass: true`"));
        }
        let thresholds = diff.get("thresholds");
        if !thresholds.is_some_and(|t| {
            num_at(t, "max_changed_pixels").is_some_and(|v| v >= 0.0) && str_at(t, "mask").is_some()
        }) {
            errors.push(format!(
                "{id}: `diff.json` thresholds need max_changed_pixels and mask"
            ));
        }
        if !diff
            .get("metrics")
            .and_then(|m| num_at(m, "changed_pixels"))
            .is_some_and(|v| v >= 0.0)
        {
            errors.push(format!("{id}: `diff.json` metrics.changed_pixels missing"));
        }
        if let (Some(inputs), Some(manifest)) = (diff.get("inputs"), manifest.as_ref()) {
            for (key, capture_key) in [("oracle", "oracle_capture"), ("native", "native_capture")] {
                let expected = str_at(manifest, capture_key)
                    .and_then(|name| content_label(&bundle.join(name)));
                if str_at(inputs, key) != expected.as_deref() {
                    errors.push(format!(
                        "{id}: `diff.json` input `{key}` does not match the capture content"
                    ));
                }
            }
        } else {
            errors.push(format!("{id}: `diff.json` inputs or manifest missing"));
        }
        let diff_states = array_at(diff, "states").cloned().unwrap_or_default();
        if diff_states.is_empty() {
            errors.push(format!("{id}: `diff.json` has no state results"));
        }
        let mut diff_ids: Vec<String> = Vec::new();
        for entry in &diff_states {
            let state = str_at(entry, "id").unwrap_or("");
            if state.is_empty() {
                errors.push(format!("{id}: `diff.json` state lacks an `id`"));
                continue;
            }
            diff_ids.push(state.to_string());
            if bool_at(entry, "pass") != Some(true) {
                errors.push(format!("{id}: diff state `{state}` does not pass"));
            }
        }
        if !state_ids.is_empty() && !diff_ids.is_empty() {
            diff_ids.sort();
            let mut states_sorted = state_ids.clone();
            states_sorted.sort();
            if diff_ids != states_sorted {
                errors.push(format!(
                    "{id}: diff state ids do not match states.json (`{}` vs `{}`)",
                    diff_ids.join(","),
                    states_sorted.join(",")
                ));
            }
        }
    }

    if let Some(perf) = &perf {
        if str_at(perf, "build_id") != Some(build_id) {
            errors.push(format!("{id}: perf build is not `{build_id}`"));
        }
        for key in ["profile", "cold_warm"] {
            if str_at(perf, key).is_none() {
                errors.push(format!("{id}: perf `{key}` is empty or missing"));
            }
        }
        let samples = array_at(perf, "samples")
            .and_then(|samples| {
                samples
                    .iter()
                    .map(|v| v.as_f64())
                    .collect::<Option<Vec<f64>>>()
            })
            .unwrap_or_default();
        if samples.len() < 3 {
            errors.push(format!("{id}: perf needs at least 3 numeric samples"));
        }
        if num_at(perf, "n") != Some(samples.len() as f64) {
            errors.push(format!("{id}: perf `n` must equal the sample count"));
        }
        let (p50, p95, budget) = (
            num_at(perf, "p50"),
            num_at(perf, "p95"),
            num_at(perf, "budget"),
        );
        match (p50, p95, budget) {
            (Some(p50), Some(p95), Some(budget)) if budget > 0.0 => {
                if p50 > p95 {
                    errors.push(format!("{id}: perf p50 is above p95"));
                }
                if p95 > budget {
                    errors.push(format!("{id}: perf p95 exceeds the adopted budget"));
                }
            }
            _ => errors.push(format!(
                "{id}: perf needs numeric p50, p95 and a positive budget"
            )),
        }
        if bool_at(perf, "pass") != Some(true) {
            errors.push(format!("{id}: perf does not record `pass: true`"));
        }
    }

    let provenance_path = bundle.join("provenance.txt");
    match provenance_map(&provenance_path) {
        Ok(provenance) => {
            for key in [
                "oracle",
                "build",
                "fixture",
                "catalog",
                "input",
                "native-rev",
                "tool",
            ] {
                if !provenance.contains_key(key) {
                    errors.push(format!("{id}: provenance lacks `{key}`"));
                }
            }
            if !same_commit(provenance.get("oracle").map(String::as_str), oracle_commit) {
                errors.push(format!(
                    "{id}: bundle provenance does not identify oracle commit `{oracle_commit}`"
                ));
            }
            if provenance.get("build").map(String::as_str) != Some(build_id) {
                errors.push(format!(
                    "{id}: bundle provenance does not record exactly `build: {build_id}`"
                ));
            }
            if let Some(manifest) = &manifest {
                for (manifest_key, provenance_key) in [
                    ("oracle_commit", "oracle"),
                    ("build_id", "build"),
                    ("fixture", "fixture"),
                ] {
                    if str_at(manifest, manifest_key)
                        != provenance.get(provenance_key).map(String::as_str)
                    {
                        errors.push(format!(
                            "{id}: manifest `{manifest_key}` disagrees with provenance `{provenance_key}`"
                        ));
                    }
                }
            }
        }
        Err(error) => errors.push(format!("{id}: {error}")),
    }

    errors
}

fn read_json(
    bundle: &Path,
    name: &str,
    id: &str,
    errors: &mut Vec<String>,
) -> Option<serde_json::Value> {
    let path = bundle.join(name);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            errors.push(format!("{id}: `{name}` is missing or unreadable"));
            return None;
        }
    };
    if text.trim().is_empty() {
        errors.push(format!("{id}: `{name}` is empty"));
        return None;
    }
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("{id}: `{name}` is not valid JSON: {error}"));
            None
        }
    }
}

fn str_at<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn num_at(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(serde_json::Value::as_f64)
}

fn bool_at(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_json::Value::as_bool)
}

fn array_at<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a Vec<serde_json::Value>> {
    value.get(key).and_then(serde_json::Value::as_array)
}

fn interaction_mentions(bundle: &Path, test: &str) -> bool {
    std::fs::read_to_string(bundle.join("interaction.txt"))
        .map(|text| text.contains(test))
        .unwrap_or(false)
}

fn provenance_map(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| "`provenance.txt` is missing or unreadable".to_string())?;
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("provenance line is not `key: value`: {line}"))?;
        let (key, value) = (key.trim().to_string(), value.trim().to_string());
        if key.is_empty() || value.is_empty() {
            return Err(format!(
                "provenance line `{line}` has an empty key or value"
            ));
        }
        if let Some(previous) = map.insert(key.clone(), value.clone()) {
            if previous != value {
                return Err(format!(
                    "provenance `{key}` is recorded twice with different values"
                ));
            }
        }
    }
    Ok(map)
}

fn content_label(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| content_label_bytes(&bytes))
}

pub(super) fn content_label_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv64:{hash:016x}")
}

fn png_dims(path: &Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width > 0 && height > 0).then_some((width, height))
}
