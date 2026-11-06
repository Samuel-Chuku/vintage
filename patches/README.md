# zcash-devtool patches

`vintage send` and `vintage link prove --zk` sign through [zcash-devtool](https://github.com/zcash/zcash-devtool). Build it from
commit `0b3436b` with these five patches, applied in this order:

| Patch | What it does |
|---|---|
| `zcash-devtool-propose-min-confirmations.patch` | `wallet propose --min-confirmations N`, so Vintage can pick the anchor |
| `zcash-devtool-pay-min-confirmations.patch` | the same for `wallet pay` (ZIP 321 requests, `vintage send --uri`) |
| `zcash-devtool-nu7-releases.patch` | builds against the librustzcash NU7 releases (21 Oct 2026: `zcash_protocol` 0.11.0, `zcash_client_sqlite` 0.23.0, …), which know the mainnet NU7 height (3,543,405) and the ZIP 318 grid after it |
| `zcash-devtool-disclose-spend.patch` | `wallet disclose-spend`: proves and signs one of the wallet's notes against a given anchor and path, for `vintage link prove --zk` (experimental), so Vintage never handles a key |
| `zcash-devtool-sync-batch.patch` | syncs in batches of at most 100 blocks, so the wallet keeps a usable anchor at every grid height (see below) |

```sh
git clone https://github.com/zcash/zcash-devtool && cd zcash-devtool
git checkout 0b3436b
for p in propose-min-confirmations pay-min-confirmations nu7-releases disclose-spend sync-batch; do
  git apply /path/to/vintage/patches/zcash-devtool-$p.patch
done
cargo build --release      # put target/release/zcash-devtool next to the vintage binary
```

Without the NU7 patch, a devtool built from `0b3436b` doesn't know mainnet NU7 and can't send once it
activates. The patch only changes the dependency versions and ports five commands that aren't used by
Vintage (`inspect`, three `pczt` commands, `migration advance`) to the renamed library APIs.

`wallet disclose-spend` signs a 32-byte message that its caller supplies, with the note's spend
authorization key (under the bundle's own randomized key) and the binding key. The bundle can't be
broadcast: its anchor is the root of a tree of one payment's outputs, not a chain root. Vintage passes
a hash of the deposit and funding txids under the personalization `Vintage_ZkLink01`. As with any
signing command, only run it for software you trust; it is part of what the construction's review
must cover.

Back up wallet folders before the first run with the new libraries: opening a wallet migrates its
database.

Why the sync batch size matters: the wallet library (`zcash_client_backend` 0.25) keeps a checkpoint at
every ZIP 318 grid height, which is what lets Vintage spend with an old anchor. When the grid block has
no shielded outputs, the library adds that checkpoint only after inserting the whole batch, pointing at
the position of the last earlier block that had outputs. A wallet keeps 100 ordinary checkpoints; in a
batch long enough to create more after it, that earlier block's checkpoint, and the tree data under it, is pruned
first, and the grid checkpoint is left without the data to compute its root. Spending against it then
fails with "Unable to compute root; missing values for nodes". The unpatched devtool scans up to 10,000
blocks at a time, so any wallet that catches up after more than about 100 blocks offline can hit this.
Batches of at most 100 blocks avoid it. `vintage send` checks the root before proposing and stops with
a clear message if it can't be computed; `vintage witness --anchor-height <H>` shows the same check.
A wallet already affected is repaired by `zcash-devtool wallet reset` and a sync with the patched build.
librustzcash fixed this on `main` in commit `5b0f6de98` (4 Nov 2026, "checkpoint only within the pruning
window"), not yet in a crates.io release; once the devtool builds against a release that includes it,
this patch can be dropped.
