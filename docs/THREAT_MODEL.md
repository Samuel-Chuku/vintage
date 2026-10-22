# Threat model

What Vintage proves, what it doesn't, what it costs in privacy, and who has to be trusted. Read
[HOW_IT_WORKS.md](HOW_IT_WORKS.md) first.

## What a PASS proves

**Every Ironwood note spent by the transaction existed before block H.**

That's all. It says nothing about identity, amounts, the coins' history before H, or anything after the
transaction. It rests on two things:

1. **Consensus:** the transaction's `anchorIronwood` must be the root of the Ironwood tree after some
   earlier block, and the spend proof shows every spent note is in that tree.
2. **The verifier's chain view:** the roots it computed for blocks H..tip are the real ones (see
   [Who you trust](#who-you-trust)).

## What it can't do

| Limit | Why |
|---|---|
| **Coins received after H** can never PASS against H. | They aren't in any tree from before H. This is the core limit: once incident coins enter the pool, any later note could descend from them, so a proof with zero disclosure can only say "older than H". The [one-hop extension](ROADMAP.md#one-hop-provenance) addresses it with a small, targeted disclosure. |
| **Transparent, Sapling or Orchard inputs** → NOT PROVABLE. | Vintage reads only the Ironwood anchor. Sapling and Orchard have their own anchors, and the same idea could apply to them later, but Ironwood is where post-2026-07-28 shielded value lives. |
| **Someone else's old keys.** | A thief holding a victim's keys to coins from before H can make a Vintage send that PASSes. PASS is about the coins' age, not about who holds them. |
| **The hacker's own old coins** PASS. | Correctly: those coins aren't the stolen ones. A PASS never says "this person is innocent". |
| **Coverage shrinks for active users.** | Every time you spend, your change is a new note. Coins untouched since before H keep qualifying; coins that keep moving don't. |
| **The wallet must have been scanning before H.** | A Vintage send needs a retained tree checkpoint at the grid height below H. A wallet restored later must rescan from before H to rebuild it. |

## The privacy cost: old anchors are visible

A Vintage send's anchor is old, and anyone looking at the chain can see that.

- Ordinary wallets anchor a few blocks back. In 119 recent mainnet Ironwood transactions, the 70 with
  Ironwood spends had anchors **1–11 blocks old** (median 4).
- A Vintage send anchored before an incident is hundreds to thousands of blocks old. Our testnet send was
  ~3,245 blocks old.
- [ZIP 318](https://zips.z.cash/zip-0318) migration transfers also use older anchors (1–4 grid steps,
  roughly 12 hours at most), but they spend Orchard notes, not Ironwood, so a Vintage send is still
  distinguishable from them.

So a Vintage send signals "this person chose to prove their coins are old", and roughly when their
coins arrived. **This is acceptable for a deposit to an exchange or bridge**, which already knows who
the depositor is. **Wallets should never use it by default**, and `vintage send` warns before every send.

## Fungibility

**The concern.** If exchanges start preferring Vintage deposits, shielded ZEC splits in two:
"provably old" coins and everything else. People who can't or won't prove age could be treated worse.

**Why Vintage is still a net improvement:**

- It's **opt-in** and **marks no one**. No coin is ever labelled bad, and no list of addresses or people
  exists anywhere.
- It replaces something worse. Today, after an incident, the only "proof of innocence" available is to
  unshield your history or hand over a viewing key (see the Zcash forum thread on a $589k NEAR Intents
  hold). Or rails screen everyone with blanket suspicion. A one-transaction proof that reveals only "older
  than H" is less invasive than either.
- Coverage is highest right after an incident, which is exactly when exits get frozen.

**The counter-argument, stated fairly.** An opt-in signal can become a de facto requirement. If a rail
starts to *require* Vintage, then holders of new coins, people who received a payment after H, are
disadvantaged through no fault of their own. Vintage should be one way to clear a hold, never the only
one. We recommend rails use a PASS to **skip** extra review, and never treat a FAIL as evidence of
anything.

## Dusting doesn't work against it

Taint-based screening can be gamed by sending a victim a few stolen coins ("dusting"). With Vintage
that does nothing. Any note received after H is automatically excluded from a Vintage send against H,
and the victim's old coins still PASS. We observed this on mainnet: the wallet held a tainted 0.005 ZEC
note from after the incident, and Vintage refused to spend it but spent the old 0.015 note.

## The registry

- It **labels heights**. Each entry gives a block height, a label, source transaction ids and public
  links. No addresses or identities are ever "flagged".
- It's signed (ed25519). Every tool checks the signature before use, and the API serves the exact signed
  bytes so anyone can check it independently.
- **Key compromise:** someone with the signing key could publish a wrong height. That can't turn a FAIL
  into a PASS for the true height. Anyone can verify against any height directly, and should for anything
  high-stakes, using a height they've checked against the incident's transactions themselves.

## Who you trust

**The chain data source.** The verifier reads transactions, compact blocks and tree states from a
lightwalletd/Zaino server. It cross-checks that server against itself: every computed root must
match the server's own tree state at each chunk boundary, and tree sizes must match block by block.
But a malicious server could lie consistently, for example serve a fake transaction or a fake chain.

- **For anything that matters, verify against your own node.** An exchange already runs one. Point
  `--server` at your own lightwalletd or Zaino in front of your own zebrad, and the verdict is exactly as
  trustworthy as your node.
- The public API is a convenience for demos and quick checks.

**The API operator** learns which transaction ids are checked and when. Self-host `vintage-api` if that
matters. It is one binary, uses ~16 MB of RAM, and needs only network access to a lightwalletd.

## Unmined transactions: same txid, different anchor

In v6 transactions the anchor is **authorizing data**: it isn't part of the txid
([ZIP 229](https://zips.z.cash/zip-0229)). Someone holding the spending key can therefore make two
versions of the same transaction, same txid, different anchors, with an old anchor in the one they show
you and a new one in the one they get mined.

- Once mined, the chain fixes one version, and the verifier reads the mined bytes.
- So **only a verdict on a mined transaction is final**. Vintage marks unmined verdicts
  `"provisional": true`, and rails should re-check after confirmation, which they already wait for anyway.

## Reorgs

- If the block an anchor refers to is reorged out, the transaction becomes invalid by consensus and the
  wallet rebuilds it. A Vintage send can only be mined with an anchor on the winning chain.
- Verifier: the root cache only stores blocks at least 100 below the tip. Newer blocks are kept in memory
  tied to the tip's block hash and discarded when the tip changes. A computed tree that disagrees with
  the server's stops the check with an error rather than giving a verdict.

## Cryptographic assumptions

Nothing new. A PASS relies on the soundness of the Ironwood Action circuit (Halo2, post-NU6.3) and the
collision resistance of the Sinsemilla-based Merkle tree. If two different trees had the same root, an
anchor could "match" the wrong block. These are the same assumptions every Ironwood spend already
relies on.

## Known risks for operators

- **ZIP 318 grid after NU7.** ZIP 318 (Draft) moves the anchor grid from every 144 blocks to every 432
  after NU7 (mainnet 2026-11-05). On testnet, after NU7 (block 4,465,026), the librustzcash version
  Vintage builds on still retained checkpoints every 144 blocks, and consensus accepted our 144-grid
  anchor. Consensus doesn't care about the grid. But if wallets switch to 432, `vintage send` must follow
  the wallet's grid.
- **Server support.** The verifier needs a server that returns `ironwoodTree` in `GetTreeState` and
  Ironwood data in compact blocks. zec.rocks does.
