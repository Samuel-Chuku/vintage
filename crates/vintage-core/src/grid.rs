//! Anchor heights Vintage can use.
//!
//! librustzcash wallets keep a durable note-commitment-tree checkpoint at every boundary of the
//! ZIP-318 anchor grid (every multiple of 144 blocks from NU6.3 activation), exempt from ordinary
//! pruning. So a wallet can always build a witness at those heights, and Vintage anchors use them.

use zcash_protocol::{consensus::BlockHeight, zip318::AnchorBucketInterval};

/// The grid interval (144 blocks).
pub fn interval() -> AnchorBucketInterval {
    AnchorBucketInterval::ZIP_318
}

/// The newest grid boundary strictly before `before`: the anchor height a Vintage send should use to
/// prove "every input existed before block `before`".
pub fn anchor_height_for(before: u32) -> Option<u32> {
    let below = before.checked_sub(1)?;
    let h = interval().boundary_at_or_below(BlockHeight::from_u32(below));
    let h = u32::from(h);
    (h > 0).then_some(h)
}

/// The confirmations requirement that makes a wallet pick `anchor` as its anchor when targeting
/// `target_height` (anchor = target − confirmations).
pub fn confirmations_for(target_height: u32, anchor: u32) -> Option<u32> {
    target_height.checked_sub(anchor).filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_anchor() {
        // 4,523,328 = 144 × 31,412 (the spike's anchor).
        assert_eq!(anchor_height_for(4_523_439), Some(4_523_328));
        assert_eq!(anchor_height_for(4_523_329), Some(4_523_328));
        // Strictly before: H itself on the grid is not allowed.
        assert_eq!(anchor_height_for(4_523_328), Some(4_523_184));
        assert_eq!(confirmations_for(4_525_340, 4_523_328), Some(2012));
    }
}
