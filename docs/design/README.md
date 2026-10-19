# SimRig design (M1)

| Deliverable | Where |
|---|---|
| Product context: users, personality, anti-references, principles | [`PRODUCT.md`](../../PRODUCT.md) |
| Style guide: tokens (both themes), typography, components, rules | [`DESIGN.md`](../../DESIGN.md) |
| User flows | [`flows.md`](flows.md) |
| Core device drawings and their animation contract | [`assets/devices`](../../assets/devices), [`devices.toml`](../../assets/devices/devices.toml) |
| Fonts (OFL) | [`assets/fonts`](../../assets/fonts) |

The app (`crates/simrig-gui`) implements them in Slint. Its `--screenshot` mode draws any screen
into a PNG with the software renderer, without a display: CI keeps the screenshots of the main
screens as an artifact. Mockups made while designing a screen are not kept in the repository.
