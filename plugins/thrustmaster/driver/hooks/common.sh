# Shared by the hooks of the Thrustmaster driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
DATA="${SIMRIG_DRIVER_DATA:?}"
PACKAGE=hid-tmff2
MODULE=hid_tmff_new
# hid-tminit's: they start the wheels up, in place of the kernel's hid-thrustmaster.
INIT_MODULES="hid_tminit_new usb_tminit_new"
VERSION=$(cat "$SIMRIG_DRIVER_DIR/hid-tmff2.version")
SOURCE="$SIMRIG_DRIVER_DIR/hid-tmff2"
RULES=71-simrig-thrustmaster.rules
CONF="$ROOT/etc/modprobe.d/simrig-thrustmaster.conf"
# The wheels hid-tmff2 drives (src/hid-tmff2.h).
WHEELS="b66e b66f b66d b696 b669 b689 b692"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

# Writes SimRig's modprobe configuration: the kernel's hid-thrustmaster blacklisted when
# SimRig's hid-tmff2 is installed, and the settings saved as module options.
write_conf() {
    options=$(tr '\n' ' ' < "$DATA/settings" 2>/dev/null | sed 's/ *$//' || true)
    mkdir -p "$ROOT/etc/modprobe.d"
    {
        echo "# Written by SimRig: the Thrustmaster driver's configuration."
        if [ -n "$(mod_ours "$PACKAGE")" ]; then
            echo "blacklist hid_thrustmaster"
        fi
        if [ -n "$options" ]; then
            echo "options $MODULE $options"
        fi
    } > "$CONF"
}
