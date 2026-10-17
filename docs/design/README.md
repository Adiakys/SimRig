# SimRig design (M1)

| Deliverable | Where |
|---|---|
| Product context: users, personality, anti-references, principles | [`PRODUCT.md`](../../PRODUCT.md) |
| Style guide: tokens (both themes), typography, components, rules | [`DESIGN.md`](../../DESIGN.md), board [`mockups/png/style-guide-light.png`](mockups/png/style-guide-light.png) / [`-dark`](mockups/png/style-guide-dark.png) |
| User flows | [`flows.md`](flows.md) |
| Static mockups, light and dark | [`mockups/png`](mockups/png) |
| Core device drawings and their animation contract | [`assets/devices`](../../assets/devices), [`devices.toml`](../../assets/devices/devices.toml) |
| Fonts (OFL) | [`assets/fonts`](../../assets/fonts) |

The mockups are HTML rendered to PNG with headless Firefox: edit `mockups/mockup.js` or
`mockups/mockup.css`, then run `docs/design/render.sh [screen…]`. They use the real fonts, tokens and
device drawings; `devices.js` renders the drawings with the same rules the app will use.

The app (`crates/simrig-gui`) implements them in Slint. Its `--screenshot` mode draws any screen
into a PNG with the software renderer, without a display: CI keeps the screenshots of the main
screens as an artifact. Since the mockups were made, the build changed a few things: the
add-device icon is a wheel with a plus (never a bare "+"), copy is impersonal ("4 devices
found"), and the pedals do not move (their travel is in the pedal bars).
