#!/usr/bin/env bash
# publish-demo-repo.sh — build the synthetic demo hub and publish it as the PUBLIC example repo
# (default codeshrew/confer-demo), so anyone can clone it and `confer serve` a real, populated confer
# hub. Synthetic data only — no real hub, no real identities (commits are authored by demo roles like
# backend@confer.local; code refs point only at the public confer repo).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFER="${CONFER:-$REPO/target/release/confer}"
[ -x "$CONFER" ] || CONFER="$REPO/target/debug/confer"
[ -x "$CONFER" ] || { echo "no confer binary — cargo build first" >&2; exit 1; }
DEST="${DEST:-codeshrew/confer-demo}"
GIT_META=(-c user.name=backend -c user.email=backend@confer.local -c commit.gpgsign=false)

echo "▸ building demo hub…"
CONFER="$CONFER" bash "$REPO/scripts/demo-hub.sh" >/tmp/confer-demo-build.log 2>&1 \
  || { echo "build failed:"; tail -8 /tmp/confer-demo-build.log; exit 1; }
WORK="$(cat "$REPO/.demo-hub-path")"; HUB="$WORK/clones/backend"

echo "▸ writing the confer-demo hub README…"
# The README lives as a maintainable static file (with the served-dashboard gallery) rather than an
# inline heredoc — easier to edit and no shell-quoting hazards. Canonical repo is codeshrew/confer-demo.
cp "$REPO/scripts/demo-repo-README.md" "$HUB/README.md"
( cd "$HUB" && git "${GIT_META[@]}" add README.md \
  && git "${GIT_META[@]}" commit -q -m "confer-demo hub readme" \
  && git push -q origin HEAD ) >/dev/null

if ! gh repo view "$DEST" >/dev/null 2>&1; then
  echo "▸ creating public ${DEST} …"
  gh repo create "$DEST" --public \
    --description "A live, explorable confer hub — synthetic demo data. https://github.com/codeshrew/confer" >/dev/null
fi

echo "▸ pushing hub history to ${DEST} …"
( cd "$HUB" \
  && git remote remove publish 2>/dev/null || true
  git remote add publish "https://github.com/$DEST.git"
  git push -q publish HEAD:main )

echo "✓ published: $(gh repo view "$DEST" --json url -q .url)"
