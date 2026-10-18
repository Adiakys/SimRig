# The brand plugins SimRig ships, with the driver sources each bundles: git submodules in
# plugins/<plugin>/driver/<source>. Sourced by the staging and install scripts.

BRANDS="fanatec moza logitech thrustmaster pxn pidff"

brand_sources() {
    case $1 in
    fanatec) echo hid-fanatecff ;;
    moza | pxn | pidff) echo universal-pidff ;;
    logitech) echo new-lg4ff logitech-trueforce ;;
    thrustmaster) echo hid-tmff2 hid-t150 ;;
    esac
}

# The parts of a source that are shipped, when not all of it: the TrueForce repository also
# holds desktop apps, Windows tools and documentation; only the kernel module is needed.
source_parts() {
    case $1 in
    logitech-trueforce) echo mainline COPYING ;;
    esac
}

# brand_check ROOT: every bundled source is checked out.
brand_check() {
    for brand in $BRANDS; do
        for source in $(brand_sources "$brand"); do
            dir="$1/plugins/$brand/driver/$source"
            [ -n "$(ls -A "$dir" 2>/dev/null)" ] || {
                echo "$dir is empty: git submodule update --init --recursive" >&2
                return 1
            }
        done
    done
}

# brand_install ROOT BIN DEST BRAND: the plugin's manifest, adapter and driver (with its
# sources, without VCS files) into DEST/BRAND.
brand_install() {
    target="$3/$4"
    install -d -m 0755 "$target"
    install -m 0644 "$1/plugins/$4/plugin.toml" "$target/"
    install -m 0755 "$2/simrig-plugin-$4" "$target/"
    excludes=""
    partial=""
    for source in $(brand_sources "$4"); do
        parts=$(source_parts "$source")
        [ -n "$parts" ] || continue
        excludes="$excludes --exclude=driver/$source"
        for part in $parts; do
            partial="$partial driver/$source/$part"
        done
    done
    # shellcheck disable=SC2086 # word splitting intended
    (cd "$1/plugins/$4" && tar cf - --exclude=.git --exclude=.github $excludes driver) |
        (cd "$target" && tar xf -)
    if [ -n "$partial" ]; then
        # shellcheck disable=SC2086
        (cd "$1/plugins/$4" && tar cf - --exclude=.git --exclude=.github $partial) |
            (cd "$target" && tar xf -)
    fi
}

# brand_licenses ROOT DOC: the license of every bundled source, as DOC/LICENSE.<source>.
brand_licenses() {
    for brand in $BRANDS; do
        for source in $(brand_sources "$brand"); do
            for name in LICENSE COPYING; do
                file="$1/plugins/$brand/driver/$source/$name"
                if [ -f "$file" ]; then
                    install -m 0644 "$file" "$2/LICENSE.$source"
                    break
                fi
            done
        done
    done
}
