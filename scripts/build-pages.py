#!/usr/bin/env python3
"""Build the two static sites from web/ into dist/ (standard library only).

    dist/landing  -> https://usevintage.site      (home, how-it-works, events, privacy, simulate)
    dist/app      -> https://app.usevintage.site  (the live verifier)

Each page in web/pages/<name>.html is the content of <main>. This script wraps it in the shared head,
nav and footer, copies web/shared and web/static/<site>, and writes each site's _headers with the
Content-Security-Policy hash of the one inline script, plus sitemap.xml. Run via scripts/build-web.sh.
"""
import base64
import html
import re
import hashlib
import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB, DIST = ROOT / "web", ROOT / "dist"
HOME, APP, API = "https://usevintage.site", "https://app.usevintage.site", "https://api.usevintage.site"
REPO = "https://github.com/Samuel-Chuku/vintage"

# site, output path, nav key, title, description, scripts
PAGES = {
    "home": ("landing", "index.html", None, "Vintage · Proof of age for private money",
             "Prove your shielded Zcash coins existed before a hack, a tax cutoff or any block you name, from one transaction, "
             "without unshielding anything. Checked by consensus, verified from public chain data.", ["site.js"]),
    "how-it-works": ("landing", "how-it-works/index.html", "how", "How Vintage works · Vintage",
                     "How Vintage proves shielded Zcash coins predate an event, and how exchanges and bridges integrate the "
                     "verifier API: diagrams, request and response examples, decision rules.", ["site.js"]),
    "events": ("landing", "events/index.html", "events", "Events · Vintage",
               "Events are dated blocks: hacks, tax cutoffs, bug windows, snapshots, or any block you name. See the signed "
               "registry and what a Vintage proof says about each.", ["site.js"]),
    "privacy": ("landing", "privacy/index.html", "privacy", "Privacy · Vintage",
                "What Vintage sees, what it keeps (nothing but anonymous daily counts), and what each party learns when you "
                "prove your coins' age.", ["site.js"]),
    "simulate": ("landing", "simulate/index.html", "simulate", "Simulate · Vintage Lab",
                 "A live lab session on Zcash mainnet data: drag your anchor to get a deposit through, move an event and "
                 "watch the verdict follow, and try five attacks against the real verifier.", ["site.js", "sim.js"]),
    "cli": ("landing", "cli/index.html", "how", "Command line · Vintage",
            "The vintage command: check any Zcash transaction's age from your own machine, make Vintage payments, "
            "and link coins received after an event. Every command, with real output.", ["site.js"]),
    "api": ("landing", "api/index.html", "how", "API · Vintage",
            "The Vintage verifier over HTTP: /verify, /age, /verify-link, /events, /stats and /health, each with a "
            "live try-it button on Zcash mainnet.", ["site.js"]),
    "app": ("app", "index.html", "verify", "Verify a deposit · Vintage",
            "Check whether every coin a Zcash transaction spends existed before a hack, a cutoff or any block you name. "
            "Live, from public chain data.", ["site.js", "verify.js"]),
    "404": ("landing", "404.html", None, "Not found · Vintage", "This page does not exist.", ["site.js"]),
    "404-app": ("app", "404.html", None, "Not found · Vintage", "This page does not exist.", ["site.js"]),
}

# Applies a theme passed in the URL fragment (#theme=dark, set when moving between usevintage.site and
# app.usevintage.site, whose browser storage is separate), remembers it, removes it from the address
# bar, then applies the saved theme before first paint. "auto" (or nothing saved) follows the device:
# no data-theme attribute, so the CSS prefers-color-scheme rules apply. No cookies.
THEME_JS = ("(function(){var d=document.documentElement,m=/[#&]theme=(light|dark|auto)/.exec(location.hash);"
            "if(m){if(m[1]!=='auto')d.setAttribute('data-theme',m[1]);var h=location.hash.replace(/[#&]theme=(light|dark|auto)/,'').replace(/^&/,'');"
            "try{history.replaceState(null,'',location.pathname+location.search+(h?(h.charAt(0)==='#'?h:'#'+h):''))}catch(e){}}"
            "try{if(m)localStorage.setItem('vt-theme',m[1]);var t=localStorage.getItem('vt-theme');if(t==='light'||t==='dark')d.setAttribute('data-theme',t)}catch(e){}})();")
THEME_HASH = "sha256-" + base64.b64encode(hashlib.sha256(THEME_JS.encode()).digest()).decode()
CSP = (f"default-src 'none'; script-src 'self' '{THEME_HASH}'; style-src 'self' 'unsafe-inline'; font-src 'self'; "
       f"img-src 'self' data:; connect-src {API}; base-uri 'none'; form-action 'none'")

def build_version():
    """Short git hash of the commit being built (Cloudflare Pages builds from a checkout). Falls back to a
    hash of web/shared, so a build outside git still changes the version when an asset changes."""
    try:
        return subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True,
                              check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        h = hashlib.sha256()
        for f in sorted((WEB / "shared").rglob("*")):
            if f.is_file():
                h.update(f.read_bytes())
        return h.hexdigest()[:7]


VERSION = build_version()

NE = ('<svg class="ne" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M3 9L9 3M4 3h5v5" '
      'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square"/></svg>')

NAV = [("how", "How it works", "/how-it-works/"), ("events", "Events", "/events/"), ("privacy", "Privacy", "/privacy/"),
       ("simulate", "Simulate", "/simulate/"), ("verify", "Verify", APP + "/")]


# The three "How it works" pages share a switcher, pinned under the main nav.
SUBNAV = [("how-it-works", "Overview", "/how-it-works/"), ("cli", "Command line", "/cli/"), ("api", "API", "/api/")]


def subnav(name):
    if name not in {key for key, _, _ in SUBNAV}:
        return ""
    links = "".join(
        f'<a href="{href}"{' aria-current="page"' if key == name else ""}>{label}</a>' for key, label, href in SUBNAV)
    return f'<div class="subnav-slot"><nav class="subnav" aria-label="How it works">{links}</nav></div>\n'


def url(site, path):
    base = HOME if site == "landing" else APP
    return base + "/" + path.replace("index.html", "")


def nav(site, current):
    home = "" if site == "landing" else HOME
    links = []
    for key, label, href in NAV:
        h = href if href.startswith("http") else home + href
        cur = ' aria-current="page"' if key == current else ""
        links.append(f'<a href="{h}" data-nav="{key}"{cur}>{label}</a>')
    full = "".join(f'<span class="hide-sm">{a}</span>' for a in links)
    return f'''<nav class="nav" id="nav" aria-label="Main"><div class="wrap">
  <a class="mark" href="{home or ''}/">VINTAGE</a>
  <div class="navlinks label">
    {full}
    <button type="button" class="toggle" id="toggle" aria-label="Switch colour theme"><svg class="sun" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg><svg class="moon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/></svg><b class="auto" aria-hidden="true">A</b></button>
    <button class="menu-btn" id="menubtn" aria-expanded="false" aria-controls="mnav">Menu</button>
  </div>
</div>
<div class="mnav label" id="mnav" hidden>{"".join(links)}</div>
</nav>'''


def footer(site):
    home = "" if site == "landing" else HOME
    return f'''<footer><div class="wrap cols">
  <a class="mark" href="{home}/">VINTAGE</a>
  <nav class="label" aria-label="Footer"><a href="{REPO}" rel="noopener noreferrer">GitHub {NE}</a><a href="{REPO}/blob/main/docs/THREAT_MODEL.md" rel="noopener noreferrer">Threat model {NE}</a><a href="{home}/api/">API</a><a href="{home}/cli/">Command line</a><span>Zecathon 2026</span></nav>
  <p style="flex-basis:100%;margin:0;font-size:14px;color:var(--vt-ink-2)">No cookies, no analytics, no third-party requests. Fonts are served from this site. Static site served by Cloudflare. Your txid goes from your browser straight to our verifier, which keeps no logs.</p>
</div></footer>'''


def jsonld():
    data = {
        "@context": "https://schema.org",
        "@graph": [
            {"@type": "WebApplication", "name": "Vintage verifier", "url": APP + "/", "applicationCategory": "FinanceApplication",
             "operatingSystem": "Any (web)", "isAccessibleForFree": True,
             "description": "Checks whether every coin a Zcash Ironwood transaction spends existed before a given block, from public chain data."},
            {"@type": "SoftwareSourceCode", "name": "Vintage", "codeRepository": REPO, "programmingLanguage": "Rust",
             "license": ["https://opensource.org/license/mit", "https://opensource.org/license/apache-2-0"], "description":
             "Proof of age for private money: anchor-dated provenance for shielded Zcash spends. CLI, HTTP verifier API and website."},
        ],
    }
    return '<script type="application/ld+json">' + json.dumps(data, separators=(",", ":")) + "</script>\n"


def faq_jsonld(body):
    """schema.org FAQPage from the page's <details><summary>Q</summary><p>A</p></details> blocks."""
    qa = re.findall(r"<details><summary>(.*?)</summary><p>(.*?)</p></details>", body, re.S)
    if not qa:
        return ""
    text = lambda s: html.unescape(re.sub(r"<[^>]+>", "", s)).strip()
    data = {"@context": "https://schema.org", "@type": "FAQPage", "mainEntity": [
        {"@type": "Question", "name": text(q), "acceptedAnswer": {"@type": "Answer", "text": text(a)}} for q, a in qa]}
    return '<script type="application/ld+json">' + json.dumps(data, separators=(",", ":"), ensure_ascii=False) + "</script>\n"


def page(name):
    site, out, current, title, desc, scripts = PAGES[name]
    canonical = url(site, out)
    body = subnav(name) + (WEB / "pages" / f"{name.removesuffix('-app')}.html").read_text()
    robots = '<meta name="robots" content="noindex">\n' if out == "404.html" else ""
    head = f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{desc}">
{robots}<meta name="referrer" content="no-referrer">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<link rel="canonical" href="{canonical}">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Vintage">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:url" content="{canonical}">
<meta property="og:image" content="{HOME}/og.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta name="twitter:card" content="summary_large_image">
<link rel="stylesheet" href="/css/tokens.css?v={VERSION}">
<link rel="stylesheet" href="/css/vintage.css?v={VERSION}">
<script>{THEME_JS}</script>
{jsonld() if name == "home" else ""}{faq_jsonld(body)}</head>
<body data-page="{name.removesuffix('-app')}">
<a class="skip tkt" href="#main"><span class="lbl">Skip to content</span><span class="stub" aria-hidden="true">↓</span></a>

'''
    js = "".join(f'<script src="/js/{s}?v={VERSION}" defer></script>\n' for s in scripts)
    html = head + nav(site, current) + f'\n\n<main class="wrap" id="main">\n{body}</main>\n\n' + footer(site) + "\n\n" + js + "</body>\n</html>\n"
    dest = DIST / site / out
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(html)
    return canonical


HEADERS = f"""# Generated by scripts/build-pages.py. Cloudflare Pages response headers.
/*
  Content-Security-Policy: {CSP}; frame-ancestors 'none'
  Referrer-Policy: no-referrer
  X-Content-Type-Options: nosniff
  Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()
  Cross-Origin-Opener-Policy: same-origin
  Cache-Control: no-transform

/assets/*
  Cache-Control: public, max-age=31536000, immutable

# Pages link CSS and JS as ?v=<commit>; the bare paths must still never go stale (without this the
# zone default of 4 hours applies).
/css/*
  Cache-Control: public, max-age=0, must-revalidate

/js/*
  Cache-Control: public, max-age=0, must-revalidate

# Nothing may be rewritten in transit: Cloudflare auto-injects its Web Analytics beacon into proxied
# pages unless the response says no-transform (developers.cloudflare.com/web-analytics/get-started). It
# is set on /* so the 404 page, served at any unknown path, has it too. Pages joins the values of every
# matching rule, so HTML gets "no-transform, public, max-age=0, must-revalidate".
"""


def main():
    if DIST.exists():
        shutil.rmtree(DIST)
    urls = {"landing": [], "app": []}
    for name, (site, out, *_rest) in PAGES.items():
        canonical = page(name)
        if out != "404.html":
            urls[site].append(canonical)
    for site in ("landing", "app"):
        d = DIST / site
        shutil.copytree(WEB / "shared", d, dirs_exist_ok=True)
        if (WEB / "static" / site).exists():
            shutil.copytree(WEB / "static" / site, d, dirs_exist_ok=True)
        html_paths = set()
        for f in d.rglob("*.html"):
            rel = "/" + str(f.relative_to(d))
            html_paths.add(rel)
            if rel.endswith("/index.html"):
                html_paths.add(rel.removesuffix("index.html"))
                if rel != "/index.html":
                    html_paths.add(rel.removesuffix("/index.html"))
            else:
                html_paths.add(rel.removesuffix(".html"))
        rules = "".join(f"{p}\n  Cache-Control: public, max-age=0, must-revalidate\n\n" for p in sorted(html_paths))
        (d / "_headers").write_text(HEADERS + rules)
        sm = "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls[site])
        (d / "sitemap.xml").write_text('<?xml version="1.0" encoding="UTF-8"?>\n'
                                       '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + sm + "</urlset>\n")
    print("built", ", ".join(f"{s}: {len(u)} page(s)" for s, u in urls.items()), "| version", VERSION, "| CSP script hash", THEME_HASH)


if __name__ == "__main__":
    main()
