//! The verification decision.
//!
//! Given the transaction's Ironwood anchor and the Ironwood tree root at every height in `[H, tip]`:
//! if the anchor equals any of those roots, it is not provably older than `H` (FAIL). Otherwise
//! consensus guarantees it is the root of some block before `H`, so every input existed before `H`
//! (PASS). Equal roots mean identical tree contents. One case needs the tree size at `H − 1`: if the
//! anchor is `H`'s root and block `H` added no notes, the anchor names the pool as it already was before
//! `H`, which is a PASS too ([`unchanged_before`]).

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

/// Consecutive heights at which the Ironwood tree has the same size. The tree is append-only, so
/// equal size on one chain means identical contents and therefore the same root: one root per run
/// stands for every height in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub from: u32,
    pub to: u32,
    pub size: u32,
}

/// Split per-height tree sizes (consecutive heights) into runs of equal size.
pub fn size_runs(sizes: &BTreeMap<u32, u32>) -> Result<Vec<Run>, String> {
    let mut runs: Vec<Run> = vec![];
    for (&h, &size) in sizes {
        match runs.last_mut() {
            Some(r) if h != r.to + 1 => return Err(format!("missing tree size for heights {}..{h}", r.to + 1)),
            Some(r) if size < r.size => {
                return Err(format!("Ironwood tree shrank from {} to {size} at height {h} (reorg?)", r.size));
            }
            Some(r) if size == r.size => r.to = h,
            _ => runs.push(Run { from: h, to: h, size }),
        }
    }
    Ok(runs)
}

/// Decide from one root per run, with the runs covering `[before, tip]`.
pub fn decide_runs(anchor: &[u8; 32], runs: &[(Run, [u8; 32])]) -> Verdict {
    match runs.iter().find(|(_, root)| root == anchor) {
        Some((r, _)) => Verdict::Fail { matched_from: r.from, matched_to: r.to },
        None => Verdict::Pass { roots_checked: runs.len() },
    }
}

/// True when a FAIL from [`decide_runs`] is really a PASS: the anchor matched from `h` itself, and the
/// tree at `h − 1` already had that size (`size_before`), so block `h` added no notes and the pool the
/// anchor names existed before `h`. Sound for the same reason as any PASS: the tree is append-only, so
/// equal size means equal contents, and every spent note sits inside it.
pub fn unchanged_before(h: u32, matched_from: u32, size_before: u32, first_run: &Run) -> bool {
    matched_from == h && first_run.from == h && first_run.size == size_before
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_decision() {
        let sizes: BTreeMap<u32, u32> = [(10, 5), (11, 5), (12, 7), (13, 7), (14, 7), (15, 9)].into();
        let runs = size_runs(&sizes).unwrap();
        assert_eq!(
            runs,
            vec![
                Run { from: 10, to: 11, size: 5 },
                Run { from: 12, to: 14, size: 7 },
                Run { from: 15, to: 15, size: 9 },
            ]
        );
        let with_roots: Vec<_> = runs.iter().map(|r| (*r, [r.size as u8; 32])).collect();
        assert_eq!(decide_runs(&[7; 32], &with_roots), Verdict::Fail { matched_from: 12, matched_to: 14 });
        assert_eq!(decide_runs(&[3; 32], &with_roots), Verdict::Pass { roots_checked: 3 });
    }

    #[test]
    fn unchanged_block_is_a_pass() {
        // Sizes from H = 12: block 12 added no notes (size 5 at 11 and at 12), so an anchor matching
        // from 12 names the pool as of block 11 or earlier.
        let runs = size_runs(&[(12, 5), (13, 5), (14, 8)].into()).unwrap();
        assert!(unchanged_before(12, 12, 5, &runs[0]));
        // Block 12 did add notes (size 4 at 11): the anchor's pool first existed at 12, not before.
        assert!(!unchanged_before(12, 12, 4, &runs[0]));
        // The anchor matched a later run: its pool is from after H.
        assert!(!unchanged_before(12, 14, 5, &runs[1]));
    }

    #[test]
    fn runs_reject_gaps_and_shrinking() {
        assert!(size_runs(&[(10, 5), (12, 5)].into()).is_err());
        assert!(size_runs(&[(10, 5), (11, 4)].into()).is_err());
    }
}
