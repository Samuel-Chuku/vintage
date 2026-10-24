# Hosting the website on Cloudflare Pages

Two static sites, built from `web/` by `scripts/build-web.sh`, which runs `scripts/build-pages.py`
(Python 3 standard library only; Cloudflare's build image already has Python). Each page in
`web/pages/` is wrapped in the shared head, nav and footer; the script also writes `_headers`,
`sitemap.xml` and a `404.html` for each site:

| Pages project | Output directory | Domain | What it is |
|---|---|---|---|
| `vintage-landing` | `dist/landing` | `usevintage.site` | The story, how it works, evidence, privacy |
| `vintage-app` | `dist/app` | `app.usevintage.site` | The live verifier |

Both call only `https://api.usevintage.site` (the API on the VPS). Each folder carries a `_headers` file
that Cloudflare sends as HTTP headers: the Content-Security-Policy (only this site and the API),
`frame-ancestors 'none'`, `Referrer-Policy: no-referrer`, `nosniff`, and long caching for fonts.

## Create the two projects

In the Cloudflare dashboard: **Workers & Pages → Create → Pages**.

**With the GitHub repo connected (recommended):**

1. *Connect to Git*, pick the repository, production branch `main`.
2. Build settings: framework preset **None**; build command `bash scripts/build-web.sh`; build output
   directory `dist/landing` (for the app project: `dist/app`); root directory: the repository root.
3. Save and deploy. Repeat for the second project.

**Without Git:** run `bash scripts/build-web.sh` locally, then *Upload assets* and drag in `dist/landing`
(or `dist/app`). Re-upload after every change.

## Domains

In each project: **Custom domains → Set up a domain**: `usevintage.site` for the landing project,
`app.usevintage.site` for the app project. Cloudflare adds the DNS records itself, proxied (orange cloud);
that is how Pages serves them.

**Leave `api.usevintage.site` exactly as it is: an A record to the VPS, DNS only (grey cloud).** The
txid is part of every API request (`/verify/<txid>`), and a proxied record would let Cloudflare see which
transactions people check. The websites never see a txid: the browser sends it straight to the API.

## Privacy settings (turn these off)

Each of these makes Cloudflare inject a script or change the page; the CSP would block the script, and
some would break the page:

- The Pages project's **Web Analytics** (Settings → General): off.
- **Speed → Optimization → Rocket Loader**: off.
- **Scrape Shield → Email Address Obfuscation**: off.
- No Cloudflare Zaraz, no Browser Insights.

## Check it after deploying

```sh
curl -sI https://usevintage.site | grep -iE "content-security-policy|referrer-policy|x-content-type"
curl -sI https://app.usevintage.site | grep -iE "content-security-policy|referrer-policy"
node scripts/site-check.mjs https://usevintage.site /tmp/vintage-landing
node scripts/site-check.mjs https://app.usevintage.site /tmp/vintage-app --verify \
  da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7 "Vintage demo"
```

`site-check` lists every request each page makes. The only origins should be the site itself, inline
`data:` and `https://api.usevintage.site`.
