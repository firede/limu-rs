#!/bin/sh
# Downloads the fixtures the tests compare with, the release of pele-oracle fixtures.toml pins,
# into target/pele-fixtures/<tag>/, and checks the archive's SHA-256. Once they are there, it
# does nothing, and the tests run offline.
#
#   ./scripts/fixtures.sh [archive]
#
# Given the path of an archive, as pele-oracle's scripts/pack.sh writes one, it unpacks that
# instead of downloading it, still checking its SHA-256. To test fixtures that are not
# packed, set LIMU_FIXTURES to the folder that holds them, as a pele-oracle's out/, rather
# than running this.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
value() { sed -n "s/^$1 *= *\"\(.*\)\"$/\1/p" "$root/fixtures.toml"; }
tag=$(value tag)
sha256=$(value sha256)
dest=$root/target/pele-fixtures/$tag
if [ -f "$dest/manifest.json" ]; then
	exit 0
fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if [ $# -gt 0 ]; then
	cp "$1" "$tmp/fixtures.tar"
else
	url=https://github.com/firede/pele-oracle/releases/download/$tag/fixtures.tar
	echo "fixtures.sh: downloading $url" >&2
	curl -fsSL -o "$tmp/fixtures.tar" "$url"
fi
got=$(shasum -a 256 "$tmp/fixtures.tar" | cut -d' ' -f1)
if [ "$got" != "$sha256" ]; then
	echo "fixtures.sh: the archive's SHA-256 is $got, not $sha256 as fixtures.toml says" >&2
	exit 1
fi
mkdir -p "$tmp/out"
tar -xf "$tmp/fixtures.tar" -C "$tmp/out"
mkdir -p "$(dirname "$dest")"
rm -rf "$dest"
mv "$tmp/out" "$dest"
echo "fixtures.sh: $tag is in target/pele-fixtures/$tag" >&2
