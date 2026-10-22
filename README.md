# Vintage

**Prove your shielded ZEC isn't the hacker's: with time, not a blocklist.**

Today, proving your shielded ZEC is clean means unshielding your history or handing over a viewing
key. Vintage proves it with time instead. A Vintage transaction shows that **every coin it spends
existed before block H**, for example before a hack's first inflow. It uses a field that every
Ironwood spend already carries, and consensus already enforces it.

Railgun and Privacy Pools need a curated list to prove innocence. Vintage needs a block height.

- No new cryptography, no consensus change, no trusted setup
- No list of people or addresses: the registry only labels block heights
- Anyone can check any transaction against any height, from public chain data

ZECATHON 2026, Core & Tooling track.

## Why

- When stolen funds enter the shielded pool, exit rails can't tell whether a shielded deposit contains
  them. NEAR Intents' SHIELD screening rejected more than $50M of Bitget-linked swaps. Since the
  2026-09-30 Bitget inflow into Ironwood, it can't tell whether any shielded deposit contains those coins.
- A Zcash forum thread (*"My experience exiting shielded ZEC from Zodl to NEAR Intents: $589k USDT still
  held 50 days…"*, forum.zcashcommunity.com/t/57497) shows the cost. The only "proof of innocence"
  available was to give up privacy, which the author called "unshielding on request".
- Honest framing: Vintage would **not** have helped that author, whose swap predates Ironwood and whose
  review wasn't tied to a specific incident. Vintage rules out **known incidents** for coins that predate
  them. It is not a full source-of-funds answer.

## How it works

To spend shielded coins, a wallet proves "my note is in the pool" against a snapshot of the pool (the
**anchor**). Consensus only accepts a snapshot that existed at the end of an earlier block. Vintage makes
the wallet use one from **before the hack**. Coins that arrived later aren't in it and can't be spent,
and anyone can check that the transaction's snapshot is older than the hack.

```mermaid
flowchart LR
    W["vintage send --before bitget-2026-09"] -->|"anchor: snapshot before the hack"| T[("deposit tx<br/>on chain")]
    T --> V["vintage verify / GET /verify"]
    V -->|"anchor ≠ any snapshot from H to tip"| P["PASS: these coins existed before the hack"]
```

Details: **[docs/HOW_IT_WORKS.md](docs/HOW_IT_WORKS.md)**. Limits and privacy cost:
**[docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)**.

## Quickstart (60 seconds)

Build (Rust 1.89+). Release builds; the first build compiles the Zcash crates.

```sh
cargo build --release      # binaries: target/release/vintage, target/release/vintage-api
```

Check any mainnet transaction against the Bitget incident (no wallet needed):

```sh
vintage verify ef383512f19ff8356d96a0f81dd6cb62ec3909f2b8d8f4e5a632523af34d50ff --before bitget-2026-09
vintage verify <txid> --before 3501240 --json        # any height; JSON for machines
```

Exit codes: `0` PASS, `1` FAIL, `2` not provable.

Run the HTTP verifier:

```sh
vintage-api --listen 127.0.0.1:8080
curl 'localhost:8080/verify/<txid>?before=bitget-2026-09'
```

Make a Vintage send from a [zcash-devtool](https://github.com/zcash/zcash-devtool) wallet. Always
`--dry-run` first; it shows which notes qualify and stops before signing:

```sh
vintage status --wallet <wallet-dir> --sync              # how much of your ZEC predates each incident
vintage send --wallet <wallet-dir> -i <age-identity> \
  --to <address> --value <zatoshis> --before <incident-or-height> --dry-run
```

`vintage send` never reads your seed. It syncs and signs through zcash-devtool, which must sit next to
the `vintage` binary or be passed with `--devtool`.

## Integrating Vintage into exit-rail screening

One call per confirmed deposit:

```
GET /verify/{deposit_txid}?before={incident}
```

- **PASS**: every shielded note the deposit spends existed before the incident's first inflow, so none
  of it can be the incident's coins. This is enforced by consensus, not claimed by the user. Use it to
  skip incident-related review.
- **FAIL** or **NOT_PROVABLE**: no information. Nearly every ordinary transaction FAILs. Never treat it
  as a flag.
- Act only on mined transactions (`"provisional": false`), and run the verifier against your own node.

Full reference: **[docs/API.md](docs/API.md)**.

## Evidence

### Real incident: Bitget, 2026-09-30

From ZachXBT's post (t.me/investigations/364), confirmed on mainnet via lightwalletd `GetTransaction`:

| txid | Height | Time (UTC) | Into Ironwood |
|---|---|---|---|
| [`9bf8d134…4e1a`](https://mainnet.zcashexplorer.app/transactions/9bf8d13474eff83ee9fcb214a3df7e1d1acbb745a0f820c837c08eb35e1e4e1a) | **3,501,240** | 08:15:37 | 876.42648607 ZEC |
| [`64e5a66c…73a2`](https://mainnet.zcashexplorer.app/transactions/64e5a66cce034745dfe1447b4779ce37b910d99365d0583cef65eb0504b573a2) | 3,501,242 | 08:19:07 | 876.42319409 ZEC |
| [`61d2cb33…b258`](https://mainnet.zcashexplorer.app/transactions/61d2cb335127636f2854e587dd41fecf2ec31c9c1e9e5e5b543c3a416048b258) | 3,501,265 | 08:46:11 | 993.46054874 ZEC |

Registry entry `bitget-2026-09`: H = 3,501,240.

### Mainnet demo (simulated incident on the real chain)

| Step | txid | Height |
|---|---|---|
| "honest" wallet funded, 0.015 ZEC | [`ef383512…50ff`](https://mainnet.zcashexplorer.app/transactions/ef383512f19ff8356d96a0f81dd6cb62ec3909f2b8d8f4e5a632523af34d50ff) | 3,525,666 |
| grid anchor H_a | (none) | 3,525,696 |
| "flagged" wallet funded = simulated incident `vintage-demo-2026-10` | [`e66bb554…3577`](https://mainnet.zcashexplorer.app/transactions/e66bb554e869a376e56753c03ba3637ec08a439fe409ac53600d9473e68d3577) | 3,525,709 |
| tainted transfer flagged → honest, 0.005 ZEC | [`a4c7622f…c143`](https://mainnet.zcashexplorer.app/transactions/a4c7622f2e3e39ce0cd5a65fc4417f490568990a6f90fda5579a924a0d83c143) | 3,525,726 |

- The honest wallet's tree has a witness for the honest note at H_a and **none for the tainted note**
  (`NO WITNESS: Tree does not contain a root at address … 824517`).
- `vintage send --dry-run --before vintage-demo-2026-10` selects only the honest note, and refuses to
  spend more than it (`have 1500000`).
- The tainted transfer verifies **FAIL** against the incident; its anchor is the root at 3,525,722.
- Headline demo, in progress: a Vintage send as the deposit into a NEAR Intents 1Click swap, ZEC → USDC.

### Testnet end-to-end

`c129e3e334285b2c60ab3d4ce674dd8b8e44355e09794aca9f9602aa57ac038e`, a `vintage send` mined at testnet
block 4,526,573, anchored at grid height 4,523,328 (about 3,245 blocks old; consensus accepted it).
`vintage --network test verify c129e3e3… --before 4523400` gives **PASS**; `--before 4523328` gives
**FAIL**, correctly, because that is the anchor's own block.

### Ordinary mainnet traffic

119 Ironwood transactions from 40 recent mainnet blocks, each verified against Bitget: **70 FAIL** (all
with anchors 1–11 blocks old), **49 NOT_PROVABLE** (transparent inputs), **0 PASS**. No false PASS.

## Benchmarks

Mainnet, checking against Bitget (~25,000 blocks, ~16,000 distinct Ironwood roots):

| | Time |
|---|---|
| One root request per block (naive) | 294 s |
| First check, 16 / 4 / 2 / 1 cores | 19–25 / 41 / 64 / 90 s |
| CLI repeat check (cache) | 1.5–1.7 s |
| API repeat check | 0.20–0.39 s |

Peak memory is 15–17 MB. The cache is about 2 MB.

## Roadmap

**One-hop provenance**: letting someone who was *paid* by a Vintage send after the incident prove their
coins came from it, without disclosing keys. It's researched and backed by a working spike that uses
the existing Ironwood circuit (`research/onehop-spike`). See **[docs/ROADMAP.md](docs/ROADMAP.md)**. A
draft ZIP describing the property is in **[docs/ZIP_DRAFT.md](docs/ZIP_DRAFT.md)**.

## Repository

```
crates/vintage-core       no I/O: grid anchors, tx parsing, the verdict, the signed registry
crates/vintage-verifier   lightwalletd access, roots from compact blocks + cache, verification report
crates/vintage-cli        `vintage`: verify, send, status, tx, witness, registry
crates/vintage-api        `vintage-api`: the HTTP verifier
registry/                 incidents.json + ed25519 signature + public key
research/onehop-spike     one-hop feasibility spike
patches/                  the zcash-devtool `propose --min-confirmations` patch
docs/                     HOW_IT_WORKS, THREAT_MODEL, API, ROADMAP, ZIP_DRAFT
```

## License

MIT OR Apache-2.0.
