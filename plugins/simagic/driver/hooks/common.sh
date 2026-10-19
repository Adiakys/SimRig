# Shared by the hooks of the Simagic driver (sourced, not run).

ROOT="${SIMRIG_TEST_ROOT:-}"
DATA="${SIMRIG_DRIVER_DATA:?}"
PACKAGE=simagic-ff
MODULE=hid_simagic_ff
VERSION=$(cat "$SIMRIG_DRIVER_DIR/simagic-ff.version")
SOURCE="$SIMRIG_DRIVER_DIR/simagic-ff"
RULES=71-simrig-simagic.rules
# The settings saved for the next plug-in.
SETTINGS=72-simrig-simagic-settings.rules
# The bases simagic-ff drives (hid-simagic.h): the EVO series, and the Alpha family's id.
EVO_BASES="0500 0501 0502"

for recipe in prereq status dkms secureboot udev module; do
    . "$SIMRIG_DRIVER_DIR/recipes/$recipe.sh"
done

rules_installed() {
    [ -f "$DATA/rules" ]
}

# The HID report descriptor on stdin declares the PID usage page (standard force feedback):
# a firmware from before v159, which the kernel drives itself.
declares_pid() {
    od -An -v -tu1 | tr -s ' \n' '\n\n' | awk '
        NF == 0 { next }
        { b[n++] = $1 }
        END {
            i = 0
            while (i < n) {
                p = b[i]
                if (p == 254) { i += 3 + b[i + 1]; continue }
                s = p % 4; if (s == 3) s = 4
                if (p - p % 4 == 4) {
                    v = 0
                    for (k = s; k >= 1; k--) v = v * 256 + b[i + k]
                    if (v == 15) { found = 1; break }
                }
                i += 1 + s
            }
            exit found ? 0 : 1
        }'
}

# A base plugged in needs simagic-ff: an EVO, or an Alpha with a newer firmware.
needs_module() {
    usb_present 3670 $EVO_BASES && return 0
    for dev in "$ROOT"/sys/bus/hid/devices/0003:0483:0522.*; do
        [ -f "$dev/report_descriptor" ] || continue
        declares_pid < "$dev/report_descriptor" || return 0
    done
    return 1
}

# A base needs simagic-ff, or SimRig built it before (so it stays up to date).
needed() {
    needs_module || [ -n "$(mod_ours "$PACKAGE")" ]
}
