# `ui` is the target CI, crates-publish, and the release pipeline all call so the web
# dashboard is built BEFORE cargo compiles anything (build.rs only ever READS
# ui/dist/index.html — it never shells out to npm, which is what broke `cargo publish
# --verify` by having build.rs modify the source tree).

.PHONY: ui build release test

ui:
	@if [ -f ui/package-lock.json ]; then \
		npm --prefix ui ci; \
	else \
		npm --prefix ui install; \
	fi
	npm --prefix ui run build

build: ui
	cargo build

release: ui
	cargo build --release

test: ui
	cargo nextest run || cargo test
