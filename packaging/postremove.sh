#!/bin/sh
# After SimRig is removed. Drivers are independent of the app: a plain remove keeps them, so the
# devices keep working; a purge (Debian, Ubuntu) removes them too. See architecture §6.5.
remove_drivers=/var/lib/simrig/remove-drivers

case "${1:-}" in
purge)
    if [ -x "$remove_drivers" ]; then
        "$remove_drivers" --all || true
    fi
    ;;
upgrade|1|2)
    # An upgrade (deb: upgrade; rpm: 1 or more installs left): nothing to say.
    ;;
*)
    if [ -x "$remove_drivers" ]; then
        echo "SimRig was removed; the drivers it installed were kept, so your devices keep working."
        echo "To remove them too: sudo $remove_drivers --all"
    fi
    ;;
esac
exit 0
