//! The ZK link against the real mainnet pair: deposit `df151ee2…9402` spends two notes from the Vintage
//! payment `da9017e0…e3a7` (demo/mainnet-zk-link.json). Offline tests use the two raw transactions
//! saved in tests/fixtures; regenerate them with `cargo test -p vintage-verifier --test zk_link -- --ignored write_fixtures`.

use std::{path::PathBuf, sync::OnceLock};

use orchard::circuit::VerifyingKey;
use vintage_core::VintageNetwork;
use vintage_verifier::{
    link, lwd, resolve_before,
    report::VerifyError,
    zk_link::{self, ProofCheck, ZkLinkFile},
};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::BranchId;

const FUNDING: &str = "da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7";
const DEPOSIT: &str = "df151ee2e670d683018edd85bb1bf8eca2ccefe3ff464853368c1ff949db9402";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(txid: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{txid}.hex"))
}

fn tx(txid: &str) -> Transaction {
    let raw = hex::decode(std::fs::read_to_string(fixture(txid)).unwrap().trim()).unwrap();
    let tx = Transaction::read(&raw[..], BranchId::Nu6_3).unwrap();
    assert_eq!(tx.txid().to_string(), txid);
    tx
}

fn file() -> ZkLinkFile {
    serde_json::from_str(&std::fs::read_to_string(root().join("demo/mainnet-zk-link.json")).unwrap()).unwrap()
}

fn vk() -> &'static VerifyingKey {
    static VK: OnceLock<VerifyingKey> = OnceLock::new();
    VK.get_or_init(zk_link::verifying_key)
}

fn check(funding: &str, deposit: &str, f: &ZkLinkFile) -> ProofCheck {
    zk_link::check_proofs(vk(), &tx(funding), &tx(deposit), f).unwrap()
}

#[test]
fn genuine_proofs_cover_every_input() {
    let c = check(FUNDING, DEPOSIT, &file());
    assert!(c.invalid().is_empty(), "{:?}", c.proofs);
    assert_eq!(c.inputs.len(), 2);
    assert_eq!(c.uncovered(), 0);
    assert_eq!(c.proven_value_zat, 600_000);
    assert_eq!(c.funding_outputs, 3);
}

#[test]
fn tampered_proof_is_rejected() {
    let mut f = file();
    let mut bytes = hex::decode(&f.proofs[0]).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 1;
    f.proofs[0] = hex::encode(bytes);
    match zk_link::check_proofs(vk(), &tx(FUNDING), &tx(DEPOSIT), &f) {
        Ok(c) => assert!(!c.proofs[0].valid && c.proofs[1].valid, "{:?}", c.proofs),
        Err(VerifyError::BadRequest(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn proofs_for_another_payment_are_rejected() {
    // Claim the deposit was funded by itself: the anchor is not that payment's outputs root.
    let c = check(DEPOSIT, DEPOSIT, &file());
    assert!(c.proofs.iter().all(|p| !p.valid && p.reason.as_deref().unwrap().contains("anchor")), "{:?}", c.proofs);
}

#[test]
fn proofs_against_another_deposit_are_rejected() {
    // Present the proofs against a transaction that doesn't spend those notes.
    let c = check(FUNDING, FUNDING, &file());
    assert!(c.proofs.iter().all(|p| !p.valid && p.reason.as_deref().unwrap().contains("not spent by the deposit")), "{:?}", c.proofs);
}

#[test]
fn proofs_relabelled_for_another_pair_are_rejected() {
    // The signatures sign the deposit and funding txids, so editing the file's deposit breaks them.
    let mut f = file();
    f.deposit = FUNDING.into();
    let c = zk_link::check_proofs(vk(), &tx(FUNDING), &tx(DEPOSIT), &f).unwrap();
    assert!(c.proofs.iter().all(|p| !p.valid && p.reason.as_deref().unwrap().contains("signature")), "{:?}", c.proofs);
}

#[test]
fn a_missing_input_fails() {
    let mut f = file();
    f.proofs.truncate(1);
    let c = check(FUNDING, DEPOSIT, &f);
    assert!(c.invalid().is_empty());
    assert_eq!(c.uncovered(), 1);
}

#[test]
fn a_duplicate_proof_fails() {
    let mut f = file();
    f.proofs[1] = f.proofs[0].clone();
    let c = check(FUNDING, DEPOSIT, &f);
    assert!(c.proofs[0].valid);
    assert!(c.proofs[1].reason.as_deref().unwrap().contains("duplicate"));
    assert_eq!(c.uncovered(), 1);
}

#[test]
fn malformed_files_are_bad_requests() {
    let mut f = file();
    f.proofs[0] = "zz".into();
    assert!(matches!(zk_link::check_proofs(vk(), &tx(FUNDING), &tx(DEPOSIT), &f), Err(VerifyError::BadRequest(_))));
    f.proofs = vec![];
    assert!(matches!(zk_link::check_proofs(vk(), &tx(FUNDING), &tx(DEPOSIT), &f), Err(VerifyError::BadRequest(_))));
}

async fn mainnet() -> lwd::Client {
    lwd::connect(VintageNetwork::Main.default_server()).await.unwrap()
}

/// Full check over the network, including the funding payment's age (needs mainnet access).
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn mainnet_pass_and_event_before_the_payment() {
    let client = mainnet().await;
    let reg = || vintage_verifier::load_registry(&root().join("registry"));
    let net = VintageNetwork::Main;
    let pass = zk_link::verify_zk_link(&client, net, None, vk(), &file(), resolve_before(reg, net, "vintage-demo-2026-10").unwrap(), 8).await.unwrap();
    assert_eq!(pass.verdict, "PASS", "{}", pass.summary);
    // An event before the payment's anchor (3,525,696): the proofs hold, the age check doesn't.
    let fail = zk_link::verify_zk_link(&client, net, None, vk(), &file(), resolve_before(reg, net, "3525600").unwrap(), 8).await.unwrap();
    assert_eq!(fail.verdict, "FAIL");
    assert!(fail.summary.contains("age check"), "{}", fail.summary);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn write_fixtures() {
    let client = mainnet().await;
    std::fs::create_dir_all(fixture(FUNDING).parent().unwrap()).unwrap();
    for txid in [FUNDING, DEPOSIT] {
        let f = link::fetch(&client, VintageNetwork::Main, txid).await.unwrap();
        std::fs::write(fixture(txid), hex::encode(&f.raw) + "\n").unwrap();
    }
}
