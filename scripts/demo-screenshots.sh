#!/usr/bin/env bash
# demo-screenshots.sh — one command: build the synthetic demo hub, bring its agents LIVE (so the
# fleet reads healthy, not "down"), serve it read-only, and screenshot every view into docs/img/.
# Exposes nothing real — isolated $HOME, synthetic hostnames, code refs into this public repo only.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFER="${CONFER:-$REPO/target/release/confer}"
[ -x "$CONFER" ] || CONFER="$REPO/target/debug/confer"
[ -x "$CONFER" ] || { echo "no confer binary — cargo build first" >&2; exit 1; }
PORT="${PORT:-8899}"
demo_host() { case "$1" in backend) echo api-1;; frontend) echo web-1;; tester) echo ci-1;; docs) echo web-1;; *) echo demo;; esac; }

echo "▸ building demo hub…"
CONFER="$CONFER" bash "$REPO/scripts/demo-hub.sh" >/tmp/confer-demo-build.log 2>&1 \
  || { echo "build failed:"; tail -5 /tmp/confer-demo-build.log; exit 1; }
WORK="$(cat "$REPO/.demo-hub-path")"; HOME_D="$WORK/home"; CLONES="$WORK/clones"

pids=()
cleanup() { for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

echo "▸ bringing agents live (heartbeat watchers)…"
for r in backend frontend tester docs; do
  ( cd "$CLONES/$r" && HOME="$HOME_D" HOSTNAME="$(demo_host "$r")" "$CONFER" watch --replace >/dev/null 2>&1 ) &
  pids+=($!)
done
sleep 5                                                     # let each publish a signed heartbeat
( cd "$CLONES/backend" && HOME="$HOME_D" HOSTNAME=api-1 "$CONFER" sync >/dev/null 2>&1 ) || true  # pull them

echo "▸ serving on http://127.0.0.1:$PORT …"
( HOME="$HOME_D" CONFER_HUB="$CLONES/backend" "$CONFER" serve --port "$PORT" >/tmp/confer-demo-serve.log 2>&1 ) &
pids+=($!)
sleep 4                                                     # warm-cache fold

echo "▸ capturing screenshots…"
( cd "$REPO/ui" && BASE="http://127.0.0.1:$PORT" node "$REPO/scripts/shoot-demo.mjs" )
echo "✓ screenshots written to $REPO/docs/img/"
