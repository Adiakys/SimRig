#!/bin/sh
# Installs SimRig from this archive, for distributions without a package. Run as root:
#   sudo ./install.sh            install (or update)
#   sudo ./install.sh remove     remove the app (the drivers it installed are kept)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
[ "$(id -u)" = 0 ] || { echo "run it with sudo" >&2; exit 1; }
case "${1:-install}" in
install)
    (cd "$here/root" && find . -type f -o -type l) | while read -r f; do
        install -D "$here/root/$f" "/$f"
    done
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q /usr/share/icons/hicolor || true
    echo "SimRig installed: run simrig"
    ;;
remove)
    (cd "$here/root" && find . -type f -o -type l) | while read -r f; do
        rm -f "/$f"
    done
    rm -rf /usr/lib/simrig /usr/libexec/simrig /usr/share/doc/simrig
    sh "$here/postremove.sh" remove
    ;;
*)
    echo "usage: sudo $0 [install|remove]" >&2
    exit 2
    ;;
esac
