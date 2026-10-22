//! Ironwood roots for every block in `[from, tip]`, computed locally, with a SQLite cache.
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
//! height and roots by size. Only blocks at least `FINAL_DEPTH` below the tip are cached, so a reorg
//! can't poison it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
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

pub struct Cache {
    conn: Connection,
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
        Ok(Cache { conn, network: network.name() })
    }

    fn sizes(&self, from: u32, to: u32) -> Result<BTreeMap<u32, u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT height, size FROM ironwood_sizes WHERE network = ?1 AND height BETWEEN ?2 AND ?3",
        )?;
        let rows = stmt.query_map(params![self.network, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn roots(&self, min_size: u32, max_size: u32) -> Result<BTreeMap<u32, [u8; 32]>> {
        let mut stmt = self.conn.prepare(
            "SELECT size, root FROM ironwood_roots WHERE network = ?1 AND size BETWEEN ?2 AND ?3",
        )?;
        let rows = stmt.query_map(params![self.network, min_size, max_size], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|r| {
            let (size, root) = r?;
            Ok((size, root.try_into().map_err(|_| anyhow!("bad cached root"))?))
        })
        .collect()
    }

    fn put(&mut self, blocks: &[(u32, u32, [u8; 32])]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut sizes =
                tx.prepare("INSERT OR REPLACE INTO ironwood_sizes (network, height, size) VALUES (?1, ?2, ?3)")?;
            let mut roots =
                tx.prepare("INSERT OR REPLACE INTO ironwood_roots (network, size, root) VALUES (?1, ?2, ?3)")?;
            for (h, size, root) in blocks {
                sizes.execute(params![self.network, h, size])?;
                roots.execute(params![self.network, size, root.as_slice()])?;
            }
        }
        Ok(tx.commit()?)
    }
}

/// What it took to produce the roots (for the output and the benchmarks).
#[derive(Debug, Default)]
pub struct Stats {
    pub blocks: u32,
    pub blocks_downloaded: u32,
    pub blocks_from_cache: u32,
    pub server_tree_checks: u32,
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

pub async fn roots_for(
    client: &lwd::Client,
    mut cache: Option<&mut Cache>,
    from: u32,
    tip: u32,
    concurrency: usize,
) -> Result<Roots> {
    let t = Instant::now();
    let lo = from.checked_sub(1).ok_or_else(|| anyhow!("H must be > 0"))?;
    let final_to = tip.saturating_sub(FINAL_DEPTH);
    let mut stats = Stats { blocks: tip - from + 1, ..Default::default() };

    // What the cache already knows.
    let (mut sizes, mut roots) = match &cache {
        Some(c) => {
            let sizes = c.sizes(lo, tip)?;
            let roots = match (sizes.values().min(), sizes.values().max()) {
                (Some(min), Some(max)) => c.roots(*min, *max)?,
                _ => BTreeMap::new(),
            };
            (sizes, roots)
        }
        None => (BTreeMap::new(), BTreeMap::new()),
    };

    // Compute every block whose size, or (from `from` on) whose root, isn't known.
    let todo = gaps(lo, tip, |h| match sizes.get(&h) {
        None => true,
        Some(size) => h >= from && !roots.contains_key(size),
    });
    let chunks: Vec<(u32, u32)> = todo
        .iter()
        .flat_map(|&(s, e)| (s..=e).step_by(CHUNK as usize).map(move |c| (c, (c + CHUNK - 1).min(e))))
        .collect();
    stats.server_tree_checks = 2 * chunks.len() as u32;
    let computed: Vec<Vec<(u32, u32, [u8; 32])>> = stream::iter(chunks)
        .map(|(s, e)| compute_chunk(client.clone(), s, e))
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;
    let computed: Vec<(u32, u32, [u8; 32])> = computed.into_iter().flatten().collect();
    stats.blocks_downloaded = computed.len() as u32;
    if let Some(c) = cache.as_deref_mut() {
        let finalized: Vec<_> = computed.iter().filter(|(h, _, _)| *h <= final_to).copied().collect();
        c.put(&finalized)?;
    }
    for (h, size, root) in computed {
        sizes.insert(h, size);
        roots.insert(size, root);
    }

    let size_before = *sizes.get(&lo).expect("size at from − 1 is known");
    sizes.remove(&lo);
    let runs = size_runs(&sizes).map_err(|e| anyhow!(e))?;
    let runs: Vec<(Run, [u8; 32])> = runs
        .into_iter()
        .map(|r| Ok((r, *roots.get(&r.size).ok_or_else(|| anyhow!("no root for tree size {}", r.size))?)))
        .collect::<Result<_>>()?;
    stats.blocks_from_cache = (tip - lo + 1) - stats.blocks_downloaded;
    stats.secs = t.elapsed().as_secs_f64();
    Ok(Roots { runs, size_before, stats })
}
