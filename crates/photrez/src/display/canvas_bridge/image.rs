use super::*;

impl CanvasImage {
    /// Document pixel dimensions, used by the canvas pointer mapping.
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Converts one owned RGBA8 buffer (row-major, `photrez-core`'s output)
    /// into the tile grid, swapping to the BGRA order GPUI's atlas reads.
    ///
    /// This is the full-document path: every tile is created, so the first paint
    /// uploads the whole document. Use [`CanvasImage::push_patch`] for a live
    /// dirty rectangle and [`CanvasImage::commit_patches`] to fold it in; use
    /// [`CanvasImage::update_region`] for a direct base-tile update.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, FrameError> {
        Self::from_rgba_with_tile_size(width, height, rgba, TILE_SIZE)
    }

    /// Same contract as [`CanvasImage::from_rgba`], with an explicit display
    /// tile size. `tile_size == 0` is refused; the default remains
    /// [`TILE_SIZE`].
    pub fn from_rgba_with_tile_size(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        tile_size: u32,
    ) -> Result<Self, FrameError> {
        Self::from_rgba_inner(width, height, rgba, tile_size, None)
    }

    /// Build only the first `tile_limit` base tiles. Used by the progressive
    /// opening experiment: the first image paints a smaller tile set, then the
    /// full image replaces it. A limit does not change the document dimensions
    /// or the covered pixels.
    pub fn from_rgba_with_tile_size_limit(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        tile_size: u32,
        tile_limit: Option<usize>,
    ) -> Result<Self, FrameError> {
        Self::from_rgba_inner(width, height, rgba, tile_size, tile_limit)
    }

    fn from_rgba_inner(
        width: u32,
        height: u32,
        mut rgba: Vec<u8>,
        tile_size: u32,
        tile_limit: Option<usize>,
    ) -> Result<Self, FrameError> {
        if tile_size == 0 {
            return Err(FrameError::ZeroTileSize);
        }
        if width == 0 || height == 0 {
            return Err(FrameError::ZeroDimension);
        }
        if width > MAX_CANVAS_DIM || height > MAX_CANVAS_DIM {
            return Err(FrameError::TooLarge { width, height });
        }
        let expected = width as usize * height as usize * 4;
        let got = rgba.len();
        if got != expected {
            return Err(FrameError::LengthMismatch { expected, got });
        }
        // RGBA -> BGRA, once, at the seam.
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let mut rects = tile_rects_with_size(width, height, tile_size);
        if let Some(limit) = tile_limit {
            rects.truncate(limit);
        }
        let mut tiles = Vec::with_capacity(rects.len());
        for rect in rects {
            let row_len = rect.width as usize * 4;
            let mut bgra = vec![0u8; rect.height as usize * row_len];
            for row in 0..rect.height {
                let src_offset = (((rect.y + row) as usize * width as usize) + rect.x as usize) * 4;
                let dst_offset = row as usize * row_len;
                bgra[dst_offset..dst_offset + row_len]
                    .copy_from_slice(&rgba[src_offset..src_offset + row_len]);
            }
            let image = render_image(rect.width, rect.height, bgra.clone())?;
            tiles.push(Tile { rect, bgra, image });
        }
        Ok(Self {
            width,
            height,
            tile_size,
            tiles,
            patches: Vec::new(),
            preview_patches: Vec::new(),
        })
    }

    /// Number of base tiles in the current display grid.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// The display tile size this snapshot was built with.
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Document-space rectangles of the pending live preview overlays.
    #[cfg(test)]
    pub fn preview_patch_rects(&self) -> Vec<(u32, u32, u32, u32)> {
        self.preview_patches
            .iter()
            .map(|patch| {
                (
                    patch.rect.x,
                    patch.rect.y,
                    patch.rect.width,
                    patch.rect.height,
                )
            })
            .collect()
    }

    fn validate_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba_len: usize,
    ) -> Result<(), FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::ZeroDimension);
        }
        let right = x.checked_add(width).ok_or(FrameError::RegionOutside {
            x,
            y,
            width,
            height,
        })?;
        let bottom = y.checked_add(height).ok_or(FrameError::RegionOutside {
            x,
            y,
            width,
            height,
        })?;
        if right > self.width || bottom > self.height {
            return Err(FrameError::RegionOutside {
                x,
                y,
                width,
                height,
            });
        }
        let expected = width as usize * height as usize * 4;
        if rgba_len != expected {
            return Err(FrameError::LengthMismatch {
                expected,
                got: rgba_len,
            });
        }
        Ok(())
    }

    /// Replaces the pixels in `[x, x + width) x [y, y + height)` from `rgba`
    /// (row-major RGBA8, exactly `width * height * 4` bytes) and returns the
    /// previous tile textures for every tile that changed.
    ///
    /// The caller must drop the returned textures with `Window::drop_image`;
    /// unchanged tiles keep their atlas entries. A rectangle outside the
    /// document, a zero edge, or a wrong buffer length is refused, never
    /// panicked on. A pending live overlay must be committed first, otherwise
    /// this returns [`FrameError::PendingPatches`] - replacing the base while
    /// uncommitted pixels still cover it would display a mixture of two
    /// snapshots.
    #[must_use = "the retired textures must be dropped with Window::drop_image"]
    pub fn update_region(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Vec<Arc<RenderImage>>, FrameError> {
        if !self.patches.is_empty() {
            return Err(FrameError::PendingPatches);
        }
        if !self.preview_patches.is_empty() {
            return Err(FrameError::PendingPreviews);
        }
        self.validate_region(x, y, width, height, rgba.len())?;
        let right = x + width;
        let bottom = y + height;

        let mut retired = Vec::new();
        for tile in &mut self.tiles {
            let x0 = x.max(tile.rect.x);
            let y0 = y.max(tile.rect.y);
            let x1 = right.min(tile.rect.x + tile.rect.width);
            let y1 = bottom.min(tile.rect.y + tile.rect.height);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }

            let copy_width = (x1 - x0) as usize;
            for row in y0..y1 {
                let src_offset = (((row - y) as usize * width as usize) + (x0 - x) as usize) * 4;
                let dst_offset = (((row - tile.rect.y) as usize * tile.rect.width as usize)
                    + (x0 - tile.rect.x) as usize)
                    * 4;
                for column in 0..copy_width {
                    let src = src_offset + column * 4;
                    let dst = dst_offset + column * 4;
                    tile.bgra[dst] = rgba[src + 2];
                    tile.bgra[dst + 1] = rgba[src + 1];
                    tile.bgra[dst + 2] = rgba[src];
                    tile.bgra[dst + 3] = rgba[src + 3];
                }
            }

            let image = render_image(tile.rect.width, tile.rect.height, tile.bgra.clone())?;
            retired.push(std::mem::replace(&mut tile.image, image));
        }
        Ok(retired)
    }

    /// Reads the current base pixels for `[x, x + width) x [y, y + height)` as
    /// straight RGBA8, in document coordinates.
    ///
    /// This is the display composite the user currently sees. Pending live
    /// overlays are refused so the caller never reads a half-updated surface;
    /// discarding previews and committing exact patches first keeps the read
    /// consistent.
    pub fn read_region_rgba(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, FrameError> {
        if !self.patches.is_empty() {
            return Err(FrameError::PendingPatches);
        }
        if !self.preview_patches.is_empty() {
            return Err(FrameError::PendingPreviews);
        }
        let expected = width as usize * height as usize * 4;
        self.validate_region(x, y, width, height, expected)?;
        let right = x + width;
        let bottom = y + height;
        let mut out = vec![0u8; expected];
        for tile in &self.tiles {
            let x0 = x.max(tile.rect.x);
            let y0 = y.max(tile.rect.y);
            let x1 = right.min(tile.rect.x + tile.rect.width);
            let y1 = bottom.min(tile.rect.y + tile.rect.height);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let copy_width = (x1 - x0) as usize;
            for row in y0..y1 {
                let src_offset = (((row - tile.rect.y) as usize * tile.rect.width as usize)
                    + (x0 - tile.rect.x) as usize)
                    * 4;
                let dst_offset = (((row - y) as usize * width as usize) + (x0 - x) as usize) * 4;
                for column in 0..copy_width {
                    let src = src_offset + column * 4;
                    let dst = dst_offset + column * 4;
                    out[dst] = tile.bgra[src + 2];
                    out[dst + 1] = tile.bgra[src + 1];
                    out[dst + 2] = tile.bgra[src];
                    out[dst + 3] = tile.bgra[src + 3];
                }
            }
        }
        Ok(out)
    }

    /// Adds a live dirty rectangle as an immutable overlay image. The base
    /// tiles are not rebuilt, so the per-move upload is exactly the dirty
    /// rectangle instead of a whole display tile.
    ///
    /// The overlay is painted after the base tiles. Call
    /// [`CanvasImage::commit_patches`] to fold pending overlays into the base
    /// tiles; until then [`CanvasImage::update_region`] refuses to run. The
    /// caller does not receive a retired texture here (a patch is new), but
    /// must eventually commit or call [`CanvasImage::into_textures`], which
    /// returns every patch texture for `Window::drop_image`.
    pub fn push_patch(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), FrameError> {
        self.validate_region(x, y, width, height, rgba.len())?;
        // RGBA -> BGRA, once, at the seam.
        let mut bgra = Vec::with_capacity(rgba.len());
        for pixel in rgba.chunks_exact(4) {
            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        let image = render_image(width, height, bgra.clone())?;
        self.patches.push(Patch {
            rect: TileRect {
                x,
                y,
                width,
                height,
            },
            bgra,
            image,
        });
        Ok(())
    }

    /// Adds a display-scale preview rectangle. The image is deliberately
    /// preview-sized while `width`/`height` are the document-space destination;
    /// the display scales the image into the destination. Preview patches are
    /// display-only and must be discarded before any exact commit or base
    /// update.
    #[allow(clippy::too_many_arguments)] // destination + source + image size are one placement record
    pub fn push_scaled_patch(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        image_width: u32,
        image_height: u32,
        rgba: &[u8],
    ) -> Result<(), FrameError> {
        if width == 0 || height == 0 || image_width == 0 || image_height == 0 {
            return Err(FrameError::ZeroDimension);
        }
        let right = x.checked_add(width).ok_or(FrameError::RegionOutside {
            x,
            y,
            width,
            height,
        })?;
        let bottom = y.checked_add(height).ok_or(FrameError::RegionOutside {
            x,
            y,
            width,
            height,
        })?;
        if right > self.width || bottom > self.height {
            return Err(FrameError::RegionOutside {
                x,
                y,
                width,
                height,
            });
        }
        let expected = image_width as usize * image_height as usize * 4;
        if rgba.len() != expected {
            return Err(FrameError::LengthMismatch {
                expected,
                got: rgba.len(),
            });
        }
        // RGBA -> BGRA, once, at the seam.
        let mut bgra = Vec::with_capacity(rgba.len());
        for pixel in rgba.chunks_exact(4) {
            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        let image = render_image(image_width, image_height, bgra)?;
        self.preview_patches.push(PreviewPatch {
            rect: TileRect {
                x,
                y,
                width,
                height,
            },
            image,
        });
        Ok(())
    }

    /// Replaces all live preview overlays with one exact-resolution dirty rectangle.
    /// The new image is built first so an error preserves the currently displayed preview.
    #[must_use = "the returned textures must be dropped with Window::drop_image"]
    pub fn replace_preview_patch(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Vec<Arc<RenderImage>>, FrameError> {
        self.validate_region(x, y, width, height, rgba.len())?;
        let mut bgra = Vec::with_capacity(rgba.len());
        for pixel in rgba.chunks_exact(4) {
            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        let image = render_image(width, height, bgra)?;
        let retired = self.discard_preview_patches();
        self.preview_patches.push(PreviewPatch {
            rect: TileRect {
                x,
                y,
                width,
                height,
            },
            image,
        });
        Ok(retired)
    }

    /// Discards every scaled preview overlay and returns its textures for
    /// `Window::drop_image`. Call this before pushing the exact final tiles.
    #[must_use = "the retired textures must be dropped with Window::drop_image"]
    pub fn discard_preview_patches(&mut self) -> Vec<Arc<RenderImage>> {
        self.preview_patches
            .drain(..)
            .map(|patch| patch.image)
            .collect()
    }

    /// Folds every pending patch into the base tiles, rebuilding only the
    /// display tiles that a patch touches. Returns the old base-tile textures
    /// followed by every patch texture; pass all of them to
    /// `Window::drop_image` before the next paint.
    ///
    /// Patches are applied in insertion order, so a later patch wins where two
    /// overlays overlap. Committing with no pending patches retires nothing.
    #[must_use = "the retired textures must be dropped with Window::drop_image"]
    pub fn commit_patches(&mut self) -> Result<Vec<Arc<RenderImage>>, FrameError> {
        if !self.preview_patches.is_empty() {
            return Err(FrameError::PendingPreviews);
        }
        if self.patches.is_empty() {
            return Ok(Vec::new());
        }
        let patches = std::mem::take(&mut self.patches);
        let mut retired = Vec::new();
        for tile in &mut self.tiles {
            let mut touched = false;
            for patch in &patches {
                let x0 = patch.rect.x.max(tile.rect.x);
                let y0 = patch.rect.y.max(tile.rect.y);
                let x1 = (patch.rect.x + patch.rect.width).min(tile.rect.x + tile.rect.width);
                let y1 = (patch.rect.y + patch.rect.height).min(tile.rect.y + tile.rect.height);
                if x0 >= x1 || y0 >= y1 {
                    continue;
                }
                touched = true;
                let row_len = (x1 - x0) as usize * 4;
                for row in y0..y1 {
                    let src_offset = (((row - patch.rect.y) as usize * patch.rect.width as usize)
                        + (x0 - patch.rect.x) as usize)
                        * 4;
                    let dst_offset = (((row - tile.rect.y) as usize * tile.rect.width as usize)
                        + (x0 - tile.rect.x) as usize)
                        * 4;
                    tile.bgra[dst_offset..dst_offset + row_len]
                        .copy_from_slice(&patch.bgra[src_offset..src_offset + row_len]);
                }
            }
            if touched {
                let image = render_image(tile.rect.width, tile.rect.height, tile.bgra.clone())?;
                retired.push(std::mem::replace(&mut tile.image, image));
            }
        }
        retired.extend(patches.into_iter().map(|patch| patch.image));
        Ok(retired)
    }

    /// Consumes the snapshot and returns every base-tile and pending-patch
    /// texture.
    ///
    /// Use this when replacing the whole canvas; pass the result to
    /// `Window::drop_image`. Dropping a painted `CanvasImage` without this
    /// leaves every tile and patch in the sprite atlas.
    #[must_use = "the returned textures must be dropped with Window::drop_image"]
    pub fn into_textures(self) -> Vec<Arc<RenderImage>> {
        let mut textures: Vec<Arc<RenderImage>> =
            self.tiles.into_iter().map(|tile| tile.image).collect();
        textures.extend(self.patches.into_iter().map(|patch| patch.image));
        textures.extend(self.preview_patches.into_iter().map(|patch| patch.image));
        textures
    }
}
