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

. "$root/packaging/brands.sh"

for b in simrig simrig-cli simrig-helper simrig-plugin-generic-usb simrig-generic-usb-service; do
    [ -x "$bin/$b" ] || { echo "$bin/$b missing: run cargo build --release" >&2; exit 1; }
done
for brand in $BRANDS; do
    [ -x "$bin/simrig-plugin-$brand" ] || { echo "$bin/simrig-plugin-$brand missing: run cargo build --release" >&2; exit 1; }
done
brand_check "$root"

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

# Brands: adapter and driver, with the bundled driver sources (no VCS files).
for brand in $BRANDS; do
    brand_install "$root" "$bin" "$plugins" "$brand"
done

doc="$dest/usr/share/doc/simrig"
install -d "$doc"
install -m 0644 "$root/README.md" "$root/THIRD-PARTY.md" "$root/LICENSE" "$doc/"
brand_licenses "$root" "$doc"
install -m 0644 "$root/assets/fonts/OFL-Barlow.txt" "$root/assets/fonts/OFL-MartianMono.txt" "$doc/"

# Nothing writable by others, whatever the build machine's umask.
chmod -R go-w "$dest"
echo "staged in $dest"
