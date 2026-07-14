#!/usr/bin/env bash
# Two-clone + bare-hub integration test for confer.
# Exercises the delivery layer that unit tests can't: cross-clone sync (B2),
# filtered-poll cursor safety (B1), divergence recovery (B3), done-routing (H6),
# id-in-output (H1), and slug validation (H2).
#
# Usage: cargo build && bash tests/two_clone.sh
set -u
CONFER="${CONFER:-$(cd "$(dirname "$0")/.." && pwd)/target/debug/confer}"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
cd "$T" || exit 1
fail=0
pass() { echo "  ok: $1"; }
die()  { echo "  FAIL: $1"; fail=1; }
has()  { grep -q -- "$2" <<<"$1"; }

git init -q --bare hub.git
git clone -q hub.git carol
( cd carol
  git config user.email s@c; git config user.name s
  printf '[roles.carol]\ndisplay="Design Studio"\n[roles.bob]\ndisplay="Reader"\n' > roles.toml
  git add roles.toml; git commit -q -m init; git push -q -u origin HEAD )
git clone -q hub.git bob

echo "== B2: bob poll FETCHES a request carol pushed =="
( cd carol && "$CONFER" append --from carol --type request --to bob --topic px \
    --summary "Wire JPEGs" --text "body" >/dev/null 2>&1 )
out=$(cd bob && "$CONFER" poll --role bob 2>/dev/null)
has "$out" "Wire JPEGs" && pass "bob saw the request without a manual fetch" || die "poll did not fetch (B2)"
has "$out" "REQUEST " && pass "output line carries a short id (H1)" || die "no id in output (H1)"

echo "== B1: a filtered poll may not --advance the shared cursor =="
err=$(cd bob && "$CONFER" poll --role bob --topic general --advance 2>&1); rc=$?
[ $rc -ne 0 ] && has "$err" "unfiltered" && pass "filtered --advance rejected" || die "filtered --advance not rejected (B1)"
# unfiltered advance, then the px request must NOT be lost on a later poll
( cd bob && "$CONFER" poll --role bob --advance >/dev/null 2>&1 )
out2=$(cd bob && "$CONFER" poll --role bob 2>/dev/null)
[ -z "$out2" ] && pass "advanced past printed request; no re-show" || die "re-showed after advance"

echo "== B3: a diverged (hub-down) clone recovers and its queued msg reaches the hub =="
( cd bob
  git remote set-url origin "$T/nonexistent.git"
  "$CONFER" append --from bob --type note --summary "queued while offline" --text x >/dev/null 2>&1
  git remote set-url origin "$T/hub.git" )
( cd carol && "$CONFER" append --from carol --type request --to bob --topic px \
    --summary "second request" --text b >/dev/null 2>&1 )
out3=$(cd bob && "$CONFER" poll --role bob 2>/dev/null)
has "$out3" "second request" && pass "diverged bob recovered and saw the new request" || die "diverged watcher stayed blind (B3)"
out4=$(cd carol && "$CONFER" poll --role carol --all 2>/dev/null)
has "$out4" "queued while offline" && pass "bob's queued message reached the hub via rebase" || die "queued message never pushed (B3)"

echo "== B4: after divergence+recovery, the advanced cursor is stable (no duplicate re-emit) =="
# catch bob up so the cursor is at the current stable tip
( cd bob && "$CONFER" poll --role bob --advance >/dev/null 2>&1 )
# bob diverges offline, carol pushes a new request, bob reconnects
( cd bob
  git remote set-url origin "$T/nonexistent.git"
  "$CONFER" append --from bob --type note --summary "offline note B4" --text x >/dev/null 2>&1
  git remote set-url origin "$T/hub.git" )
( cd carol && "$CONFER" append --from carol --type request --to bob --topic px \
    --summary "B4 request" --text b >/dev/null 2>&1 )
b4a=$(cd bob && "$CONFER" poll --role bob --advance 2>/dev/null)
has "$b4a" "B4 request" && pass "B4: new request seen once after recovery" || die "B4: missed request after divergence"
# bob's offline commit was just rebased (sha rewritten); a second poll must be
# EMPTY — the cursor was anchored at a stable pushed commit, not the rewritten HEAD.
b4b=$(cd bob && "$CONFER" poll --role bob --advance 2>/dev/null)
[ -z "$b4b" ] && pass "B4: no duplicate re-emit after rebase (cursor stable)" || die "B4: re-emitted after rebase (cursor not stable)"

echo "== H6: done routes back to the requester (visible under --to-me) =="
rid=$(cd bob && "$CONFER" read --topic px --json 2>/dev/null | grep '"summary":"Wire JPEGs"' | sed 's/.*"id":"\([^"]*\)".*/\1/')
( cd bob && "$CONFER" append --from bob --type done --of "$rid" --summary "wired it" --text done >/dev/null 2>&1 )
out5=$(cd carol && "$CONFER" poll --role carol --to-me 2>/dev/null)
has "$out5" "wired it" && pass "requester sees the done under --to-me" || die "done not routed to requester (H6)"

echo "== H2: path-traversal topic/role is rejected =="
err2=$(cd carol && "$CONFER" append --from carol --type note --topic "../../evil" --summary s --text x 2>&1); rc2=$?
[ $rc2 -ne 0 ] && [ ! -e "$T/evil" ] && pass "traversal topic rejected, no file escaped" || die "path traversal not prevented (H2)"

echo "== init: fresh + existing hub both land on main (no split-brain) =="
git init -q --bare "$T/h2.git"
"$CONFER" init "$T/h2.git" "$T/ic_a" >/dev/null 2>&1
"$CONFER" init "$T/h2.git" "$T/ic_b" >/dev/null 2>&1
ba=$(git -C "$T/ic_a" branch --show-current 2>/dev/null)
bb=$(git -C "$T/ic_b" branch --show-current 2>/dev/null)
[ "$ba" = main ] && [ "$bb" = main ] && pass "both init clones on main" || die "init branch mismatch ($ba/$bb)"

echo
[ $fail -eq 0 ] && echo "ALL PASS" || echo "FAILURES ABOVE"
exit $fail
