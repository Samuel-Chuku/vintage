//! `vintage request --to <addr> --amount <zec> --before <event|height>`: ask a payer for a Vintage
//! payment, as a ZIP-321 URI.
//!
//! ZIP 321 has no field for "use an anchor before H", and an unknown `req-` parameter would make other
//! wallets reject the URI, so the rule travels in each shielded payment's memo: `VINTAGE before=<H>`.
//! A Vintage-aware payer (`vintage send --uri`) reads it from there; any other wallet still pays.

use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use vintage_core::VintageNetwork;
use zcash_address::ZcashAddress;
use zcash_keys::address::Address;
use zcash_protocol::{
    memo::{Memo, MemoBytes},
    value::Zatoshis,
};
use zip321::{Payment, TransactionRequest};

use crate::send::fmt_amount;

pub(crate) const MEMO_PREFIX: &str = "VINTAGE before=";

/// Parse a decimal ZEC amount ("0.003") into zatoshis, rejecting more than 8 decimals.
pub(crate) fn parse_zec(s: &str) -> Result<Zatoshis> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty() && frac.is_empty() || frac.len() > 8 || !(whole.chars().chain(frac.chars())).all(|c| c.is_ascii_digit()) {
        bail!("invalid ZEC amount `{s}` (e.g. 0.003, at most 8 decimals)");
    }
    let zat = whole.parse::<u64>().unwrap_or(0) * 100_000_000 + format!("{frac:0<8}").parse::<u64>()?;
    Zatoshis::from_u64(zat).map_err(|_| anyhow!("amount `{s}` is out of range"))
}

/// The `before` height carried by a request's Vintage memos, if any. All of them must agree.
pub(crate) fn before_from_request(req: &TransactionRequest) -> Result<Option<u32>> {
    let mut found: Option<u32> = None;
    for p in req.payments().values() {
        let Some(Ok(Memo::Text(t))) = p.memo().map(Memo::try_from) else { continue };
        let Some(rest) = t.strip_prefix(MEMO_PREFIX) else { continue };
        let h: u32 = rest
            .split_whitespace()
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| anyhow!("malformed Vintage memo `{}`", &*t))?;
        match found {
            Some(prev) if prev != h => bail!("the request's Vintage memos disagree ({prev} vs {h})"),
            _ => found = Some(h),
        }
    }
    Ok(found)
}

pub(crate) fn run(network: VintageNetwork, to: &[String], amounts: &[String], before: u32, label: &str) -> Result<()> {
    if to.len() != amounts.len() || to.is_empty() {
        bail!("give one --amount per --to ({} --to, {} --amount)", to.len(), amounts.len());
    }
    let memo_text = format!("{MEMO_PREFIX}{before}");
    let mut payments = vec![];
    let mut transparent = vec![];
    for (addr, amount) in to.iter().zip(amounts) {
        if Address::decode(&network.params(), addr).is_none() {
            bail!("`{addr}` is not a valid {} address", network.name());
        }
        let recipient = ZcashAddress::from_str(addr).map_err(|e| anyhow!("`{addr}`: {e}"))?;
        let memo = if recipient.can_receive_memo() {
            Some(MemoBytes::from(Memo::from_str(&memo_text).map_err(|e| anyhow!("memo: {e:?}"))?))
        } else {
            transparent.push(addr.clone());
            None
        };
        payments.push(Payment::new(recipient, Some(parse_zec(amount)?), memo, None, None, vec![]).map_err(|e| anyhow!("{e:?}"))?);
    }
    let req = TransactionRequest::new(payments).map_err(|e| anyhow!("{e:?}"))?;
    let total = req.total().ok().flatten().map_or(0, |z| z.into_u64());

    println!("Vintage payment request: pay with coins that existed before block {before} ({label})");
    for (to, amount) in to.iter().zip(amounts) {
        println!("  {} → {to}", fmt_amount(network, parse_zec(amount)?.into_u64()));
    }
    println!("  total {}", fmt_amount(network, total));
    if transparent.len() < to.len() {
        println!("  memo  \"{memo_text}\" (on each shielded payment; ZIP 321 has no field for it)");
    }
    if !transparent.is_empty() {
        println!(
            "  note: transparent recipients can't carry a memo; tell the payer `--before {before}` directly for: {}",
            transparent.join(", ")
        );
    }
    println!("\n{}", req.to_uri());
    println!("\nThe payer runs:  vintage send --wallet <dir> -i <identity> --uri '<the URI above>'");
    println!("Then anyone can check the payment:  vintage verify <txid> --before {before}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zec_amounts() {
        assert_eq!(parse_zec("0.003").unwrap().into_u64(), 300_000);
        assert_eq!(parse_zec("1").unwrap().into_u64(), 100_000_000);
        assert_eq!(parse_zec(".5").unwrap().into_u64(), 50_000_000);
        assert_eq!(parse_zec("0.00000001").unwrap().into_u64(), 1);
        assert!(parse_zec("0.000000001").is_err());
        assert!(parse_zec("1e3").is_err());
        assert!(parse_zec("").is_err());
    }
}
