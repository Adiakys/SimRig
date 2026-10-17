# Shared by the hooks of the PXN driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
PACKAGE=hid-universal-pidff
MODULE=hid_universal_pidff
VERSION=$(cat "$SIMRIG_DRIVER_DIR/universal-pidff.version")
SOURCE="$SIMRIG_DRIVER_DIR/universal-pidff"
DATA="${SIMRIG_DRIVER_DATA:?}"
RULES=71-simrig-pxn.rules
# The bases hid-universal-pidff drives (hid-ids.h), by vendor.
LITE_STAR_BASES="3245 1212 1112 1211 2141"
PXN_BASES="400f"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules_installed() {
    [ -f "$DATA/rules" ]
}
