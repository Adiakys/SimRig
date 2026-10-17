#!/bin/sh
# Moves the bundled hid-fanatecff (git submodule) to an upstream commit or tag, and records its
# version for DKMS. Drivers ship with SimRig releases: update only to a version tested with the
# Fanatec adapter.
#
#   scripts/update-hid-fanatecff.sh            show the bundled version and what upstream has
#   scripts/update-hid-fanatecff.sh <ref>      move to <ref> (a commit, a tag, origin/master)
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
sub="$root/plugins/fanatec/driver/hid-fanatecff"
version_file="$root/plugins/fanatec/driver/hid-fanatecff.version"

git -C "$root" submodule update --init plugins/fanatec/driver/hid-fanatecff
git -C "$sub" fetch --tags --quiet origin

if [ $# -eq 0 ]; then
    echo "bundled: $(cat "$version_file")"
    echo "upstream: $(git -C "$sub" describe --tags origin/HEAD)"
    git -C "$sub" log --oneline HEAD..origin/HEAD
    exit 0
fi

git -C "$sub" checkout --quiet "$1"
git -C "$sub" describe --tags > "$version_file"
echo "bundled now: $(cat "$version_file"). Test it, then commit the submodule and $version_file."
