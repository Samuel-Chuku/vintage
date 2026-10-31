# zcash-devtool patches

`vintage send` signs through [zcash-devtool](https://github.com/zcash/zcash-devtool). Build it from
commit `0b3436b` with these three patches, applied in this order:

| Patch | What it does |
|---|---|
| `zcash-devtool-propose-min-confirmations.patch` | `wallet propose --min-confirmations N`, so Vintage can pick the anchor |
| `zcash-devtool-pay-min-confirmations.patch` | the same for `wallet pay` (ZIP 321 requests, `vintage send --uri`) |
| `zcash-devtool-nu7-releases.patch` | builds against the librustzcash NU7 releases (21 Oct 2026: `zcash_protocol` 0.11.0, `zcash_client_sqlite` 0.23.0, …), which know the mainnet NU7 height (3,543,405) and the ZIP 318 grid after it |

```sh
git clone https://github.com/zcash/zcash-devtool && cd zcash-devtool
git checkout 0b3436b
for p in propose-min-confirmations pay-min-confirmations nu7-releases; do
  git apply /path/to/vintage/patches/zcash-devtool-$p.patch
done
cargo build --release      # put target/release/zcash-devtool next to the vintage binary
```

Without the NU7 patch, a devtool built from `0b3436b` doesn't know mainnet NU7 and can't send once it
activates. The patch only changes the dependency versions and ports five commands that aren't used by
Vintage (`inspect`, three `pczt` commands, `migration advance`) to the renamed library APIs.

Back up wallet folders before the first run with the new libraries: opening a wallet migrates its
database.
