# Shared by the hooks of the standard force feedback driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
PACKAGE=hid-universal-pidff
MODULE=hid_universal_pidff
VERSION=$(cat "$SIMRIG_DRIVER_DIR/universal-pidff.version")
SOURCE="$SIMRIG_DRIVER_DIR/universal-pidff"
DATA="${SIMRIG_DRIVER_DATA:?}"
RULES=71-simrig-pidff.rules
# The bases hid-universal-pidff drives (hid-ids.h), as "vendor:product" pairs.
BASES="3416:0301 3416:0302 2433:f300 2433:f301 2433:f303 2433:f306 0483:a355 045b:59d7 10f5:7077"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules_installed() {
    [ -f "$DATA/rules" ]
}

# The bases start again, as if replugged: the module just built takes them.
replug_bases() {
    for base in $BASES; do
        usb_replug "${base%:*}" "${base#*:}"
    done
}
