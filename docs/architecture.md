# simrig — Architecture

> Draft 0.10 (2026-10-17). Core (M0) and app (M1) implemented; generic USB plugin (M2) and Fanatec
> plugin (M3) implemented, to be verified on more hardware. Companion to [requirements.md](requirements.md).

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
7. **No privileges at runtime.** The app runs as the user and needs no privileges to read
   inputs, change settings (rotation, force feedback…), run force-feedback tests or apply
   presets live: it uses the driver's interfaces like a game. It asks for authorization only when
   something touches the system: installing or removing a driver, and saving a setting when the
   driver persists it in a system file. Drivers must therefore grant the user access to their
   devices at install (e.g. udev `uaccess` rules bound to `bind`).

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

### 2.1 What the GUI builds on

`simrig_host::Host` is the whole app core; the GUI only calls it:

| Area | API |
|---|---|
| Rig | `start`, `components`, `find`, `describe`, `plugins`, `problems` |
| Changes | `poll()` → `HostEvent`: plugin started/stopped/failed, device added/removed, components changed (rim swap), description changed, live inputs |
| Settings | `set`, `param`, `save`, `run_action`, `write_output`, `read_inputs`, `subscribe` |
| Composition | `set_excluded`, `choose_plugin`, `set_planned`, `usb_devices`, `devices`, `rig_config` |
| Procedures | `target`, `procedures`, `start_procedure`, `submit_procedure`, `cancel_procedure`; updates arrive as `HostEvent::ProcedureUpdated` |
| Catalog | `catalog`, `image` (user picture → plugin picture → core drawing with the rim shape) |
| Drivers | `driver_status`, `install_driver`, `uninstall_driver`, `installed_drivers`, `restart_plugin` |
| Presets | `presets()` (list, get, save, delete, export, import), `capture_preset`, `apply_preset` |
| Wizard | `system()`: distribution, kernel, Secure Boot, session, systemd, pkexec |

`simrig-cli` exposes the same functions from the terminal.

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

The catalog is **data** provided by plugins:

```toml
# plugins/fanatec/catalog/csl-dd.toml
id         = "fanatec/csl-dd"
vendor     = "Fanatec"
name       = "CSL DD"
kind       = "wheel_base"
usb        = [{ vid = 0x0eb7, pid = 0x0020 }]
image      = "images/csl-dd"                # optional: the plugin's own picture (file or layer folder)
image_license = "CC-BY-4.0, illustration by <author>"
provides   = ["wheel_base", "steering_wheel?", "pedals?"]   # ? = optional, the wizard asks
```

### 4.1 Device pictures

**The core ships one generic drawing per component type** (`assets/devices`): invented but
realistic devices with no brand, drawn as layered SVG so the moving parts can be animated from
live inputs, also in Slint, which cannot animate inside a single SVG:

| Type | Drawing | Live animation |
|---|---|---|
| Wheel base | three-quarter view, static | marker on the hub ellipse (when no rim is mounted) |
| Steering wheel | front view, drawn on its own; **round, GT (GT3-style, cut top and bottom) or formula** | turns around its centre with the steering angle |
| Pedals | frame + one pedal drawing per pedal, from the driver's seat | the pedals stay still: their travel is shown by the live bars next to the drawing; only the pedals the component declares are drawn (2-pedal sets have no clutch) |
| Shifter | top view of the plate + knob | the app draws the gate of the current configuration (6+R, 7+R, reverse position, sequential) and puts the knob on the engaged slot |
| Handbrake | side view, body + lever | the lever turns on its pivot |

`assets/devices/devices.toml` is the contract between the drawings and the renderer: view boxes,
layers, pivots, hinge points, gate geometry and the rules to draw a gate. `docs/design/devices.js`
implements the same rules for the mockups.

**Plugins give hints, the core picks the drawing.** A component's description carries an
`appearance`; today it has the rim shape (`round`, `gt`, `formula`), so a plugin gets the right
wheel without shipping any image.

**Plugins may ship their own pictures** for their models (`image` in the catalog): a single
image (static; a rim still turns as a whole) or a folder with the same layers as the core's set
and its own `devices.toml`, animated with the same rules.

Resolution (`Host::image`): the user's own picture
(`~/.local/share/simrig/images/<vendor>_<model>.{svg,webp,png,jpg}`), then the plugin's picture of
the model, then the core's generic drawing with the plugin's appearance hints.

- **Licensing**: no manufacturer photos without permission; license declared per entry. The
  core's drawings are original work under the project license.
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
usb = [{ vid = 0x0eb7 }]                  # pid optional
# virtual = true                          # the plugin provides devices itself (simulated rig)
# on_request = true                       # generic plugin: only USB devices the user assigns to it
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
| plugin → app | `components/changed` | components of a device changed (rim swap), with their descriptions |
| plugin → app | `devices/changed` | the plugin's virtual devices changed |
| app → plugin | `procedures/list` | guided procedures offered on a device (component ones come with `component/describe`) |
| app → plugin | `procedure/start` `procedure/submit` `procedure/cancel` | run a guided procedure step by step (§5.5) |
| plugin → app | `procedure/update` | the procedure moved on by itself (live value, detected input…) |
| app → plugin | `param/save` | make the current value of a persistent item the one restored at plug-in, the way the driver decides (§6.3) |
| plugin → app | `privileged/request` | during a request, run one of the driver's privileged operations (§6.1); the host runs the helper and answers |

Error codes: `-32001` unsupported (data: `Unsupported`), `-32002` invalid value, `-32003` not
found, `-32004` incompatible API, `-32005` device failure, `-32006` not authorized. Messages and codes are defined in
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
    fn take_changes(&mut self) -> Vec<Change> { Vec::new() }       // e.g. a rim swap
    fn device_procedures(&mut self, _device_id: &str) -> Vec<ProcedureDescriptor> { Vec::new() }
    fn start_device_procedure(&mut self, _device_id: &str, _id: &str) -> Result<Box<dyn Procedure>> { … }
}

// On every component: saving goes through the host when the driver needs privileges.
fn save(&mut self, item: &str, value: &Value, host: &mut HostLink<'_>) -> Result<()> {
    host.privileged("save-setting", &[("key", item), ("value", &value.to_text())]).map(|_| ())
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

### 5.5 Guided procedures

Calibrations, input mappings, the first setup of a generic device… are **defined by plugins**.
The core only provides a generic engine: it knows nothing about what a procedure does, and a
plugin can add any procedure without touching the core.

- A plugin declares procedures on a **component** (e.g. "Calibrate the brake") or on a **device**
  (e.g. "Set up this device", for a device that has no components until it is configured). Each
  has a label, help and an availability with a reason, like any other item.
- **The plugin drives, the app renders.** Every step is made of generic elements:

  | Element | Use |
  |---|---|
  | `Field` | something the user fills, typed like a parameter (`ParamType`) |
  | `Value` | a read-only value, live when the plugin sends updates (e.g. raw position) |
  | `Table` | e.g. the summary of a mapping |
  | `Notice` | info, success, warning, error |
  | actions | buttons with a role (primary, secondary, cancel) and an optional confirmation |

  A step can be **waiting**: the plugin moves on by itself (e.g. "move the lever to 3rd gear",
  then it detects the input) and notifies `procedure/update`.
  A step can carry a **preview**: a component type with standard values (`gears`,
  `reverse_position`, `mode`, `gear`, `position`) and a `target` (`3`, `R`, `+`). The app draws it
  with the core's drawing of that type, the target marked (the gear to engage on the gate), so
  a device with no components yet still shows what is asked.
- The SDK keeps the sessions, validates the fields of the current step before the plugin sees
  them, and applies the values a finished procedure returns to its component (`apply`), validated
  like any change. During a step the plugin may run privileged operations of its driver (e.g. to
  save a mapping) and change its components (a generic device becomes a shifter).
- Sessions of devices or components that go away are cancelled.

```rust
pub trait Procedure {
    fn start(&mut self, host: &mut HostLink<'_>) -> Result<Outcome>;
    fn submit(&mut self, action: &str, values: &BTreeMap<String, Value>, host: &mut HostLink<'_>) -> Result<Outcome>;
    fn poll(&mut self) -> Option<Outcome> { None }   // move on by itself
    fn cancel(&mut self) {}
}
pub enum Outcome { Step(Step), Finished { message: Option<String>, apply: Vec<(String, Value)> }, Cancelled }
```

### 5.6 Generic plugins

A plugin with `match.on_request = true` never gets devices on its own: only the USB devices the
user assigns to it (`choose_plugin`, applied at once). This is how a plugin for unbranded devices
works: the user picks any connected device, then the plugin's own setup procedure asks what it is
and maps its inputs. Hubs are never offered.

**The official generic USB plugin** (`plugins/generic-usb`, M2) makes any USB HID device a
shifter (H-pattern or sequential) or a handbrake, whatever its HID descriptor says:

| Part | What it does |
|---|---|
| Setup procedure (adapter) | asks what the device becomes, learns its rest position (bits that move by themselves are noise and never count), then detects each gear, or the handbrake's travel, while the user moves it: a position is taken once it holds still. A gear is a *signature* (bytes and bits that differ from rest); a handbrake is a little-endian 8/16-bit axis, or a switch. |
| Setup file | one TOML per device in the driver's data directory (`devices/<vid>-<pid>[-serial].toml`), written by the privileged `save-device` operation; read by both the adapter and the service, so the app and the games always agree. |
| Driver | a small service (`simrig-generic-usb-service`), started by udev with each set-up device (`systemd` template unit `simrig-generic-usb@hidrawN`) and stopped when it is unplugged. It reads `hidraw` and drives a virtual joystick through `uinput`: one button per gear (and reverse), up/down, or the handbrake as an axis (`ABS_RX`) plus a button. The raw device is hidden from games (udev clears `ID_INPUT_JOYSTICK`), so they see only the joystick. Devices the kernel's HID driver rejects (a broken report descriptor: no `hidraw`) are read straight from their USB interrupt endpoint instead (`nusb`, pure Rust), by a second template unit `simrig-generic-usb-raw@<usb device>`; since reading claims the interface for one reader, the service relays the reports to the app (`/run/simrig-generic-usb/<device>.report`), which reads the device itself only while setting it up. |
| udev rules | generated from the setups: read access (`uaccess`) to the device's `hidraw`, or its USB device when it has no `hidraw`, while it is being set up and once set up, the service, and the raw device hidden. |
| Components | the adapter decodes the same raw reports with the same setup, so the gear and the handbrake move live in the app; calibration, dead zone, curve and mode change at once in the app and reach games when saved. |
| Forget | a device procedure (and a component one) that deletes the setup through the privileged `forget-device` operation: the service stops serving it and its components go away. |

The driver needs systemd to start the service with the device; without it, `status` says so.
Two identical devices (same vendor and product) without a serial number share one setup. In
development (no root) the driver keeps setups in its data directory but installs no rules or
service. The adapter reads a fake device in tests: a USB directory outside `/sys` holding a
`simrig-fake-report` file.

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
status    = "hooks/status"       # user: prints JSON, see below

# Prerequisites the app installs via PackageKit before `install` (abstract names, mapped per distro)
requires = ["dkms", "kernel-headers"]

# Optional privileged operations the adapter may invoke, with validated arguments
[[privileged]]
name = "save-setting"
exec = "hooks/save-setting"
args = { key = "[a-z_]+", value = "[0-9]+" }   # each value must fully match its pattern
```

`status` prints `{"installed", "active", "version", "needs_reboot", "provided_by_system",
"user_access", "message"}`. It runs as the user, so `user_access` says whether the user can really
use the device without privileges; the wizard flags it when it is false.

Hooks run with a clean environment: `SIMRIG_PLUGIN`, `SIMRIG_DRIVER_DIR`, `SIMRIG_DRIVER_DATA`
(the driver's own data directory, also given to the adapter) and one `SIMRIG_ARG_<NAME>` per
argument of a privileged operation. The contract, hooks and registry are implemented in
`simrig-driver`.

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

Recipes are POSIX shell libraries in `crates/simrig-driver/recipes` (`dkms.sh`, `module.sh`,
`secureboot.sh`, `udev.sh`, `prereq.sh`, `status.sh`). A driver keeps copies of the ones it uses
in its own `recipes/` directory and sources them from its hooks, so it keeps working, and can be
removed, after the app is gone; a test checks that the copies match the originals. The recipes
run for real in tests against stub system commands (`simrig_driver::testing`, which drivers'
own tests use too).

`module.sh` is what every kernel-module driver shares: where a module comes from (SimRig's
version, an older one of SimRig's, one installed by other means, the kernel's, none), SimRig's
DKMS build of the bundled sources with upgrade, reload and Secure Boot key enrollment, removal,
making devices already plugged in start again as if replugged (USB re-enumeration: devices
do what they do at start-up, like a Thrustmaster wheel's switch to its own id), and checking
which USB devices are present. `dkms.sh`
takes upstream packages as they are: their fixed name and version are rewritten to SimRig's,
and their `dkms.conf` may live in a subdirectory.

On the adapter side, `crates/simrig-plugin-kit` holds what plugins of kernel-driven wheels and
pedals share: the HID device a driver is bound to and its sysfs attributes, the event node
(axes, buttons, force feedback test), and pedals: a standard pedals component over evdev axes.

**Pedals are recognised, not configured.** Like Oversteer, each plugin knows where the pedals of
its wheels are: a table of axes and directions per model (`Layout`), from Oversteer's and
Boxflat's tables and checked on hardware where possible. Pedals with a known table show up as
soon as the device does. Only pedals no table covers need mapping: the **Map the pedals**
procedure is then marked as part of the device's setup (`ProcedureDescriptor::setup`), so the
app runs it while adding the device, not afterwards. Mapping by hand stays available for pedals
that respond wrong, and wins over the table. It takes each pedal's direction from where it is
pressed all the way to, not from a reading at rest (a device may report an axis only once it
moves). Pedals that are not plugged in, or not used (a separate pedal set), are excluded from
the rig.

**Third-party drivers ship with SimRig.** A plugin that builds an existing open-source driver
bundles its sources as a git submodule pinned to an upstream commit (the Fanatec plugin bundles
`hid-fanatecff`), so installing works offline and always installs the version tested with the
adapter. Driver updates come with SimRig releases (the app offers the upgrade); SimRig never
fetches newer upstream versions by itself. A driver the user installed by other means (distro
package, by hand, a newer version) is reported as provided by the system and never touched; the
adapter checks what the installed driver actually exposes and shows anything missing as
unsupported, with the reason.

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

- **Generic, closed helper** (`simrig-helper`): actions `install`, `uninstall`, `privileged`
  (operation name plus arguments validated against `driver.toml`) and `list`. It takes no paths or
  commands from its caller and finds plugins only in system directories when run as root.
- **Authorization**: for system plugins the host runs `pkexec simrig-helper …`; pkexec authorizes
  per executable, so one polkit action covers the helper (`packaging/polkit/org.simrig.helper.policy`,
  `auth_admin_keep`: a password once for a while, not at every change). Plugins in explicit
  development directories run the helper as the user with a private state directory
  (`~/.local/state/simrig/dev`), so the whole flow works without privileges; plugins in the user's
  home may not install drivers.
- **Prerequisites** (`requires`) through PackageKit: not implemented yet (M3); the app asks the
  user to install them.
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

- The **`sim`** plugin implements every interface with fake components on separate virtual
  devices: base with rim (`rig0`), pedals (`pedals0`), shifter (`shifter0`), handbrake (`hb0`).
  Inputs move by themselves (waves, optional noise), and items are deliberately unsupported with
  every kind of reason, including live `DependsOn`.
- It simulates what hardware does to the app: **rim swap** (custom parameter `rim`: formula, GT,
  none; the two rims support different items), **device unplugged** (`handbrake_connected`),
  and a **fake driver** (`plugins/sim/driver`) with install, status, uninstall and a
  `save-setting` privileged operation that only touches its data directory: saved settings are
  restored by the next session.
- **Procedures**: a guided calibration on each pedal (the simulated pedal follows the
  instructions), and an unconfigured device (`generic0`) whose setup procedure turns it into a
  shifter or a handbrake, detecting each position live and saving the setup through the fake
  driver.
- `--id`, `--devices`, `--usb-pedals` and `--usb-generic` options let tests run it as a second
  "brand" or as a generic plugin handling USB devices from a fake sysfs tree.
- **Conformance checks** (`simrig-host::conformance`): every standard item present once with the
  right class and value kind, valid current values, known `DependsOn` targets, unique and valid
  custom parameters, `test_effect` ⇒ `stop_force`. The host runs them on every attach.
- **End-to-end tests** (`plugins/sim/tests`) start real plugin processes and the real helper
  through the host, with temporary configuration, state and sysfs directories: parameters,
  reasons, dependencies, actions, live inputs, rim swap, virtual and USB hotplug, mixed brands,
  driver install/save/uninstall without leftovers, exclusions, presets.
- *(Planned, M2)* `--uinput` option, and **record and replay** of real devices through an `Io`
  layer.

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
│   ├── simrig-plugin-sdk/          adapter SDK: interfaces, validation, protocol server, host link
│   ├── simrig-driver/              driver contract, hooks, installation registry, removal scripts
│   ├── simrig-helper/              privileged helper (run through pkexec)
│   ├── simrig-host/                the app core: plugins, rig, events, drivers, presets, catalog, system
│   ├── simrig-cli/                 `simrig-cli`
│   └── simrig-gui/                 (M1) Slint
├── plugins/                        official plugins: same repo and release, separate packages
│   ├── sim/                        simulated rig, fake driver, catalog, end-to-end tests
│   ├── generic-usb/                (M2) setup and mapping procedures + driver service translating raw inputs
│   └── fanatec/                    (M3) adapter + driver/ (hid-fanatecff submodule + hooks)
├── packaging/polkit/               polkit action of the helper
└── docs/
```

Driver recipes (dkms, udev, hid-bpf, systemd) come with the first real driver (M2).

## 11. Example: adding Fanatec

- **New**: `plugins/fanatec/` (adapter, catalog, images, labels) and `plugins/fanatec/driver/`.
- **Core**: nothing. Brand features that are not in the standard interfaces become custom parameters.

**The driver** (implemented):

| Piece | What it does |
|---|---|
| `hid-fanatecff/` | upstream sources, a git submodule pinned to a tested commit; its DKMS version in `hid-fanatecff.version` |
| `hooks/install` | refuses nothing it does not own: with a `hid-fanatec` installed by other means (another DKMS version, a package, a module built by hand) it does nothing. Otherwise checks the prerequisites (dkms, make, gcc, kernel headers; names the packages per distribution when missing), removes an older version SimRig installed, builds and installs the module with the `dkms` recipe for every kernel with headers, installs SimRig's udev rules and loads the module. Devices already plugged in start again as if replugged. When Secure Boot refuses the module, asks the firmware to enroll DKMS's signing key with a one-time password the user types at the next boot |
| `hooks/status` | ours (version, loaded, waiting for the key to be enrolled, an update available), someone else's (*provided by the system*, with its version), or not installed |
| `hooks/uninstall` | removes only SimRig's DKMS version and udev rules |
| `udev/71-simrig-fanatec.rules` | raw HID with `uaccess` (Fanatec SDK games under Proton), and the driver's settings (rotation, display, tuning menu, LEDs) writable by local users, re-applied at every `bind`: no password for live settings. A test checks that every tuning attribute of the bundled driver is covered |

A `hid-fanatec` installed by other means is kept as it is: `install` then adds only SimRig's
udev rules (and the saved rotation), and `uninstall` removes only those. Without root
(development plugins) nothing is built. The hooks are tested for real against stub system
commands and a fake `/sys`.

**The adapter** (implemented) talks to the driver like a game: it finds the HID device the
`fanatec` driver is bound to (a base has several; the one with its settings and inputs), reads
and writes its sysfs attributes, and reads its event node on request (axes and buttons) and
uploads force feedback effects for the test. Components:

| Device | Components |
|---|---|
| A base | wheel base (angle from the steering axis and the rotation, firmware, rotation, the tuning menu, force feedback test, reset); steering wheel when a rim is mounted (name, rim shape from `wheel_id`, buttons, display, paddle mode, vibration), swapped live; pedals plugged into the base, recognised (throttle ABS_Z, brake ABS_RZ, clutch ABS_Y, released at the top: checked on a CSL DD) |
| A USB pedal set | pedals, once mapped (no known table yet: mapped while the device is added) |

Which axis is which pedal is not in the driver: the plugin knows it for pedals on a base, and
USB pedal sets are mapped while they are added, by pressing each pedal; a mapping made by hand
is kept in the user's configuration (`fanatec.toml`). The
tuning menu lives in the base's memory, and the base sends it to the driver only when it is used
on the wheel: hid-fanatecff cannot ask for it ([#75](https://github.com/gotzl/hid-fanatecff/discussions/75)),
and refuses to change it, the rotation included, until then. The adapter gets it by itself
(`plugins/fanatec/src/unlock.rs`): switching the menu between standard and advanced mode is a
change the driver lets through, and the base answers it with the whole menu; the adapter then
switches it back (waiting for the base, which ignores a second switch sent right away) and
selects again the slot the user was on, which it remembers (`fanatec.slots`), since switching
modes goes back to slot 1. Checked on a CSL DD. Meanwhile those settings show as unavailable,
saying what is going on (and, if the base does not answer, to open the tuning menu on the wheel
and change a value), and they show up as soon as the menu arrives; the rotation is saved as the driver's `init_range`
option (`save-range`, a privileged operation writing `/etc/modprobe.d/simrig-fanatec.conf`).
What the driver does not expose (a tuning entry an older driver lacks, centering, pedal dead
zones and curves) stays visible, unsupported, with the reason. Tested end to end on a fake
CSL DD in sysfs, and checked read-only on a real CSL DD.

Mapping of `hid-fanatecff` (`bc2601b`) sysfs attributes (tuning attributes live under the
`ftec_tuning` class device):

| `hid-fanatecff` sysfs | simrig | Kind | How the Fanatec driver persists it |
|---|---|---|---|
| `range` | `wheel_base.rotation` | standard | the module's `init_range` option (a modprobe.d file written by a privileged operation), or a generated udev rule |
| `tuning_menu/FF` | `wheel_base.ffb_strength` | standard | base memory (tuning slot) |
| `tuning_menu/NDP`, `NFR`, `NIN` | `wheel_base.natural_damper`, `natural_friction`, `natural_inertia` | standard | base memory |
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

Unsupported with this driver, shown disabled with their reason: e.g. `pedals.force` on pedals without a load cell (`NotInHardware`, detail: "add the
Load Cell Kit"), `steering_wheel.display` on rims without a display (`NotInHardware`).

Whether the tuning menu really persists in the base's memory must be **verified on hardware**.

### 11.1 Moza

Moza bases need no driver of their own on recent kernels: force feedback goes through
`hid-universal-pidff`, in Linux from 6.15 (backported to 6.12.24, 6.13.12, 6.14.3). Their
settings do not go through the kernel at all: every Moza device has a serial port (`ttyACM`)
speaking a protocol worked out by [Boxflat](https://github.com/Lawstorant/boxflat).

**The driver** installs SimRig's udev rules (the serial port and event nodes for the logged-in
user, the port hidden from ModemManager) and, only when the kernel has no
`hid-universal-pidff`, builds the bundled [universal-pidff](https://github.com/JacKeTUs/universal-pidff)
with DKMS (the `module.sh` recipe), making the bases already plugged in start again with it. The kernel's module,
or one installed by other means, is used as it is.

**The adapter** claims every device of the vendor (Moza does not publish its product ids) and
tells them apart by USB product name, like Boxflat: bases, SR-P/CRP pedal sets, the HBP
handbrake. Shifters, hubs and the like are attached without components for now (they can be
given to the generic USB plugin). Settings are read and written over the serial port: rotation
(stored as half the angle, with the games' maximum kept equal), force feedback strength, natural
damper and friction, the games' spring and damper gains as standard parameters; torque, maximum
speed, natural inertia, centering spring, road sensitivity, the games' friction and inertia
gains and the protection settings as custom ones. The base keeps them in its memory. Values are
cached (the base answers in milliseconds, not microseconds); a port that fails is reopened.
Force feedback (test, stop) needs `hid-universal-pidff` bound to the base; without it the base
still has inputs and settings, and says why forces are missing. The SR-P's axes are known (no
mapping); pedals plugged into a base are mapped by pressing them. Tested end to end on a fake R9
whose fake serial port decodes requests and answers like the real one.

### 11.2 Logitech

Logitech wheels are spread over three kernel drivers, which decide what SimRig can show:

| Family | Wheels | Driver | Settings |
|---|---|---|---|
| lg4ff | G29, G27, G25, Driving Force GT/Pro/EX, MOMO, Speed Force Wireless, G923 for PlayStation | the kernel's `hid-logitech`, replaced by the bundled [new-lg4ff](https://github.com/berarma/new-lg4ff) when one of these wheels is plugged in | rotation (kernel); force feedback gain, spring, damper and friction effect levels, autocenter, force feedback on the LEDs (new-lg4ff) |
| hidpp | G920, G923 for Xbox | the kernel's `hid-logitech-hidpp` | rotation |
| direct drive | G PRO (Xbox/PC, PlayStation), RS50 | the bundled [TrueForce driver](https://github.com/mescon/logitech-trueforce-linux-driver) (`hid-logitech-dd`), built when one of them is plugged in | rotation, strength, damping, TrueForce, force feedback filter, firmware, centering |

**The driver** always installs SimRig's udev rules (the attributes above writable by local
users, raw HID of the direct drive wheels for TrueForce through Proton), and builds each module
only for the wheels plugged in that need it: `status` reports the driver as not installed while
such a wheel is plugged in and its module is missing, so adding the wheel installs it. A module
installed by other means is kept. The TrueForce driver also claims the G923; SimRig does not
build it for a G923, which keeps the kernel's driver (or new-lg4ff). Settings are saved for the
next plug-in by the `save-setting` privileged operation, as udev rules that write the attribute
when the driver binds to that model (`72-simrig-logitech-settings.rules`).

**The adapter** gives each wheel a wheel base, its rim (buttons) and the pedals plugged into it
(Oversteer's tables per model; the RS50's, which no table covers, are mapped while it is added). What a family's driver lacks stays visible with the reason (new-lg4ff's
settings on the kernel's driver say it is needed). Tested end to end on a fake G29 (with and
without new-lg4ff) and a fake G PRO.

### 11.3 Thrustmaster

The kernel's `hid-thrustmaster` only starts Thrustmaster wheels up (they appear with a generic
id, then switch to their own); force feedback needs [hid-tmff2](https://github.com/Kimplul/hid-tmff2).

**The driver** builds hid-tmff2 with DKMS: its `dkms.conf` lives in `dkms/` and its version is
fixed upstream, both handled by the `dkms` recipe, and it bundles
[hid-tminit](https://github.com/scarburato/hid-tminit) as a nested submodule, which starts the
wheels in place of the kernel's module; that one is blacklisted while SimRig's hid-tmff2 is
installed, as upstream recommends (`/etc/modprobe.d/simrig-thrustmaster.conf`), and restored on
removal. A hid-tmff2 installed by other means is kept, and the user's modprobe configuration
with it. Settings are saved as options of the module (`save-setting`): hid-tmff2 applies them
to every wheel it starts.

**The adapter** gives each wheel a wheel base (rotation within the wheel's limits, gain, spring,
damper and friction levels, force feedback test), its rim (buttons) and the pedals
plugged into it. A wheel still starting up (hid-tminit's ids) is claimed without components, so
adding it installs the driver. Tested end to end on a fake T300 RS.

### 11.4 PXN

PXN bases are direct drive bases with standard HID PID force feedback, like Moza's: the V10,
V12 and GT987 (Lite Star's vendor id) are driven by `hid-universal-pidff`, in Linux from 6.15;
the VD6 (PXN's own vendor id) by the kernel's generic PID force feedback from 6.15 too, and by
the bundled universal-pidff on older kernels. The driver is Moza's, with SimRig's udev rules for
PXN; both bundle the same universal-pidff, and a module one of them built is used as it is by
the other.

Settings exist for the VD series only: its vendor control interface (a hidraw node, usage page
0xff00) speaks a protocol [Pitwall](https://github.com/unionyxx/pitwall) reverse engineered and
verified on a VD6: a session handshake, then the 31-byte base settings block read and written
whole (read, change, write, so fields SimRig does not show are kept), with the steering stream
the base sends 100 times a second skipped. Rotation, force feedback strength, natural damper,
friction and inertia and the spring and damper effect strengths are standard parameters;
torque, speed, centering, end stops, road feel, filter, the other effect strengths, hands-off
protection and reversed forces are custom ones. Whether the base keeps them when unplugged is
not verified, so they are not offered as saved. The V10 and V12 families have no known settings
protocol: their settings are shown as unavailable, with the reason. Force feedback is offered
when the event node takes effects. Tested end to end on a fake VD6 whose control interface
answers like the real one, and a fake V12.

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
| D15 | No privileges at runtime: live settings, tests and presets go through the drivers' interfaces; authorization only for system changes, through pkexec + `simrig-helper` | the app is always started as a normal user; the password is asked only when it is really needed |
| D16 | Guided procedures (calibration, mapping, setup) are defined by plugins; the core only renders generic steps | new procedures without touching the core, like custom parameters |
| D17 | The core ships generic layered drawings per component type; plugins give appearance hints (rim shape) or their own pictures | every device looks right without a single brand asset; brand-agnostic by construction |
| D18 | The generic USB plugin's driver is a userspace service started by udev with each set-up device (hidraw in, or the USB interrupt endpoint for devices the kernel's HID driver rejects; uinput out), not HID-BPF or a kernel module | works on any kernel with free-form mappings; nothing runs while no set-up device is connected; HID-BPF stays an option for later |

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
