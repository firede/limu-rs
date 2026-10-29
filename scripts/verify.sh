#!/bin/sh
# Runs every check of the repository: that the core crates depend on nothing but each other
# and serde, rustfmt, clippy, the docs, and the tests, which compare what limu does with the
# fixtures scripts/fixtures.sh downloads. Works offline once the fixtures and the crates the
# backends need are downloaded, from any directory.
set -eu
cd "$(dirname "$0")/.."
./scripts/fixtures.sh
# The core crates use the standard library alone, and serde behind their feature.
for crate in limu-scene limu; do
	deps=$(cargo tree -q -p "$crate" -e normal --all-features --prefix none |
		sed 's/ .*//' | sort -u | grep -v -x -e limu-scene -e limu -e 'serde' -e 'serde_core' -e 'serde_derive' -e 'proc-macro2' -e 'quote' -e 'syn' -e 'unicode-ident' || true)
	if [ -n "$deps" ]; then
		echo "$crate depends on what it may not:"
		echo "$deps"
		exit 1
	fi
done
cargo fmt --all -- --check
cargo clippy -q --workspace --all-targets --all-features -- -D warnings
# The playground is built for WebAssembly, where the functions its page calls are compiled.
rustup target list --installed | grep -qx wasm32-unknown-unknown ||
	rustup target add wasm32-unknown-unknown
cargo clippy -q -p limu-playground --target wasm32-unknown-unknown -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --workspace --no-deps --all-features
cargo test -q --workspace --all-features
cargo test -q --workspace --no-default-features
echo PASSED
