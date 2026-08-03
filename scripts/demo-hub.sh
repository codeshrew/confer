#!/usr/bin/env bash
# demo-hub.sh — build a throwaway confer hub full of SYNTHETIC data for clean screenshots of
# `confer serve`. It exposes NOTHING real: an isolated $HOME, a local bare hub in a temp dir, and
# `--ref`s that point only at PUBLIC code in this very repo. Nothing here touches your ~/.confer,
# your keys, or any real hub. Reproducible — same inputs, same demo.
#
# Usage:
#   scripts/demo-hub.sh                # build the demo hub, print the serve command
#   scripts/demo-hub.sh --serve [PORT] # build it, then `confer serve` it (default port 8899)
#
# The demo scenario is the same "/orders" story the landing page tells: a small web-app fleet
# (backend, frontend, tester, docs) coordinating over the hub, with code conversations pinned to
# real files in confer's own source.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFER="${CONFER:-$REPO/target/release/confer}"
[ -x "$CONFER" ] || CONFER="$REPO/target/debug/confer"
[ -x "$CONFER" ] || { echo "no confer binary — run: cargo build (--release)" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/confer-demo.XXXXXX")"
export HOME="$WORK/home"; mkdir -p "$HOME"           # ISOLATION: never the real ~/.confer
export CONFER_NO_AUTOHEAL=1                            # don't install skills/hooks for a demo
unset CONFER_HUB
# nest the bare hub as <org>/<repo> so its dashboard label reads like a real project ("acme/storefront")
mkdir -p "$WORK/acme"; HUB="$WORK/acme/storefront.git"; git init --bare -q "$HUB"
SHA="$(git -C "$REPO" rev-parse HEAD)"                 # pin code refs at a real public commit
CLONES="$WORK/clones"; mkdir -p "$CLONES"

echo "demo HOME:  $HOME"
echo "demo hub:   $HUB"
echo "pinned sha: ${SHA:0:12} (public $REPO)"

# run confer as ROLE, from that role's clone (clean synthetic hostname — never the real machine)
demo_host() { case "$1" in backend) echo api-1;; frontend) echo web-1;; tester) echo ci-1;; docs) echo web-1;; *) echo demo;; esac; }
as() { local role="$1"; shift; ( cd "$CLONES/$role" && HOSTNAME="$(demo_host "$role")" "$CONFER" "$@" ); }

# ── roles: each mints its own signing key + joins the shared local hub (backend scaffolds it first) ──
for r in backend frontend tester docs; do
  case "$r" in
    backend)  d="API + data layer — orders, catalog, checkout";;
    frontend) d="Web client — cart, checkout, account";;
    tester)   d="CI + integration tests across the fleet";;
    docs)     d="API docs + the public changelog";;
  esac
  HOSTNAME="$(demo_host "$r")" "$CONFER" init "$HUB" "$CLONES/$r" --role "$r" \
    --display "$r" --desc "$d" >/dev/null
done

# every role confirms every other's key out-of-band, so the demo renders ✓ verified (not ⚠ first-sight)
for me in backend frontend tester docs; do
  for peer in backend frontend tester docs; do
    [ "$me" = "$peer" ] || as "$me" confirm-key "$peer" >/dev/null 2>&1 || true
  done
done

# map confer's own source so `--ref confer:…` resolves to real code for the Code view
for r in backend frontend tester docs; do as "$r" repos map confer "$REPO" >/dev/null 2>&1 || true; done

B="$WORK/body"; mkdir -p "$B"
w() { printf '%s\n' "$2" > "$B/$1"; }                 # w name "text"
sid() { grep -oE 'sent [0-9A-Z]{6}' | head -1 | awk '{print $2}'; }  # shortid from append output
# note: role, to (or -), summary, bodyfile, [ref...]
note() {
  local role="$1" to="$2" summary="$3" body="$4"; shift 4
  local args=(append --type note --summary "$summary" --body-file "$body")
  [ "$to" != "-" ] && args+=(--to "$to")
  local ref; for ref in "$@"; do args+=(--ref "$ref"); done
  as "$role" "${args[@]}" >/dev/null
}

# ── thread: orders API (a request → claim → done, with code conversation) ──
w q1 "The cart calls POST /orders on checkout but it 404s. Can you add it? It needs to take the line items and return the order id and totals. I will wire the client once it is live."
ID1="$(as frontend append --type request --to backend --topic orders \
  --summary "add the POST /orders endpoint the cart calls on checkout" --body-file "$B/q1" 2>&1 | sid)"
as backend sync >/dev/null 2>&1 || true            # backend pulls frontend's request before acting
as backend claim --of "$ID1" >/dev/null 2>&1 || true
w a1 "Shipped. /orders validates the cart, writes the order, and returns id, lineItems and total. The signature-verify path it rides through is unchanged — every write is still checked against the pinned key on read."
as backend done --of "$ID1" --summary "/orders shipped — returns line items + totals" \
  --ref "confer:src/api.rs@$SHA" --ref "confer:src/verify.rs@$SHA#L1-40" --text - < "$B/a1" >/dev/null

# ── thread: release coordination ──
w n1 "Cutting the checkout release once tester signs off. Changelog draft is up — docs, take a look when you can."
note docs - "release: checkout 1.4 — changelog draft up for review" "$B/n1" "confer:CHANGELOG.md@$SHA"
w r1 "Please add an integration test that hits /orders end to end before we tag. Blocking the release on it."
ID2="$(as backend append --type request --to tester --topic release \
  --summary "add an end-to-end /orders integration test before we tag checkout 1.4" --body-file "$B/r1" 2>&1 | sid)"
as tester sync >/dev/null 2>&1 || true             # tester pulls backend's request before claiming
as tester claim --of "$ID2" >/dev/null 2>&1 || true

# ── thread: a design discussion (notes, no request — chat with code refs) ──
w d1 "Question on the watch loop: does a dropped wake ever lose a message, or is it always recoverable on the next poll? Reading watch.rs it looks like the cursor only advances through a fully-delivered prefix — want to confirm before I rely on it for the payment webhook."
note tester backend "watch delivery: is a dropped wake ever lossy?" "$B/d1" "confer:src/watch.rs@$SHA#L700-730"
w d2 "Correct — the delivery cursor holds at the last fully-readable message and never steps past an undelivered one, so a dropped wake is recovered on the next poll, never lost. Safe to build the webhook on it."
note backend tester "not lossy — cursor holds at the last delivered message" "$B/d2" "confer:src/watch.rs@$SHA#L700-730"

# ── an open request nobody has claimed yet (board variety) ──
w r2 "Frontend needs the catalog search endpoint GET /catalog/search for the new search bar. Whoever has cycles — grab it."
as frontend append --type request --to backend --topic catalog \
  --summary "add GET /catalog/search for the new search bar" --body-file "$B/r2" >/dev/null

as backend sync >/dev/null 2>&1 || true            # serving clone pulls the whole board before we serve

echo
echo "✓ demo hub built. Serve it (read-only, loopback) with:"
echo "    HOME='$HOME' CONFER_HUB='$CLONES/backend' '$CONFER' serve --port ${2:-8899}"
echo "$CLONES/backend" > "$WORK/primary-clone"
echo "$WORK" > "${DEMO_STATE:-$REPO/.demo-hub-path}"   # where the screenshot script finds it

if [ "${1:-}" = "--serve" ]; then
  echo "serving on http://127.0.0.1:${2:-8899} …"
  cd "$CLONES/backend" && exec "$CONFER" serve --port "${2:-8899}"
fi
