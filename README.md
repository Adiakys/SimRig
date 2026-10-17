# SimRig

Configure your sim-racing rig on Linux — wheel base, steering wheel, pedals, shifter and
handbrake — whatever the brand, from one polished app.

> **Status: early development.** The core (M0) is done and tested on a simulated rig: plugins,
> hotplug, drivers with a privileged helper, saved settings, rig composition, presets, catalog,
> guided procedures defined by plugins, generic plugins for unbranded devices. The desktop app
> (M1, Slint) runs on it: live rig, settings, guided procedures, adding a device, plugins and
> drivers, presets, first run. Next: the generic USB plugin, then the first real hardware.

## How it works

- **The app is a client of the drivers, just like a game.** It is never in the data path, and
  when it is closed it leaves no process running.
- **Drivers are independent.** They make devices work with the app closed, are installed when
  you add a device that needs them, and decide for themselves how they work.
- **The core defines standard interfaces** for each component type. Plugins implement them and
  report anything their hardware cannot do as *unsupported, with a reason*: the app shows it
  disabled with the explanation, never hides it.
- **Brand support comes from plugins**: external processes speaking a versioned JSON-RPC
  protocol, which may add their own self-described custom parameters. Any mix of brands works.
- **No root at runtime.** Changing settings, testing force feedback and applying presets never
  need privileges; the app asks for your password only to install or remove a driver, or when a
  driver keeps a saved setting in a system file.

Read more in [docs/requirements.md](docs/requirements.md) and
[docs/architecture.md](docs/architecture.md).

## Try it

Requires a recent stable Rust toolchain (1.92+ for the app, 1.86+ for the rest) and, to build
the app, the fontconfig and xkbcommon development files (`libfontconfig1-dev libxkbcommon-dev`
on Debian and Ubuntu, `fontconfig-devel libxkbcommon-devel` on Fedora).

### The app

```sh
scripts/dev-gui.sh              # the app on the simulated rig, in throwaway directories
scripts/dev-gui.sh plug         # while it runs: plug a fake USB device, then "Add device"
scripts/dev-gui.sh reset        # forget the rig, the presets and the first run
```

Everything lives under `target/dev`: your real configuration is not touched. Without a display
(or to check a screen), the app can draw itself into a PNG:

```sh
./target/debug/simrig --plugin-dir plugins/sim --no-default-dirs --config-dir target/shots/cfg \
    --screenshot target/shots/home.png --theme dark --screen component:pedals
```

`--screen` takes `home`, `component:<component>`, `procedure:<component>/<id>`, `plugins`,
`settings`, `add`, `first:<step>`, and can chain actions with `;` (see `src/main.rs`).

### The command line

```sh
cargo build --workspace
alias simrig-cli='./target/debug/simrig-cli --plugin-dir plugins --no-default-dirs'

simrig-cli system                                 # what the setup wizard checks
simrig-cli list                                   # the simulated rig
simrig-cli show base                              # every item, unsupported ones with their reason
simrig-cli set shifter mode sequential            # watch `gear` become unavailable
simrig-cli watch pedals                           # live inputs, Ctrl+C to stop
simrig-cli run base test_effect                   # asks for confirmation

simrig-cli driver status                          # the simulated driver is not installed
simrig-cli driver install sim                     # development plugin: no password needed
simrig-cli set base rotation 540 --save           # restored by the driver next time
simrig-cli get base rotation
simrig-cli preset capture GT3 && simrig-cli preset show GT3
simrig-cli exclude handbrake && simrig-cli list
simrig-cli configure pedals calibrate-brake       # a guided procedure defined by the plugin
simrig-cli devices                                # generic0 has no components yet…
simrig-cli configure sim/generic0 setup           # …until its setup procedure maps it
simrig-cli monitor                                # rig changes as they happen
simrig-cli driver uninstall sim                   # leaves nothing behind
```

Each command starts the plugins, talks to them and stops them. The simulated devices live inside
the plugin process, so a value set by one command is gone in the next unless saved (`--save`):
real devices keep it.

To experiment without touching your real configuration, set `XDG_CONFIG_HOME` and
`XDG_STATE_HOME` to a temporary directory.

## Repository layout

| Path | What |
|---|---|
| `crates/simrig-model` | Domain model: component interfaces, parameters, unsupported reasons, custom parameters |
| `crates/simrig-protocol` | App ↔ plugin protocol (JSON-RPC 2.0 over stdio) |
| `crates/simrig-plugin-sdk` | SDK to write plugin adapters in Rust |
| `crates/simrig-driver` | Driver contract (`driver.toml`), hooks, installation registry |
| `crates/simrig-helper` | Privileged helper, run through `pkexec` only for driver operations |
| `crates/simrig-host` | The app core: plugins, rig, events, drivers, presets, catalog, system check |
| `crates/simrig-cli` | Command-line interface |
| `crates/simrig-gui` | The desktop app (`simrig`, Slint) |
| `plugins/sim` | Simulated rig plugin with a fake driver, and end-to-end tests |
| `assets/devices` | The core's device drawings and how they move (`devices.toml`) |
| `docs/design` | Design: flows, mockups, the style guide (`DESIGN.md`, `PRODUCT.md` at the root) |
| `scripts` | Development helpers |
| `packaging/polkit` | polkit action of the helper |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Set `SIMRIG_PLUGIN_LOG=1` to see plugins' stderr.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
