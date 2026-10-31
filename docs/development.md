# SimRig: development

How to build, run and test SimRig from its sources. For what SimRig is and how to install it,
see the [README](../README.md); for how it is designed, [architecture.md](architecture.md) and
[requirements.md](requirements.md).

## Build and run

Clone with the bundled third-party drivers (git submodules):

```sh
git clone --recursive https://github.com/Adiakys/SimRig.git
# or, in an existing clone: git submodule update --init
```

Requires a recent stable Rust toolchain (1.92+ for the app, 1.86+ for the rest) and, to build
the app, the fontconfig and xkbcommon development files (`libfontconfig1-dev libxkbcommon-dev`
on Debian and Ubuntu, `fontconfig-devel libxkbcommon-devel` on Fedora).

### The app

```sh
scripts/dev-gui.sh              # the app on the simulated rig, in throwaway directories
scripts/dev-gui.sh plug         # while it runs: plug a fake USB device, then "Add device"
scripts/dev-gui.sh report 00 01 # move it (its raw report, in hex) while setting it up
scripts/dev-gui.sh reset        # forget the rig, the presets, the drivers and the first run
```

Everything lives under `target/dev`: your real configuration is not touched. Without a display
(or to check a screen), the app can draw itself into a PNG:

```sh
./target/debug/simrig --plugin-dir plugins/sim --no-default-dirs --config-dir target/shots/cfg \
    --screenshot target/shots/home.png --theme dark --screen component:pedals
```

`--screen` takes `home`, `component:<component>`, `procedure:<component>/<id>`, `plugins`,
`settings`, `add`, `first:<step>`, and can chain actions with `;` (see `src/main.rs`). Device
checks can be shown on the simulated rig with made-up findings on the open device, e.g.
`component:base;findings:driver,!steam` (`!` marks one ignored; `;ignored` shows those again).

`simrig-cli device-checks` runs the device checks on the real devices.

### A development build with real hardware

Drivers are installed only by system plugins (root-owned). To try a development build with real
devices, `scripts/install-system.sh` installs `simrig-helper`, its polkit action and the official
plugins system-wide, like a package does; the app then installs a plugin's driver with your
password when a device is set up. `SIMRIG_USB_ROOT=/sys/bus/usb/devices scripts/dev-gui.sh` runs
the development plugins on the real USB devices instead.

```sh
cargo build --release
sudo scripts/install-system.sh          # sudo scripts/install-system.sh remove to undo
./target/release/simrig                 # Add device
```

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
| `crates/simrig-driver` | Driver contract (`driver.toml`), hooks, installation registry, recipes for drivers (`recipes/`: DKMS, kernel modules, Secure Boot, udev); `testing`: a sandbox for driver hooks |
| `crates/simrig-helper` | Privileged helper, run through `pkexec` only for driver operations |
| `crates/simrig-host` | The app core: plugins, rig, events, drivers, presets, catalog, system check |
| `crates/simrig-cli` | Command-line interface |
| `crates/simrig-gui` | The desktop app (`simrig`, Slint) |
| `plugins/sim` | Simulated rig plugin with a fake driver, and end-to-end tests |
| `plugins/generic-usb` | Generic USB plugin: any USB device as a shifter or a handbrake, with its driver service |
| `crates/simrig-plugin-kit` | Shared by brand plugins: the HID device a kernel driver is bound to, sysfs, evdev inputs and force feedback, pedal and shifter mapping |
| `plugins/fanatec` | Fanatec plugin: bases, rims and pedals; bundles [hid-fanatecff](https://github.com/gotzl/hid-fanatecff) as a submodule, built with DKMS |
| `plugins/moza` | Moza plugin: bases, pedals, handbrake, sequential shifter; settings over the serial port ([Boxflat](https://github.com/Lawstorant/boxflat)'s protocol); bundles [universal-pidff](https://github.com/JacKeTUs/universal-pidff) for kernels older than 6.15 |
| `plugins/logitech` | Logitech plugin: G29 and G920 families, G PRO, RS50, WingMan; bundles [new-lg4ff](https://github.com/berarma/new-lg4ff) and the [TrueForce driver](https://github.com/mescon/logitech-trueforce-linux-driver), built with DKMS for the wheels that need them |
| `plugins/thrustmaster` | Thrustmaster plugin: T300 RS, T248, TX, TS-PC, TS-XW, T150, TMX, T500 RS and T80 (inputs only); bundles [hid-tmff2](https://github.com/Kimplul/hid-tmff2) (with hid-tminit nested) and [hid-t150](https://github.com/scarburato/t150_driver) as submodules, built with DKMS |
| `plugins/pxn` | PXN plugin: V10, V12, VD4, VD6; VD settings over the vendor control interface ([Pitwall](https://github.com/unionyxx/pitwall)'s protocol); bundles [universal-pidff](https://github.com/JacKeTUs/universal-pidff) for kernels older than 6.15 |
| `plugins/pidff` | Standard force feedback plugin: Cammus, Asetek, VRS, FFBeast, Turtle Beach, OpenFFBoard bases (inputs, force feedback, strength and centering through the input interface); bundles [universal-pidff](https://github.com/JacKeTUs/universal-pidff) for kernels older than 6.15 |
| `plugins/simagic` | Simagic plugin: Alpha and EVO bases; bundles [simagic-ff](https://github.com/JacKeTUs/simagic-ff) as a submodule, built with DKMS for the bases that need it |
| `assets/devices` | The core's device drawings and how they move (`devices.toml`) |
| `docs/design` | Design: flows; the style guide and product context are `DESIGN.md` and `PRODUCT.md` at the root |
| `scripts` | Development helpers |
| `packaging/polkit` | polkit action of the helper |

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Set `SIMRIG_PLUGIN_LOG=1` to see plugins' stderr.

**Changing a driver** (anything under `plugins/*/driver`) makes the fingerprints test fail on
purpose. Installed drivers are offered an update only when `version` or `revision` in their
`driver.toml` changes: raise `revision` when drivers already installed need the change (udev
rules, a service such as the generic USB one, built from `plugins/generic-usb/src`, hooks run
from the installed copy: uninstall and privileged operations); leave it for changes that only
matter to new installations. Then record the files:

```sh
SIMRIG_UPDATE_FINGERPRINTS=1 cargo test -p simrig-driver --test fingerprints
```


## Releases

Publishing a GitHub Release (tag `v1.2.3`) runs `.github/workflows/release.yml`: it stamps the
version, tests, builds, and attaches the packages (.deb, .rpm, Arch, .tar.gz) to the release.
Locally, with
[nfpm](https://nfpm.goreleaser.com) installed:

```sh
cargo build --release
packaging/build-packages.sh 1.2.3       # → dist/packages
```
