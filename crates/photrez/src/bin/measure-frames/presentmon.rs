//! PresentMon CSV summary for the displayed-interval gate.
//!
//! `MsBetweenPresents` is the application submit interval (the benchmark's
//! `frame_p95_ms`); `MsBetweenDisplayChange` is the interval the user actually
//! sees. PresentMon owns the display timeline, so the bench consumes its CSV
//! rather than inventing a display timestamp.
//!
//! The parser is deliberately strict about its inputs: a missing column or a
//! CSV with no usable rows is a defined error, never a `0.0` that could pass a
//! budget. Startup and full-upload rows above 100 ms are counted as skipped,
//! not silently dropped; `NA` rows are skipped too.

use std::path::Path;

/// The displayed-interval distribution from one PresentMon capture.
#[derive(Debug, PartialEq)]
pub struct DisplaySummary {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub frames_ge_33: usize,
    pub sample_count: usize,
    pub skipped: usize,
}

/// Why a PresentMon CSV could not be summarized.
#[derive(Debug)]
pub enum PresentmonError {
    Io(std::io::Error),
    Header(String),
    NoRows,
}

impl std::fmt::Display for PresentmonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read the PresentMon CSV: {error}"),
            Self::Header(message) => write!(f, "invalid PresentMon CSV header: {message}"),
            Self::NoRows => write!(f, "no usable measure-frames.exe rows in the PresentMon CSV"),
        }
    }
}

impl std::error::Error for PresentmonError {}

/// Reads and summarizes one PresentMon CSV. Rows whose `TimeInMs` is below
/// `skip_ms` are skipped; the opening/full-upload sequence must not be charged
/// to the stroke target.
pub fn summarize_csv(path: &Path, skip_ms: f64) -> Result<DisplaySummary, PresentmonError> {
    let csv = std::fs::read_to_string(path).map_err(PresentmonError::Io)?;
    summarize_csv_str(&csv, skip_ms)
}

/// Summarizes one PresentMon CSV already in memory.
pub fn summarize_csv_str(csv: &str, skip_ms: f64) -> Result<DisplaySummary, PresentmonError> {
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| PresentmonError::Header("empty PresentMon CSV".to_string()))?;
    // PresentMon writes a UTF-8 BOM before `Application`; without this the
    // first column name never matches and a valid capture is rejected.
    let columns: Vec<&str> = header
        .trim_start_matches('\u{feff}')
        .split(',')
        .map(str::trim)
        .collect();
    let application = columns
        .iter()
        .position(|column| *column == "Application")
        .ok_or_else(|| PresentmonError::Header("missing Application column".to_string()))?;
    let time = columns
        .iter()
        .position(|column| *column == "TimeInMs")
        .ok_or_else(|| PresentmonError::Header("missing TimeInMs column".to_string()))?;
    let display = columns
        .iter()
        .position(|column| *column == "MsBetweenDisplayChange")
        .ok_or_else(|| {
            PresentmonError::Header("missing MsBetweenDisplayChange column".to_string())
        })?;

    let mut values = Vec::new();
    let mut skipped = 0;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(application_value) = fields.get(application) else {
            skipped += 1;
            continue;
        };
        if !application_value.ends_with("measure-frames.exe") {
            skipped += 1;
            continue;
        }
        let before_skip = match fields.get(time).and_then(|raw| raw.parse::<f64>().ok()) {
            Some(value) if value.is_finite() => value < skip_ms,
            // A row without a usable time cannot be placed on the displayed
            // timeline; refusing it is safer than guessing.
            _ => true,
        };
        if before_skip {
            skipped += 1;
            continue;
        }
        let Some(raw) = fields.get(display) else {
            skipped += 1;
            continue;
        };
        if raw.eq_ignore_ascii_case("NA") {
            skipped += 1;
            continue;
        }
        match raw.parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 && value <= 100.0 => values.push(value),
            _ => skipped += 1,
        }
    }

    if values.is_empty() {
        return Err(PresentmonError::NoRows);
    }
    values.sort_by(f64::total_cmp);
    let nearest = |fraction: f64| -> f64 {
        let index = ((values.len() as f64) * fraction).ceil() as usize;
        values[index.clamp(1, values.len()) - 1]
    };
    Ok(DisplaySummary {
        p50_ms: nearest(0.50),
        p95_ms: nearest(0.95),
        frames_ge_33: values.iter().filter(|&&value| value >= 33.0).count(),
        sample_count: values.len(),
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        "Application,ProcessID,TimeInMs,MsBetweenPresents,MsBetweenDisplayChange\n\
measure-frames.exe,1,3000.0,57.4,33.27\n\
measure-frames.exe,1,3100.0,17.6,16.9\n\
measure-frames.exe,1,3200.0,17.6,16.8\n\
measure-frames.exe,1,3300.0,17.6,33.4\n\
measure-frames.exe,1,3400.0,17.6,NA\n\
measure-frames.exe,1,3500.0,17.6,150.0\n";

    #[test]
    fn display_summary_uses_display_change_and_reports_skips() {
        let summary = summarize_csv_str(FIXTURE, 3050.0).expect("valid fixture");
        assert_eq!(summary.sample_count, 3); // 16.9, 16.8, 33.4
        assert_eq!(summary.frames_ge_33, 1);
        assert_eq!(summary.skipped, 3); // opening row + NA + 150
        assert!((summary.p50_ms - 16.9).abs() < 0.001);
        assert!((summary.p95_ms - 33.4).abs() < 0.001);
    }

    #[test]
    fn skip_ms_zero_keeps_the_opening_row() {
        let summary = summarize_csv_str(FIXTURE, 0.0).expect("valid fixture");
        assert_eq!(summary.sample_count, 4); // opening + 16.9 + 16.8 + 33.4
        assert_eq!(summary.frames_ge_33, 2); // opening + the 33.4 stroke
        assert!((summary.p95_ms - 33.4).abs() < 0.001);
    }

    #[test]
    fn utf8_bom_before_the_header_is_ignored() {
        let csv =
            "\u{feff}Application,ProcessID,TimeInMs,MsBetweenPresents,MsBetweenDisplayChange\n\
measure-frames.exe,1,3100.0,17.6,16.7\n";
        let summary = summarize_csv_str(csv, 0.0).expect("BOM header");
        assert_eq!(summary.sample_count, 1);
        assert!((summary.p95_ms - 16.7).abs() < 0.001);
    }

    #[test]
    fn missing_display_column_is_a_defined_error() {
        let err = summarize_csv_str(
            "Application,TimeInMs,MsBetweenPresents\nmeasure-frames.exe,1000.0,17.6\n",
            0.0,
        )
        .expect_err("missing display column");
        assert!(matches!(err, PresentmonError::Header(_)));
    }
}
