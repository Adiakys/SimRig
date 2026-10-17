#!/bin/sh
# Lays out SimRig as it is installed, from a release build (`cargo build --release`), under a
# staging root: the packages (nfpm.yaml) and the tarball are made from it.
#
#   packaging/stage.sh [DEST]      default: dist/root
#
# Layout: /usr/bin (app, CLI), /usr/libexec/simrig (helper, run through pkexec),
# /usr/lib/simrig/plugins (official plugins with their drivers), polkit action, desktop entry,
# icon, documentation and licenses.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release"
dest="${1:-$root/dist/root}"

for b in simrig simrig-cli simrig-helper simrig-plugin-generic-usb simrig-generic-usb-service simrig-plugin-fanatec; do
    [ -x "$bin/$b" ] || { echo "$bin/$b missing: run cargo build --release" >&2; exit 1; }
done
[ -f "$root/plugins/fanatec/driver/hid-fanatecff/Kbuild" ] || {
    echo "plugins/fanatec/driver/hid-fanatecff is empty: git submodule update --init" >&2
    exit 1
}

rm -rf "$dest"
install -D -m 0755 "$bin/simrig" "$dest/usr/bin/simrig"
install -D -m 0755 "$bin/simrig-cli" "$dest/usr/bin/simrig-cli"
install -D -m 0755 "$bin/simrig-helper" "$dest/usr/libexec/simrig/simrig-helper"
install -D -m 0644 "$root/packaging/polkit/org.simrig.helper.policy" \
    "$dest/usr/share/polkit-1/actions/org.simrig.helper.policy"
install -D -m 0644 "$root/packaging/simrig.desktop" "$dest/usr/share/applications/simrig.desktop"
install -D -m 0644 "$root/crates/simrig-gui/ui/icons/mark.svg" \
    "$dest/usr/share/icons/hicolor/scalable/apps/simrig.svg"

plugins="$dest/usr/lib/simrig/plugins"

# Generic USB: adapter, the driver's service next to it, the driver.
generic="$plugins/generic-usb"
install -d "$generic"
install -m 0644 "$root/plugins/generic-usb/plugin.toml" "$generic/"
install -m 0755 "$bin/simrig-plugin-generic-usb" "$bin/simrig-generic-usb-service" "$generic/"
cp -r "$root/plugins/generic-usb/driver" "$generic/driver"

# Fanatec: adapter and driver, with the bundled hid-fanatecff sources (no VCS files).
fanatec="$plugins/fanatec"
install -d "$fanatec"
install -m 0644 "$root/plugins/fanatec/plugin.toml" "$fanatec/"
install -m 0755 "$bin/simrig-plugin-fanatec" "$fanatec/"
(cd "$root/plugins/fanatec" && tar cf - --exclude=.git --exclude=.github driver) | (cd "$fanatec" && tar xf -)

doc="$dest/usr/share/doc/simrig"
install -d "$doc"
install -m 0644 "$root/README.md" "$root/THIRD-PARTY.md" "$root/LICENSE" "$doc/"
install -m 0644 "$root/plugins/fanatec/driver/hid-fanatecff/LICENSE" "$doc/LICENSE.hid-fanatecff"
install -m 0644 "$root/assets/fonts/OFL-Barlow.txt" "$root/assets/fonts/OFL-MartianMono.txt" "$doc/"

# Nothing writable by others, whatever the build machine's umask.
chmod -R go-w "$dest"
echo "staged in $dest"
