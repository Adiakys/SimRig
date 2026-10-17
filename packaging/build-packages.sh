#!/bin/sh
# Builds every package of a release from a release build: .deb, .rpm, Arch (.pkg.tar.zst) with
# nfpm, and a .tar.gz with an install script, plus SHA256SUMS. Needs nfpm.
#
#   cargo build --release && packaging/build-packages.sh 1.2.3
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
version="${1:?usage: $0 VERSION}"
out="$root/dist"
cd "$root"

packaging/stage.sh "$out/root"
mkdir -p "$out/packages"
rm -f "$out/packages/"*
for format in deb rpm archlinux; do
    SIMRIG_VERSION="$version" nfpm package -f packaging/nfpm.yaml -p "$format" -t "$out/packages/"
done

tarball="simrig-$version-x86_64-linux"
rm -rf "$out/$tarball"
mkdir -p "$out/$tarball"
cp -a "$out/root" "$out/$tarball/root"
install -m 0755 packaging/tarball-install.sh "$out/$tarball/install.sh"
install -m 0755 packaging/postremove.sh "$out/$tarball/postremove.sh"
tar -C "$out" -czf "$out/packages/$tarball.tar.gz" "$tarball"
rm -rf "$out/$tarball"

(cd "$out/packages" && sha256sum -- * > SHA256SUMS)
ls -l "$out/packages"
