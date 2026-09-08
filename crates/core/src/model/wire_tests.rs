// Wire-compat guards for the extended RenderLayer schema (sibling of the
// moved model tests; kept separate to stay under the file-size guard).
use super::*;

#[test]
fn old_shape_envelope_parses_with_all_extended_fields_none() {
    // A pre-extension payload (exactly the 11 original fields) must still
    // deserialize; every extended field defaults to None so old senders
    // keep working within the same build.
    let json = r#"{"id":"L1","name":"Old","visible":true,"opacity":0.5,"resourceId":4,"x":1.0,"y":2.0,"scaleX":1.0,"scaleY":1.0,"rotation":0.0,"dirtyRect":null}"#;
    let layer: RenderLayer = serde_json::from_str(json).expect("old shape parses");
    assert_eq!(layer.id, "L1");
    assert_eq!(layer.resource_id, 4);
    assert!(layer.layer_type.is_none());
    assert!(layer.blend_mode.is_none());
    assert!(layer.locked.is_none());
    assert!(layer.lock_transparency.is_none());
    assert!(layer.lock_position.is_none());
    assert!(layer.lock_rotation.is_none());
    assert!(layer.is_background.is_none());
    assert!(layer.has_adjustments.is_none());
    assert!(layer.width.is_none());
    assert!(layer.height.is_none());
}

#[test]
fn extended_fields_round_trip_when_present() {
    let layer = RenderLayer {
        id: "L2".into(),
        name: "Ext".into(),
        visible: true,
        opacity: 1.0,
        resource_id: 7,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        layer_type: Some(crate::canonical_model::LayerType::Text),
        blend_mode: Some(crate::canonical_model::BlendMode::Multiply),
        locked: Some(true),
        width: Some(64.0),
        height: Some(32.0),
        ..Default::default()
    };
    let json = serde_json::to_string(&layer).expect("serializes");
    // Absent Options are skipped; present ones carry camelCase keys.
    assert!(json.contains("\"layerType\":\"text\""));
    assert!(json.contains("\"blendMode\":\"multiply\""));
    assert!(!json.contains("lockTransparency"));
    let back: RenderLayer = serde_json::from_str(&json).expect("parses back");
    assert_eq!(back, layer, "value-identical round trip");
}
