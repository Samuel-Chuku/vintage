//! Ironwood roots for every block in `[from, tip]`, computed locally, with a cache.
//!
//! Asking the server for each root (`GetTreeState`) runs at ~40/s, and on mainnet most blocks add
//! Ironwood notes, so that takes minutes. Instead, per chunk of up to `CHUNK` blocks:
//! 1. take the server's Ironwood tree at the end of the block before the chunk;
//! 2. stream the chunk's compact blocks and append each Ironwood action's `cmx`, in order;
//! 3. after each block, check the tree size against the block's `ironwoodCommitmentTreeSize` and,
//!    if the block added notes, compute the new root;
//! 4. at the end, check the computed tree equals the server's tree at the chunk's last block
//!    (same size, same root).
//!
//! The tree is append-only, so a root is determined by the tree size: the cache stores sizes by
//! height and roots by size. Blocks at least `FINAL_DEPTH` below the tip go to SQLite. Newer blocks
//! are kept in memory only, tagged with the tip's block hash, and dropped as soon as the tip changes,
//! so a reorg can't poison either layer.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::stream::{self, StreamExt, TryStreamExt};
use orchard::{note::ExtractedNoteCommitment, tree::MerkleHashOrchard};
use rusqlite::{Connection, params};
use vintage_core::{
    VintageNetwork,
    verify::{Run, size_runs},
};

use crate::lwd;

/// Blocks this deep are treated as final (the wallet's own pruning depth).
const FINAL_DEPTH: u32 = 100;
/// Blocks per chunk: one GetBlockRange stream and two GetTreeState calls each.
const CHUNK: u32 = 500;

/// Recent (not yet final) blocks, valid only while the chain tip is `tip_hash`.
#[derive(Default)]
struct Recent {
    tip_hash: Vec<u8>,
    sizes: BTreeMap<u32, u32>,
    roots: BTreeMap<u32, [u8; 32]>,
}

/// Shared by concurrent requests: each layer has its own lock, held only for reads and writes.
pub struct Cache {
    conn: Mutex<Connection>,
    recent: Mutex<Recent>,
    network: &'static str,
}

/// `$XDG_CACHE_HOME/vintage/cache.sqlite`, else `~/.cache/vintage/cache.sqlite`.
pub fn default_cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("vintage").join("cache.sqlite"))
}

impl Cache {
    pub fn open(path: &Path, network: VintageNetwork) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening cache {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ironwood_sizes (
                 network TEXT NOT NULL, height INTEGER NOT NULL, size INTEGER NOT NULL,
                 PRIMARY KEY (network, height));
             CREATE TABLE IF NOT EXISTS ironwood_roots (
                 network TEXT NOT NULL, size INTEGER NOT NULL, root BLOB NOT NULL,
                 PRIMARY KEY (network, size));",
        )?;
        Ok(Cache { conn: Mutex::new(conn), recent: Mutex::default(), network: network.name() })
    }

    /// Known sizes for `[from, to]` and roots for those sizes: SQLite, plus recent blocks if they
    /// belong to the chain ending in `tip_hash`.
    fn get(&self, from: u32, to: u32, tip_hash: &[u8]) -> Result<(BTreeMap<u32, u32>, BTreeMap<u32, [u8; 32]>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT height, size FROM ironwood_sizes WHERE network = ?1 AND height BETWEEN ?2 AND ?3",
        )?;
        let mut sizes: BTreeMap<u32, u32> = stmt
            .query_map(params![self.network, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut roots = BTreeMap::new();
        if let (Some(min), Some(max)) = (sizes.values().min(), sizes.values().max()) {
            let mut stmt = conn.prepare_cached(
                "SELECT size, root FROM ironwood_roots WHERE network = ?1 AND size BETWEEN ?2 AND ?3",
            )?;
            let rows = stmt.query_map(params![self.network, min, max], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
            })?;
            for row in rows {
                let (size, root) = row?;
                roots.insert(size, root.try_into().map_err(|_| anyhow!("bad cached root"))?);
            }
        }
        drop(stmt);
        drop(conn);
        let recent = self.recent.lock().unwrap();
        if !tip_hash.is_empty() && recent.tip_hash == tip_hash {
            sizes.extend(recent.sizes.range(from..=to));
            roots.extend(recent.roots.iter());
        }
        Ok((sizes, roots))
    }

    /// Store computed blocks: final ones in SQLite, newer ones in memory under `tip_hash`.
    fn put(&self, blocks: &[(u32, u32, [u8; 32])], final_to: u32, tip_hash: &[u8]) -> Result<()> {
        {
            let mut conn = self.conn.lock().unwrap();
            let tx = conn.transaction()?;
            {
                let mut sizes = tx.prepare_cached(
                    "INSERT OR REPLACE INTO ironwood_sizes (network, height, size) VALUES (?1, ?2, ?3)",
                )?;
                let mut roots = tx.prepare_cached(
                    "INSERT OR REPLACE INTO ironwood_roots (network, size, root) VALUES (?1, ?2, ?3)",
                )?;
                for (h, size, root) in blocks.iter().filter(|(h, _, _)| *h <= final_to) {
                    sizes.execute(params![self.network, h, size])?;
                    roots.execute(params![self.network, size, root.as_slice()])?;
                }
            }
            tx.commit()?;
        }
        if !tip_hash.is_empty() {
            let mut recent = self.recent.lock().unwrap();
            if recent.tip_hash != tip_hash {
                *recent = Recent { tip_hash: tip_hash.to_vec(), ..Default::default() };
            }
            for (h, size, root) in blocks.iter().filter(|(h, _, _)| *h > final_to) {
                recent.sizes.insert(*h, *size);
                recent.roots.insert(*size, *root);
            }
        }
        Ok(())
    }
}

/// What it took to produce the roots (for the output and the benchmarks).
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Stats {
    pub blocks: u32,
    pub blocks_downloaded: u32,
    pub blocks_from_cache: u32,
    pub server_tree_checks: u32,
    #[serde(skip)]
    pub secs: f64,
}

pub struct Roots {
    /// Runs covering `[from, tip]`, each with its root.
    pub runs: Vec<(Run, [u8; 32])>,
    /// Tree size at `from − 1`; equal to the first run's size when the root at `from` already existed
    /// before `from`.
    pub size_before: u32,
    pub stats: Stats,
}

/// Contiguous ranges of heights in `[from, to]` for which `missing(h)`.
fn gaps(from: u32, to: u32, missing: impl Fn(u32) -> bool) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = vec![];
    for h in (from..=to).filter(|h| missing(*h)) {
        match out.last_mut() {
            Some((_, e)) if *e + 1 == h => *e = h,
            _ => out.push((h, h)),
        }
    }
    out
}

/// `(height, tree size, root)` for every block in `[s, e]`, computed and checked as described above.
async fn compute_chunk(client: lwd::Client, s: u32, e: u32) -> Result<Vec<(u32, u32, [u8; 32])>> {
    let (start, blocks, end) = tokio::try_join!(
        lwd::ironwood_tree(&client, s - 1),
        lwd::ironwood_blocks(&client, s, e),
        lwd::ironwood_tree(&client, e),
    )?;
    tokio::task::spawn_blocking(move || {
        let mut tree = start;
        let mut root = tree.root().to_bytes();
        let mut out = Vec::with_capacity(blocks.len());
        for b in blocks {
            for cmx in &b.cmxs {
                let cmx = Option::<ExtractedNoteCommitment>::from(ExtractedNoteCommitment::from_bytes(cmx))
                    .ok_or_else(|| anyhow!("non-canonical cmx in block {}", b.height))?;
                tree.append(MerkleHashOrchard::from_cmx(&cmx))
                    .map_err(|_| anyhow!("Ironwood tree full at block {}", b.height))?;
            }
            if tree.size() != b.size as usize {
                bail!("block {}: tree has {} notes after appending, compact block says {}", b.height, tree.size(), b.size);
            }
            if !b.cmxs.is_empty() {
                root = tree.root().to_bytes();
            }
            out.push((b.height, b.size, root));
        }
        if tree.size() != end.size() || root != end.root().to_bytes() {
            bail!("computed Ironwood tree at block {e} does not match the server's (reorg during the check? retry)");
        }
        Ok(out)
    })
    .await?
}

/// Sizes for `[lo, to]` and the root of every size from `roots_from` on, read from the cache or
/// computed (and then cached). Returns the maps, the number of blocks computed, and the number of chunks.
#[allow(clippy::too_many_arguments)]
async fn fill(
    client: &lwd::Client,
    cache: Option<&Cache>,
    lo: u32,
    to: u32,
    roots_from: u32,
    tip: u32,
    tip_hash: &[u8],
    concurrency: usize,
) -> Result<(BTreeMap<u32, u32>, BTreeMap<u32, [u8; 32]>, u32, u32)> {
    let (mut sizes, mut roots) = match cache {
        Some(c) => c.get(lo, to, tip_hash)?,
        None => (BTreeMap::new(), BTreeMap::new()),
    };

    // Compute every block whose size, or (from `roots_from` on) whose root, isn't known.
    let todo = gaps(lo, to, |h| match sizes.get(&h) {
        None => true,
        Some(size) => h >= roots_from && !roots.contains_key(size),
    });
    let chunks: Vec<(u32, u32)> = todo
        .iter()
        .flat_map(|&(s, e)| (s..=e).step_by(CHUNK as usize).map(move |c| (c, (c + CHUNK - 1).min(e))))
        .collect();
    let n_chunks = chunks.len() as u32;
    let computed: Vec<Vec<(u32, u32, [u8; 32])>> = stream::iter(chunks)
        .map(|(s, e)| compute_chunk(client.clone(), s, e))
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;
    let computed: Vec<(u32, u32, [u8; 32])> = computed.into_iter().flatten().collect();
    let downloaded = computed.len() as u32;
    if let Some(c) = cache {
        c.put(&computed, tip.saturating_sub(FINAL_DEPTH), tip_hash)?;
    }
    for (h, size, root) in computed {
        sizes.insert(h, size);
        roots.insert(size, root);
    }
    Ok((sizes, roots, downloaded, n_chunks))
}

/// Roots for `[from, tip]`. `tip_hash` is the tip block's hash (empty disables the in-memory layer).
pub async fn roots_for(
    client: &lwd::Client,
    cache: Option<&Cache>,
    from: u32,
    tip: u32,
    tip_hash: &[u8],
    concurrency: usize,
) -> Result<Roots> {
    let t = Instant::now();
    let lo = from.checked_sub(1).ok_or_else(|| anyhow!("H must be > 0"))?;
    let mut stats = Stats { blocks: tip - from + 1, ..Default::default() };
    let (mut sizes, roots, downloaded, chunks) = fill(client, cache, lo, tip, from, tip, tip_hash, concurrency).await?;
    stats.blocks_downloaded = downloaded;
    stats.server_tree_checks = 2 * chunks;

    let size_before = *sizes.get(&lo).expect("size at from − 1 is known");
    sizes.retain(|h, _| *h >= from && *h <= tip);
    let runs = size_runs(&sizes).map_err(|e| anyhow!(e))?;
    let runs: Vec<(Run, [u8; 32])> = runs
        .into_iter()
        .map(|r| Ok((r, *roots.get(&r.size).ok_or_else(|| anyhow!("no root for tree size {}", r.size))?)))
        .collect::<Result<_>>()?;
    stats.blocks_from_cache = (tip - lo + 1) - stats.blocks_downloaded;
    stats.secs = t.elapsed().as_secs_f64();
    Ok(Roots { runs, size_before, stats })
}

/// Where an anchor sits in the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Located {
    /// The anchor is the Ironwood root at the end of every block in `first..=last`, and of no other
    /// block: the tree first held exactly these notes at the end of block `first`.
    At { first: u32, last: u32 },
    /// The anchor is the root of no block in `[floor, below)`, so it is from a block before `floor`.
    Before { floor: u32 },
}

/// Find the first and last block whose Ironwood root is `anchor`, searching down from `below − 1` to
/// `floor`, in windows that double in size. Consensus makes an anchor the root of some block before the
/// transaction, and the tree is append-only, so a root names one tree size and one run of blocks.
#[allow(clippy::too_many_arguments)]
pub async fn locate(
    client: &lwd::Client,
    cache: Option<&Cache>,
    anchor: &[u8; 32],
    below: u32,
    floor: u32,
    tip: u32,
    tip_hash: &[u8],
    concurrency: usize,
) -> Result<Located> {
    let mut hi = below.checked_sub(1).ok_or_else(|| anyhow!("nothing below block 0"))?;
    let mut window = 1_000u32;
    while hi >= floor {
        let lo = hi.saturating_sub(window - 1).max(floor);
        let (sizes, roots, _, _) = fill(client, cache, lo, hi, lo, tip, tip_hash, concurrency).await?;
        if let Some((&size, _)) = roots.iter().find(|(_, r)| *r == anchor) {
            let heights: Vec<u32> = sizes.range(lo..=hi).filter(|(_, s)| **s == size).map(|(h, _)| *h).collect();
            if let (Some(&first), Some(&last)) = (heights.first(), heights.last()) {
                // The run may continue below the window: walk down while the size stays the same.
                let mut first = first;
                while first > floor {
                    let (s, _, _, _) = fill(client, cache, first - 1, first - 1, first, tip, tip_hash, concurrency).await?;
                    if s.get(&(first - 1)) != Some(&size) {
                        break;
                    }
                    first -= 1;
                }
                return Ok(Located::At { first, last });
            }
        }
        if lo == floor {
            break;
        }
        hi = lo - 1;
        window = window.saturating_mul(2);
    }
    Ok(Located::Before { floor })
}
