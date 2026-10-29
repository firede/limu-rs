#!/bin/sh
# Runs the checks of limu-gpui, a workspace of its own that scripts/verify.sh leaves out:
# rustfmt, clippy, the docs, and the tests. GPUI builds only where it has a platform, and CI
# runs it on macOS.
set -eu
cd "$(dirname "$0")/../crates/limu-gpui"
cargo fmt -- --check
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps
cargo test -q
echo PASSED
