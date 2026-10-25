# How Vintage works

Vintage proves one thing about a Zcash transaction: **every Ironwood note it spends already existed
before block H.** H is any dated event: a hack's first inflow, a tax cutoff, a bug window, a snapshot, or
a block you name. It needs no new cryptography and no list of suspects. It reads a field that every
Ironwood spend already carries, and that consensus already checks.

- [The one-paragraph version](#the-one-paragraph-version)
- [Background: anchors](#background-anchors)
- [Why an old anchor proves old coins](#why-an-old-anchor-proves-old-coins)
- [Making a Vintage send](#making-a-vintage-send)
- [Checking a transaction](#checking-a-transaction)
- [The fast verifier](#the-fast-verifier)
- [The event registry](#the-event-registry)
- [What the verdicts mean](#what-the-verdicts-mean)
- Fresh coins (received after the event): [LINK.md](LINK.md)

## The one-paragraph version

To spend shielded coins, a wallet proves "my note is in the pool" against a snapshot of the pool, called
the **anchor**. Consensus only accepts a snapshot that existed at the end of some earlier block. Wallets
normally use one from a few blocks ago. Vintage makes the wallet use one from **before a hack**. Coins
that arrived after the hack aren't in that snapshot, so they can't be spent in that transaction, and the
network enforces it. Anyone can then look at the transaction's anchor and check that it is an
**older snapshot than the hack**.

## Background: anchors

Ironwood keeps every shielded note ever created as a leaf in an append-only Merkle tree: the **note
commitment tree**. After each block, the tree has a root. That root is a 32-byte fingerprint of every
note in the pool up to that block.

```mermaid
flowchart LR
    subgraph B1["block 100"]
        R1["root r100<br/>notes 1..50"]
    end
    subgraph B2["block 101"]
        R2["root r101<br/>notes 1..53"]
    end
    subgraph B3["block 102 (no new notes)"]
        R3["root r102 = r101<br/>notes 1..53"]
    end
    subgraph B4["block 103"]
        R4["root r103<br/>notes 1..60"]
    end
    R1 --> R2 --> R3 --> R4
```

An Ironwood spend names one of these roots as its **anchor** and includes a zero-knowledge proof that
each note it spends is a leaf of the tree with that root. The consensus rule, as cited in Zebra's
`zebra_state/src/service/check/anchors.rs` from the
[protocol specification](https://zips.z.cash/protocol/protocol.pdf#actions):

> The anchorIronwood field of the transaction, whenever it exists (i.e. when there are any Ironwood
> Action descriptions), MUST refer to some earlier block's final Ironwood treestate.

[ZIP 229](https://zips.z.cash/zip-0229) defines the field: `anchorIronwood`, 32 bytes, one per
transaction, shared by every action in it: "A root of the Ironwood note commitment tree at some block
height in the past."

## Why an old anchor proves old coins

Suppose the anchor is the root after block A, and A < H:

1. The proof shows every spent note is a leaf of the tree as it stood after block A.
2. The tree after block A only contains notes created at or before block A.
3. So every spent note existed at block A, before H.

```mermaid
timeline
    title One honest note, one hack
    block 3,525,666 : honest note created (0.015 ZEC)
    block 3,525,696 : anchor H_a (a grid height, see below)
    block 3,525,709 : event H (staged hack inflow)
    block 3,525,726 : tainted note arrives in the same wallet
    later : a Vintage send anchored at 3,525,696 can spend the honest note, never the tainted one
```

These are the real mainnet heights from our demo. The tainted note is **not in the tree at
3,525,696**, so no proof exists that it is. We asked the wallet for each note's Merkle path at that
anchor. For the honest note it gave `WITNESS OK`. For the tainted note it refused: `Tree does not
contain a root at address … 824517`.

## Making a Vintage send

Wallets don't choose an anchor directly. librustzcash derives it as `anchor = target height −
confirmations` (`zcash_client_backend` `propose_transfer`, which reads the target from the wallet's own
stored chain tip), and it has no "use this anchor" option. So `vintage send` sets the confirmations
instead:

```mermaid
sequenceDiagram
    participant U as User
    participant V as vintage send
    participant D as zcash-devtool
    participant W as Wallet DB
    participant N as lightwalletd

    U->>V: --before bitget-2026-09 --to <addr> --value <zat>
    V->>D: wallet sync
    D->>N: compact blocks
    D->>W: notes, tree, tip
    V->>V: H_a = newest multiple of 144 below H
    V->>W: retained checkpoint at H_a? which notes are older than H_a?
    V->>W: target = tip + 1  (get_target_and_anchor_heights)
    V->>V: N = target − H_a
    V->>W: propose_transfer(confirmations = N), read-only
    V->>V: assert proposal anchor == H_a
    V->>U: show usable notes, amount, fee, "old anchor is visible" warning
    U->>V: yes
    V->>D: wallet send --min-confirmations N
    D->>N: broadcast
    V->>W: read the signed tx, check its anchor == root at H_a
    V->>U: txid + "vintage verify <txid> --before …"
```

Design points:

- **Grid heights.** H_a is a multiple of 144 (the [ZIP 318](https://zips.z.cash/zip-0318) anchor grid).
  librustzcash wallets keep a durable tree checkpoint at those heights, so a witness can still be built
  there long after ordinary checkpoints are pruned. A wallet can only make a Vintage send for H if it
  was scanning before H_a.
- **No race.** Nothing syncs between reading the target and proposing, so the tip can't move between
  them. A new block between two runs just changes N; the anchor stays on H_a (observed: N 171 → 172,
  anchor 3,525,696 both times).
- **Vintage never touches the seed.** Signing happens in zcash-devtool. Vintage proposes read-only first,
  then devtool re-proposes from the same unchanged wallet state and gets the same plan.
- **Spending is restricted, not just displayed.** With 0.02 ZEC in the wallet, of which 0.015 is older
  than H_a, asking for 0.016 fails with `Insufficient balance (have 1500000 …)`.

## Checking a transaction

Given a transaction and a height H:

```mermaid
flowchart TD
    A[Fetch the mined transaction] --> B{Inputs only from Ironwood?}
    B -- "transparent, Sapling or Orchard inputs,<br/>or no Ironwood spends" --> NP[NOT PROVABLE]
    B -- yes --> C[Read its anchor]
    C --> D[Compute the Ironwood root after every block from H to the tip]
    D --> E{Anchor equals any of them?}
    E -- yes --> F["FAIL: the anchor is a snapshot from H or later"]
    E -- no --> P["PASS: consensus says the anchor is some earlier block's root,<br/>and it isn't any root from H on, so it is from before H"]
```

PASS needs no knowledge of where the anchor is from. It only needs to rule out every snapshot from H
onwards. That is complete, because there are finitely many blocks from H to the tip.

**When block H added no notes.** If no notes were added between some block A < H and H, the roots at A
and H are identical, so the anchor matches a root "from H on". Vintage checks the root at H − 1: if it
is the same, the anchor names the pool as it already was before H, so every spent note existed before
H, and the verdict is PASS. (Before 25 Oct this case was a conservative FAIL.) Mainnet example: the demo
payment's anchor is the pool as of block 3,525,693, and blocks 3,525,694 to 3,525,696 added no notes.
Against H = 3,525,694 it now PASSes; against H = 3,525,693 it FAILs, correctly.

## Proven age, with no event

The anchor gives more than a yes or no. The tree is append-only, so a root names exactly one tree size,
and the first block whose tree reached that size is the moment every spent note was in the pool.
Vintage finds that block by searching down from the transaction (`roots::locate`, from the same cached,
server-checked roots) and reports it with its time:

```
$ vintage age da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7
age:         in the pool by block 3525693 (2026-10-21 16:03 UTC)
             × bitget-2026-09         block   3501240  Bitget hot-wallet exploit: first shielding into Ironwood
             ✓ vintage-demo-2026-10   block   3525709  Vintage demo: simulated event (not a real hack)
```

One lookup answers every event at once: the coins predate each event whose height is above that block.
Every report from `GET /verify` carries the same `age`, and `GET /age/{txid}` gives it alone. For an
ordinary transaction the age is a few blocks before it was mined, which is true and not interesting;
for a Vintage send it is the date the holder chose to prove.

## The fast verifier

The naive check asks the server for each block's root (`GetTreeState`), at about 40 per second. Against
the Bitget event that is ~25,000 blocks, and on mainnet ~63% of blocks add Ironwood notes (15,685 of
24,762). That came to 294 s.

Instead, Vintage rebuilds the roots itself from compact blocks, in parallel chunks, and cross-checks
the server:

```mermaid
flowchart LR
    subgraph chunk["each chunk of 500 blocks (in parallel)"]
        S["server tree at block s−1<br/>(GetTreeState)"] --> X
        CB["compact blocks s..e<br/>(GetBlockRange, Ironwood only)"] --> X
        X["append each action's cmx in order<br/>check size == ironwoodCommitmentTreeSize per block<br/>root after each block that added notes"]
        X --> CHK{"computed tree at e ==<br/>server tree at e?<br/>(size and root)"}
        E["server tree at block e"] --> CHK
    end
    CHK -- no --> ERR[stop with an error, no verdict]
    CHK -- yes --> CACHE[("cache<br/>sizes by height, roots by size")]
```

- The tree only grows, so its root is determined by its size. The cache stores **sizes by height** and
  **roots by size**.
- Only blocks at least 100 below the tip go into the SQLite cache. Newer ones are kept in memory, tagged
  with the tip's block hash, and dropped when the tip changes. So a reorg can't leave a wrong root
  behind.
- The proto fields used: `ChainMetadata.ironwoodCommitmentTreeSize`, `CompactTx.ironwoodActions[].cmx`,
  `BlockRange.poolTypes = [IRONWOOD]` (`zcash_client_backend/proto`).

Measured on mainnet against Bitget (~25,000 blocks):

| | Time |
|---|---|
| Naive, one root request per block | 294 s |
| Fast, first check (16 cores) | 19–25 s |
| Fast, first check (4 / 2 / 1 cores) | 41 / 64 / 90 s |
| Fast, CLI repeat check (cache) | 1.5–1.7 s |
| API, repeat check (open connection + memory layer) | 0.20–0.39 s |

Peak memory is 15–17 MB in every case.

## The event registry

`registry/events.json` labels block heights, for example "Bitget hot-wallet exploit: first shielding
into Ironwood, block 3,501,240", with the source transactions and links. Each event has a type: `hack`,
`tax_cutoff`, `bug_window`, `snapshot` or `custom`. It is signed with ed25519
(`registry/events.json.sig`, public key `registry/pubkey.hex`), and every tool checks the signature
before using it. (The first version of the file was `incidents.json`; it still loads.)

The registry is a convenience, not an authority. It **names no people and no addresses**, and anyone
can verify against any height they like (`--before 3501240` works without it). It is not a blocklist:
it never says a coin is bad, only "this is when something happened".

## What the verdicts mean

| Verdict | Meaning | What it does **not** mean |
|---|---|---|
| **PASS** | Every Ironwood note this transaction spends existed before block H, and so before every later event too. | Nothing about who sent it, or where the coins came from before that. |
| **FAIL** | Vintage can't show the inputs predate H: the anchor is a snapshot from H or later. | That the coins are stolen. Almost every ordinary transaction FAILs: in a sample of 119 recent mainnet Ironwood transactions, all 70 with Ironwood spends had anchors 1–11 blocks old. |
| **NOT PROVABLE** | The transaction spends transparent, Sapling or Orchard coins, or no Ironwood notes. Vintage can only reason about Ironwood. | Anything about those coins. |

A verdict on an unmined transaction is marked `provisional`. In v6 transactions the anchor is
authorizing data and not part of the txid ([ZIP 229](https://zips.z.cash/zip-0229)), so until it is
mined, the same txid could be mined with a different anchor. See [THREAT_MODEL.md](THREAT_MODEL.md).
