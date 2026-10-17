# Shared by the hooks of the Moza driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
PACKAGE=hid-universal-pidff
MODULE=hid_universal_pidff
VERSION=$(cat "$SIMRIG_DRIVER_DIR/universal-pidff.version")
SOURCE="$SIMRIG_DRIVER_DIR/universal-pidff"
DATA="$SIMRIG_DRIVER_DATA"
RULES=71-simrig-moza.rules
# The bases hid-universal-pidff drives (hid-ids.h).
BASES="0000 0002 0004 0005 0006 0010 0012 0014 0015 0016"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules_installed() {
    [ -f "$DATA/rules" ]
}
