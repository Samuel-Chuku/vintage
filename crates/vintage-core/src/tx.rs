//! Parse a raw transaction and extract what Vintage needs.

use zcash_primitives::transaction::Transaction;
use zcash_protocol::{
    TxId,
    consensus::{BlockHeight, BranchId},
};

use crate::VintageNetwork;

#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("txid must be 64 hex characters")]
    BadTxid,
    #[error("could not parse transaction: {0}")]
    Parse(#[from] std::io::Error),
    #[error("server returned a different transaction ({0})")]
    TxidMismatch(String),
}

/// What Vintage reads from an Ironwood bundle.
#[derive(Clone, Debug)]
pub struct IronwoodInfo {
    pub anchor: [u8; 32],
    pub actions: usize,
    pub spends_enabled: bool,
    pub outputs_enabled: bool,
    /// Net value leaving the Ironwood pool, in zatoshis (negative = value entered the pool).
    pub value_balance: i64,
}

#[derive(Clone, Debug)]
pub struct TxSummary {
    pub txid: String,
    pub version: String,
    pub mined_height: Option<u32>,
    pub transparent_inputs: usize,
    pub transparent_outputs: usize,
    pub sapling_spends: usize,
    pub orchard_spends_enabled: bool,
    pub ironwood: Option<IronwoodInfo>,
}

impl TxSummary {
    /// Reasons the transaction's inputs are outside what Vintage can prove. Empty = in scope.
    pub fn out_of_scope(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.transparent_inputs > 0 {
            v.push("transparent inputs");
        }
        if self.sapling_spends > 0 {
            v.push("Sapling spends");
        }
        if self.orchard_spends_enabled {
            v.push("Orchard spends");
        }
        match &self.ironwood {
            None => v.push("no Ironwood bundle"),
            Some(i) if !i.spends_enabled => v.push("Ironwood bundle has no inputs (spends disabled)"),
            _ => {}
        }
        v
    }
}

/// Parse a displayed (byte-reversed) txid.
pub fn parse_txid(s: &str) -> Result<TxId, TxError> {
    let mut bytes: [u8; 32] = hex::decode(s)
        .map_err(|_| TxError::BadTxid)?
        .try_into()
        .map_err(|_| TxError::BadTxid)?;
    bytes.reverse();
    Ok(TxId::from_bytes(bytes))
}

/// Parse a raw transaction as returned by lightwalletd `GetTransaction`.
///
/// `mined_height` selects the consensus branch; for an unmined tx pass the expected target height.
pub fn summarize(
    network: VintageNetwork,
    raw: &[u8],
    mined_height: Option<u32>,
    branch_height: u32,
    expected_txid: Option<&TxId>,
) -> Result<TxSummary, TxError> {
    let branch = BranchId::for_height(&network.params(), BlockHeight::from_u32(branch_height));
    let tx = Transaction::read(raw, branch)?;
    if let Some(expected) = expected_txid {
        if tx.txid() != *expected {
            return Err(TxError::TxidMismatch(tx.txid().to_string()));
        }
    }
    let ironwood = tx.ironwood_bundle().map(|b| IronwoodInfo {
        anchor: b.anchor().to_bytes(),
        actions: b.actions().len(),
        spends_enabled: b.flags().spends_enabled(),
        outputs_enabled: b.flags().outputs_enabled(),
        value_balance: i64::from(*b.value_balance()),
    });
    Ok(TxSummary {
        txid: tx.txid().to_string(),
        version: format!("{:?}", tx.version()),
        mined_height,
        transparent_inputs: tx.transparent_bundle().map_or(0, |b| b.vin.len()),
        transparent_outputs: tx.transparent_bundle().map_or(0, |b| b.vout.len()),
        sapling_spends: tx.sapling_bundle().map_or(0, |b| b.shielded_spends().len()),
        orchard_spends_enabled: tx
            .orchard_bundle()
            .is_some_and(|b| b.flags().spends_enabled()),
        ironwood,
    })
}
