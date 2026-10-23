# Mainnet demo: reproduce it

Two real NEAR Intents swaps (ZEC → USDC on Base) on 2026-10-23, both made with Vintage and both
checkable from public chain data. Event: `vintage-demo-2026-10` (a staged event at block 3,525,709; see
`registry/events.json`).

## 1. Old coins: the deposit itself proves its age

The deposit is a Vintage payment: one transaction paid the NEAR deposit address and funded the receipt
account below.

```sh
vintage verify da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7 --before vintage-demo-2026-10
# PASS: these coins existed before block 3525709
curl 'https://api.usevintage.site/verify/da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7?before=vintage-demo-2026-10'
```

NEAR Intents delivered 3.668896 USDC: Base tx
[`0xc1f7ac73…a7a4`](https://basescan.org/tx/0xc1f7ac737d646c31d00539006887bf507488a5f9115bffe5d433eb1cdb9aa7a4).

## 2. Fresh coins: the receipt-account link

A single-use receipt account received two notes from the payment above, then made deposit `df151ee2…`
to a second NEAR swap. `mainnet-link-bundle.json` lets anyone check that every coin in that deposit
came from the Vintage payment.

```sh
vintage link verify demo/mainnet-link-bundle.json --before vintage-demo-2026-10
# PASS: every coin this deposit spends came from 1 payment whose coins existed before block 3525709
```

NEAR Intents delivered 5.52164 USDC: Base tx
[`0xf25a0201…fc22`](https://basescan.org/tx/0xf25a0201f6521cf7aca2174b6f3f30bc5cf6a45b95a96447612a95d1580ffc22).

**What the bundle discloses, by design:** the receipt account's viewing key, therefore the funding
payments, the amounts received, the receipt address, and the deposit. Nothing from the user's other
accounts. The viewing key cannot spend. This account was single-use and is retired; it still holds
0.00135 ZEC of change, which anyone with the key can see.
