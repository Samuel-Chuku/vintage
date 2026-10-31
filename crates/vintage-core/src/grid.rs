//! Anchor heights Vintage can use.
//!
//! librustzcash wallets keep a durable note-commitment-tree checkpoint at every boundary of the
//! ZIP-318 anchor grid, exempt from ordinary pruning: every multiple of 144 blocks before NU7, and
//! every multiple of 432 from NU7 (blocks are 3× faster, so boundaries stay ~3 hours apart). So a
//! wallet can always build a witness at those heights, and Vintage anchors use them.

use zcash_protocol::{
    consensus::BlockHeight,
    zip318::{AnchorBucketInterval, AnchorGrid, BlockSpacing},
};

use crate::network::VintageNetwork;

/// The ZIP-318 grid of `network`, with its NU7 activation height.
pub fn grid(network: VintageNetwork) -> AnchorGrid {
    AnchorGrid::new(AnchorBucketInterval::ZIP_318, BlockSpacing::for_network(&network.params()))
}

/// The newest grid boundary strictly before `before`: the anchor height a Vintage send should use to
/// prove "every input existed before block `before`".
pub fn anchor_height_for(network: VintageNetwork, before: u32) -> Option<u32> {
    let h = u32::from(grid(network).boundary_before(BlockHeight::from_u32(before))?);
    (h > 0).then_some(h)
}

/// The expected time from block `from` to block `to`, in seconds: 75 s per block before NU7 and
/// 25 s from NU7 (ZIP 218).
pub fn seconds_between(network: VintageNetwork, from: u32, to: u32) -> u64 {
    let nu7 = grid(network).spacing().nu7_activation().map_or(u32::MAX, u32::from);
    let before_nu7 = to.min(nu7).saturating_sub(from.min(nu7));
    let after_nu7 = to.max(nu7).saturating_sub(from.max(nu7));
    u64::from(before_nu7) * 75 + u64::from(after_nu7) * 25
}

/// The confirmations requirement that makes a wallet pick `anchor` as its anchor when targeting
/// `target_height` (anchor = target − confirmations).
pub fn confirmations_for(target_height: u32, anchor: u32) -> Option<u32> {
    target_height.checked_sub(anchor).filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: VintageNetwork = VintageNetwork::Main;
    const TEST: VintageNetwork = VintageNetwork::Test;

    #[test]
    fn grid_anchor_before_nu7() {
        // Bitget and the demo event: multiples of 144.
        assert_eq!(anchor_height_for(MAIN, 3_501_240), Some(3_501_216));
        assert_eq!(anchor_height_for(MAIN, 3_525_709), Some(3_525_696));
        // Strictly before: H itself on the grid is not allowed.
        assert_eq!(anchor_height_for(MAIN, 3_525_696), Some(3_525_552));
        assert_eq!(confirmations_for(4_525_340, 4_523_328), Some(2012));
    }

    #[test]
    fn grid_anchor_after_nu7() {
        // Mainnet NU7 = 3,543,405. The last 144-boundary before it is 3,543,264; the first
        // 432-boundary from it is 3,543,696 (ZIP 318's example bucket).
        assert_eq!(anchor_height_for(MAIN, 3_543_405), Some(3_543_264));
        assert_eq!(anchor_height_for(MAIN, 3_543_696), Some(3_543_264));
        assert_eq!(anchor_height_for(MAIN, 3_543_697), Some(3_543_696));
        assert_eq!(anchor_height_for(MAIN, 3_544_200), Some(3_544_128));
        let h = anchor_height_for(TEST, 4_470_100).unwrap();
        assert_eq!(h % 432, 0);
        assert!(h >= 4_465_026 && h < 4_470_100 && 4_470_100 - h <= 432);
    }

    #[test]
    fn block_time_across_nu7() {
        assert_eq!(seconds_between(MAIN, 3_525_696, 3_525_709), 13 * 75);
        assert_eq!(seconds_between(MAIN, 3_543_400, 3_543_410), 5 * 75 + 5 * 25);
        assert_eq!(seconds_between(TEST, 4_562_352, 4_562_864), 512 * 25);
    }
}
