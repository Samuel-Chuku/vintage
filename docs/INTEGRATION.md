# Integrating Vintage: a guide for exchanges, bridges and swap services

This guide is for teams that hold or review shielded Zcash deposits after an incident. It covers what a
Vintage answer means, how to act on it, the request itself, self-hosting, and how to test against real
mainnet transactions. The full field and error reference is in [API.md](API.md).

## The problem it solves

After a hack, stolen coins that reach the shielded pool can't be told apart from anyone else's. A rail
then has two bad options: hold every shielded deposit for review, or accept the risk. Asking users to
prove where their coins came from today means viewing keys, transaction histories or unshielding, which
costs honest users their privacy and costs the rail review time.

Vintage gives a narrower, cheaper answer. From the deposit's txid alone, it proves that **every shielded
Ironwood coin the deposit spends already existed before the event's block**. Coins that entered the pool
after the event, including the hack's, can't be in it. The proof is the transaction's anchor, which Zcash
consensus checks on every spend, so the answer doesn't depend on trusting Vintage.

What you get:

- **About a second per check** once the event is warm (0.06 s on our server for a registered event).
- **No keys, no histories.** The user shares a txid you already have.
- **No list to maintain.** Events are dated block heights, not addresses or people.
- **Independent.** Run the same open-source verifier against your own node and get the same answer.

## What each answer means, and what to do

| Verdict | Meaning | Recommended action |
|---|---|---|
| `PASS` | Every shielded coin the deposit spends existed before the event, so none of it can be that event's coins. | Skip the review you hold because of that event. Keep every other check you run. |
| `FAIL` | The anchor is a pool state from the event or later. Nearly every ordinary deposit looks like this: wallets anchor a few blocks back (1 to 11 blocks in 119 mainnet transactions we sampled). | No information. Apply your normal process. **Never treat FAIL as a flag.** |
| `NOT_PROVABLE` | The deposit spends transparent, Sapling or Orchard coins. Vintage reads Ironwood only. | No information. Apply your normal process. |
| `provisional: true` | The transaction isn't mined yet. Until it is, the same txid could be mined with a different anchor. | Act only on a mined result, which you wait for anyway. |

A `PASS` is a statement about coins and one event. It is not a source-of-funds report and doesn't
identify anyone. One deposit that passes an older event also passes every later one.

**Make it opt-in.** Vintage is something an honest user offers to get through faster, never a
requirement for every depositor. Don't ask users to make their wallets anchor old by default: an old
anchor is visible on chain, which is fine for a deposit to an account that already knows the user, and
nowhere else.

## The flow

```
user wallet          Zcash mainnet         your exchange               Vintage API
    |  deposit, anchored     |                    |                          |
    |  before the event ---->|                    |                          |
    |                        |-- confirmed ------>|                          |
    |                        |                    |-- GET /verify/{txid}     |
    |                        |                    |   ?before={event} ------>|
    |                        |                    |<-- PASS + evidence ------|
    |<-- credited, no event review ---------------|                          |
```

1. A user who holds older coins sends with `vintage send --before <event>` (or any wallet that supports
   an anchor choice). Nothing else changes for them.
2. When the deposit confirms, your backend calls the verifier with the txid and the event.
3. On `PASS` (and `provisional: false`), release the event-related hold.

## The request

```
GET https://api.usevintage.site/verify/{txid}?before={event-id | block height}
```

`before` takes a registry event id (see `GET /events`) or a plain block height. Example:

```bash
curl 'https://api.usevintage.site/verify/da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7?before=vintage-demo-2026-10'
```

JavaScript:

```js
const res = await fetch(`https://api.usevintage.site/verify/${txid}?before=bitget-2026-09`);
const v = await res.json();
if (res.ok && v.verdict === "PASS" && !v.provisional) {
  skipEventReview(deposit);   // the coins predate the event
} else {
  normalProcess(deposit);     // FAIL is not a flag
}
```

Python:

```python
import requests

r = requests.get(f"https://api.usevintage.site/verify/{txid}",
                 params={"before": "bitget-2026-09"}, timeout=30)
v = r.json()
if r.ok and v["verdict"] == "PASS" and not v["provisional"]:
    skip_event_review(deposit)   # the coins predate the event
else:
    normal_process(deposit)      # FAIL is not a flag
```

Real response, trimmed:

```json
{
  "verdict": "PASS",
  "summary": "These coins existed before block 3525709 (...).",
  "provisional": false,
  "mined_height": 3527813,
  "before": { "height": 3525709, "event_type": "custom", "requested": "vintage-demo-2026-10" },
  "anchor": "855ccc99a779159d9d98664d1a09737814b930cc782b149660a94a3881d98502",
  "evidence": { "distinct_roots": 1303, "blocks_covered": 2105 },
  "elapsed_secs": 0.056
}
```

Store the `verdict`, `anchor`, `before.height` and `mined_height` with the deposit if you want an audit
trail: anyone can re-run the check later from those values and get the same answer.

### Errors and timeouts

Errors are JSON `{"error": "..."}` with status 400 (bad txid, unknown event, height out of range), 404
(transaction unknown to the server) or 502 (lightwalletd unreachable). Treat any error as "no
information" and apply your normal process. Use a timeout of about 30 seconds: a registered event
answers in about a second, but the first check against a new height can take up to about a minute while
the verifier fills its cache.

## Coins received after the event

A user whose coins arrived after the event can't prove their age. Two paths exist:

- **Payer side.** The person paying the user sends with an older anchor (`vintage request` makes a
  payment request that asks for this). The payment is then provably clean on arrival.
- **Receipt-account link.** The user receives that payment into a fresh single-use account and deposits
  from it. The rail checks the link: the payment passes, and every input of the deposit is one of its
  outputs. This needs that one account's viewing key, so it discloses that account's payments, amounts
  and address, and nothing from the user's other accounts. Run it locally with
  `vintage link verify <bundle.json> --before <event>` so the key goes to nobody else, or call
  `POST /verify-link` (stateless; the key is dropped when the response is sent). Details:
  [LINK.md](LINK.md).

## Run it yourself (recommended for production)

Our public instance is for demos and quick checks. In production, run `vintage-api` next to your own
Zcash node:

- Nobody, including us, learns which deposits you check.
- The answer depends only on your node.
- One binary, about 16 MB of memory, any core count.

```bash
cargo build --release -p vintage-api
vintage-api --network main --listen 127.0.0.1:8080 \
            --server <your-lightwalletd-or-zaino:443> --registry registry \
            --cache /var/lib/vintage/cache.sqlite
```

Put it behind a TLS reverse proxy with access logging off. A systemd unit and a proxy example are in
[`deploy/`](../deploy). Flags are documented in [API.md](API.md).

The event registry (`registry/events.json`) is signed with ed25519; the API checks the signature at
startup and serves the exact signed bytes at `/registry/events.json(.sig)`. You can keep your own
registry or skip it and pass block heights directly.

## Test against real mainnet transactions

| Txid | Event | Expected |
|---|---|---|
| `da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7` | `vintage-demo-2026-10` | `PASS` |
| `a4c7622f2e3e39ce0cd5a65fc4417f490568990a6f90fda5579a924a0d83c143` | `vintage-demo-2026-10` | `FAIL` (spends coins received after the event) |
| `df151ee2e670d683018edd85bb1bf8eca2ccefe3ff464853368c1ff949db9402` | `vintage-demo-2026-10` | `PASS` via `/verify-link` with [`demo/mainnet-link-bundle.json`](../demo/mainnet-link-bundle.json) |

`vintage-demo-2026-10` (block 3,525,709) is a staged event, not a real hack. The full demo, including
the two NEAR Intents swaps these transactions funded, is in [demo/README.md](../demo/README.md).

## Limits to keep in mind

- Ironwood only. Transparent, Sapling and Orchard inputs give `NOT_PROVABLE`.
- It proves age, not innocence: coins older than one event can still come from an earlier one. Check
  each event you care about; one call per event.
- An old anchor is public. Users choose when to reveal that they proved their coins' age.

The full list, with what an attacker can and can't do, is in [THREAT_MODEL.md](THREAT_MODEL.md).
