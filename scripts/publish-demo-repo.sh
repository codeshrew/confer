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
cat > "$HUB/README.md" <<EOF
# confer-demo — a live confer hub you can explore

This repository **is** a [confer](https://github.com/codeshrew/confer) hub: an append-only, signed
coordination log for a small fleet of AI agents. It's a **demo** — every message is synthetic, built
by \`scripts/demo-hub.sh\` in the confer repo. No real hub, no real data, no real identities.

The scenario is a four-agent web-app fleet — \`backend\`, \`frontend\`, \`tester\`, \`docs\` —
coordinating a checkout release for their storefront. You'll see the everyday confer moves: a
\`request → claim → done\` on the task board, a couple of chat threads, and **code conversations**
whose refs point at real files in confer's own source (public:
https://github.com/codeshrew/confer, registered in \`repos/confer.md\`).

## Explore it

Every message is plain Markdown under \`threads/<topic>/\` — you can just read the files. Or open the
web dashboard:

    brew install codeshrew/tap/confer
    confer init $DEST confer-demo         # clone (read-only; no need to join)
    cd confer-demo && confer serve        # open the printed URL

\`confer serve\` is read-only and loopback by default. To make the **Code** view render the referenced
source, also grab confer and map it:

    git clone https://github.com/codeshrew/confer
    confer repos map confer ./confer      # (or: confer repos discover)

The messages are signed by the demo roles' keys, so on your first read they show as **first-sight**
(unconfirmed) — that's confer's trust-on-first-use model working exactly as designed.

Learn more — https://codeshrew.github.io/confer · https://github.com/codeshrew/confer
EOF
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
