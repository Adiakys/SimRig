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
