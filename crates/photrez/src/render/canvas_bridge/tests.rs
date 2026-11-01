use super::*;

/// The reference capture, measured: the 781x668 canvas-container crop
/// draws the 800x600 page as a 701x525.75 box at (40, 71.125) - exactly
/// the fit formula's prediction.
#[test]
fn fit_placement_matches_the_reference_capture() {
    let (x, y, w, h) = fit_placement(781., 668., 800., 600.).expect("fits");
    assert!((x - 40.).abs() < 0.01, "x: {x}");
    assert!((y - 71.125).abs() < 0.01, "y: {y}");
    assert!((w - 701.).abs() < 0.01, "w: {w}");
    assert!((h - 525.75).abs() < 0.01, "h: {h}");
}

/// `paint_image` positions its destination rect in window coordinates:
/// the element-relative placement origin must be offset by the element's
/// own origin, or the page lands near the window's top-left corner and is
/// clipped by the element bounds instead of being centered in it.
#[test]
fn page_origin_is_offset_into_window_coordinates() {
    let (x, y) = page_origin(52., 127.2, 40., 90.5);
    assert!((x - 92.).abs() < 0.01, "x: {x}");
    assert!((y - 217.7).abs() < 0.01, "y: {y}");
}

/// GPUI's atlas reads raw frame bytes as BGRA (its own producers swap
/// RGBA -> BGRA before `Frame::new`); the seam must hand it BGRA.
#[test]
fn rgba_becomes_bgra_in_the_frame() {
    let image = CanvasImage::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).expect("valid");
    let bytes = image.tiles[0].image.as_bytes(0).expect("one frame");
    assert_eq!(&bytes[0..4], &[0, 0, 255, 255], "red must land blue-first");
    assert_eq!(&bytes[4..8], &[255, 0, 0, 255], "blue must land red-first");
}

#[test]
fn scaled_preview_patch_is_display_only_until_discarded() {
    let mut image = CanvasImage::from_rgba(2, 1, vec![0; 2 * 4]).expect("valid");
    image
        .push_scaled_patch(0, 0, 2, 1, 1, 1, &solid(1, 1, [255, 0, 0, 255]))
        .expect("scaled preview");
    assert_eq!(image.preview_patches.len(), 1);
    assert!(matches!(
        image.commit_patches(),
        Err(FrameError::PendingPreviews)
    ));
    assert!(matches!(
        image.update_region(0, 0, 1, 1, &solid(1, 1, [0, 0, 0, 255])),
        Err(FrameError::PendingPreviews)
    ));
    assert!(matches!(
        image.push_scaled_patch(2, 0, 1, 1, 1, 1, &solid(1, 1, [0, 0, 0, 255])),
        Err(FrameError::RegionOutside { .. })
    ));
    assert!(matches!(
        image.push_scaled_patch(0, 0, 2, 1, 1, 1, &[0; 3]),
        Err(FrameError::LengthMismatch { .. })
    ));
    let retired = image.discard_preview_patches();
    assert_eq!(retired.len(), 1);
    assert!(image.preview_patches.is_empty());
    assert!(image.commit_patches().is_ok());
    assert!(image
        .update_region(0, 0, 1, 1, &solid(1, 1, [0, 0, 0, 255]))
        .is_ok());
}

#[test]
fn exact_preview_replacement_retires_old_overlay_and_keeps_full_resolution() {
    let mut image = CanvasImage::from_rgba(2, 1, vec![0; 2 * 4]).expect("valid");
    image
        .push_scaled_patch(0, 0, 2, 1, 1, 1, &solid(1, 1, [255, 0, 0, 255]))
        .expect("initial preview");
    let old_texture = Arc::clone(&image.preview_patches[0].image);

    let replacement = [255, 0, 0, 255, 0, 0, 255, 255];
    let retired = image
        .replace_preview_patch(0, 0, 2, 1, &replacement)
        .expect("exact replacement");

    assert_eq!(retired.len(), 1);
    assert!(Arc::ptr_eq(&retired[0], &old_texture));
    assert_eq!(image.preview_patches.len(), 1);
    assert_eq!(
        image.preview_patches[0].rect,
        TileRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        }
    );
    assert_eq!(
        image.preview_patches[0]
            .image
            .as_bytes(0)
            .expect("one frame"),
        &[0, 0, 255, 255, 255, 0, 0, 255]
    );
}

fn solid(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..(width as usize * height as usize) {
        rgba.extend_from_slice(&color);
    }
    rgba
}

fn tile_at(image: &CanvasImage, x: u32, y: u32) -> &Tile {
    image
        .tiles
        .iter()
        .find(|tile| {
            tile.rect.x <= x
                && x < tile.rect.x + tile.rect.width
                && tile.rect.y <= y
                && y < tile.rect.y + tile.rect.height
        })
        .expect("every document pixel belongs to exactly one tile")
}

fn pixel_at(image: &CanvasImage, x: u32, y: u32) -> [u8; 4] {
    let tile = tile_at(image, x, y);
    let local_x = x - tile.rect.x;
    let local_y = y - tile.rect.y;
    let bytes = tile.image.as_bytes(0).expect("one frame");
    let offset = ((local_y * tile.rect.width + local_x) as usize) * 4;
    [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]
}

const RED_BGRA: [u8; 4] = [0, 0, 255, 255];
const WHITE_BGRA: [u8; 4] = [255, 255, 255, 255];

#[test]
fn tile_grid_covers_every_pixel_once() {
    let width = TILE_SIZE * 2 + 1;
    let height = TILE_SIZE + 44;
    let rects = tile_rects(width, height);
    assert_eq!(rects.len(), 6);
    assert_eq!(
        rects[0],
        TileRect {
            x: 0,
            y: 0,
            width: TILE_SIZE,
            height: TILE_SIZE
        }
    );
    assert_eq!(
        rects[2],
        TileRect {
            x: TILE_SIZE * 2,
            y: 0,
            width: 1,
            height: TILE_SIZE
        }
    );
    assert_eq!(
        rects[3],
        TileRect {
            x: 0,
            y: TILE_SIZE,
            width: TILE_SIZE,
            height: 44
        }
    );
    assert_eq!(
        rects[5],
        TileRect {
            x: TILE_SIZE * 2,
            y: TILE_SIZE,
            width: 1,
            height: 44
        }
    );

    let mut coverage = vec![0u8; width as usize * height as usize];
    for rect in &rects {
        assert!(rect.width > 0 && rect.height > 0);
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                coverage[(y * width + x) as usize] += 1;
            }
        }
    }
    assert!(
        coverage.iter().all(|&count| count == 1),
        "tiles must neither overlap nor leave gaps"
    );
}

#[test]
fn tile_destination_edges_share_the_snapped_grid() {
    let doc_w = TILE_SIZE * 2 + 100;
    let doc_h = TILE_SIZE + 100;
    let (page_x, page_y, page_w, page_h) = (40.3f32, 71.125f32, 701.0f32, 525.75f32);
    let rects = tile_rects(doc_w, doc_h);

    for scale in [1.0f32, 1.25, 1.5] {
        let dests: Vec<(f32, f32, f32, f32)> = rects
            .iter()
            .map(|tile| {
                tile_destination(
                    page_x,
                    page_y,
                    page_w,
                    page_h,
                    doc_w as f32,
                    doc_h as f32,
                    *tile,
                    scale,
                )
                .expect("every tile falls inside the page")
            })
            .collect();

        for (i, a) in rects.iter().enumerate() {
            for (j, b) in rects.iter().enumerate() {
                if a.y == b.y && a.height == b.height && a.x + a.width == b.x {
                    let (ax, _, aw, _) = dests[i];
                    let (bx, _, _, _) = dests[j];
                    assert!(
                        (ax + aw - bx).abs() < 1e-4,
                        "scale {scale}: horizontal seam {ax}+{aw} vs {bx}"
                    );
                }
                if a.x == b.x && a.width == b.width && a.y + a.height == b.y {
                    let (_, ay, _, ah) = dests[i];
                    let (_, by, _, _) = dests[j];
                    assert!(
                        (ay + ah - by).abs() < 1e-4,
                        "scale {scale}: vertical seam {ay}+{ah} vs {by}"
                    );
                }
            }
        }

        let first = rects
            .iter()
            .position(|tile| tile.x == 0 && tile.y == 0)
            .expect("origin tile");
        let last_x = rects
            .iter()
            .position(|tile| tile.x + tile.width == doc_w && tile.y == 0)
            .expect("right tile");
        let last_y = rects
            .iter()
            .position(|tile| tile.x == 0 && tile.y + tile.height == doc_h)
            .expect("bottom tile");

        assert!((dests[first].0 - snap_edge(page_x, scale)).abs() < 1e-4);
        assert!(
            (dests[last_x].0 + dests[last_x].2 - snap_edge(page_x + page_w, scale)).abs() < 1e-4
        );
        assert!((dests[first].1 - snap_edge(page_y, scale)).abs() < 1e-4);
        assert!(
            (dests[last_y].1 + dests[last_y].3 - snap_edge(page_y + page_h, scale)).abs() < 1e-4
        );
        assert!(dests.iter().all(|(_, _, w, h)| *w > 0.0 && *h > 0.0));
    }
}

#[test]
fn subpixel_one_pixel_edge_tile_is_skipped_not_squeezed() {
    let doc_w = TILE_SIZE * 2 + 1;
    let rects = tile_rects(doc_w, 12);
    let edge = rects
        .iter()
        .find(|rect| rect.x + rect.width == doc_w)
        .copied()
        .expect("right edge tile");
    assert_eq!(edge.width, 1);
    let destination = tile_destination(40.4, 71.2, 50., 30., doc_w as f32, 12., edge, 1.);
    assert!(
        destination.is_none(),
        "a tile thinner than one device pixel must be skipped, not squeezed to zero"
    );
}

#[test]
fn dirty_update_replaces_only_intersecting_tiles() {
    let doc = TILE_SIZE * 2 + 88;
    let mut image =
        CanvasImage::from_rgba(doc, doc, solid(doc, doc, [255, 255, 255, 255])).expect("valid");
    assert_eq!(image.tiles.len(), 9);

    let old: Vec<Arc<RenderImage>> = image
        .tiles
        .iter()
        .map(|tile| Arc::clone(&tile.image))
        .collect();
    let x = TILE_SIZE - 5;
    let y = TILE_SIZE - 5;
    let retired = image
        .update_region(x, y, 10, 10, &solid(10, 10, [255, 0, 0, 255]))
        .expect("valid dirty region");
    assert_eq!(retired.len(), 4);

    let changed: Vec<usize> = (0..image.tiles.len())
        .filter(|&index| !Arc::ptr_eq(&image.tiles[index].image, &old[index]))
        .collect();
    assert_eq!(changed, vec![0, 1, 3, 4]);
    for &index in &changed {
        assert!(
            retired
                .iter()
                .any(|texture| Arc::ptr_eq(texture, &old[index])),
            "retired tile {index}"
        );
    }
    for (index, old_image) in old.iter().enumerate() {
        if !changed.contains(&index) {
            assert!(
                Arc::ptr_eq(&image.tiles[index].image, old_image),
                "untouched tile {index} must keep its identity"
            );
        }
    }

    assert_eq!(pixel_at(&image, x, y), RED_BGRA);
    assert_eq!(pixel_at(&image, x + 9, y + 9), RED_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE, y), RED_BGRA);
    assert_eq!(pixel_at(&image, x, TILE_SIZE), RED_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE, TILE_SIZE), RED_BGRA);
    assert_eq!(pixel_at(&image, x - 1, y), WHITE_BGRA);
    assert_eq!(pixel_at(&image, x + 10, y + 9), WHITE_BGRA);
}

#[test]
fn dirty_update_inside_one_tile_retires_only_that_tile() {
    let doc = TILE_SIZE * 2 + 88;
    let mut image =
        CanvasImage::from_rgba(doc, doc, solid(doc, doc, [255, 255, 255, 255])).expect("valid");
    let old_tile = Arc::clone(&image.tiles[2].image);

    let x = TILE_SIZE * 2 + 8;
    let y = 10;
    let retired = image
        .update_region(x, y, 2, 2, &solid(2, 2, [255, 0, 0, 255]))
        .expect("valid dirty region");
    assert_eq!(retired.len(), 1);
    assert!(Arc::ptr_eq(&retired[0], &old_tile));
    assert!(!Arc::ptr_eq(&image.tiles[2].image, &old_tile));
    assert_eq!(pixel_at(&image, x, y), RED_BGRA);
    assert_eq!(pixel_at(&image, x + 1, y + 1), RED_BGRA);
    assert_eq!(pixel_at(&image, x - 1, y), WHITE_BGRA);
}

#[test]
fn dirty_update_rejects_invalid_regions_without_panicking() {
    let mut image =
        CanvasImage::from_rgba(600, 600, solid(600, 600, [255, 255, 255, 255])).expect("valid");

    assert!(matches!(
        image.update_region(0, 0, 0, 1, &[]),
        Err(FrameError::ZeroDimension)
    ));
    assert!(matches!(
        image.update_region(0, 0, 1, 0, &[]),
        Err(FrameError::ZeroDimension)
    ));
    assert!(matches!(
        image.update_region(600, 0, 1, 1, &[0; 4]),
        Err(FrameError::RegionOutside { .. })
    ));
    assert!(matches!(
        image.update_region(0, 600, 1, 1, &[0; 4]),
        Err(FrameError::RegionOutside { .. })
    ));
    assert!(matches!(
        image.update_region(0, 0, u32::MAX, 1, &[]),
        Err(FrameError::RegionOutside { .. })
    ));
    assert!(matches!(
        image.update_region(0, 0, 2, 2, &[0; 4]),
        Err(FrameError::LengthMismatch {
            expected: 16,
            got: 4
        })
    ));
}

#[test]
fn from_rgba_rejects_invalid_input_without_panicking() {
    assert!(matches!(
        CanvasImage::from_rgba(0, 1, Vec::new()),
        Err(FrameError::ZeroDimension)
    ));
    assert!(matches!(
        CanvasImage::from_rgba(1, 0, Vec::new()),
        Err(FrameError::ZeroDimension)
    ));
    assert!(matches!(
        CanvasImage::from_rgba(MAX_CANVAS_DIM + 1, 1, Vec::new()),
        Err(FrameError::TooLarge { .. })
    ));
    assert!(matches!(
        CanvasImage::from_rgba(2, 1, vec![0; 4]),
        Err(FrameError::LengthMismatch {
            expected: 8,
            got: 4
        })
    ));
}

#[test]
fn whole_snapshot_yields_every_tile_texture() {
    let edge = TILE_SIZE + 44;
    let image =
        CanvasImage::from_rgba(edge, edge, solid(edge, edge, [255, 255, 255, 255])).expect("valid");
    assert_eq!(image.tiles.len(), 4);
    let textures = image.into_textures();
    assert_eq!(textures.len(), 4);
}

#[test]
fn patch_overlay_leaves_base_tiles_untouched() {
    let mut image =
        CanvasImage::from_rgba(600, 600, solid(600, 600, [255, 255, 255, 255])).expect("valid");
    let old_tile = Arc::clone(&image.tiles[0].image);
    image
        .push_patch(4, 5, 2, 2, &solid(2, 2, [255, 0, 0, 255]))
        .expect("valid patch");
    assert_eq!(image.patches.len(), 1);
    assert!(Arc::ptr_eq(&image.tiles[0].image, &old_tile));
    let patch = &image.patches[0];
    assert_eq!(
        patch.rect,
        TileRect {
            x: 4,
            y: 5,
            width: 2,
            height: 2
        }
    );
    assert_eq!(
        patch.image.as_bytes(0).expect("one frame"),
        &RED_BGRA.repeat(4)[..]
    );
}

#[test]
fn commit_patches_rebuilds_touched_tiles_and_returns_all_textures() {
    let doc = TILE_SIZE * 2 + 88;
    let mut image =
        CanvasImage::from_rgba(doc, doc, solid(doc, doc, [255, 255, 255, 255])).expect("valid");
    let old: Vec<Arc<RenderImage>> = image
        .tiles
        .iter()
        .map(|tile| Arc::clone(&tile.image))
        .collect();
    image
        .push_patch(TILE_SIZE + 3, 7, 2, 2, &solid(2, 2, [255, 0, 0, 255]))
        .expect("valid patch");
    let patch_texture = Arc::clone(&image.patches[0].image);

    let retired = image.commit_patches().expect("commit");
    assert!(image.patches.is_empty());
    assert_eq!(retired.len(), 2, "one touched tile + one patch");
    assert!(Arc::ptr_eq(&retired[0], &old[1]));
    assert!(Arc::ptr_eq(&retired[1], &patch_texture));
    assert!(!Arc::ptr_eq(&image.tiles[1].image, &old[1]));
    assert!(Arc::ptr_eq(&image.tiles[0].image, &old[0]));
    assert_eq!(pixel_at(&image, TILE_SIZE + 3, 7), RED_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE + 3, 9), WHITE_BGRA);
}

#[test]
fn commit_patch_straddling_a_tile_boundary_updates_both_tiles() {
    let doc = TILE_SIZE * 2;
    let mut image =
        CanvasImage::from_rgba(doc, 8, solid(doc, 8, [255, 255, 255, 255])).expect("valid");
    let old: Vec<Arc<RenderImage>> = image
        .tiles
        .iter()
        .map(|tile| Arc::clone(&tile.image))
        .collect();
    image
        .push_patch(TILE_SIZE - 1, 3, 2, 2, &solid(2, 2, [255, 0, 0, 255]))
        .expect("valid patch");

    let retired = image.commit_patches().expect("commit");
    assert_eq!(retired.len(), 3, "two straddled tiles + one patch");
    assert!(Arc::ptr_eq(&retired[0], &old[0]));
    assert!(Arc::ptr_eq(&retired[1], &old[1]));
    assert_eq!(pixel_at(&image, TILE_SIZE - 1, 3), RED_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE, 3), RED_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE - 2, 3), WHITE_BGRA);
    assert_eq!(pixel_at(&image, TILE_SIZE + 1, 3), WHITE_BGRA);
}

#[test]
fn pending_patches_block_base_updates() {
    let mut image = CanvasImage::from_rgba(2, 2, solid(2, 2, [255, 255, 255, 255])).expect("valid");
    image
        .push_patch(0, 0, 1, 1, &solid(1, 1, [255, 0, 0, 255]))
        .expect("valid patch");
    assert!(matches!(
        image.update_region(0, 0, 1, 1, &solid(1, 1, [0, 255, 0, 255])),
        Err(FrameError::PendingPatches)
    ));
    assert_eq!(pixel_at(&image, 0, 0), WHITE_BGRA);
}

#[test]
fn patch_push_rejects_invalid_regions_without_panicking() {
    let mut image = CanvasImage::from_rgba(2, 2, solid(2, 2, [255, 255, 255, 255])).expect("valid");
    assert!(matches!(
        image.push_patch(0, 0, 0, 1, &[]),
        Err(FrameError::ZeroDimension)
    ));
    assert!(matches!(
        image.push_patch(2, 0, 1, 1, &[0; 4]),
        Err(FrameError::RegionOutside { .. })
    ));
    assert!(matches!(
        image.push_patch(0, 0, 2, 2, &[0; 4]),
        Err(FrameError::LengthMismatch {
            expected: 16,
            got: 4
        })
    ));
    assert!(image.patches.is_empty());
}

#[test]
fn into_textures_includes_pending_patches() {
    let mut image = CanvasImage::from_rgba(2, 2, solid(2, 2, [255, 255, 255, 255])).expect("valid");
    image
        .push_patch(0, 0, 1, 1, &solid(1, 1, [255, 0, 0, 255]))
        .expect("valid patch");
    let textures = image.into_textures();
    assert_eq!(textures.len(), 2, "one base tile + one patch");
}

#[test]
fn commit_without_patches_retires_nothing() {
    let mut image = CanvasImage::from_rgba(2, 2, solid(2, 2, [255, 255, 255, 255])).expect("valid");
    assert!(image.commit_patches().expect("commit").is_empty());
}

#[test]
fn tile_size_1024_keeps_the_same_pixels_and_texture_count() {
    let image =
        CanvasImage::from_rgba_with_tile_size(2048, 2048, solid(2048, 2048, [9, 8, 7, 255]), 1024)
            .expect("valid");
    assert_eq!(image.tile_count(), 4);
    assert_eq!(image.tile_size(), 1024);
    assert_eq!(pixel_at(&image, 0, 0), [7, 8, 9, 255]);
    assert_eq!(pixel_at(&image, 2047, 2047), [7, 8, 9, 255]);
}

#[test]
fn zero_tile_size_is_refused() {
    assert!(matches!(
        CanvasImage::from_rgba_with_tile_size(2, 2, solid(2, 2, [1, 2, 3, 255]), 0),
        Err(FrameError::ZeroTileSize)
    ));
}

#[test]
fn union_of_disjoint_and_overlapping_rects_is_pixel_exact_after_commit() {
    let a = (4, 5, 10, 10);
    let b = (12, 14, 3, 3);
    assert_eq!(union_rect(a, b), (4, 5, 11, 12));

    let mut image =
        CanvasImage::from_rgba(32, 32, solid(32, 32, [255, 255, 255, 255])).expect("valid");
    image
        .push_patch(a.0, a.1, a.2, a.3, &solid(a.2, a.3, [255, 0, 0, 255]))
        .expect("patch a");
    image
        .push_patch(b.0, b.1, b.2, b.3, &solid(b.2, b.3, [0, 0, 255, 255]))
        .expect("patch b");
    image.commit_patches().expect("commit");

    let inside = |rect: (u32, u32, u32, u32), x: u32, y: u32| {
        x >= rect.0 && x < rect.0 + rect.2 && y >= rect.1 && y < rect.1 + rect.3
    };
    let union = union_rect(a, b);
    for y in union.1..union.1 + union.3 {
        for x in union.0..union.0 + union.2 {
            let expected = if inside(b, x, y) {
                [255, 0, 0, 255]
            } else if inside(a, x, y) {
                RED_BGRA
            } else {
                WHITE_BGRA
            };
            assert_eq!(pixel_at(&image, x, y), expected, "pixel ({x},{y})");
        }
    }
}

#[test]
fn progressive_tile_limit_keeps_covered_pixels_equal_to_the_full_build() {
    let rgba = solid(600, 600, [9, 8, 7, 255]);
    let first = CanvasImage::from_rgba_with_tile_size_limit(600, 600, rgba.clone(), 512, Some(2))
        .expect("partial build");
    let full = CanvasImage::from_rgba_with_tile_size(600, 600, rgba, 512).expect("full build");

    assert_eq!(first.tile_count(), 2);
    assert_eq!(full.tile_count(), 4);
    for (x, y) in [(0, 0), (511, 511), (512, 0), (599, 511)] {
        assert_eq!(pixel_at(&first, x, y), pixel_at(&full, x, y), "({x},{y})");
    }
}

#[test]
fn read_region_rgba_returns_the_display_pixels_as_rgba() {
    let mut rgba = Vec::with_capacity(4 * 4 * 4);
    for y in 0..4u8 {
        for x in 0..4u8 {
            rgba.extend_from_slice(&[x * 10, y * 10, 7, 200]);
        }
    }
    let image = CanvasImage::from_rgba(4, 4, rgba).expect("valid");
    let region = image.read_region_rgba(1, 1, 2, 2).expect("region reads");
    assert_eq!(region.len(), 2 * 2 * 4);
    assert_eq!(&region[0..4], &[10, 10, 7, 200]);
    assert_eq!(&region[4..8], &[20, 10, 7, 200]);
    assert_eq!(&region[8..12], &[10, 20, 7, 200]);
    assert_eq!(&region[12..16], &[20, 20, 7, 200]);
}

#[test]
fn read_region_rgba_refuses_a_surface_with_pending_previews() {
    let mut image = CanvasImage::from_rgba(8, 8, solid(8, 8, [255, 255, 255, 255])).expect("valid");
    image
        .push_scaled_patch(0, 0, 4, 4, 4, 4, &solid(4, 4, [255, 0, 0, 255]))
        .expect("scaled preview");
    assert!(matches!(
        image.read_region_rgba(0, 0, 4, 4),
        Err(FrameError::PendingPreviews)
    ));
    let _ = image.discard_preview_patches();
    assert!(image.read_region_rgba(0, 0, 4, 4).is_ok());
}
