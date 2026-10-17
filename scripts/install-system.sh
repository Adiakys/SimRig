#!/bin/sh
# Installs simrig-helper, its polkit action and the official plugins (generic USB and brands)
# system-wide, to try real hardware before packages exist (M5). Everything installed is owned
# by root.
#
#   cargo build --release
#   sudo scripts/install-system.sh           install, or update
#   sudo scripts/install-system.sh remove    remove it (remove the plugin's driver from the app first)
#
# Then run the app without development options (`target/release/simrig`): it finds the plugin in
# /usr/local/lib/simrig/plugins and asks for the password when its driver is installed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release"
helper=/usr/libexec/simrig/simrig-helper
policy=/usr/share/polkit-1/actions/org.simrig.helper.policy
plugins=/usr/local/lib/simrig/plugins

[ "$(id -u)" = 0 ] || { echo "run it with sudo" >&2; exit 1; }
. "$root/packaging/brands.sh"

case "${1:-install}" in
install)
    for b in simrig-helper simrig-plugin-generic-usb simrig-generic-usb-service; do
        [ -x "$bin/$b" ] || { echo "$bin/$b missing: run cargo build --release first" >&2; exit 1; }
    done
    brand_check "$root"
    install -D -o root -g root -m 0755 "$bin/simrig-helper" "$helper"
    install -D -o root -g root -m 0644 "$root/packaging/polkit/org.simrig.helper.policy" "$policy"

    dest="$plugins/generic-usb"
    rm -rf "$dest"
    install -d -o root -g root -m 0755 "$dest"
    install -o root -g root -m 0644 "$root/plugins/generic-usb/plugin.toml" "$dest/"
    install -o root -g root -m 0755 "$bin/simrig-plugin-generic-usb" "$bin/simrig-generic-usb-service" "$dest/"
    cp -r "$root/plugins/generic-usb/driver" "$dest/driver"
    chown -R root:root "$dest/driver"
    chmod -R go-w "$dest"

    # Brands: the adapters and the drivers, with the bundled driver sources.
    for brand in $BRANDS; do
        # A fresh copy: nothing left over from the previous version.
        rm -rf "${plugins:?}/$brand"
        brand_install "$root" "$bin" "$plugins" "$brand"
        chown -R root:root "$plugins/$brand"
        chmod -R go-w "$plugins/$brand"
    done
    echo "installed: $helper, $policy, $dest, $(echo "$BRANDS" | sed "s|[^ ]*|$plugins/&|g")"
    ;;
remove)
    rm -rf "$plugins/generic-usb"
    for brand in $BRANDS; do
        rm -rf "${plugins:?}/$brand"
    done
    rmdir "$plugins" /usr/local/lib/simrig 2>/dev/null || true
    rm -f "$helper" "$policy"
    rmdir /usr/libexec/simrig 2>/dev/null || true
    echo "removed (drivers installed from the app are kept: see /var/lib/simrig/remove-drivers)"
    ;;
*)
    echo "usage: sudo $0 [install|remove]" >&2
    exit 2
    ;;
esac
