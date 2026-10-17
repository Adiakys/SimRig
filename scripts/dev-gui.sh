#!/bin/sh
# Runs the app on the simulated rig, with a fake USB tree and a simulated generic plugin, in
# throwaway directories under target/dev. Nothing outside the repository is touched.
#
#   scripts/dev-gui.sh            start the app
#   scripts/dev-gui.sh plug       plug a fake unknown USB device (while the app runs)
#   scripts/dev-gui.sh unplug     unplug it
#   scripts/dev-gui.sh reset      forget the rig, the presets and the first run
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
    echo "plugged: USB Game Controller (USB 1234:0001)"
    ;;
unplug)
    rm -rf "$usb/9-1"
    echo "unplugged"
    ;;
reset)
    rm -rf "$dev/config" "$dev/state"
    echo "rig, presets and first run forgotten"
    ;;
run)
    cargo build --manifest-path "$root/Cargo.toml" -q
    exec "$root/target/debug/simrig" \
        --plugin-dir "$root/plugins/sim" \
        --plugin-dir "$root/plugins/sim/dev/generic-usb" \
        --no-default-dirs \
        --config-dir "$dev/config" \
        --state-dir "$dev/state" \
        --usb-root "$usb"
    ;;
*)
    echo "usage: $0 [run|plug|unplug|reset]" >&2
    exit 2
    ;;
esac
