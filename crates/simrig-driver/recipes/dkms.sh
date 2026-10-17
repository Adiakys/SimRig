# Out-of-tree kernel modules through DKMS: built for every installed kernel and rebuilt by DKMS
# on kernel upgrades. Install and remove run as root.

dkms_src_dir() {
    echo "${SIMRIG_TEST_ROOT:-}/usr/src/$1-$2"
}

# Versions of a DKMS package known to DKMS, one per line (any state, any kernel).
dkms_versions() {
    dkms status -m "$1" 2>/dev/null |
        sed -n "s|^$1[/,] *\([^,:]*\).*|\1|p" | sort -u
}

# The package version is installed for a kernel (default: the running one).
dkms_installed() {
    dkms status -m "$1" -v "$2" -k "${3:-$(uname -r)}" 2>/dev/null | grep -q ': installed'
}

# dkms_install NAME VERSION SOURCE_DIR
# Copies the sources to /usr/src/NAME-VERSION (without VCS files), replaces the `#VERSION#`
# placeholder of dkms.conf and the sources, then adds, builds and installs the module for the
# running kernel and for every other kernel that has headers. Idempotent. On a build failure,
# prints the build log.
dkms_install() {
    name="$1" version="$2" source="$3"
    dest=$(dkms_src_dir "$name" "$version")
    rm -rf "$dest"
    mkdir -p "$dest"
    (cd "$source" && tar cf - --exclude=.git --exclude=.github .) | (cd "$dest" && tar xf -)
    find "$dest" -type f \( -name dkms.conf -o -name '*.c' -o -name '*.h' \) \
        -exec sed -i "s/#VERSION#/$version/g" {} +
    if ! dkms status -m "$name" -v "$version" 2>/dev/null | grep -q .; then
        dkms add -m "$name" -v "$version" >/dev/null
    fi
    running=$(uname -r)
    for kernel in "$running" $(ls "${SIMRIG_TEST_ROOT:-}/lib/modules" 2>/dev/null); do
        [ "$kernel" = "$running" ] || [ -d "${SIMRIG_TEST_ROOT:-}/lib/modules/$kernel/build" ] || continue
        dkms_installed "$name" "$version" "$kernel" && continue
        if ! dkms install -m "$name" -v "$version" -k "$kernel" >/dev/null 2>&1; then
            if [ "$kernel" = "$running" ]; then
                log="${SIMRIG_TEST_ROOT:-}/var/lib/dkms/$name/$version/build/make.log"
                echo "building $name $version for $kernel failed" >&2
                [ -r "$log" ] && tail -n 40 "$log" >&2
                return 1
            fi
            # Other kernels are best effort: DKMS retries when they boot.
            echo "warning: $name $version not built for $kernel" >&2
        fi
    done
}

# dkms_remove NAME VERSION: removes the package from every kernel and its sources.
dkms_remove() {
    dkms remove -m "$1" -v "$2" --all >/dev/null 2>&1 || true
    rm -rf "$(dkms_src_dir "$1" "$2")"
}

# module_load MODULE: loads it. Returns 2 when the kernel refuses its signature (Secure Boot
# with a key not enrolled yet), 1 on other failures.
module_load() {
    out=$(modprobe "$1" 2>&1) && return 0
    case "$out" in
        *"Key was rejected"*|*"Required key not available"*) return 2 ;;
    esac
    echo "$out" >&2
    return 1
}
