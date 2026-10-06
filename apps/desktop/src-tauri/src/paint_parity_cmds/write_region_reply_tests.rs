//! The `rust_pixels_write_region` REPLY shape, at the wire.
//!
//! Split out of `paint_parity_cmds.rs` because that file was already over the
//! 1000-line guard and this block is 80-odd lines on top of it. The command
//! itself stays where it is; only these tests moved, which is the same move
//! `crates/core/src/pixel_store.rs` makes for its `composite_destination_seed_tests`.
//!
//! The single fact pinned here: the reply carries the POST-image and no
//! pre-image. Every stroke's reply crosses the process boundary, and the
//! pre-image was a whole dirty region's worth of base64 in it - measured at
//! 4,545,641 characters removed per 4096x4096 stroke. Nothing could read it:
//! the write had already appended the pre-image to Rust's own pixel history, so
//! undo returns those exact bytes from there, and the host's brush memento is
//! built from tiles the host snapshotted off its own surface.
use super::*;

/// The reply serializes the post-image and NOTHING else.
///
/// Asserted on the SERIALIZED form, not the struct: the cost being removed is
/// bytes on the wire, so a reply that merely stopped naming the field in Rust
/// while still serializing it would pass a struct-shape test and keep the cost.
///
/// The lock is the process-global registry lock, which every test touching
/// `registry()` must hold: `reset()` sets the global to `None`, so a parallel
/// sibling would otherwise wipe this test's documents mid-run. `reset()` at the
/// top also means a document leaked by an earlier panicking test is cleared here
/// rather than inherited.
#[test]
fn write_region_reply_serializes_the_post_image_and_no_pre_image() {
    let _g = super::TEST_REGISTRY_LOCK.lock().unwrap();
    *registry() = None;
    let doc = "doc-wr-shape".to_string();
    let layer = "L".to_string();
    rust_pixels_open_document(doc.clone());
    seed_layer_bytes(doc.clone(), layer.clone(), 4, 4, vec![7u8; 4 * 4 * 4]).unwrap();

    let res = rust_pixels_write_region(
        doc.clone(),
        layer.clone(),
        0,
        0,
        2,
        2,
        b64(&[9u8; 2 * 2 * 4]),
    )
    .expect("write_region");
    let json = serde_json::to_value(&res).expect("the reply serializes");
    assert!(
        json.get("before").is_none(),
        "the reply must not carry a pre-image, but it serialized one: {json}"
    );
    assert_eq!(
        json["after"].as_array().map(|a| a.len()),
        Some(1),
        "the post-image is still there: {json}"
    );

    // And the post-image is the region's own bytes, so this is not a reply that
    // dropped `before` by dropping the write.
    let tile = &json["after"][0];
    let bytes = unb64(
        tile["dataBase64"].as_str().expect("tile bytes are base64"),
        "tile",
    )
    .expect("tile decodes");
    // Tile geometry is 256x256, so on a 4x4 layer the single reply tile is the
    // whole 4x4 and only the 2x2 region written is replaced; the rest keeps the
    // seeded bytes. Asserted per pixel so this cannot pass on a reply that
    // returned the pre-image unchanged.
    let (tw, th) = (
        tile["w"].as_u64().unwrap() as usize,
        tile["h"].as_u64().unwrap() as usize,
    );
    assert_eq!((tw, th), (4, 4), "the reply tile covers the layer: {tile}");
    for row in 0..th {
        for col in 0..tw {
            let px = bytes[(row * tw + col) * 4];
            let expected: u8 = if row < 2 && col < 2 { 9 } else { 7 };
            assert_eq!(
                px, expected,
                "pixel ({col},{row}) is the written region, not the pre-image"
            );
        }
    }

    // Undo still restores the pre-image, from Rust's own history rather than from
    // the reply - the reason the reply does not need to carry it.
    let undone = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo");
    assert!(
        !undone.tiles.is_empty(),
        "a Pixel tip yields tiles, so undo has the pre-image to restore"
    );
    let pre = unb64(undone.tiles[0].data_base64.as_str(), "tile").expect("undo tile decodes");
    assert!(
        pre.iter().all(|b| *b == 7),
        "undo restored the pre-image bytes Rust recorded: {pre:?}"
    );
    rust_pixels_close_document(doc);
}
