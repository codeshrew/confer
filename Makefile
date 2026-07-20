# `ui` is the target CI, crates-publish, and the release pipeline all call so the web
# dashboard is built BEFORE cargo compiles anything (build.rs only ever READS
# ui/dist/index.html — it never shells out to npm, which is what broke `cargo publish
# --verify` by having build.rs modify the source tree).

.PHONY: ui build release test

ui:
	cd ui && if [ -f package-lock.json ]; then npm ci; else npm install; fi
	cd ui && npm run build

build: ui
	cargo build

release: ui
	cargo build --release

test: ui
	cargo nextest run || cargo test
