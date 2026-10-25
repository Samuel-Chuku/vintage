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
