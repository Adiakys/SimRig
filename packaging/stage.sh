#!/bin/sh
# Lays out SimRig as it is installed, from a release build (`cargo build --release`), under a
# staging root: the packages (nfpm.yaml) and the tarball are made from it.
#
#   packaging/stage.sh [DEST]      default: dist/root
#
# Layout: /usr/bin (app, CLI), /usr/libexec/simrig (helper and updater, run through pkexec),
# /usr/lib/simrig/plugins (official plugins with their drivers), polkit actions, desktop entry,
# icon, documentation and licenses. The packages and the archive add
# /usr/lib/simrig/package-format, which tells the app which file of a release updates it.
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
install -D -m 0755 "$root/packaging/simrig-update" "$dest/usr/libexec/simrig/simrig-update"
for policy in org.simrig.helper org.simrig.update; do
    install -D -m 0644 "$root/packaging/polkit/$policy.policy" \
        "$dest/usr/share/polkit-1/actions/$policy.policy"
done
install -D -m 0644 "$root/packaging/simrig.desktop" "$dest/usr/share/applications/simrig.desktop"
install -D -m 0644 "$root/crates/simrig-gui/ui/icons/mark.svg" \
    "$dest/usr/share/icons/hicolor/scalable/apps/simrig.svg"

plugins="$dest/usr/lib/simrig/plugins"

# Generic USB: adapter and driver, with the driver's service.
generic="$plugins/generic-usb"
install -d "$generic"
install -m 0644 "$root/plugins/generic-usb/plugin.toml" "$generic/"
install -m 0755 "$bin/simrig-plugin-generic-usb" "$generic/"
cp -r "$root/plugins/generic-usb/driver" "$generic/driver"
# Inside the driver: its hooks run from the copy SimRig keeps when it installs it.
install -D -m 0755 "$bin/simrig-generic-usb-service" "$generic/driver/bin/simrig-generic-usb-service"

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
