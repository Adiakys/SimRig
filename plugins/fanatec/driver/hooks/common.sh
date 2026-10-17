# Shared by the hooks of the Fanatec driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
NAME=hid-fanatec
MODULE=hid_fanatec
VERSION=$(cat "$SIMRIG_DRIVER_DIR/hid-fanatecff.version")
SOURCE="$SIMRIG_DRIVER_DIR/hid-fanatecff"
DATA="$SIMRIG_DRIVER_DATA"
RULES=71-simrig-fanatec.rules

for recipe in prereq status dkms secureboot udev; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

# The version SimRig installed, if it did.
ours() {
    cat "$DATA/installed" 2>/dev/null || true
}

# A hid-fanatec SimRig did not install: another DKMS version, or a module from elsewhere (a
# distribution package, built by hand, in the kernel). Prints its version, "unknown" if the
# module does not say.
foreign() {
    mine=$(ours)
    other=$(dkms_versions "$NAME" | grep -vxF "${mine:-//none//}" | head -n 1)
    if [ -n "$other" ]; then
        echo "$other"
        return 0
    fi
    [ -n "$mine" ] && return 1
    path=$(modinfo -n "$MODULE" 2>/dev/null) || return 1
    [ -n "$path" ] || return 1
    version=$(modinfo -F version "$MODULE" 2>/dev/null)
    echo "${version:-unknown}"
}

loaded() {
    [ -d "$ROOT/sys/module/$MODULE" ]
}

# Fanatec devices already plugged in and bound to another driver (hid-generic): give them to
# the fanatec driver now, instead of asking to replug them.
rebind() {
    for dev in "$ROOT"/sys/bus/hid/devices/*:0EB7:*; do
        [ -e "$dev" ] || continue
        id=$(basename "$dev")
        driver=$(basename "$(readlink "$dev/driver" 2>/dev/null)" 2>/dev/null || true)
        [ "$driver" = fanatec ] && continue
        if [ -n "$driver" ]; then
            echo "$id" > "$ROOT/sys/bus/hid/drivers/$driver/unbind" 2>/dev/null || true
        fi
        echo "$id" > "$ROOT/sys/bus/hid/drivers/fanatec/bind" 2>/dev/null || true
    done
}
