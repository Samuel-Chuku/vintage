```
ZIP: Unassigned
Title: Anchor-Dated Provenance for Shielded Spends
Owners: Vintage team
Status: Draft (not submitted)
Category: Informational / Wallet
Created: 2026-10-22
License: MIT
```

# Terminology

The key words "MUST", "SHOULD", "MAY" and "RECOMMENDED" are to be interpreted as described in BCP 14.

- **Anchor**: the `anchorIronwood` field of a v6 transaction ([ZIP 229]); one per transaction, shared by
  all its Ironwood actions.
- **Root at h**: the root of the Ironwood note commitment tree after block h.
- **Anchor-dated spend**: a transaction whose anchor was deliberately chosen as the root at a height
  before some boundary H.
- **Grid height**: a multiple of the anchor bucket interval in [ZIP 318] (144 blocks before NU7).

# Abstract

This ZIP describes a property that existing Zcash consensus already provides: a transaction can prove
that every Ironwood note it spends existed before block H, by using an anchor from before H. It
specifies how a wallet creates such a spend, how anyone can verify it from public chain data, and the
privacy trade-off. It needs no consensus change.

# Motivation

When stolen funds enter a shielded pool, exit rails (exchanges and bridges) can't tell whether any
shielded deposit contains them. Today the only way for an honest holder to show otherwise is to give up
privacy: unshield their history, or disclose a viewing key. For the large class of holders whose coins
predate the incident, the anchor gives a narrower answer: "these coins existed before H", and nothing
else.

# Specification

## The property

Consensus requires: "The anchorIronwood field of the transaction, whenever it exists (i.e. when there
are any Ironwood Action descriptions), MUST refer to some earlier block's final Ironwood treestate."
([Protocol specification], as cited in Zebra `zebra_state/src/service/check/anchors.rs`.) Each Ironwood
action's proof shows the spent note (if its value is non-zero) is a leaf of the tree with that root.

Therefore, if a mined transaction T's anchor is the root at some height A < H, every non-zero Ironwood
note T spends was created at or before block A.

## Verification

Given a mined transaction T and a height H:

1. If T has transparent inputs, Sapling spends, Orchard spends, or no Ironwood bundle with spends
   enabled, the result is NOT PROVABLE.
2. Let `a` = T's anchor. For every height h in [H, tip], compute the root at h.
3. If `a` equals any of them, the result is FAIL. Otherwise it is PASS.

PASS is correct because `a` must equal the root at some earlier block (consensus), and that block is not
in [H, tip], so it is before H. FAIL is conservative: if the tree didn't change between some A < H and H,
the root at A equals the root at H, and a spend anchored at A FAILs. Verifiers SHOULD report this case
(the root at H − 1 equals the matched root).

Implementations MAY compute roots from compact blocks (append each Ironwood action's `cmx` in order),
and SHOULD cross-check them against `GetTreeState` at intervals. Roots depend only on tree size, so they
MAY be cached by size for blocks deep enough to be final.

Verifiers MUST treat a result on an unmined transaction as provisional. In v6 the anchor is
authorizing data and not part of the txid ([ZIP 229]), so the same txid can be mined with a different
anchor.

## Wallet behaviour

To make an anchor-dated spend for boundary H, a wallet:

1. SHOULD use the newest grid height H_a < H, because wallets retain tree checkpoints at grid heights
   ([ZIP 318]). The wallet MUST hold a checkpoint at H_a.
2. MUST select only notes mined at or before H_a, and MUST build every witness against the root at H_a.
3. With librustzcash, which derives `anchor = target − confirmations`, MAY set the confirmations
   requirement to `target − H_a` for both trusted and untrusted notes, reading the target from the
   wallet's stored tip with no sync in between, and MUST check the resulting proposal's anchor is H_a.
4. MUST NOT make anchor-dated spends by default, and SHOULD warn the user that an old anchor is visible
   on chain.

# Privacy implications

An anchor-dated spend is distinguishable: ordinary wallets use anchors a few blocks old (1–11 blocks
in a sample of 70 recent mainnet Ironwood spends), and an anchor-dated spend's anchor is typically
hundreds to thousands of blocks old. It reveals that the spender chose to prove age, and roughly when
their coins last moved. This is acceptable where the spender is already known to the recipient, such as
an exchange deposit. It is NOT RECOMMENDED for ordinary payments.

# Security considerations

- A PASS proves the age of the spent notes, not the spender's identity or right to them.
- The verifier must trust its chain data source. Production verifiers SHOULD use their own node.
- No new cryptographic assumptions beyond the Ironwood Action circuit's soundness and the
  collision resistance of its Merkle tree.

# Reference implementation

Vintage: `vintage send` (wallet side, via zcash-devtool) and `vintage verify` / `vintage-api` (verifier).
Mainnet and testnet evidence is in the repository README.

# Possible library improvement

librustzcash `propose_transfer` derives the anchor from a confirmations policy only. An option to
propose at an explicit anchor height (failing if the wallet lacks a checkpoint there) would make
anchor-dated spends direct and harder to get wrong.

# References

- [ZIP 229]: Version 6 Transaction Format (Draft)
- [ZIP 318]: Orchard to Ironwood Migration (Draft)
- [ZIP 311]: Zcash Payment Disclosures (Draft)
- [Protocol specification], § Actions

[ZIP 229]: https://zips.z.cash/zip-0229
[ZIP 318]: https://zips.z.cash/zip-0318
[ZIP 311]: https://zips.z.cash/zip-0311
[Protocol specification]: https://zips.z.cash/protocol/protocol.pdf#actions
