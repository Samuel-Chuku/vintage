//! Feasibility spike for the one-hop extension (docs/ROADMAP.md).
//!
//! Claim under test: with the existing Ironwood (post-NU6.3) Action circuit and no new circuit, a
//! note's owner can prove "the note with nullifier `nf` is one of the outputs of transaction T1",
//! by proving an ordinary spend of that note against a *synthetic anchor*: the root of a Merkle tree
//! whose leaves are exactly T1's output commitments. Anyone can recompute that root from T1.
//!
//! The circuit enforces "v_old = 0 or computed root = anchor" (orchard circuit.rs:328), derives `nf`
//! from the note, and checks the note belongs to the prover's key. So a verifying proof whose anchor
//! is T1's synthetic root and whose `nf` appears in a later transaction T2 shows that T2 spent an
//! output of T1 (or a zero-value note), without revealing which output, the address or the memo.
//!
//! Everything here is local: no chain, no broadcast. Output: each check and its timing.

use std::time::Instant;

use anyhow::{Result, anyhow, ensure};
use incrementalmerkletree::{Hashable, Level, frontier::CommitmentTree};
use orchard::{
    Action, Anchor, Bundle,
    builder::{Builder, BundleType},
    bundle::{Authorized, BundleVersion},
    circuit::{OrchardCircuitVersion, ProvingKey, VerifyingKey},
    keys::{FullViewingKey, Scope, SpendAuthorizingKey, SpendingKey},
    note::{ExtractedNoteCommitment, Note},
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
    value::NoteValue,
};
use rand::{RngExt, rand_core::UnwrapErr, rngs::SysRng};
use zcash_note_encryption::try_note_decryption;

type Rng = UnwrapErr<SysRng>;
const DEPTH: usize = 32;

fn spending_key(rng: &mut Rng) -> SpendingKey {
    loop {
        let bytes: [u8; 32] = rng.random();
        if let Some(sk) = Option::from(SpendingKey::from_bytes(bytes)) {
            return sk;
        }
    }
}

/// An ordinary Ironwood payment ("T1"): outputs only, padded like a wallet would pad it.
fn payment(rng: &mut Rng, to: &[(&FullViewingKey, u64)]) -> Result<Bundle<orchard::builder::InProgress<orchard::builder::Unproven, orchard::builder::Unauthorized>, i64>> {
    let version = BundleVersion::ironwood_v3();
    let mut b = Builder::new(
        BundleType::Transactional { bundle_required: false, pad_to_minimum: None },
        version,
        version.default_flags(),
        Anchor::empty_tree(),
    )
    .map_err(|e| anyhow!("{e:?}"))?;
    for (fvk, v) in to {
        b.add_output(None, fvk.address_at(0u32, Scope::External), NoteValue::from_raw(*v), [0; 512])
            .map_err(|e| anyhow!("{e:?}"))?;
    }
    Ok(b.build::<i64>(&mut *rng).map_err(|e| anyhow!("{e:?}"))?.expect("bundle").0)
}

/// The recipient's view: trial-decrypt every action with their incoming viewing key.
fn received<'a, T: 'a>(actions: impl IntoIterator<Item = &'a Action<T>>, fvk: &FullViewingKey) -> Vec<Note> {
    let ivk = fvk.to_ivk(Scope::External).prepare();
    actions
        .into_iter()
        .filter_map(|a| try_note_decryption(&IronwoodDomain::for_action(a), &ivk, a).map(|(n, _, _)| n))
        .collect()
}

/// Root and authentication path for leaf `pos` in a depth-32 tree holding exactly `leaves`
/// (positions 0..n, the rest empty), as anyone can compute it from T1's public output commitments.
fn synthetic_path(leaves: &[MerkleHashOrchard], pos: usize) -> (MerkleHashOrchard, [MerkleHashOrchard; DEPTH]) {
    let mut nodes = leaves.to_vec();
    let mut idx = pos;
    let mut auth = Vec::with_capacity(DEPTH);
    for l in 0..DEPTH {
        let level = Level::from(l as u8);
        let empty = MerkleHashOrchard::empty_root(level);
        auth.push(nodes.get(idx ^ 1).copied().unwrap_or(empty));
        nodes = nodes
            .chunks(2)
            .map(|c| MerkleHashOrchard::combine(level, &c[0], c.get(1).unwrap_or(&empty)))
            .collect();
        idx /= 2;
    }
    (nodes[0], auth.try_into().unwrap())
}

fn leaves<'a, T: 'a>(actions: impl IntoIterator<Item = &'a Action<T>>) -> Vec<MerkleHashOrchard> {
    actions.into_iter().map(|a| MerkleHashOrchard::from_cmx(a.cmx())).collect()
}

/// The disclosure: a one-action Ironwood spend of `note` against `anchor`, proven and signed over
/// `challenge` (the spend authorization signature shows control of the key, as in ZIP 311). Never
/// broadcastable: consensus only accepts anchors that are real chain roots.
fn disclose(
    rng: &mut Rng,
    pk: &ProvingKey,
    sk: &SpendingKey,
    note: Note,
    anchor: Anchor,
    path: MerklePath,
    challenge: [u8; 32],
) -> Result<Bundle<Authorized, i64>> {
    let version = BundleVersion::ironwood_v3();
    let mut b = Builder::new(
        BundleType::Transactional { bundle_required: true, pad_to_minimum: Some(1) },
        version,
        version.default_flags(),
        anchor,
    )
    .map_err(|e| anyhow!("{e:?}"))?;
    b.add_spend(FullViewingKey::from(sk), note, path).map_err(|e| anyhow!("add_spend: {e:?}"))?;
    let (bundle, _) = b.build::<i64>(&mut *rng).map_err(|e| anyhow!("{e:?}"))?.expect("bundle");
    let proven = bundle.create_proof(pk, &mut *rng).map_err(|e| anyhow!("{e:?}"))?;
    proven
        .prepare(&mut *rng, challenge)
        .sign(&mut *rng, &SpendAuthorizingKey::from(sk))
        .finalize()
        .map_err(|e| anyhow!("{e:?}"))
}

fn main() -> Result<()> {
    let mut rng = UnwrapErr(SysRng);
    let (alice, bob, carol) = (spending_key(&mut rng), spending_key(&mut rng), spending_key(&mut rng));
    let (bob_fvk, carol_fvk) = (FullViewingKey::from(&bob), FullViewingKey::from(&carol));
    let _ = alice; // Alice's old-coin spend is what Vintage already proves; T1 here is its outputs.

    let t = Instant::now();
    let pk = ProvingKey::build(OrchardCircuitVersion::PostNu6_3);
    let vk = VerifyingKey::build(OrchardCircuitVersion::PostNu6_3);
    println!("keys:        post-NU6.3 (Ironwood) proving + verifying key built in {:.1}s", t.elapsed().as_secs_f64());

    // T1: Alice's Vintage-anchored payment to Bob and Carol. Its output commitments are public.
    let t1 = payment(&mut rng, &[(&bob_fvk, 150_000), (&carol_fvk, 50_000)])?;
    let t1_leaves = leaves(t1.actions());
    println!("T1:          {} Ironwood actions (outputs to Bob 150000 zat, Carol 50000 zat, padding)", t1_leaves.len());

    // Bob finds his note in T1 the way a wallet does.
    let bob_note = received(t1.actions(), &bob_fvk).pop().ok_or_else(|| anyhow!("Bob found no note"))?;
    let cmx = ExtractedNoteCommitment::from(bob_note.commitment());
    let pos = t1.actions().iter().position(|a| *a.cmx() == cmx).unwrap();
    println!("Bob:         decrypted his note in T1 (value {}), at output {pos}", bob_note.value().inner());

    // The synthetic anchor, recomputed two independent ways.
    let (root, auth) = synthetic_path(&t1_leaves, pos);
    let mut tree = CommitmentTree::<MerkleHashOrchard, 32>::empty();
    for l in &t1_leaves {
        tree.append(*l).map_err(|_| anyhow!("tree full"))?;
    }
    ensure!(tree.root() == root, "synthetic root disagrees with incrementalmerkletree");
    let anchor_t1 = Anchor::from(root);
    let path = MerklePath::from_parts(pos as u32, auth);
    ensure!(path.root(cmx) == anchor_t1, "path does not lead to the synthetic root");
    println!("anchor:      synthetic root of T1's outputs = {} (matches incrementalmerkletree)", hex::encode(anchor_t1.to_bytes()));

    // Bob's later deposit T2 spends this note, publishing its nullifier.
    let nf_in_t2 = bob_note.nullifier(&bob_fvk);

    // 1. The disclosure proof.
    let challenge = [7u8; 32]; // e.g. a hash of the exchange's nonce and the deposit txid
    let t = Instant::now();
    let d = disclose(&mut rng, &pk, &bob, bob_note, anchor_t1, path, challenge)?;
    println!("prove:       one-action disclosure proven and signed in {:.1}s", t.elapsed().as_secs_f64());

    // 2. What a verifier checks, from public data only (T1's outputs, T2's nullifiers, the disclosure).
    let t = Instant::now();
    d.verify_proof(&vk).map_err(|e| anyhow!("proof rejected: {e:?}"))?;
    let verify_secs = t.elapsed().as_secs_f64();
    ensure!(*d.anchor() == anchor_t1, "disclosure anchor is not T1's synthetic root");
    let action = d.actions().first();
    ensure!(*action.nullifier() == nf_in_t2, "disclosed nullifier is not the one T2 spends");
    action
        .rk()
        .verify(&challenge, &action.authorization().clone())
        .map_err(|_| anyhow!("spend authorization signature over the challenge fails"))?;
    println!("verify:      PASS in {verify_secs:.2}s: proof valid, anchor = T1's outputs root, nf = T2's spend, signature over the challenge valid");
    println!("             disclosed: nothing about which T1 output, Bob's address, the memo, or nk");

    // 3. Soundness checks: a note that did NOT come from T1.
    let t1b = payment(&mut rng, &[(&bob_fvk, 150_000)])?; // another payment to Bob
    let other = received(t1b.actions(), &bob_fvk).pop().unwrap();
    let other_cmx = ExtractedNoteCommitment::from(other.commitment());
    let other_pos = t1b.actions().iter().position(|a| *a.cmx() == other_cmx).unwrap();
    // 3a. The builder refuses to pair it with T1's root (its path leads to T1b's root).
    let (root_b, auth_b) = synthetic_path(&leaves(t1b.actions()), other_pos);
    let path_b = MerklePath::from_parts(other_pos as u32, auth_b);
    match disclose(&mut rng, &pk, &bob, other.clone(), anchor_t1, path_b.clone(), challenge) {
        Err(e) => println!("forge 1:     note from another tx with T1's anchor → builder refuses ({e})"),
        Ok(_) => anyhow::bail!("builder accepted a foreign note against T1's anchor"),
    }
    // 3b. Prove it honestly against its own tx, then present that proof as if its anchor were T1's.
    let d_b = disclose(&mut rng, &pk, &bob, other, Anchor::from(root_b), path_b, challenge)?;
    let forged: Vec<_> = d_b.actions().iter().map(|a| a.to_instance(*d_b.flags(), anchor_t1)).collect();
    match d_b.authorization().proof().verify(&vk, &forged) {
        Err(e) => println!("forge 2:     valid proof for another tx, relabelled with T1's anchor → verifier rejects ({e:?})"),
        Ok(()) => anyhow::bail!("verifier accepted a relabelled proof"),
    }
    // 3c. Claim a different nullifier (e.g. the tainted note T2 really spent).
    let carol_note = received(t1.actions(), &carol_fvk).pop().unwrap();
    let wrong_nf = carol_note.nullifier(&carol_fvk);
    let a = d.actions().first();
    let tampered = orchard::circuit::Instance::from_parts(anchor_t1, a.cv_net().clone(), wrong_nf, a.rk().clone(), *a.cmx(), *d.flags())
        .unwrap();
    match d.authorization().proof().verify(&vk, &[tampered]) {
        Err(e) => println!("forge 3:     Bob's proof presented for a different nullifier → verifier rejects ({e:?})"),
        Ok(()) => anyhow::bail!("verifier accepted a proof for a different nullifier"),
    }
    Ok(())
}
