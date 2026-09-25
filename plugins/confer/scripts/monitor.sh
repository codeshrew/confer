#!/bin/sh
# The confer plugin monitor. Claude Code starts this when the session starts and keeps it for the
# whole session: no 30-minute expiry, nothing to re-arm. Its stdout reaches the agent as
# notifications, so it prints nothing except confer's own delivery line and the wakes.
#
# A plugin monitor that exits is not restarted for the rest of the session, so this never exits:
# if confer is missing or too old, it waits quietly and checks again. Diagnostics go to a log.

project="$1"
log="$HOME/.confer/plugin/monitor.log"
mkdir -p "$HOME/.confer/plugin" 2>/dev/null
note() { echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) [$$] $*" >>"$log" 2>/dev/null; }

# Monitors may start with a minimal PATH; add the usual install locations.
PATH="$PATH:/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:$HOME/.local/bin"
export PATH

note "start: session=${CLAUDE_CODE_SESSION_ID:-?} project=${project:-?}"
while :; do
  if ! command -v confer >/dev/null 2>&1; then
    note "confer is not on PATH; checking again in 5 minutes"
    sleep 300
    continue
  fi
  if ! confer attach --help 2>/dev/null | grep -q -- '--plugin'; then
    note "$(confer --version 2>/dev/null) has no 'attach --plugin' (needs 0.8.35+); checking again in 5 minutes"
    sleep 300
    continue
  fi
  # Run from $HOME, not the project: a project dir can look like a hub.
  ( cd "$HOME" && exec confer attach --plugin --project "$project" ) 2>>"$log"
  note "confer attach --plugin exited with $?; restarting in 30s"
  sleep 30
done
