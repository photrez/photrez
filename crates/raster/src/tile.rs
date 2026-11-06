// SPDX-License-Identifier: AGPL-3.0-or-later
//! The tile patch DTO: one absolute tile region of RGBA pixels (the changed
//! region only). Leaf type - it depends on serde alone.

use serde::{Deserialize, Serialize};

/// One absolute tile region of RGBA pixels (the changed region only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TilePatch {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}
