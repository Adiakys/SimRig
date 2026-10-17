# The brand plugins SimRig ships, with the driver sources each bundles: git submodules in
# plugins/<plugin>/driver/<source>. Sourced by the staging and install scripts.

BRANDS="fanatec moza"

brand_sources() {
    case $1 in
    fanatec) echo hid-fanatecff ;;
    moza) echo universal-pidff ;;
    logitech) echo new-lg4ff logitech-trueforce ;;
    thrustmaster) echo hid-tmff2 ;;
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
    rm -rf "$target"
    install -d -m 0755 "$target"
    install -m 0644 "$1/plugins/$4/plugin.toml" "$target/"
    install -m 0755 "$2/simrig-plugin-$4" "$target/"
    (cd "$1/plugins/$4" && tar cf - --exclude=.git --exclude=.github driver) | (cd "$target" && tar xf -)
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
