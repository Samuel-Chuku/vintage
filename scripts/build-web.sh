#!/usr/bin/env bash
# Build the two Cloudflare Pages projects from web/ (see scripts/build-pages.py):
#   dist/landing  -> usevintage.site       (home, how it works, events, privacy, simulate)
#   dist/app      -> app.usevintage.site   (the live verifier)
# Cloudflare Pages: build command "bash scripts/build-web.sh", output directory "dist/landing" or "dist/app".
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/build-pages.py
du -sh dist/landing dist/app
