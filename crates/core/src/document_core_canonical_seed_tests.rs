use super::*;
use crate::canonical_model::CanonicalDocument;

/// Minimal `CanonicalDocument` (no layers) keyed by id, for shadow-copy tests.
fn empty_doc(id: &str) -> CanonicalDocument {
    CanonicalDocument {
        id: id.to_string(),
        name: "n".to_string(),
        width: 10.0,
        height: 10.0,
        layers: vec![],
        selection: None,
    }
}

#[test]
fn canonical_default_is_none() {
    let engine = ProtocolEngine::new();
    assert!(engine.canonical().is_none());
}

#[test]
fn seed_canonical_stores_copy() {
    let mut engine = ProtocolEngine::new();
    engine.seed_canonical(empty_doc("doc-A"));
    let c = engine.canonical().expect("canonical shadow stored");
    assert_eq!(c.id, "doc-A");
}

#[test]
fn seed_canonical_replaces_existing() {
    let mut engine = ProtocolEngine::new();
    engine.seed_canonical(empty_doc("doc-A"));
    engine.seed_canonical(empty_doc("doc-B"));
    let c = engine.canonical().expect("canonical shadow stored");
    assert_eq!(c.id, "doc-B", "second seed must replace the first shadow");
}
