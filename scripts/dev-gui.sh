#!/bin/sh
# Runs the app on the simulated rig and the generic USB plugin, with a fake USB tree, in
# throwaway directories under target/dev. Nothing outside the repository is touched.
#
#   scripts/dev-gui.sh              start the app
#   scripts/dev-gui.sh plug         plug a fake unknown USB device (while the app runs)
#   scripts/dev-gui.sh report 00 04 move it: its current raw report, in hex
#   scripts/dev-gui.sh unplug       unplug it
#   scripts/dev-gui.sh reset        forget the rig, the presets, the drivers and the first run
#
# SIMRIG_USB_ROOT=/sys/bus/usb/devices scripts/dev-gui.sh uses the real USB devices instead
# (e.g. a Fanatec base with hid-fanatec installed).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dev="$root/target/dev"
usb="$dev/usb"
mkdir -p "$usb"

case "${1:-run}" in
plug)
    mkdir -p "$usb/9-1"
    printf '1234\n' > "$usb/9-1/idVendor"
    printf '0001\n' > "$usb/9-1/idProduct"
    printf 'USB Game Controller\n' > "$usb/9-1/product"
    printf '00 00 00\n' > "$usb/9-1/simrig-fake-report"
    echo "plugged: USB Game Controller (USB 1234:0001); move it with: $0 report <hex bytes>"
    ;;
report)
    shift
    [ -d "$usb/9-1" ] || { echo "plug the device first" >&2; exit 1; }
    printf '%s\n' "$*" > "$usb/9-1/simrig-fake-report"
    ;;
unplug)
    rm -rf "$usb/9-1"
    echo "unplugged"
    ;;
reset)
    rm -rf "$dev/config" "$dev/state"
    echo "rig, presets, drivers and first run forgotten"
    ;;
run)
    cargo build --manifest-path "$root/Cargo.toml" -q
    exec "$root/target/debug/simrig" \
        --plugin-dir "$root/plugins/sim" \
        --plugin-dir "$root/plugins/generic-usb" \
        --plugin-dir "$root/plugins/fanatec" \
        --plugin-dir "$root/plugins/moza" \
        --plugin-dir "$root/plugins/logitech" \
        --plugin-dir "$root/plugins/thrustmaster" \
        --no-default-dirs \
        --config-dir "$dev/config" \
        --state-dir "$dev/state" \
        --usb-root "${SIMRIG_USB_ROOT:-$usb}"
    ;;
*)
    echo "usage: $0 [run|plug|unplug|reset]" >&2
    exit 2
    ;;
esac
