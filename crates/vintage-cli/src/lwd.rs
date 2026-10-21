//! lightwalletd / Zaino gRPC access.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use futures_util::stream::{self, StreamExt, TryStreamExt};
use tonic::transport::{Channel, ClientTlsConfig};
use zcash_client_backend::proto::service::{
    BlockId, ChainSpec, RawTransaction, TxFilter, compact_tx_streamer_client::CompactTxStreamerClient,
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

/// Roots for every height in `[from, to]`. (Day 3 replaces this with change-point fetching + cache.)
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
