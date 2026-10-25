# zk-link: the receipt-account link without a viewing key (research prototype)

The shipped link ([docs/LINK.md](../../docs/LINK.md)) proves that a deposit spent only outputs of a
Vintage payment, but the rail has to see the receipt account's viewing key. This prototype proves the
same statement with zero-knowledge proofs instead, and discloses no key.

**Status:** works on real mainnet data, unreviewed. Not part of the CLI or the API. Don't rely on it for
anything that matters until the construction has been reviewed (see [docs/ROADMAP.md](../../docs/ROADMAP.md)).

## How it works

For each note the deposit `D` spends, the holder proves an ordinary Ironwood spend of that note against
a *synthetic anchor*: the root of a Merkle tree whose leaves are exactly the funding payment `T1`'s
output commitments, which are public. The existing Ironwood circuit, unchanged, then shows:

- the note is one of `T1`'s outputs (its commitment is in that tree);
- its nullifier is the one `D` published on chain;
- the prover controls it (spend-authorization signature over a challenge bound to `D` and `T1`).

The proofs can't be broadcast, because consensus only accepts anchors that are real chain roots, but
anyone can verify them off-chain.

The verifier rebuilds the root from `T1` (two independent ways), verifies each proof and both
signatures, requires a **positive value balance** on each proof (the circuit skips the Merkle check for
zero-value notes; the binding signature proves the value), requires a proof for **every** nullifier in
`D`, and runs the ordinary Vintage age check on `T1`.

| Disclosed | Not disclosed |
|---|---|
| That every input of `D` is an output of `T1` | Any key (no viewing key, no `nk`) |
| Each input's value | Which output of `T1`, addresses, memos |

## Mainnet run (2026-10-24)

The deposit `df151ee2…9402` spends two notes from the Vintage payment `da9017e0…e3a7` (the same pair as
the published viewing-key bundle, [demo/README.md](../../demo/README.md)). The proof file is
[`demo/mainnet-zk-link.json`](../../demo/mainnet-zk-link.json).

```
$ zk-link prove --deposit df151ee2… --funding da9017e0… --wallet <receipt wallet> --identity <age key> --out zk-link.json
funding:  da9017e0…: 3 Ironwood outputs, 2 to this account
deposit:  df151ee2…: 2 Ironwood inputs
keys:     proving key built in 2.0s
proof:    input 60f41a42… proven and signed in 0.6s (5985 bytes)
proof:    input 3d933873… proven and signed in 0.6s (5985 bytes)
wall 4.00s, peak 92256KB

$ zk-link verify demo/mainnet-zk-link.json --before vintage-demo-2026-10
funding root:  2ea7abdeab8487130677b60487d0309a334b715a94e0a2b3ec063364b1809d2c (3 outputs of da9017e0…)
keys:          verifying key built in 1.3s
input 60f41a42…:  proof valid in 0.01s, value ≥ 300000 zat, came from the funding payment
input 3d933873…:  proof valid in 0.01s, value ≥ 300000 zat, came from the funding payment
coverage:      all 2 inputs of df151ee2… proven, 600000 zat in total
funding age:   PASS: These coins existed before block 3,525,709 (Vintage demo, a staged event, not a real hack).

ZK LINK PASS: every coin df151ee2… spends came from a payment whose coins existed before block 3,525,709.
Disclosed: each input's value. Not disclosed: any key, which output, addresses, memos.
wall 8.00s, peak 18152KB
```

Forgeries, each rejected:

| Forgery | Result |
|---|---|
| One bit flipped in a proof | `proof rejected (ConstraintSystemFailure)` |
| Claim the inputs came from another payment | `anchor is not the funding payment's outputs root` |
| Present the proofs for another deposit | `spend signature over the challenge fails` |
| Drop one proof | `LINK FAIL: 1 of the deposit's 2 inputs have no proof` |
| The same proof twice | `duplicate nullifier` |
| An event before the payment's anchor | `LINK FAIL: the funding payment doesn't predate block 3525600` |

## Run it

```
cargo build --release -p zk-link
zk-link verify demo/mainnet-zk-link.json --before vintage-demo-2026-10
```

Verification needs only the file and public chain data (default server zec.rocks).

## Limits

- **Every input must be proven.** Wallets pad transactions with dummy spends whose keys they discard, so
  a deposit with a dummy input can't pass. This deposit spends two real receipt notes and has no dummy.
  A Vintage-aware wallet could keep the dummy secrets, or a rail could accept a value-coverage rule.
- One hop, mainnet only, one funding payment per file.
- Needs a written spec (an Ironwood extension of ZIP 311) and cryptographic review before real use, and
  a check against NU7.
