//! Vintage core: prove that every Ironwood input of a Zcash transaction existed before block `H`.
//!
//! The property: an Ironwood spend's anchor must be the Ironwood note commitment tree root at the end
//! of some earlier block (consensus), and the spend proof shows each spent note is in that tree. So if
//! the anchor is a root from before `H`, every spent note existed before `H`.
//!
//! This crate does no I/O. Callers fetch raw transactions and tree roots and pass them in.

pub mod grid;
pub mod network;
pub mod registry;
pub mod time;
pub mod tx;
pub mod verify;

pub use network::VintageNetwork;

/// A block height or count with thousands separators, for sentences people read: `3,525,709`.
pub fn commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod commas_tests {
    #[test]
    fn groups_of_three() {
        assert_eq!(super::commas(0), "0");
        assert_eq!(super::commas(999), "999");
        assert_eq!(super::commas(1000), "1,000");
        assert_eq!(super::commas(3525709), "3,525,709");
    }
}
