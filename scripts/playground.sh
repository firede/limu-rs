#!/bin/sh
# Builds the playground, a page that lays Mermaid out with limu as it is typed, into
# target/playground. With --serve, it then serves the page at http://localhost:8000, as a
# browser loads its module only over HTTP. Needs the wasm32-unknown-unknown target, which
# `rustup target add wasm32-unknown-unknown` installs.
set -eu
cd "$(dirname "$0")/.."
# The module is built small, as a page downloads it, without changing the workspace's
# release profile.
CARGO_PROFILE_RELEASE_LTO=true \
CARGO_PROFILE_RELEASE_OPT_LEVEL=s \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_STRIP=true \
	cargo build -q --release -p limu-playground --target wasm32-unknown-unknown
out=target/playground
mkdir -p "$out"
cp target/wasm32-unknown-unknown/release/limu_playground.wasm "$out/"
cp crates/limu-playground/web/index.html crates/limu-playground/web/limu.js "$out/"
echo "$out"
if [ "${1:-}" = --serve ]; then
	python3 -m http.server -d "$out" 8000
fi
