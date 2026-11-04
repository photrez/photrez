//! Class-map identity: how many pixels differ, and how many of those change class.
//!
//! The percentage gate answers "how much moved"; the class map answers "did anything
//! change role". A surface can pass on colour and still have text pixels painted where
//! a rule belongs, so the three class-map bundles (`S053`, `S055`, `S056`) record their
//! verdict as a class-mismatch count. This module recomputes exactly that number from
//! two PNGs, so a recorded class-map verdict can be checked by a later run.
//!
//! The pixel rule here is deliberately different from `rawdiff`'s: a pixel counts as
//! differing when any RGB channel differs at all, not only past `rawdiff::THRESHOLD`.
//! That is the rule the recorded counts were produced under, and the difference matters:
//! on the `S053` tool rail 7,116 pixels differ at all while only 2,134 exceed the
//! raw-diff tolerance.

use serde::Serialize;

use super::classify::{classify, CLASS_ORDER};
use super::pngio::RgbaImage;

/// One class's share of the class-mismatch pixels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClassCount {
    pub class: String,
    pub pixels: u64,
}

/// The class-map verdict for one pair of same-sized images.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClassReport {
    pub width: u32,
    pub height: u32,
    pub total_pixels: u64,
    pub differing_pixels: u64,
    pub class_mismatch_pixels: u64,
    pub class_mismatch_pct: f64,
    pub by_class: Vec<ClassCount>,
}

/// Classify one pair of same-sized images. A size mismatch is an error, not a verdict:
/// a class comparison only means something when both images cover the same region.
pub fn report(a: &RgbaImage, b: &RgbaImage) -> Result<ClassReport, String> {
    if a.width != b.width || a.height != b.height {
        return Err(format!(
            "class diff: size mismatch: {}x{} vs {}x{}",
            a.width, a.height, b.width, b.height
        ));
    }
    let mut differing = 0u64;
    let mut mismatch = 0u64;
    let mut counts = [0u64; CLASS_ORDER.len()];
    for y in 0..a.height {
        for x in 0..a.width {
            let i = ((y as usize) * (a.width as usize) + (x as usize)) * 4;
            let j = ((y as usize) * (b.width as usize) + (x as usize)) * 4;
            let delta = a.data[i]
                .abs_diff(b.data[j])
                .max(a.data[i + 1].abs_diff(b.data[j + 1]))
                .max(a.data[i + 2].abs_diff(b.data[j + 2]));
            if delta == 0 {
                continue;
            }
            differing += 1;
            let ca = classify(a.data[i], a.data[i + 1], a.data[i + 2]);
            let cb = classify(b.data[j], b.data[j + 1], b.data[j + 2]);
            if ca != cb {
                mismatch += 1;
                if let Some(slot) = CLASS_ORDER.iter().position(|k| *k == ca) {
                    counts[slot] += 1;
                }
            }
        }
    }
    let total = u64::from(a.width) * u64::from(a.height);
    Ok(ClassReport {
        width: a.width,
        height: a.height,
        total_pixels: total,
        differing_pixels: differing,
        class_mismatch_pixels: mismatch,
        class_mismatch_pct: super::round3(mismatch as f64 / total as f64 * 100.0),
        by_class: CLASS_ORDER
            .iter()
            .zip(counts)
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| ClassCount {
                class: k.as_str().to_string(),
                pixels: n,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[(u8, u8, u8)]) -> RgbaImage {
        let mut data = Vec::with_capacity(pixels.len() * 4);
        for (r, g, b) in pixels {
            data.extend_from_slice(&[*r, *g, *b, 255]);
        }
        RgbaImage {
            width: pixels.len() as u32,
            height: 1,
            data,
        }
    }

    #[test]
    fn identical_images_have_nothing_to_report() {
        let grey = image(&[(0x1B, 0x1B, 0x1B), (0x7A, 0x7A, 0x7A)]);
        let report = report(&grey, &grey).expect("same size");
        assert_eq!(report.differing_pixels, 0);
        assert_eq!(report.class_mismatch_pixels, 0);
        assert!((report.class_mismatch_pct - 0.0).abs() < f64::EPSILON);
        assert!(report.by_class.is_empty());
    }

    #[test]
    fn a_within_class_change_is_not_a_mismatch() {
        // 0x1B and 0x1A are both below luma 34, so both classify as the background.
        let report = report(&image(&[(0x1B, 0x1B, 0x1B)]), &image(&[(0x1A, 0x1A, 0x1A)]))
            .expect("same size");
        assert_eq!(report.differing_pixels, 1);
        assert_eq!(report.class_mismatch_pixels, 0);
    }

    #[test]
    fn a_cross_class_change_is_counted_and_named_by_the_first_class() {
        let report = report(
            &image(&[(0x1B, 0x1B, 0x1B), (0xE1, 0x5A, 0x17)]),
            &image(&[(0x7A, 0x7A, 0x7A), (0x7A, 0x7A, 0x7A)]),
        )
        .expect("same size");
        assert_eq!(report.differing_pixels, 2);
        assert_eq!(report.class_mismatch_pixels, 2);
        assert!((report.class_mismatch_pct - 100.0).abs() < f64::EPSILON);
        assert_eq!(
            report.by_class,
            vec![
                ClassCount {
                    class: "bg".to_string(),
                    pixels: 1
                },
                ClassCount {
                    class: "orange".to_string(),
                    pixels: 1
                },
            ]
        );
    }

    #[test]
    fn a_size_mismatch_is_refused_rather_than_scored() {
        let error = report(&image(&[(0, 0, 0)]), &image(&[(0, 0, 0), (0, 0, 0)]))
            .expect_err("different width");
        assert!(error.contains("size mismatch"), "{error}");
    }
}
