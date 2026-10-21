//! The verification decision.
//!
//! Given the transaction's Ironwood anchor and the Ironwood tree root at every height in `[H, tip]`:
//! if the anchor equals any of those roots, it is not provably older than `H` (FAIL). Otherwise
//! consensus guarantees it is the root of some block before `H`, so every input existed before `H`
//! (PASS). Equal roots mean identical tree contents, so the repeated-root case is sound but
//! conservative: it can say FAIL for inputs that truly predate `H`, but it never gives a false PASS.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The anchor matched no root in `[before, tip]`.
    Pass { roots_checked: usize },
    /// The anchor equals the root at these heights (all `>= before`).
    Fail { matched_from: u32, matched_to: u32 },
    /// Inputs Vintage cannot reason about.
    NotProvable { reasons: Vec<&'static str> },
}

/// Decide from a complete map of `height → Ironwood root` covering `[before, tip]`.
pub fn decide(anchor: &[u8; 32], roots: &BTreeMap<u32, [u8; 32]>) -> Verdict {
    let matches: Vec<u32> = roots
        .iter()
        .filter(|(_, r)| *r == anchor)
        .map(|(h, _)| *h)
        .collect();
    match (matches.first(), matches.last()) {
        (Some(lo), Some(hi)) => Verdict::Fail {
            matched_from: *lo,
            matched_to: *hi,
        },
        _ => Verdict::Pass {
            roots_checked: roots.len(),
        },
    }
}
