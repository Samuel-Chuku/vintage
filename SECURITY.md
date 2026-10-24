# Security

Vintage is a hackathon project (ZECATHON 2026) and has not been audited. Its claims rest on Zcash
consensus, and the verifier is meant to be run by anyone, so a wrong verdict is the most serious kind of
bug. The threat model, including what an attacker can and can't do, is in
[docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

## Reporting a problem

Please report privately through GitHub's
[security advisories](https://github.com/Samuel-Chuku/vintage/security/advisories/new) for this
repository, not in a public issue. Most useful:

- a transaction and event height for which the verifier says `PASS` but any input is newer than the
  event, or says `FAIL` for a transaction whose anchor predates it;
- a way to make `POST /verify-link` accept a deposit whose inputs don't all come from the funding
  payments;
- anything that makes the API or the website record, log or leak a txid, an IP address or a viewing key;
- a forged event registry entry that the signature check accepts.

## What the public service keeps

`api.usevintage.site` keeps no request log. Its only record is anonymous daily counts by verdict and
event (`GET /stats`). Viewing keys sent to `POST /verify-link` are held in memory for that request and
never written anywhere. The website sets no cookies and loads nothing from third parties.
