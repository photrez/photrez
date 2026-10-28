//! Minimal RFC 4180 CSV reader/writer.
//!
//! The frozen inventory carries quoted fields with embedded commas (for
//! example a native target naming two files), so a naive split-on-comma
//! corrupts rows. One small parser shared by every parity validator removes
//! that class of failure; adding a CSV crate for this was judged a worse
//! trade than the ~80 lines below.

use std::collections::HashMap;

/// Parse CSV text into rows of fields. Lines beginning with `#` before the
/// header are treated as comments and skipped; a `"` inside a quoted field is
/// escaped by doubling. Returns the header row and the data rows.
pub fn parse(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut field_started = false;
    for line in text.lines() {
        let mut chars = line.chars().peekable();
        let is_comment = !in_quotes
            && record.is_empty()
            && field.is_empty()
            && !field_started
            && line.trim_start().starts_with('#');
        if is_comment {
            continue;
        }
        loop {
            match chars.next() {
                None => break,
                Some('"') if !in_quotes && field.is_empty() && !field_started => {
                    in_quotes = true;
                    field_started = true;
                }
                Some('"') if in_quotes => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                Some(',') if !in_quotes => {
                    record.push(std::mem::take(&mut field));
                    field_started = false;
                }
                Some(c) => {
                    field.push(c);
                    field_started = true;
                }
            }
        }
        if in_quotes {
            // A quoted field spanning lines: keep reading.
            field.push('\n');
            continue;
        }
        record.push(std::mem::take(&mut field));
        field_started = false;
        records.push(std::mem::take(&mut record));
    }
    if in_quotes {
        return Err("unterminated quoted field".to_string());
    }
    if records.is_empty() {
        return Err("no header row".to_string());
    }
    let header = records.remove(0);
    Ok((header, records))
}

/// Rows as `header -> index` keyed maps, one per data row.
pub fn records_as_maps(
    header: &[String],
    rows: &[Vec<String>],
) -> Result<Vec<HashMap<String, String>>, String> {
    rows.iter()
        .map(|row| {
            if row.len() != header.len() {
                return Err(format!(
                    "row has {} fields, header has {}",
                    row.len(),
                    header.len()
                ));
            }
            Ok(header
                .iter()
                .cloned()
                .zip(row.iter().cloned())
                .collect::<HashMap<String, String>>())
        })
        .collect()
}

/// Escape one CSV field: quote only when required.
pub fn escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Serialize a full CSV document from a header and rows of fields.
pub fn write(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(
        &header
            .iter()
            .map(|h| escape(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        out.push_str(&row.iter().map(|f| escape(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}
