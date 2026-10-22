//! lightwalletd / Zaino gRPC access.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use futures_util::stream::{self, StreamExt, TryStreamExt};
use incrementalmerkletree::frontier::CommitmentTree;
use orchard::tree::MerkleHashOrchard;
use tonic::transport::{Channel, ClientTlsConfig};
use zcash_client_backend::proto::service::{
    BlockId, BlockRange, ChainSpec, PoolType, RawTransaction, TxFilter,
    compact_tx_streamer_client::CompactTxStreamerClient,
};
use zcash_protocol::TxId;

pub type Client = CompactTxStreamerClient<Channel>;

pub async fn connect(server: &str) -> Result<Client> {
    let (host, _port) = server
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("--server must be host:port"))?;
    let tls = ClientTlsConfig::new()
        .domain_name(host.to_string())
        .assume_http2(true)
        .with_webpki_roots();
    let channel = Channel::from_shared(format!("https://{server}"))?
        .tls_config(tls)?
        .connect()
        .await
        .with_context(|| format!("connecting to {server}"))?;
    Ok(CompactTxStreamerClient::new(channel).max_decoding_message_size(64 * 1024 * 1024))
}

pub async fn tip(client: &Client) -> Result<u32> {
    Ok(client
        .clone()
        .get_latest_block(ChainSpec {})
        .await
        .context("GetLatestBlock")?
        .into_inner()
        .height as u32)
}

/// Fetch a raw transaction. Returns (raw, mined height if mined).
pub async fn get_tx(client: &Client, txid: &TxId) -> Result<RawTransaction> {
    Ok(client
        .clone()
        .get_transaction(TxFilter {
            block: None,
            index: 0,
            hash: txid.as_ref().to_vec(),
        })
        .await
        .context("GetTransaction")?
        .into_inner())
}

/// Block time (unix seconds) from the compact block.
pub async fn block_time(client: &Client, height: u32) -> Result<u32> {
    Ok(client
        .clone()
        .get_block(BlockId {
            height: height.into(),
            hash: vec![],
        })
        .await
        .with_context(|| format!("GetBlock({height})"))?
        .into_inner()
        .time)
}

/// Ironwood tree root at the end of `height`, and whether the server's field was empty.
pub async fn ironwood_root(client: Client, height: u32) -> Result<(u32, [u8; 32], bool)> {
    let ts = client
        .clone()
        .get_tree_state(BlockId {
            height: height.into(),
            hash: vec![],
        })
        .await
        .with_context(|| format!("GetTreeState({height})"))?
        .into_inner();
    let empty = ts.ironwood_tree.is_empty();
    let tree = ts
        .ironwood_tree()
        .with_context(|| format!("decoding ironwoodTree at {height}"))?;
    Ok((height, tree.root().to_bytes(), empty))
}

pub type IronwoodTree = CommitmentTree<MerkleHashOrchard, { orchard::NOTE_COMMITMENT_TREE_DEPTH as u8 }>;

/// The server's Ironwood tree as of the end of `height`.
pub async fn ironwood_tree(client: &Client, height: u32) -> Result<IronwoodTree> {
    let ts = client
        .clone()
        .get_tree_state(BlockId { height: height.into(), hash: vec![] })
        .await
        .with_context(|| format!("GetTreeState({height})"))?
        .into_inner();
    if ts.ironwood_tree.is_empty() {
        return Err(anyhow!("server returned an empty ironwoodTree at {height}; it does not expose Ironwood trees"));
    }
    ts.ironwood_tree().with_context(|| format!("decoding ironwoodTree at {height}"))
}

/// One compact block's Ironwood content: tree size at its end and the note commitments it adds.
pub struct IronwoodBlock {
    pub height: u32,
    pub size: u32,
    pub cmxs: Vec<[u8; 32]>,
}

/// Compact blocks `[from, to]` filtered to Ironwood (`poolTypes = [IRONWOOD]`), in height order.
pub async fn ironwood_blocks(client: &Client, from: u32, to: u32) -> Result<Vec<IronwoodBlock>> {
    let mut blocks = client
        .clone()
        .get_block_range(BlockRange {
            start: Some(BlockId { height: from.into(), hash: vec![] }),
            end: Some(BlockId { height: to.into(), hash: vec![] }),
            pool_types: vec![PoolType::Ironwood as i32],
        })
        .await
        .with_context(|| format!("GetBlockRange({from}..={to})"))?
        .into_inner();
    let mut out = Vec::with_capacity((to - from + 1) as usize);
    while let Some(b) = blocks.message().await.with_context(|| format!("GetBlockRange({from}..={to})"))? {
        let size = b
            .chain_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("block {} has no chain_metadata", b.height))?
            .ironwood_commitment_tree_size;
        let cmxs = b
            .vtx
            .iter()
            .flat_map(|tx| tx.ironwood_actions.iter())
            .map(|a| a.cmx.as_slice().try_into().map_err(|_| anyhow!("bad cmx in block {}", b.height)))
            .collect::<Result<_>>()?;
        out.push(IronwoodBlock { height: b.height as u32, size, cmxs });
    }
    if out.len() != (to - from + 1) as usize || out.iter().zip(from..).any(|(b, h)| b.height != h) {
        return Err(anyhow!("GetBlockRange({from}..={to}) returned {} blocks, not the full range in order", out.len()));
    }
    Ok(out)
}

/// `(height, txid)` of every transaction with Ironwood actions in `[from, to]` (for test samples).
pub async fn ironwood_txids(client: &Client, from: u32, to: u32) -> Result<Vec<(u32, String)>> {
    let mut blocks = client
        .clone()
        .get_block_range(BlockRange {
            start: Some(BlockId { height: from.into(), hash: vec![] }),
            end: Some(BlockId { height: to.into(), hash: vec![] }),
            pool_types: vec![PoolType::Ironwood as i32],
        })
        .await
        .context("GetBlockRange")?
        .into_inner();
    let mut out = vec![];
    while let Some(b) = blocks.message().await.context("GetBlockRange")? {
        for tx in b.vtx.iter().filter(|tx| !tx.ironwood_actions.is_empty()) {
            out.push((b.height as u32, tx.txid().to_string()));
        }
    }
    Ok(out)
}

/// Roots for every height in `[from, to]`: one GetTreeState per block. Slow; kept for `--locate-back`.
pub async fn roots_in_range(
    client: &Client,
    from: u32,
    to: u32,
    concurrency: usize,
) -> Result<(BTreeMap<u32, [u8; 32]>, usize)> {
    let all: Vec<(u32, [u8; 32], bool)> = stream::iter(from..=to)
        .map(|h| ironwood_root(client.clone(), h))
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;
    let empty = all.iter().filter(|(_, _, e)| *e).count();
    Ok((all.into_iter().map(|(h, r, _)| (h, r)).collect(), empty))
}
