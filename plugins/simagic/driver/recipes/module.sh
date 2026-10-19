# Kernel modules a driver builds from bundled sources with DKMS. SimRig's own version is
# installed, upgraded and removed; a module installed by other means (a distribution package,
# by hand) is used as it is and never touched. Needs dkms.sh and secureboot.sh. State, in the
# driver's data directory:
#   <package>.installed     the version SimRig installed
#   mok-pending             the one-time password to enroll the signing key (Secure Boot)

mod_data() {
    echo "${SIMRIG_DRIVER_DATA:?}"
}

# mod_ours PACKAGE: the version SimRig installed, if it did.
mod_ours() {
    cat "$(mod_data)/$1.installed" 2>/dev/null || true
}

# mod_in_kernel MODULE: the module comes with the kernel.
mod_in_kernel() {
    [ "$(modinfo -F intree "$1" 2>/dev/null)" = Y ]
}

# mod_foreign PACKAGE MODULE: a version of the module SimRig did not install and that is not
# the kernel's: another DKMS version of the package, or a module from elsewhere (a distribution
# package, built by hand). Prints its version, "unknown" if the module does not say.
mod_foreign() {
    mine=$(mod_ours "$1")
    other=$(dkms_versions "$1" | grep -vxF "${mine:-//none//}" | head -n 1)
    if [ -n "$other" ]; then
        echo "$other"
        return 0
    fi
    [ -n "$mine" ] && return 1
    modinfo -n "$2" >/dev/null 2>&1 || return 1
    mod_in_kernel "$2" && return 1
    version=$(modinfo -F version "$2" 2>/dev/null)
    echo "${version:-unknown}"
}

mod_loaded() {
    [ -d "${SIMRIG_TEST_ROOT:-}/sys/module/$1" ]
}

# mod_prerequisites: the tools and headers to build modules for the running kernel. Names what
# is missing, with the commands that install it, and fails.
mod_prerequisites() {
    missing=$(missing_commands dkms make gcc | tr '\n' ' ')
    kernel=$(uname -r)
    kernel_headers_present "$kernel" || missing="${missing}kernel headers for $kernel"
    [ -z "$(echo "$missing" | tr -d ' ')" ] && return 0
    echo "Missing to build the driver: $missing" >&2
    echo "Debian, Ubuntu: apt install dkms build-essential linux-headers-$kernel" >&2
    echo "Fedora: dnf install dkms gcc make kernel-devel-$kernel" >&2
    echo "Arch: pacman -S dkms base-devel linux-headers" >&2
    return 1
}

# mod_build PACKAGE VERSION SOURCE DKMS_CONF MODULE…
# Builds and installs SimRig's version of the package with DKMS (replacing a previous version
# SimRig installed), then loads the modules again so the new ones run. Returns 0 when they are
# loaded, 3 when Secure Boot refuses them until the signing key is enrolled at the next restart
# (the one-time password is in mok-pending), 1 on failures. Call it as `mod_build … || rc=$?`.
mod_build() {
    package="$1" version="$2" source="$3" conf="$4"
    shift 4
    data=$(mod_data)
    mkdir -p "$data"
    previous=$(mod_ours "$package")
    for module in "$@"; do
        modprobe -r "$module" 2>/dev/null || true
    done
    if [ -n "$previous" ] && [ "$previous" != "$version" ]; then
        dkms_remove "$package" "$previous"
    fi
    dkms_install "$package" "$version" "$source" "$conf" || return 1
    echo "$version" > "$data/$package.installed"

    for module in "$@"; do
        rc=0
        module_load "$module" || rc=$?
        case $rc in
        0) ;;
        2)
            rc=0
            mod_request_key || rc=$?
            return $rc
            ;;
        *)
            echo "$package $version was built but $module cannot be loaded" >&2
            return 1
            ;;
        esac
    done
    rm -f "$data/mok-pending"
    return 0
}

# Secure Boot refuses modules until the key DKMS signed them with is enrolled: asks the
# firmware to enroll it at the next restart. Returns 3 when that is pending.
mod_request_key() {
    key=$(sb_key) || {
        echo "Secure Boot refuses the driver and DKMS has no signing key on this system." >&2
        return 1
    }
    if sb_key_enrolled "$key"; then
        echo "Secure Boot refuses the driver although its signing key is enrolled." >&2
        return 1
    fi
    password=$(sb_request_enrollment "$key") || {
        echo "cannot ask the firmware to enroll the signing key" >&2
        return 1
    }
    echo "$password" > "$(mod_data)/mok-pending"
    chmod 0644 "$(mod_data)/mok-pending"
    return 3
}

# The message for a pending enrollment.
mod_key_message() {
    echo "restart, choose \"Enroll MOK\" and type $(cat "$(mod_data)/mok-pending")"
}

# mod_remove PACKAGE MODULE…: unloads the modules and removes SimRig's version of the package.
mod_remove() {
    package="$1"
    shift
    mine=$(mod_ours "$package")
    [ -n "$mine" ] || return 0
    for module in "$@"; do
        modprobe -r "$module" 2>/dev/null || true
    done
    dkms_remove "$package" "$mine"
    rm -f "$(mod_data)/$package.installed"
}

# mod_state PACKAGE MODULE VERSION: where the module comes from, as `<state> [version]`:
#   ours V      SimRig's version V, the bundled one
#   outdated V  SimRig's version V, older than the bundled one
#   pending V   SimRig's version V, waiting for the signing key to be enrolled
#   foreign V   someone else's version V
#   kernel      the kernel's
#   none        no module
mod_state() {
    mine=$(mod_ours "$1")
    if [ -n "$mine" ]; then
        if [ -f "$(mod_data)/mok-pending" ] && ! mod_loaded "$2"; then
            echo "pending $mine"
        elif [ "$mine" != "$3" ]; then
            echo "outdated $mine"
        else
            echo "ours $mine"
        fi
    elif other=$(mod_foreign "$1" "$2"); then
        echo "foreign $other"
    elif mod_in_kernel "$2"; then
        echo "kernel"
    else
        echo "none"
    fi
}

# hid_rebind VENDOR DRIVER [PID…]: gives HID devices of the vendor (hex, as in sysfs) already
# plugged in to the driver, instead of asking to replug them. With PIDs, only those products.
hid_rebind() {
    vendor=$(echo "$1" | tr '[:lower:]' '[:upper:]') driver="$2"
    shift 2
    pids=$(echo "$*" | tr '[:lower:]' '[:upper:]')
    for dev in "${SIMRIG_TEST_ROOT:-}"/sys/bus/hid/devices/*:"$vendor":*; do
        [ -e "$dev" ] || continue
        id=$(basename "$dev")
        if [ -n "$pids" ]; then
            pid=$(echo "$id" | cut -d: -f3 | cut -d. -f1)
            case " $pids " in *" $pid "*) ;; *) continue ;; esac
        fi
        current=$(basename "$(readlink "$dev/driver" 2>/dev/null)" 2>/dev/null || true)
        [ "$current" = "$driver" ] && continue
        if [ -n "$current" ]; then
            echo "$id" > "${SIMRIG_TEST_ROOT:-}/sys/bus/hid/drivers/$current/unbind" 2>/dev/null || true
        fi
        echo "$id" > "${SIMRIG_TEST_ROOT:-}/sys/bus/hid/drivers/$driver/bind" 2>/dev/null || true
    done
}

# usb_replug VENDOR [PID…]: makes USB devices of the vendor (hex) start again, as if they were
# unplugged and plugged in: the kernel enumerates them anew and the drivers just installed take
# them, and the devices send what they send at start-up (e.g. a Fanatec base's tuning menu).
usb_replug() {
    vendor=$(echo "$1" | tr '[:upper:]' '[:lower:]')
    shift
    pids=$(echo "$*" | tr '[:upper:]' '[:lower:]')
    for dev in "${SIMRIG_TEST_ROOT:-}"/sys/bus/usb/devices/*; do
        [ "$(cat "$dev/idVendor" 2>/dev/null)" = "$vendor" ] || continue
        if [ -n "$pids" ]; then
            case " $pids " in *" $(cat "$dev/idProduct" 2>/dev/null) "*) ;; *) continue ;; esac
        fi
        [ -w "$dev/authorized" ] || [ -n "${SIMRIG_TEST_ROOT:-}" ] || continue
        echo 0 > "$dev/authorized" 2>/dev/null || continue
        sleep 1
        echo 1 > "$dev/authorized" 2>/dev/null || true
    done
}

# usb_present VENDOR [PID…]: a USB device of the vendor (hex), of one of the products, is
# plugged in.
usb_present() {
    vendor=$(echo "$1" | tr '[:upper:]' '[:lower:]')
    shift
    pids=$(echo "$*" | tr '[:upper:]' '[:lower:]')
    for dev in "${SIMRIG_TEST_ROOT:-}"/sys/bus/usb/devices/*; do
        [ "$(cat "$dev/idVendor" 2>/dev/null)" = "$vendor" ] || continue
        [ -z "$pids" ] && return 0
        case " $pids " in *" $(cat "$dev/idProduct" 2>/dev/null) "*) return 0 ;; esac
    done
    return 1
}
