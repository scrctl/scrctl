//! Apple cursor storage and selection (RFB encoding 1104).

use std::collections::VecDeque;

use flate2::{Decompress, FlushDecompress};
use rfb_codec::{CursorShape, Rect};

const MAX_SIDE: u16 = 512;
const CACHE_SIZE: usize = 32;
pub(crate) const MAX_COMPRESSED_SIZE: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct AppleCursorCache {
    shapes: VecDeque<(u32, CursorShape)>,
}

impl AppleCursorCache {
    /// Empty payloads select a cached shape, and carry no dimensions or hotspot.
    pub(crate) fn update(&mut self, id: u32, rect: Rect, compressed: &[u8]) -> Option<CursorShape> {
        if compressed.is_empty() {
            return self
                .shapes
                .iter()
                .find(|(key, _)| *key == id)
                .map(|(_, shape)| shape.clone());
        }

        let shape = decode(rect, compressed)?;
        self.shapes.retain(|(key, _)| *key != id);
        if self.shapes.len() == CACHE_SIZE {
            self.shapes.pop_front();
        }
        self.shapes.push_back((id, shape.clone()));
        Some(shape)
    }
}

fn decode(rect: Rect, compressed: &[u8]) -> Option<CursorShape> {
    if rect.width > MAX_SIDE || rect.height > MAX_SIDE {
        return None;
    }
    let pixel_count = usize::from(rect.width) * usize::from(rect.height);
    let expected_len = pixel_count * 5;
    // The sender may use Z_SYNC_FLUSH. Validate the decoded byte count without
    // requiring a final zlib trailer, and cap the output to the cursor geometry.
    let mut decoded = vec![0; expected_len + 1];
    let mut inflator = Decompress::new(true);
    inflator
        .decompress(compressed, &mut decoded, FlushDecompress::Sync)
        .ok()?;
    if inflator.total_out() != expected_len as u64 || inflator.total_in() != compressed.len() as u64
    {
        return None;
    }
    let (pixels, alpha) = decoded[..expected_len].split_at_mut(pixel_count * 4);
    for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(alpha) {
        pixel[3] = *alpha;
    }
    decoded.truncate(pixel_count * 4);
    Some(CursorShape {
        hotspot_x: rect.x,
        hotspot_y: rect.y,
        width: rect.width,
        height: rect.height,
        pixels: decoded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compress, Compression, FlushCompress};

    #[test]
    fn restores_prior_cursor_geometry_and_alpha_when_server_selects_it() {
        let mut cursors = AppleCursorCache::default();
        let rect = Rect {
            x: 1,
            y: 0,
            width: 2,
            height: 1,
        };
        let mut payload = Vec::with_capacity(64);
        Compress::new(Compression::default(), true)
            .compress_vec(
                &[1, 2, 3, 0, 4, 5, 6, 0, 192, 64],
                &mut payload,
                FlushCompress::Sync,
            )
            .unwrap();
        let first = cursors.update(1000, rect, &payload).unwrap();
        assert_eq!(first.pixels, [1, 2, 3, 192, 4, 5, 6, 64]);
        let other = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert!(cursors.update(999, other, &[]).is_none());
        assert_eq!(cursors.update(1000, other, &[]), Some(first));
        assert!(cursors.update(1001, other, &payload).is_none());
    }
}
