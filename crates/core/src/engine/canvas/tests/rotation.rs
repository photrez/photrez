// SPDX-License-Identifier: AGPL-3.0-or-later
// Rotation normalization pins for the Apply Crop arm.

use super::super::normalize_rotation;

#[test]
fn normalize_rotation_wraps_negative_dividend() {
    // -270 % 360 in f64 is -270, then < -180 -> +360 == 90.
    assert_eq!(normalize_rotation(-270.0), 90.0);
}
#[test]
fn normalize_rotation_full_turn_is_zero() {
    assert_eq!(normalize_rotation(360.0), 0.0);
}
#[test]
fn normalize_rotation_passthrough_in_range() {
    assert_eq!(normalize_rotation(45.0), 45.0);
}
#[test]
fn normalize_rotation_above_180_wraps_negative() {
    assert_eq!(normalize_rotation(181.0), -179.0);
}
