# Real vs simulated in the Vintage Lab

The lab at [usevintage.site/simulate](https://usevintage.site/simulate/) mixes three kinds of step. Each
step is tagged on screen, and a legend under the session bar explains the tags.

| Tag | What it means | Where it runs |
|---|---|---|
| **Live** | A real request to the Vintage verifier (`api.usevintage.site`), the same production code as the CLI, against Zcash mainnet, made when you click. | Our verifier and a public lightwalletd server |
| **Sandbox** | Computed in your browser from real mainnet block heights. No request is made. | Your browser |
| **Replay** | A real result recorded on mainnet during our demo (21 to 23 October 2026), shown as it happened. | Nowhere: it is a recording |

## Station by station

| Station | Step | Kind | Notes |
|---|---|---|---|
| 1 · Get your deposit through | The wallet's notes and their arrival blocks | Real data | The demo wallet's two notes: 0.015 ZEC mined at 3,525,666 (`ef383512…`), 0.005 ZEC at 3,525,726 (`a4c7622f…`). |
| | Which notes the anchor allows | Sandbox | The consensus rule, applied locally: a note can be spent at an anchor only if it was in the pool by that block. |
| | Ledger Exchange | Simulated party | An illustrative exchange. Its holding and crediting are sandbox. |
| | The deposit's verdict | Sandbox | **A copy of the verifier's rule in JavaScript** (anchor before the event block). The arbitrary anchors you can drag to have no real transaction behind them, so the real verifier can't be asked about them. Every sandbox verdict is followed by a live check of a real transaction in the same situation. |
| | Check the real one live | Live | `GET /verify` on `da9017e0…` (PASS against the demo event) or `a4c7622f…` (FAIL). |
| 2 · Move the event | Date a transaction | Live | `GET /age/{txid}` for any mainnet txid. |
| | Dragging the event | Sandbox, from live data | The verdict compares the live age with the height you choose. "Confirm with the full verifier" sends `GET /verify` and shows whether it agrees. |
| 3 · Attack the verifier | Attacks 1 to 4 | Live | Each states the rejection it expects and shows whether that rule fired: a FAIL with a matched anchor, a FAIL with a matched anchor, a link FAIL for uncovered inputs, a 400 for a height before Ironwood. |
| | Attack 5 | Replay | Our wallet's real refusal on mainnet (`NO WITNESS` for the note at 3,525,726 at anchor 3,525,696). A browser can't build a Zcash transaction. |
| 4 · Paid after the event | Ordinary payment, age-only check | Live | `GET /verify` on the receipt deposit `df151ee2…`: FAIL, its coins arrived after the event. |
| | Vintage payment | Live | `GET /verify` on `da9017e0…`: PASS. |
| | Receipt account, deposit | Replay | The real transactions `da9017e0…` (outputs 1 and 2, 0.003 ZEC each) and `df151ee2…`. |
| | Viewing-key link | Live | `POST /verify-link` with the published bundle (`demo/mainnet-link-bundle.json`). |
| | Zero-knowledge link | Live, experimental | `POST /verify-zk-link` with the published proof file `demo/mainnet-zk-link.json`: the verifier checks both proofs and the payment's age. Making proofs doesn't run in the browser. The construction is unreviewed. Falls back to the recorded result (Replay) if the verifier can't be reached. |

## The session

- The session ID is made in your browser and never sent anywhere. The session exists only in that tab.
- Live requests count toward the verifier's anonymous daily totals (`GET /stats`), like any other check.
  Nothing else is recorded.
- "Copy log" exports the session's steps as plain text.

## What the lab does not do

- It moves no money. Vintage is a verifier; the only value that moved was in the mainnet demo itself.
- Live results depend on the chain tip, so evidence numbers (for example, how many pool states were
  ruled out) grow over time. Verdicts don't change.
- The sandbox steps in station 1 are not checked by the verifier, because there is no transaction to
  check. The live step after them is.
