// SPDX-License-Identifier: AGPL-3.0-or-later
// Selection for DocumentEngine — mirrors TS SelectionState

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub shape: Option<String>,
    pub inverted: Option<bool>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            angle: 0.0,
            shape: None,
            inverted: None,
        }
    }
}
