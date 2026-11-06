// SPDX-License-Identifier: AGPL-3.0-or-later
// Seed-payload validation tests: the parse contract every canonical push must
// meet.
use crate::validate::validate_payload;

const VALID_DOC: &str = r##"{
    "id":"v-1",
    "name":"Valid",
    "width":800,
    "height":600,
    "layers":[
        {
            "id":"L1","name":"T","type":"text","visible":true,"opacity":1,"locked":false,
            "blendMode":"normal",
            "transform":{"x":1,"y":2,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
            "width":100,"height":20,
            "textData":{"content":"Hi","fontFamily":"Arial","fontSize":32,"fontWeight":400,"fontStyle":"normal","color":"#000000","align":"left","lineHeight":1.2,"letterSpacing":0,"boxMode":"point","boxWidth":0,"boxHeight":0,"stroke":{"width":0,"color":"#000000","align":"outside"},"underline":false,"strikethrough":false,"uppercase":false}
        }
    ]
}"##;

#[test]
fn valid_payload_returns_empty_string() {
    // Returns "" (ok) - the contract every canonical push must satisfy.
    assert_eq!(validate_payload(VALID_DOC), "");
}

#[test]
fn malformed_payload_returns_error_string() {
    let err = validate_payload("{ not valid json");
    assert!(!err.is_empty(), "malformed json must surface a serde error");
}
