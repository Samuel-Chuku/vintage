//! Gate 0 check for the receipt-account link (see docs/ROADMAP.md), on real chain data.
//!
//! `gate0-check <network> <ufvk-file> <funding-txid> <spend-txid>`:
//! 1. decode the UFVK and take its Orchard full viewing key (Ironwood shares it);
//! 2. trial-decrypt every Ironwood output of the funding tx with the external and internal IVKs;
//! 3. recompute each recovered note's nullifier with `Note::nullifier(&fvk)`;
//! 4. list the spend tx's Ironwood nullifiers and report which recomputed ones appear there.

use anyhow::{Context, Result, anyhow};
use orchard::keys::{PreparedIncomingViewingKey, Scope};
use vintage_core::{VintageNetwork, tx::parse_txid};
use vintage_verifier::lwd;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_note_encryption::try_note_decryption;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId};

async fn fetch(client: &lwd::Client, network: VintageNetwork, txid: &str) -> Result<(Transaction, u32)> {
    let raw = lwd::get_tx(client, &parse_txid(txid)?).await?;
    let h = u32::try_from(raw.height).ok().filter(|h| *h > 0).ok_or_else(|| anyhow!("{txid} not mined"))?;
    let tx = Transaction::read(&raw.data[..], BranchId::for_height(&network.params(), BlockHeight::from_u32(h)))?;
    Ok((tx, h))
}

#[tokio::main]
async fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let [_, net, ufvk_file, funding, spend] = a.as_slice() else {
        return Err(anyhow!("usage: gate0-check <main|test> <ufvk-file> <funding-txid> <spend-txid>"));
    };
    let network: VintageNetwork = net.parse().map_err(|e: String| anyhow!(e))?;
    let ufvk = UnifiedFullViewingKey::decode(&network.params(), std::fs::read_to_string(ufvk_file)?.trim())
        .map_err(|e| anyhow!("UFVK: {e}"))?;
    let fvk = ufvk.orchard().context("UFVK has no Orchard component")?;
    println!("check 3: UFVK decoded; Orchard/Ironwood FVK present");

    let client = lwd::connect(network.default_server()).await?;
    let (ftx, fh) = fetch(&client, network, funding).await?;
    let (stx, sh) = fetch(&client, network, spend).await?;
    let fb = ftx.ironwood_bundle().context("funding tx has no Ironwood bundle")?;
    let sb = stx.ironwood_bundle().context("spend tx has no Ironwood bundle")?;
    println!("funding {funding}… mined {fh}: {} Ironwood actions", fb.actions().len());
    println!("spend   {spend}… mined {sh}: {} Ironwood actions", sb.actions().len());

    let spend_nfs: Vec<[u8; 32]> = sb.actions().iter().map(|a| a.nullifier().to_bytes()).collect();
    let mut matched = 0;
    for scope in [Scope::External, Scope::Internal] {
        let ivk = PreparedIncomingViewingKey::new(&fvk.to_ivk(scope));
        for (i, action) in fb.actions().iter().enumerate() {
            let domain = orchard::note_encryption::IronwoodDomain::for_action(action);
            if let Some((note, _addr, _memo)) = try_note_decryption(&domain, &ivk, action) {
                let nf = note.nullifier(fvk).to_bytes();
                let pos = spend_nfs.iter().position(|x| *x == nf);
                println!(
                    "check 2: decrypted output {i} ({scope:?}), value {} zat; recomputed nullifier {}… → {}",
                    note.value().inner(),
                    &hex::encode(nf)[..16],
                    match pos {
                        Some(p) => format!("FOUND as action {p} of the spend tx"),
                        None => "not spent in that tx".into(),
                    }
                );
                matched += pos.is_some() as usize;
            }
        }
    }
    println!(
        "check 1: {matched} recomputed nullifier(s) found among the spend tx's {} nullifiers",
        spend_nfs.len()
    );
    Ok(())
}
