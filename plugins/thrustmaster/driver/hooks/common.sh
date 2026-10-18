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

# hid-t150, for the T150 and TMX, which hid-tmff2 does not drive: built only when one is
# plugged in (or SimRig built it before, so it stays up to date).
T150_PACKAGE=hid-t150
T150_MODULE=hid_t150
T150_VERSION=$(cat "$SIMRIG_DRIVER_DIR/hid-t150.version")
T150_SOURCE="$SIMRIG_DRIVER_DIR/hid-t150"
T150_CONF="$SIMRIG_DRIVER_DIR/dkms/hid-t150.conf"
T150_WHEELS="b677 b67f"
# hid-t150 has no module options: its settings are saved as udev rules.
SETTINGS=72-simrig-thrustmaster-settings.rules

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

# A T150 or TMX is plugged in, or SimRig built hid-t150 before.
t150_needed() {
    usb_present 044f $T150_WHEELS || [ -n "$(mod_ours "$T150_PACKAGE")" ]
}

# hid-t150 is needed and nothing provides it.
t150_missing() {
    t150_needed || return 1
    case $(mod_state "$T150_PACKAGE" "$T150_MODULE" "$T150_VERSION") in
    ours* | outdated* | pending* | foreign*) return 1 ;;
    esac
    return 0
}

# Builds SimRig's hid-t150 when it is needed and not provided otherwise, or upgrades SimRig's.
# Returns 0, 3 (waiting for the signing key) or 1.
t150_ensure() {
    case $(mod_state "$T150_PACKAGE" "$T150_MODULE" "$T150_VERSION") in
    foreign*) return 0 ;;
    "ours $T150_VERSION") return 0 ;;
    esac
    t150_needed || return 0
    mod_prerequisites || return 1
    t_rc=0
    mod_build "$T150_PACKAGE" "$T150_VERSION" "$T150_SOURCE" "$T150_CONF" "$T150_MODULE" || t_rc=$?
    if [ "$t_rc" = 0 ]; then
        usb_replug 044f $T150_WHEELS
        echo "hid-t150 $T150_VERSION installed"
    fi
    return "$t_rc"
}

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
