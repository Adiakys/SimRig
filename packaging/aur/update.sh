#!/bin/sh
# Points the AUR package at a stable release: its version, the archive's checksum from the
# release's SHA256SUMS, and .SRCINFO (by makepkg, or in an Arch container without it).
#
#   packaging/aur/update.sh 1.2.3
#
# Then publish the two files: copy PKGBUILD, simrig.install and .SRCINFO into a clone of
# ssh://aur@aur.archlinux.org/simrig-bin.git, commit and push.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
version="${1:?usage: $0 VERSION}"
case "$version" in
*-*) echo "$version is a pre-release: the AUR package follows stable releases" >&2; exit 1 ;;
esac

archive="simrig-$version-x86_64-linux.tar.gz"
sum=$(curl -sSfL "https://github.com/Adiakys/SimRig/releases/download/v$version/SHA256SUMS" |
    awk -v f="$archive" '$2 == f { print $1 }')
[ -n "$sum" ] || { echo "$archive not in the release v$version" >&2; exit 1; }

sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$sum')/" "$here/PKGBUILD"

if command -v makepkg >/dev/null; then
    (cd "$here" && makepkg --printsrcinfo > .SRCINFO)
else
    docker run --rm -v "$here:/pkg:ro" archlinux:latest sh -c \
        'useradd -m b && cp -r /pkg /tmp/p && chown -R b /tmp/p && cd /tmp/p && su b -c "makepkg --printsrcinfo"' \
        > "$here/.SRCINFO"
fi
echo "PKGBUILD and .SRCINFO at $version"
