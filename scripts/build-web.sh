#!/usr/bin/env bash
# Assemble the two Cloudflare Pages projects from web/:
#   dist/landing  -> usevintage.site       (web/shared + web/landing)
#   dist/app      -> app.usevintage.site   (web/shared + web/app)
# Cloudflare Pages: build command "bash scripts/build-web.sh", output directory "dist/landing" or "dist/app".
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist
for site in landing app; do
  mkdir -p "dist/$site"
  cp -r web/shared/. "dist/$site/"
  cp -r "web/$site/." "dist/$site/"
done
du -sh dist/landing dist/app
