# simrig — Architecture

> Draft 0.6 (2026-10-17). M0 implemented. Companion to [requirements.md](requirements.md).

## 1. Principles

1. **The app is a client of the drivers, just like a game.** It reads and configures devices
   through the drivers' public interfaces (evdev, FFB effects, sysfs, and the device's
   configuration protocol where needed). It is never in the data path between device and game.
2. **Every driver is independent and decides how it works**: kernel module, HID-BPF, udev rules,
   its own background services, any combination. It works with the app closed. The app only
   talks to it.
3. **A closed app leaves no running processes.** No agent, no simrig service. (Any process a
   driver runs belongs to the driver, not to the app.)
4. **Drivers are installed when a device is added**, only if needed, through a minimal contract
   (install / check / remove), and stay installed independently of the app.
5. **The core defines standard interfaces, plugins implement them.** For each component type
   (wheel base, steering wheel, pedals, shifter, handbrake) the core defines the standard inputs,
   parameters and actions. A plugin implements the interface and may mark any item as
   *unsupported*.
6. **Plugins may add custom parameters.** Anything beyond the standard interface is declared by
   the plugin at runtime, fully self-described, and the app loads and renders it dynamically.
   Nothing brand-specific is ever written in the core.
7. **Least privilege.** The app runs as the user. Privileged driver operations go through a
   single-shot helper.

## 2. Two worlds: system and app

```
 ── SYSTEM (always on, even without simrig) ───────────────────────────────────────────────
                                                                         ┌──────────────┐
   device ──USB──▶ driver (built however it wants)                    ──▶│ game         │
                     ▲                         │  evdev, FFB, sysfs        └──────────────┘
                     │ configuration           ▼
             decided by the driver (udev, files, device memory…)
 ─────────────────────│──────────────────────────│──────────────────────────────────────────
 ── APP (only while open)                         │ same interfaces games use
                     │                  ┌────────▼─────────┐
             simrig-helper ◀──pkexec────│  simrig-gui/cli  │──stdio──▶ plugins (adapters)
           (runs driver hooks:          │  host: rig,      │
            install, ...)               │  catalog, presets │
                                        └──────────────────┘
```

| Piece | Lives in | Runs | Role |
|---|---|---|---|
| **Driver** | system | as the driver decides (possibly with its own processes) | makes the device work and remembers its settings |
| `simrig-gui` / `simrig-cli` | user | only while open | configure, calibrate, test, install, apply presets |
| **Plugin (adapter)** | user | started by the app, stopped with the app | translates a driver's interfaces into components |
| `simrig-helper` | root, single call | on demand only, via polkit | runs hooks declared by drivers (install, uninstall, privileged operations) |

With the app closed, no simrig process runs. GUI and CLI use the `simrig-host` library directly;
there is no background agent.

## 3. Domain model

### 3.1 Physical device ≠ component

```
Rig
 ├── Device  "Fanatec CSL DD"   (USB 0eb7:0020, plugin: fanatec, driver: hid-fanatec)
 │     ├── Component  WheelBase       model: fanatec/csl-dd
 │     ├── Component  SteeringWheel   model: fanatec/csl-steering-wheel-p1   ← hot-swappable
 │     └── Component  Pedals          model: fanatec/csl-pedals              ← excluded: nothing on the RJ12 port
 ├── Device  "MOZA SR-P pedals" (USB 346e:…, plugin: moza)
 │     └── Component  Pedals          model: moza/sr-p
 ├── Device  "Sequential shifter" (plugin: generic-hid, driver: in-kernel hid-generic)
 │     └── Component  Shifter         (sequential)
 └── Device  "USB handbrake"    (plugin: generic-hid, driver: in-kernel hid-generic)
       └── Component  Handbrake
```

The plugin declares which components each device provides *right now* and notifies changes.
When it cannot know (e.g. pedals plugged into the base), the wizard asks the user and stores the
answer in the rig.

### 3.1.1 Mixing brands

A rig is any combination of devices: **there is no constraint between brands or plugins**. A
Fanatec base and wheel with Moza pedals, a Simagic handbrake and a generic shifter are as normal
as a single-brand rig.

- **Each device belongs to exactly one plugin.** When several plugins match the same USB device,
  the most specific match wins (vendor and product over vendor only); a tie goes to the plugin
  found first and is reported. *(Planned)* the user can override the choice per device.
- **Several components of the same type** are allowed: two pedal sets, an H-pattern and a
  sequential shifter, a base with a separate USB rim. Components are always addressed by
  `<plugin>/<component id>`, never by type alone.
- **Optional components can be excluded** from the rig by the user, e.g. the pedal inputs a
  Fanatec base always exposes when the pedals actually used are Moza ones. Excluded components are
  listed apart and can be re-enabled; this is the user's rig composition, unrelated to
  unsupported items (§3.5).
- Drivers of different brands are independent of each other (§6); presets apply each value to the
  components it targets (§7).

### 3.2 Standard interfaces

For each component type the core defines an **interface**: standard inputs, parameters and
actions with fixed semantics and units. The plugin implements it, and **every item is optional**:
anything the hardware or driver cannot do is reported as `Unsupported` **with a reason**. The UI
never hides it: it shows the control disabled, with the explanation next to it (§3.5).
The plugin only declares *whether* an item is supported and, for parameters, its actual range.

Normalized values: pedal axes `0.0–1.0`, angles in degrees, forces in % or kg depending on the
item. Every input also exposes its raw value. Inputs are read through evdev, like a game.

**`WheelBase`**

| Inputs | Standard parameters | Actions |
|---|---|---|
| `angle` (°) | `rotation` (°) | `set_center` |
| `angular_velocity` (°/s) | `ffb_strength` (%) | `test_effect` (evdev FFB API, like a game) |
| `status` (FFB active, thermal protection) | `natural_damper`, `natural_friction`, `natural_inertia` (%) | `stop_force` |
| `firmware` | `spring_strength`, `damper_strength` (%) | `reset` |

**`SteeringWheel`**

| Inputs | Standard parameters | Actions |
|---|---|---|
| `buttons`, `encoders` | `analog_paddle_mode` (choice) | `led_test` |
| `shift_paddles`, `analog_paddles` | `vibration` (%) | `calibrate_paddles` |
| `rim_id` | `led_brightness` (%) | |
| | outputs: `rpm_leds`, `flag_leds`, `display` | |

**`Pedals`** (each pedal: throttle, brake, clutch)

| Inputs | Standard parameters | Actions |
|---|---|---|
| `position` (raw + calibrated) | `calibration_min`, `calibration_max` (raw), `deadzone_low`, `deadzone_high` (%) | `calibrate` (guided) |
| `force` (load cell, kg) | `curve` (points), `inverted` | `vibration_test` |
| | `max_force` (load-cell full scale, kg), `vibration` (%) | |

Pedal item ids are prefixed with the pedal: `brake.position`, `throttle.curve`.

**`Shifter`**

| Inputs | Standard parameters | Actions |
|---|---|---|
| `gear` (`R`, `N`, `1..n`) — H | `gears` (forward gear count), `reverse_position` (choice) | `calibrate_gears` |
| `position_xy` (`-1..1`) — analog | `mode` (H / sequential, if the model has both) | |
| `sequential` (up, down) — sequential | `engage_threshold` (%, analog) | |

**`Handbrake`**

| Inputs | Standard parameters | Actions |
|---|---|---|
| `position` (raw + calibrated) | `calibration_min`, `calibration_max`, `curve`, `deadzone` | `calibrate` |
| `pressed` — button mode | `mode` (analog / button), `button_threshold` | |

The authoritative list (keys, labels, value kinds, units) is `simrig-model/src/component.rs`.

Safety rules: if a plugin supports `test_effect` it must support `stop_force` (checked by the
host's conformance checks); force parameters declare a `force_limit` that the SDK enforces; actions
that move the wheel declare a confirmation text the app must show first.

The app never processes inputs at runtime: a curve or dead zone the device does not support must be
offered by a driver (e.g. an HID-BPF program or a driver service), otherwise it stays `Unsupported`.

### 3.3 Interfaces in the SDK

In the Rust SDK each interface is a trait whose methods all default to `Unsupported` with reason
`NotInDriver`: a plugin implements only what its hardware can do, and should override the items
its hardware physically lacks with `NotInHardware`, so the user gets the right explanation.

Inputs are typed methods; parameters and actions are addressed by per-type enums
(`WheelBaseParam::Rotation`), whose semantics and units are fixed by the core. The SDK validates
every value against the plugin's declared `ParamSpec` (type, range, force limit) before calling
`set_param`, so plugins only receive valid values.

```rust
pub enum Support<T> { Supported(T), Unsupported(Unsupported) }

pub struct Unsupported { pub reason: Reason, pub detail: Option<String> }

pub enum Reason {
    NotInHardware,                        // "Not available on this hardware."
    NotInDriver,                          // "Not supported by the driver (yet)."
    NeedsFirmware { min: String },        // "Requires firmware 2.0 or later."
    NeedsComponent { what: String },      // "Requires a rim with flag LEDs."
    DependsOn { item: String, value: Value }, // "Available when mode is sequential."
    Unavailable,                          // temporary: device busy, reconnecting…
}

/// A parameter as declared by the plugin: type and range, current value, persistence.
pub struct ParamSpec { pub ty: ParamType, pub value: Value, pub persistence: Persistence, pub force_limit: Option<f64> }

pub trait Component {
    fn id(&self) -> String;
    fn name(&self) -> String;
    fn model(&self) -> Option<String> { None }
    fn custom_params(&mut self) -> Vec<CustomParam> { Vec::new() }          // §3.4
    fn set_custom(&mut self, _id: &str, _value: &Value) -> Result<()> { Err(Error::unsupported()) }
}

pub trait WheelBase: Component {
    fn angle(&mut self) -> Support<Degrees> { Support::not_in_driver() }
    fn angular_velocity(&mut self) -> Support<f64> { Support::not_in_driver() }
    fn status(&mut self) -> Support<String> { Support::not_in_driver() }
    fn firmware(&mut self) -> Support<String> { Support::not_in_driver() }
    fn param(&mut self, _p: WheelBaseParam) -> Support<ParamSpec> { Support::not_in_driver() }
    fn set_param(&mut self, _p: WheelBaseParam, _v: &Value) -> Result<()> { Err(Error::unsupported()) }
    fn action(&mut self, _a: WheelBaseAction) -> Support<ActionSpec> { Support::not_in_driver() }
    fn run_action(&mut self, _a: WheelBaseAction, _args: &Value) -> Result<ActionOutcome> { Err(Error::unsupported()) }
}
// SteeringWheel, Pedals (methods take a PedalRole), Shifter, Handbrake follow the same shape.
```

Plugins written in other languages implement the same interfaces through the protocol (§5.3):
each standard item has a fixed id (`rotation`, `brake.position`) and "unsupported" is an
explicit answer carrying its reason.

### 3.4 Custom parameters

Everything a plugin offers beyond the standard interface is a **custom parameter**, declared at
runtime and fully self-described, so the app can render it without knowing anything about it:

```rust
pub struct CustomParam {
    pub id: String,                       // unique within the plugin, e.g. "drift_mode"
    pub label: String,                    // already translated by the plugin (its i18n files)
    pub help: Option<String>,
    pub ty: ParamType,                    // Bool | Int{min,max,step} | Float{..} | Choice{options} | Curve{..} | Text{..}
    pub unit: Option<String>,             // free label: "%", "Nm", "Hz"…
    pub value: Value,
    pub default: Value,
    pub group: Option<String>,            // UI section, e.g. "Force feedback filters"
    pub order: i32,
    pub persistence: Persistence,
    pub advanced: bool,                   // placed in the collapsible "Advanced" section
    pub enabled_if: Option<Condition>,    // item ("mode" or "custom.<id>") must equal a value (→ DependsOn)
    pub availability: Support<()>,        // a custom parameter can be unsupported too, with a reason
    pub force_limit: Option<f64>,         // mandatory if it controls force
}
```

- **Dynamic**: the list can change at any time (e.g. a new rim exposes new options); the plugin
  notifies `params/changed` and the UI rebuilds that section.
- **Rendering**: the GUI maps types to generic widgets (switch, slider, spinner, dropdown, curve
  editor), groups them by `group`, and shows them in a "<plugin name> settings" section next to
  the standard controls.
- **Isolation**: in presets and in the UI, custom parameters are always namespaced by plugin
  (`fanatec:drift_mode`); they are applied only when that plugin's device is present.
- When the same custom parameter appears in several plugins, it is a candidate to become a
  standard interface item in a future version of the core (versioned API, §5.3).

### 3.5 Unsupported items in the UI

Nothing is hidden: the user always sees the full interface of each component type and learns
what their hardware can and cannot do.

- **Disabled, not hidden**: unsupported inputs, parameters and actions are shown greyed out, in
  their usual place, with a short explanation inline and the full one in a tooltip.
- **Explanations**: the core provides the translated text for each `Reason`; the plugin may add a
  `detail` (e.g. "The CSL Pedals have no load cell; the Load Cell Kit adds one").
- **Different reasons look different**: `NotInHardware` is neutral information; `NotInDriver` and
  `NeedsFirmware` suggest an upgrade; `NeedsComponent` and `DependsOn` say what to do to enable
  it; `Unavailable` is shown as a temporary state.
- **Dependencies are live**: when the condition of a `DependsOn` item becomes true, the control
  enables itself.
- **Wizard**: calibration and test steps for unsupported items are shown as skipped, with the reason.
- **Custom parameters** follow the same rules (`availability`, `enabled_if`).

## 4. Catalog and images

The catalog is **data** provided by plugins, plus the core's generic entries:

```toml
# plugins/fanatec/catalog/csl-dd.toml
id         = "fanatec/csl-dd"
vendor     = "Fanatec"
name       = "CSL DD"
kind       = "wheel-base"
usb        = [{ vid = 0x0eb7, pid = 0x0020 }]
image      = "images/csl-dd.webp"
image_license = "CC-BY-4.0, illustration by <author>"
driver     = "hid-fanatec"                 # → the plugin's driver.toml, §6.1
provides   = ["wheel-base", "steering-wheel?", "pedals?"]   # ? = optional, the wizard asks

[overlay]                                  # anchor points to animate the rig view
rotation_center = [0.50, 0.46]
```

- **Images**: model image first, then the user's own (`~/.local/share/simrig/images/`), then the
  generic illustration for the type (`assets/generic/<kind>.svg`).
- **Anchors** (`overlay`): where to rotate the wheel, fill the pedals, light up the gear.
- **Licensing**: no manufacturer photos without permission; license declared per entry.
- The catalog also lists models that are not plugged in: they can be added to the rig (and their
  drivers installed) before connecting them.

## 5. Plugins

### 5.1 One plugin, two independent parts

A plugin is a brand's "compatibility package" and contains two things with different lifecycles:

| Part | What it is | Who decides how it is built | Runs |
|---|---|---|---|
| **Driver** | What makes the device work in the system | **the driver itself**: kernel module, HID-BPF, udev rules, background service, any combination | always, independently of the app |
| **Adapter** | Executable that talks to the driver and implements the standard component interfaces (§3.2), plus custom parameters, catalog, images, labels | follows the simrig protocol (§5.3) | only while the app is open |

The app **neither knows nor imposes** how a driver is built: it installs it through its contract
(§6.1) and talks to it through the adapter, like a game or the manufacturer's software would. If
simrig is uninstalled, drivers stay and keep working, unless the user chooses to remove them (§6.5).

### 5.2 Layout

```
/usr/lib/simrig/plugins/fanatec/          ← system plugin (package)
~/.local/share/simrig/plugins/<id>/       ← user plugin (adapter only: no driver)
 ├── plugin.toml
 ├── simrig-plugin-fanatec                adapter
 ├── catalog/*.toml   images/   i18n/{en,it}.ftl
 └── driver/                              the driver: free layout, decided by the driver
     ├── driver.toml                      contract (§6.1)
     └── ...                              DKMS sources, udev rules, services, scripts: driver's business
```

```toml
# plugin.toml
id      = "fanatec"
version = "0.3.0"
api     = 1                               # adapter protocol version
exec    = "simrig-plugin-fanatec"         # file name inside the plugin directory
driver  = "driver/driver.toml"            # optional: absent when the stock kernel is enough

[match]
usb = [{ vid = 0x0eb7 }]                  # pid optional; `virtual = true` for plugin-provided devices
```

### 5.3 Adapter protocol

- Separate process started by the app and stopped with it, **JSON-RPC 2.0 over stdio** (one
  message per line): crash isolation, any language, no Rust ABI to keep stable.
- Handshake with the API version; incompatible plugins are disabled with a clear message.

| Direction | Method | Purpose |
|---|---|---|
| app → plugin | `initialize` | API version, host, locale |
| app → plugin | `devices/discover` | virtual devices the plugin provides itself (`match.virtual`) |
| app → plugin | `device/attach` `device/detach` | a device appeared / went away; attach returns its component descriptions |
| app → plugin | `component/describe` | every standard item (supported with spec, or unsupported with reason) + custom parameters |
| app → plugin | `std/get` `std/set` | standard parameters by item id (`rotation`, `brake.curve`), validated by the SDK |
| app → plugin | `custom/set` | custom parameters (current values come with `component/describe`) |
| app → plugin | `action/run` | standard actions: calibration, centering, test effect, stop force |
| app → plugin | `output/write` | standard outputs: LEDs, display |
| app → plugin | `input/read` | all supported inputs once |
| app → plugin | `input/subscribe` `input/unsubscribe` → `input/state` | live inputs at a capped rate |
| plugin → app | `params/changed` | fresh description after a set or action (availability may have changed) |
| app → plugin | `shutdown` | the app is closing |
| plugin → app | `components/changed` | *(planned)* components of a device changed (rim swap) |
| app → plugin | `param/save` | *(planned, M1/M2)* persist a value, the way the driver decides (§6.3) |
| plugin → app | `privileged/request` | *(planned, M2)* run one of the driver's privileged operations (§6.1) via the helper |

Error codes: `-32001` unsupported (data: `Unsupported`), `-32002` invalid value, `-32003` not
found, `-32004` incompatible API, `-32005` device failure. Messages and codes are defined in
`simrig-protocol`.

- **Rust SDK** (`simrig-plugin-sdk`): the author implements the component interfaces (§3.3) and
  one entry point that returns the components of an attached device; the SDK handles protocol,
  handshake and the main loop.

```rust
pub trait Adapter {
    fn info(&self) -> PluginInfo;
    fn discover(&mut self) -> Vec<DeviceInfo> { Vec::new() }       // virtual devices
    fn attach(&mut self, device: &DeviceInfo) -> Result<Vec<AnyComponent>>;  // AnyComponent::WheelBase(Box<dyn WheelBase>), ...
    fn detach(&mut self, _device_id: &str) {}
}

fn main() -> ExitCode { simrig_plugin_sdk::run(MyAdapter::default()) }
```

*(Planned)* `Io` is the only access to hardware and driver (evdev, sysfs, hidraw, serial, driver sockets):
real in production, recorded or simulated in tests.

### 5.4 Trust

- Adapters: code running as the user, like any installed program. The GUI always shows the origin
  (official / third-party).
- **Drivers**: become system code, with the same trust as a distro package. The helper runs a
  driver's operations **only from plugins in system directories** (root-owned), never from home.

## 6. Drivers

### 6.1 Contract

A driver is free to be built however it wants. The app only requires it to declare, in
`driver.toml`, **how it is installed, checked and removed**. Optionally it also declares the
privileged operations its adapter may request.

```toml
# driver/driver.toml
id       = "hid-fanatec"
version  = "dd78ef4"
summary  = "hid-fanatecff kernel driver (DKMS) + udev rules"
license  = "GPL-2.0"

# Required hooks: executables in the driver directory, launched with argv (never through a shell)
install   = "hooks/install"      # root, via helper
uninstall = "hooks/uninstall"    # root, via helper
status    = "hooks/status"       # user: prints JSON {installed, active, version, needs_reboot, message}

# Prerequisites the app installs via PackageKit before `install` (abstract names, mapped per distro)
requires = ["dkms", "kernel-headers"]

# Optional privileged operations the adapter may invoke, with validated arguments
[[privileged]]
name = "save-setting"
exec = "hooks/save-setting"
args = { key = "^[a-z_]+$", value = "^[0-9]+$" }   # every argument validated by a regex
```

- Hooks are **any executable** (shell, Rust, Python…). The helper runs them with pre-validated
  arguments and a clean environment, and records what was installed in
  `/var/lib/simrig/installed.toml`.
- `status` must also recognize an equivalent driver already present (e.g. the distro's
  `hid-fanatecff-dkms` package, or an in-kernel module): in that case the driver counts as
  installed and the app does nothing.
- A driver may run **its own background processes** if it needs them: systemd units, services
  started by udev, whatever it prefers. That is the driver's choice: the app does not start them,
  supervise them or depend on them.

### 6.2 SDK recipes

So that drivers do not reinvent the same things, the SDK offers ready-made hooks a driver may use
(or ignore):

| Recipe | What it does |
|---|---|
| `dkms` | copies sources to `/usr/src`, `dkms add/build/install`, Secure Boot signing (DKMS MOK key), `modprobe`, blacklists conflicting modules |
| `udev` | installs udev rules (`uaccess` permissions, bound to `bind`) and runs `udevadm trigger` |
| `hid-bpf` | installs an HID-BPF program the kernel loads on plug-in (`udev-hid-bpf`) |
| `systemd-device-unit` | installs a service started when the device is plugged in and stopped when unplugged |
| `udev-attr-settings` | persists sysfs attributes via a generated udev rule (`ATTR{range}="900"`) |

Guideline for **official drivers** (not a constraint): use the lightest solution that works, in
the order stock kernel → HID-BPF → kernel module → service. Fewer processes, less to maintain,
works on distros without systemd.

### 6.3 Persistent settings

How a setting survives with the app closed is up to the driver. The adapter only declares, for
each parameter, whether it is persistent, and persists it when the app calls `param/save`:

```rust
pub enum Persistence {
    Persistent,   // the driver (or the device) remembers it with the app closed and across replugs
    Volatile,     // lasts until unplugged, or until a game changes it
}
```

Examples of how a driver may do it: device memory, a udev rule generated through a privileged
operation, a config file read by its own service, module options in `/etc/modprobe.d`.

### 6.4 Installation

It happens when a **device is added** (§9), only if `status` reports the driver missing:

```
wizard ──▶ hooks/status (user) → not installed
                 │
                 ▼
     PackageKit: prerequisites (dkms, kernel headers…)
                 │
                 ▼
     pkexec simrig-helper install fanatec
                 ├─ system plugin? valid driver.toml?
                 ├─ runs the driver's hooks/install (root, argv, clean environment)
                 ├─ copies the driver's uninstall hook (and what it needs) to /var/lib/simrig/drivers/<id>/
                 └─ records it in /var/lib/simrig/installed.toml
                 │
                 ▼
     hooks/status → installed / reboot needed (e.g. MOK enrollment for Secure Boot)
```

- **Generic, closed helper**: actions `install`, `uninstall`, `privileged` (operation name plus
  arguments validated against the regexes in `driver.toml`). No arbitrary paths or commands. Each
  action has its own polkit rule `org.simrig.helper.*`.
- **Upgrades**: when a plugin ships a new driver version, the app offers the upgrade when opened
  (old `uninstall` → new `install`, or an `upgrade` hook if the driver declares one). Kernel
  upgrades are handled by DKMS (or whatever the driver chose).
- **Immutable distros and Flatpak**: instructions only in v1.

### 6.5 Uninstallation

Drivers are independent of the app, so removing the app **keeps them by default**: devices keep
working. Removing them too is an explicit, optional choice.

**Prerequisite**: at install time the helper copies each driver's `uninstall` hook into
`/var/lib/simrig/drivers/<id>/`, together with the registry `/var/lib/simrig/installed.toml`.
Drivers can therefore be removed even after the app and its plugins are gone.

| How the user uninstalls | What happens to the drivers |
|---|---|
| **From the app**: Settings → "Uninstall simrig" (recommended) | A dialog lists the installed drivers with a checkbox each ("also remove"). The helper runs the selected `uninstall` hooks, then the app removes its own packages via PackageKit. A single password prompt. |
| **From the app**: removing a single plugin | Same dialog, limited to that plugin's driver. |
| **Package manager, plain remove** (`apt remove`, `dnf remove`, `pacman -R`, GUI stores) | Package managers cannot ask questions, so drivers are **kept**. A notice in the package's removal output points to the command below. |
| **Package manager, purge** (`apt purge`) | Purge means "remove everything": the package's removal script also runs every recorded `uninstall` hook. |
| **After the app is gone** | `sudo /var/lib/simrig/remove-drivers [--all \| <id>…]`: a small standalone script installed with the first driver, it runs the recorded hooks and deletes `/var/lib/simrig` when nothing is left. |

Rules:
- `uninstall` hooks must work on partial installs and be idempotent.
- A driver detected as **provided by the system** (in-kernel or a distro package, §6.1) was never
  installed by simrig and is never removed by it.
- After removal, the registry and `/var/lib/simrig/` disappear: nothing from simrig is left on
  the system.

## 7. Presets

- A **preset** is a named set of values ("GT3", "Drift", "Rally"), stored in
  `~/.config/simrig/presets/`.
- The user applies a preset from the app (or `simrig-cli apply <preset>`). Applying it sets the
  values through the adapters and **saves them into the drivers as persistent settings** (§6.3):
  from then on they are active with the app closed, until another preset is applied.
- Nothing needs to be configured outside the app: no game launch options, no scripts.
- **No automatic per-game switching in the app**: it would require a background process. A
  driver that wants to offer it may do so on its own (e.g. with its own service); the app is not
  involved.
- Standard parameters are grouped by component type and apply to every component of that type in
  the rig (usually one), whatever its brand. When the rig has several components of a type, a
  value can target one of them by address. Custom parameters are stored per plugin. Anything a
  device does not support is ignored and reported.

```toml
# ~/.config/simrig/presets/gt3.toml
name = "GT3"

[wheel_base]                                # any brand
rotation     = 540
ffb_strength = 70

[pedals]
"brake.curve" = [[0, 0], [0.5, 0.35], [1, 1]]

[component."moza/usb-1-4/pedals"]           # only this pedal set, when there are several
"brake.max_force" = 60

[custom.fanatec]                            # applied only when a Fanatec device is present
ffb_scaling = 1
```

## 8. Simulated rig and tests

- The **`sim`** plugin (M0, done) implements every interface with fake components: base, rim with
  RPM LEDs and display, 3-pedal set with a brake load cell, H/sequential shifter, handbrake with
  analog/button mode. Inputs move by themselves (waves, optional noise), parameters live in
  memory, and items are deliberately unsupported with every kind of reason, including live
  `DependsOn` (shifter mode, handbrake mode, a custom parameter depending on another).
- **Conformance checks** (`simrig-host::conformance`, M0, done): every standard item present once
  with the right class and value kind, valid current values, known `DependsOn` targets, unique and
  valid custom parameters, `test_effect` ⇒ `stop_force`. The host runs them on every attach.
- **End-to-end tests** (M0, done): `plugins/sim/tests` start the real plugin process through the
  host, as the app does.
- *(Planned)* a fake driver with `install`/`status`/`uninstall` hooks that do not touch the system,
  to exercise the wizard and the installation flow (M1).
- *(Planned)* `--uinput` option: exposes the simulated rig as real evdev devices.
- *(Planned)* **record and replay**: `simrig-cli record` captures interactions with a real device;
  `Io` replays them in tests.

## 9. Wizard

Two forms: **first-time setup** (whole rig) and **add device** (a single one). Steps are composed
from the components:

1. **System**: distro, kernel, Secure Boot, installed plugins.
2. **Scan**: recognized devices (with image) and unknown ones; add from the catalog for things not
   plugged in yet.
3. **Your rig**: component assignment, confirmation of ambiguous connections.
4. **Drivers**: what will be installed (version, origin) → a single password prompt → reboot if
   needed for Secure Boot (the wizard resumes from there the next time the app opens).
5. **Calibration**: one step per component, generic or provided by the plugin.
6. **Tuning**: rotation, strength, presets. Each shows whether it is persistent.
7. **Test**: live rig view and FFB test effect (with a "hands on the wheel" confirmation).
8. **Save**: persists settings through the drivers and stores them as the first preset.

## 10. Repository layout

```
simrig/
├── Cargo.toml                      workspace
├── crates/
│   ├── simrig-model/               domain types, standard component interfaces, custom parameters (no I/O)
│   ├── simrig-protocol/            adapter ↔ app messages and JSON-RPC framing, versioned
│   ├── simrig-plugin-sdk/          adapter SDK: interfaces, validation, protocol server
│   ├── simrig-host/                plugin discovery and processes, rig, USB scan, conformance (used by GUI and CLI)
│   ├── simrig-cli/                 `simrig-cli`
│   ├── simrig-gui/                 (M1) Slint
│   ├── simrig-driver-recipes/      (M2) ready-made driver hooks (dkms, udev, hid-bpf, systemd…)
│   └── simrig-helper/              (M2) runs driver hooks + polkit policy + installation registry
├── plugins/                        official plugins: same repo and release, separate packages
│   ├── sim/                        simulated rig + end-to-end tests
│   └── fanatec/                    (M2) adapter + driver/ (hid-fanatecff submodule + hooks)
├── assets/generic/                 (M1)
├── packaging/                      (M5)
└── docs/
```

## 11. Example: adding Fanatec

- **New**: `plugins/fanatec/` (adapter, catalog, images, labels) and `plugins/fanatec/driver/`
  (`hid-fanatecff` as a pinned submodule, hooks built on the `dkms`, `udev` and
  `udev-attr-settings` recipes).
- **New**: `simrig-plugin-fanatec` package.
- **Core**: nothing. Brand features that are not in the standard interfaces become custom parameters.

Mapping of `hid-fanatecff` (`dd78ef4`) sysfs attributes:

| `hid-fanatecff` sysfs | simrig | Kind | How the Fanatec driver persists it |
|---|---|---|---|
| `range` | `wheel_base.rotation` | standard | generated udev rule (`udev-attr-settings`) |
| `tuning_menu/FF` | `wheel_base.ffb_strength` | standard | base memory (tuning slot) |
| `tuning_menu/NDP`, `NFR` | `wheel_base.natural_damper`, `natural_friction` | standard | base memory |
| `tuning_menu/SPR`, `DPR` | `wheel_base.spring_strength`, `damper_strength` | standard | base memory |
| `tuning_menu/SHO` | `steering_wheel.vibration` | standard | base memory |
| `tuning_menu/ACP` | `steering_wheel.analog_paddle_mode` | standard | base memory |
| `tuning_menu/FOR` | `fanatec:force_effect_strength` | custom | base memory |
| `tuning_menu/SEN` | `fanatec:sensitivity` | custom | base memory — overlaps with `range`, to clarify |
| `tuning_menu/FEI` | `fanatec:ffb_effect_intensity` | custom | base memory |
| `tuning_menu/FFS` | `fanatec:ffb_scaling` | custom | base memory |
| `tuning_menu/DRI` | `fanatec:drift_mode` | custom | base memory |
| `tuning_menu/SLOT` | `fanatec:tuning_slot` | custom | base memory |
| `tuning_menu/BLI` | `fanatec:brake_level_indicator` | custom | base memory |
| `tuning_menu/RESET` | `wheel_base.reset` | standard action | — |
| `wheel_id` | `steering_wheel.rim_id` + `components/changed` | standard input | — |
| `display` | `steering_wheel.display` | standard output | — |
| `fw_version` | `wheel_base.firmware` | standard input | — |
| `rumble`, `advanced_mode` | to be verified | — | — |

Unsupported with this driver, shown disabled with their reason: e.g. `wheel_base.natural_inertia`
(`NotInDriver`), `pedals.force` on pedals without a load cell (`NotInHardware`, detail: "add the
Load Cell Kit"), `steering_wheel.display` on rims without a display (`NotInHardware`).

Whether the tuning menu really persists in the base's memory must be **verified on hardware**.

## 12. Decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | Rust for app, helper, adapters and recipes; drivers in whatever language they need (C for kernel) | memory safety in userspace; freedom for drivers |
| D2 | The app is a client of the drivers, never in the data path | no added latency; drivers work without the app |
| D3 | No simrig process with the app closed | requirement; persistence and automation are the drivers' job |
| D4 | Independent drivers, free to have their own processes; the app knows them only through the install / status / uninstall / privileged-operations contract | each driver picks the best solution for its device |
| D5 | Plugin = driver + adapter; out-of-process adapter via JSON-RPC over stdio | isolation, any language, separate lifecycles |
| D6 | Generic helper that runs only hooks declared by system plugins, with validated arguments, authorized via polkit | a single, small, auditable privileged component |
| D7 | Ready-made recipes in the SDK (dkms, udev, hid-bpf, systemd) | drivers don't reinvent the wheel; lightness guideline for official ones |
| D8 | Presets applied manually and saved into the drivers; no per-game switching in the app | no app background process, nothing to configure outside the app |
| D9 | Slint for the GUI | native Rust, declarative UI with live preview, same look on every desktop, Wayland and X11; GPL-3.0 licensing compatible with the app |
| D10 | Code and documentation in English | open-source community project |
| D11 | Standard per-type interfaces in the core, every item optional; brand extras only as self-described custom parameters | presets portable across brands; zero brand knowledge in the core |
| D12 | Drivers kept by default on app removal; removal optional via the app's uninstall dialog, purge, or a standalone script | devices keep working unless the user explicitly asks otherwise |
| D13 | Unsupported items shown disabled with a reason, never hidden | the user understands what the hardware can do and how to enable missing features |
| D14 | Any mix of brands; each device belongs to exactly one plugin (most specific match); components addressed by plugin, never by type alone | no constraint on how users build their rig |

## 13. Risks

| Risk | Mitigation |
|---|---|
| Undocumented protocols (Simagic) | recordings from volunteers; "experimental" plugins |
| No hardware available for testing | record and replay; per-brand beta testers |
| Secure Boot and MOK enrollment confuse users | dedicated wizard step that resumes after reboot |
| Modules conflicting with in-kernel ones | handled by the `dkms` recipe (blacklist), declared by the driver |
| Drivers with their own services on non-systemd distros | `status` reports it; official drivers prefer kernel solutions |
| Curves and dead zones without hardware support | drivers' job (e.g. HID-BPF); for official drivers after v1 |
| Driver hooks failing mid-install | `status` after each hook; installation registry; `uninstall` must handle partial installs |
| Upstream changing sysfs attributes | driver version pinned in the plugin; tests on recordings |
| Leftover system files | `/var/lib/simrig/installed.toml` registry; uninstall dialog, `apt purge`, standalone `remove-drivers` (§6.5) |
| User removes the app via the package manager and expects drivers gone | removal notice pointing to `remove-drivers`; documented in the FAQ |
| Direct-drive bases: physical risk | limits in descriptors, ramped tests, stop-force always visible |
