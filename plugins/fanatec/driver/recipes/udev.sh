# udev rules of a driver: installed, removed and applied to the devices already plugged in.

udev_rules_dir() {
    echo "${SIMRIG_TEST_ROOT:-}/etc/udev/rules.d"
}

# udev_install FILE: writes the rules read from stdin to /etc/udev/rules.d/FILE.
udev_install() {
    dir=$(udev_rules_dir)
    mkdir -p "$dir"
    cat > "$dir/.$1.tmp"
    mv "$dir/.$1.tmp" "$dir/$1"
}

udev_remove() {
    rm -f "$(udev_rules_dir)/$1"
}

# udev_apply [SUBSYSTEM…]: reloads the rules and replays "change" for the subsystems, so
# devices already plugged in get them.
udev_apply() {
    udevadm control --reload
    for subsystem in "$@"; do
        udevadm trigger --action=change --subsystem-match="$subsystem"
    done
    udevadm settle --timeout=10 || true
}
