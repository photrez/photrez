// SPDX-License-Identifier: AGPL-3.0-or-later
// Engine version accessor pins.

use super::*;

#[test]
fn version_returns_seeded_engine_version() {
    let mut engine = DocumentCore::new();
    let layer = LayerMeta {
        id: "L1".to_string(),
        name: "Base".to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: 1,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        ..Default::default()
    };
    engine.seed_layers(vec![layer], 42);
    assert_eq!(engine.version(), 42);
}
