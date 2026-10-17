# Shared by the hooks of the Fanatec driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
PACKAGE=hid-fanatec
MODULE=hid_fanatec
VERSION=$(cat "$SIMRIG_DRIVER_DIR/hid-fanatecff.version")
SOURCE="$SIMRIG_DRIVER_DIR/hid-fanatecff"
DATA="$SIMRIG_DRIVER_DATA"
RULES=71-simrig-fanatec.rules
SUBSYSTEMS="hid hidraw leds ftec_tuning"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules() {
    udev_install "$RULES" < "$SIMRIG_DRIVER_DIR/udev/$RULES"
}
