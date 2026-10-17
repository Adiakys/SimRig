#!/bin/sh
# Renders the static mockups to PNG with headless Firefox (light and dark themes).
# Usage: docs/design/render.sh [screen...]
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
profile="$here/../../target/ffprofile-design"
out="$here/mockups/png"
mkdir -p "$profile" "$out"
screens="${*:-home home-empty component add-1-connect add-2-driver add-3-setup add-4-done first-1-welcome first-2-devices first-3-rig style-guide}"
for s in $screens; do
  for t in light dark; do
    size=1440,900
    [ "$s" = style-guide ] && size=1440,1880
    timeout 60 firefox --headless --no-remote --profile "$profile" --window-size=$size \
      --screenshot "$out/$s-$t.png" "file://$here/mockups/mockup.html?screen=$s&theme=$t" >/dev/null 2>&1 || echo "failed: $s $t"
    echo "$s-$t.png"
  done
done
