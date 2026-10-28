#!/bin/sh
# Downloads what the tests compare with: the fixtures of the release of pele-oracle
# fixtures.toml pins, into target/pele-fixtures/<tag>/, and the fonts the golden images are
# drawn in, into target/fonts/. It checks each archive's SHA-256. Once they are there, it does
# nothing, and the tests run offline.
#
#   ./scripts/fixtures.sh [archive]
#
# Given the path of an archive of fixtures, as pele-oracle's scripts/pack.sh writes one, it
# unpacks that instead of downloading it, still checking its SHA-256. To test fixtures that
# are not packed, set LIMU_FIXTURES to the folder that holds them, as a pele-oracle's out/,
# rather than running this.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
value() { sed -n "s/^$1 *= *\"\(.*\)\"$/\1/p" "$root/fixtures.toml"; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# fetch url sha256 dest folder [archive] unpacks the archive at url, or the one given, and
# moves the folder in it, "." for the whole of it, to dest, when dest is not there yet.
fetch() {
	url=$1 sha256=$2 dest=$3 folder=$4
	shift 4
	if [ -d "$dest" ]; then
		return
	fi
	rm -rf "$tmp/archive" "$tmp/out"
	if [ $# -gt 0 ]; then
		cp "$1" "$tmp/archive"
	else
		echo "fixtures.sh: downloading $url" >&2
		curl -fsSL -o "$tmp/archive" "$url"
	fi
	got=$(shasum -a 256 "$tmp/archive" | cut -d' ' -f1)
	if [ "$got" != "$sha256" ]; then
		echo "fixtures.sh: the SHA-256 of $url is $got, not $sha256 as fixtures.toml says" >&2
		exit 1
	fi
	mkdir -p "$tmp/out"
	tar -xf "$tmp/archive" -C "$tmp/out"
	mkdir -p "$(dirname "$dest")"
	# GNU mv will not move a folder named "folder/.", so the whole of it is moved by its name.
	if [ "$folder" = . ]; then
		mv "$tmp/out" "$dest"
	else
		mv "$tmp/out/$folder" "$dest"
	fi
	echo "fixtures.sh: unpacked into ${dest#"$root"/}" >&2
}

tag=$(value tag)
fetch "https://github.com/firede/pele-oracle/releases/download/$tag/fixtures.tar" \
	"$(value sha256)" "$root/target/pele-fixtures/$tag" . "$@"
fonts=$(value fonts)
name=$(basename "$fonts" .tar.gz)
fetch "$fonts" "$(value fonts_sha256)" "$root/target/fonts/$name" "$name"
