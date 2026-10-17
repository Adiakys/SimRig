# Shared by the hooks of the Logitech driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
DATA="${SIMRIG_DRIVER_DATA:?}"
RULES=71-simrig-logitech.rules
# The settings saved for the next plug-in.
SETTINGS=72-simrig-logitech-settings.rules

# new-lg4ff replaces the kernel's hid-logitech, for the G29 family.
LG4FF_PACKAGE=new-lg4ff
LG4FF_MODULE=hid_logitech
LG4FF_VERSION=$(cat "$SIMRIG_DRIVER_DIR/new-lg4ff.version")
LG4FF_SOURCE="$SIMRIG_DRIVER_DIR/new-lg4ff"
LG4FF_WHEELS="c293 c294 c295 c298 c299 c29a c29b c29c ca03 c24f c266 c267"

# The TrueForce driver, for the direct drive wheels.
TF_PACKAGE=logitech-trueforce
TF_MODULE=hid_logitech_dd
TF_VERSION=$(cat "$SIMRIG_DRIVER_DIR/logitech-trueforce.version")
TF_SOURCE="$SIMRIG_DRIVER_DIR/logitech-trueforce/mainline"
TF_WHEELS="c268 c272 c276"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules_installed() {
    [ -f "$DATA/rules" ]
}

# needed PACKAGE WHEELS…: a wheel that needs the package is plugged in, or SimRig built it
# before (so it stays up to date).
needed() {
    n_package=$1
    shift
    usb_present 046d "$@" || [ -n "$(mod_ours "$n_package")" ]
}

# missing PACKAGE MODULE VERSION KERNEL_IS_ENOUGH WHEELS…: the package is needed and nothing
# provides it (not SimRig, not someone else; the kernel's module only when it is enough).
missing() {
    m_package=$1 m_module=$2 m_version=$3 m_kernel=$4
    shift 4
    needed "$m_package" "$@" || return 1
    case $(mod_state "$m_package" "$m_module" "$m_version") in
    ours* | outdated* | pending* | foreign*) return 1 ;;
    kernel) [ "$m_kernel" = yes ] && return 1 ;;
    esac
    return 0
}

# ensure PACKAGE MODULE VERSION SOURCE HID_DRIVER KERNEL_IS_ENOUGH WHEELS…: builds SimRig's
# version when it is needed and not provided otherwise, or upgrades SimRig's; gives the wheels
# plugged in to its HID driver. Returns 0, 3 (waiting for the signing key) or 1.
ensure() {
    e_package=$1 e_module=$2 e_version=$3 e_source=$4 e_driver=$5 e_kernel=$6
    shift 6
    e_state=$(mod_state "$e_package" "$e_module" "$e_version")
    case $e_state in
    foreign*)
        echo "$e_package ${e_state#foreign } is installed, not by SimRig: it is used as it is"
        return 0
        ;;
    "ours $e_version") return 0 ;;
    kernel) [ "$e_kernel" = yes ] && return 0 ;;
    esac
    needed "$e_package" "$@" || return 0
    mod_prerequisites || return 1
    e_rc=0
    mod_build "$e_package" "$e_version" "$e_source" dkms.conf "$e_module" || e_rc=$?
    if [ "$e_rc" = 0 ]; then
        hid_rebind 046D "$e_driver" "$@"
        echo "$e_package $e_version installed"
    fi
    return "$e_rc"
}
