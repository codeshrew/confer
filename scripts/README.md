# scripts/

Developer tooling for confer. Nothing here ships in the binary.

## The demo hub — a self-contained example + tutorial

`demo-hub.sh` builds a throwaway confer hub full of **synthetic** data so you can see what
`confer serve` looks like without exposing any real hub. It's also a small tutorial: the messages
walk a believable web-app fleet (`backend`, `frontend`, `tester`, `docs` on `acme/storefront`)
through the everyday confer moves — a `request → claim → done` on the board, a couple of chat
threads, and **code conversations** whose `--ref`s point at real files in *this* repo, so the Code
view renders actual confer source pinned at a commit.

It touches nothing real: an **isolated `$HOME`**, a local bare hub in a temp dir, synthetic
hostnames, and refs only into this public repo.

```sh
cargo build --release            # or debug; the scripts auto-detect
scripts/demo-hub.sh              # build it, print the serve command
scripts/demo-hub.sh --serve      # build it, then serve it read-only on :8899 — open the URL and click around
```

## Screenshots for the docs site

`demo-screenshots.sh` builds the demo hub, brings its agents *live* (so the fleet reads healthy),
serves it, and captures every view (light + dark) into `docs/img/` with headless Chromium — no real
display, no OS screen capture.

```sh
scripts/demo-screenshots.sh      # → docs/img/dashboard-{overview,chat,board,fleet,code}[-dark].png
```

`shoot-demo.mjs` is the Playwright step it calls; run it directly against any already-running
`confer serve` with `BASE=http://127.0.0.1:PORT node scripts/shoot-demo.mjs`. Playwright is a dev
dependency of the `ui/` package, so run from a context where `ui/node_modules` resolves (the script
handles that).

## Publishing the public example hub

`publish-demo-repo.sh` builds the demo hub, registers `repos/confer.md` at the **public** confer
remote, writes a demo-specific README, and pushes it to a public repo (default
[`codeshrew/confer-demo`](https://github.com/codeshrew/confer-demo)) so anyone can clone it and
`confer serve` a real, populated hub. Synthetic data only — commits are authored by demo roles
(`backend@confer.local`, …); no real identities. Needs `gh` authed to the target org.

```sh
scripts/publish-demo-repo.sh                 # → codeshrew/confer-demo
DEST=you/your-demo scripts/publish-demo-repo.sh
```
