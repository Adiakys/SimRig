#!/usr/bin/env bash
# The public site (site/), published on GitHub Pages by .github/workflows/pages.yml.
#
#   scripts/site.sh stage   copy the shared assets (fonts, the mark, device drawings) into site/
#   scripts/site.sh build   stage, then build into target/site as GitHub Pages does (Docker)
#   scripts/site.sh serve   build, then serve it at http://localhost:4000/SimRig/
#   scripts/site.sh shots   redraw the app's screenshots, the demo video and the link preview
#                           (Pillow, ffmpeg)
set -euo pipefail
cd "$(dirname "$0")/.."

# The image of actions/jekyll-build-pages@v1: the same Jekyll and plugins as GitHub Pages.
PAGES_IMAGE=ghcr.io/actions/jekyll-build-pages:v1.0.13

stage() {
    # Not kept in git (site/.gitignore): the originals stay where the app uses them.
    mkdir -p site/assets/fonts site/assets/images site/assets/devices
    cp assets/fonts/Barlow-Regular.ttf assets/fonts/Barlow-Medium.ttf \
        assets/fonts/Barlow-SemiBold.ttf assets/fonts/MartianMono-Variable.ttf site/assets/fonts/
    cp crates/simrig-gui/ui/icons/mark.svg site/assets/images/mark.svg
    cp assets/devices/*.svg site/assets/devices/
}

build() {
    stage
    rm -rf target/site
    mkdir -p target/site
    # Rootless Docker already maps the container's root to you.
    local user=(--user "$(id -u):$(id -g)")
    docker info -f '{{.SecurityOptions}}' | grep -q rootless && user=()
    docker run --rm "${user[@]}" -v "$PWD:/work" \
        -e GITHUB_WORKSPACE=/work -e INPUT_SOURCE=site -e INPUT_DESTINATION=target/site/SimRig \
        -e GITHUB_REPOSITORY=Adiakys/SimRig "$PAGES_IMAGE"
}

shots() {
    cargo build -p simrig-gui
    local tmp out=site/assets/images
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' RETURN
    mkdir -p "$out"
    for theme in light dark; do
        for screen in home component:base component:pedals component:shifter; do
            local name=${screen#component:}
            ./target/debug/simrig --plugin-dir plugins/sim --no-default-dirs \
                --config-dir "$tmp/cfg-$theme-$name" --screenshot "$tmp/$name-$theme.png" \
                --theme "$theme" --screen "$screen" --size 1440x900
        done
    done
    # The README's demo, as a light video loop.
    mkdir -p site/assets/video
    ffmpeg -v error -y -i docs/images/simrig.gif -movflags +faststart -pix_fmt yuv420p \
        -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" -c:v libx264 -crf 24 -preset slow site/assets/video/simrig.mp4
    ffmpeg -v error -y -i docs/images/simrig.gif -vframes 1 "$tmp/poster.png"
    python3 -I - "$tmp" "$out" <<'EOF'
import sys
from PIL import Image, ImageDraw

src, out = sys.argv[1], sys.argv[2]
Image.open(f"{src}/poster.png").convert("RGB").save("site/assets/video/simrig-poster.webp", quality=85)
for name in ("home", "base", "pedals", "shifter"):
    for theme in ("light", "dark"):
        Image.open(f"{src}/{name}-{theme}.png").convert("RGB").save(
            f"{out}/{name}-{theme}.webp", quality=88, method=6)

# Link previews: the rig, 1200x630.
home = Image.open(f"{src}/home-dark.png").convert("RGB")
home.resize((1200, 750), Image.LANCZOS).crop((0, 0, 1200, 630)).save(f"{out}/social.png", optimize=True)

# The mark (crates/simrig-gui/ui/icons/mark.svg) as a PNG, for search engines' logos.
s = 512 / 22
mark = Image.new("RGBA", (512, 512), (0, 0, 0, 0))
d = ImageDraw.Draw(mark)
ink = "#231e0d"
d.rounded_rectangle((0, 0, 511, 511), radius=5 * s, fill="#f3c726")
r, w = 6.3 * s, 2.2 * s
cx, cy = 11 * s, 11.6 * s
d.ellipse((cx - r - w / 2, cy - r - w / 2, cx + r + w / 2, cy + r + w / 2), outline=ink, width=round(w))
d.rectangle((4.6 * s, cy - w / 2, 17.4 * s, cy + w / 2), fill=ink)
d.rounded_rectangle((9.9 * s, 3.4 * s, 12.1 * s, 7.0 * s), radius=0.6 * s, fill=ink)
mark.save(f"{out}/mark.png", optimize=True)
EOF
}

case "${1:-}" in
    stage) stage ;;
    build) build ;;
    serve)
        build
        echo "http://localhost:4000/SimRig/"
        python3 -m http.server --directory target/site 4000
        ;;
    shots) shots ;;
    *)
        sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
        exit 1
        ;;
esac
