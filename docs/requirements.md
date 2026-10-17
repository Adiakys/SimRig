# simrig — Requirements

> Working project name. Status: draft 0.6 (2026-10-17). See also [architecture.md](architecture.md).

## 1. Vision

A single, polished graphical Linux application to configure a sim-racing rig of any brand:
wheel base, steering wheel, pedals, shifter, handbrake.

The user installs **one package**, describes their rig with a setup wizard, and the app installs
the required drivers (shipped alongside it), then lets them calibrate, tune and save presets.

The app is **a client of the drivers, just like a game**: it talks to devices through the drivers'
interfaces and is never in the data path. Every driver is **independent**: it decides how it is
built and how it works (including its own background processes, if it needs any), and it works
while the app is closed. Drivers are installed when a device that needs them is added.
**When the app is closed, it leaves no process running.**

The software is **generic**: it knows device *types* and what they can do. Support for each
manufacturer comes from **plugins**: components external to the program, with a versioned
interface, installable and updatable separately. Official plugins, released together with the
app, use exactly the same mechanism as third-party ones.

## 2. Entities

| Entity | Definition |
|---|---|
| **Rig** | A user's set of devices, how they are connected, and their presets. |
| **Physical device** | An actual USB peripheral (VID/PID, `hidraw`/`evdev`/serial/sysfs nodes). |
| **Component** | A logical function of the rig. One physical device may provide several (e.g. a Fanatec base provides base + mounted wheel + pedals plugged in via RJ12). |
| **Model** | Catalog entry: brand, name, image, provided components, required driver. |
| **Plugin** | Package external to the app that adds support for one or more manufacturers. Contains a *driver* and an *adapter*. |
| **Driver** | What makes the device work in the system, with the app closed. Independent: it decides how it is built (kernel module, HID-BPF, udev rules, its own services…). Towards the app it only exposes a contract: install, check, remove, optional privileged operations. |
| **Adapter** | The part of the plugin used by the app: talks to the driver like a game does and translates it into components. Runs only while the app is open. |
| **Persistent setting** | A value the driver remembers while the app is closed, in whatever way the driver chooses. |
| **Preset** | A named set of parameter values (e.g. "GT3", "Drift") the user applies from the app. |

Component types (v1):

- **Wheel base** (motor)
- **Steering wheel** (rim)
- **Pedals**, with individual pedals: throttle, brake, clutch
- **Shifter**: H-pattern, sequential (some models do both)
- **Handbrake**

Later: button boxes, dashboards/displays, seat movers and bass shakers.

## 3. Main use cases

1. **First run**: install the app, the wizard starts, plug in the devices; the app recognizes them
   and shows them with their images, installs the drivers, I calibrate, test, save.
2. **Device not plugged in**: in the wizard I add from the catalog a device I will connect later
   (e.g. "the pedals are plugged into the base").
3. **Daily use**: I power on the rig and play. The app is closed: drivers and saved settings work
   on their own. I open the app only to change something.
4. **Presets**: before a session I open the app and switch from my "GT3" preset to "Drift"; the
   values are saved into the drivers and stay active after I close the app.
5. **New device**: I buy a handbrake, open the app, "Add device": it is recognized, its driver is
   installed if needed, I calibrate, save.
6. **Rim swap**: I swap the wheel rim; the app updates components and image.
7. **Development**: without hardware, I use the **simulated rig** to build and test GUI and logic.

## 4. Functional requirements

### Detection and rig model
- **FR-01** Detect devices being plugged and unplugged (hotplug) while the app is open.
- **FR-02** Match each device to a catalog model and a plugin.
- **FR-03** Determine the components each device provides, including hot changes (e.g. rim swap).
- **FR-04** Let the user confirm or correct the device → components mapping (e.g. where the pedals
  are plugged in).
- **FR-05** Handle unknown devices as "generic HID": the user maps axes and buttons to components.
- **FR-06** A rig may combine devices of **any brands**, with no constraint between plugins (e.g. a
  Fanatec base and wheel with Moza pedals), and several components of the same type (two pedal
  sets, an H-pattern and a sequential shifter).
- **FR-07** Each device is handled by exactly one plugin: the most specific match wins, ties are
  reported, and the user can override the choice.
- **FR-08** The user can exclude from the rig an optional component a device exposes but that is
  not in use (e.g. the pedal inputs of a base when other pedals are used), and re-enable it.

### Inputs (read)
- **FR-10** Show the live state of every component (angle, pedals, gear, buttons…).
- **FR-11** Show both raw and calibrated values.
- **FR-12** Read device information: firmware, rim ID, status.

### Outputs (operations)
- **FR-20** The core defines, for each component type, a standard interface of inputs, parameters
  and actions. Plugins implement it and may report any item as unsupported, always with a
  reason (not in hardware, not in driver, needs firmware, needs another component, depends on
  another setting, temporarily unavailable).
- **FR-20a** The UI never hides unsupported items: it shows them disabled, in their usual place,
  with the explanation of why. Items depending on another setting enable themselves live.
- **FR-20b** Plugins may declare additional **custom parameters** at runtime, fully self-described
  (type, range, unit, labels, group, dependencies, availability, persistence). The app loads and renders them
  dynamically. No brand-specific parameter is written in the core.
- **FR-20c** Read and write both standard and custom parameters.
- **FR-21** Run the supported actions: calibration, centering, FFB test effects, reset.
- **FR-22** Drive outputs: LEDs, displays, vibration, where available.
- **FR-23** The app talks to devices only through the drivers' interfaces (evdev, FFB, sysfs,
  the device's configuration protocol), like a game. It never processes inputs at runtime.
- **FR-24** Every parameter states whether it is persistent (the driver remembers it with the app
  closed) or lasts only until unplugged. How it is persisted is up to the driver.

### Setup wizard
- **FR-30** System check: distro, kernel, Secure Boot, Wayland/X11 session.
- **FR-31** Scan and recognition, with model images.
- **FR-32** Rig composition: component assignment, manual addition from the catalog.
- **FR-33** List and install required drivers, including Secure Boot key enrollment (MOK).
- **FR-34** Calibration of each component, with steps provided by the plugin or generic ones.
- **FR-35** Basic settings and final test (live inputs, FFB test).
- **FR-36** Save persistent settings and the first preset.
- **FR-37** Two forms: first-time setup of the whole rig, and "add device" for a single one.

### Catalog and images
- **FR-40** Model catalog as data files, extensible without recompiling.
- **FR-41** Every model has an image; when missing, the generic illustration of its type is used.
- **FR-42** The user can replace an image with their own (e.g. a photo of their rig).
- **FR-43** "Rig view" with component images animated live (the wheel rotates, pedals fill up,
  the engaged gear lights up).

### Drivers and installation
- **FR-50** Ship the required drivers inside the plugins, at pinned versions.
- **FR-51** Install them when a device that needs them is added, through the driver contract
  (`install` / `status` / `uninstall` hooks), asking for privileges only at that moment.
- **FR-52** Installed drivers keep working with the app closed and after the app is uninstalled.
  Every installation is recorded so it can be upgraded and removed.
- **FR-52b** Uninstalling the app optionally removes the drivers too: the in-app uninstall lets
  the user pick which drivers to remove (default: keep). Drivers can also be removed after the
  app is gone (purge, or a standalone removal script). Drivers provided by the system are never
  removed.
- **FR-53** Detect an equivalent driver already present (in-kernel or a distro package) and install
  nothing in that case.
- **FR-54** Check driver status every time the app is opened and report problems (e.g. module not
  built for the running kernel).
- **FR-55** Every driver is independent: it chooses its technology, structure and any background
  processes of its own. The app neither starts nor depends on them.
- **FR-56** The SDK provides ready-made recipes for common cases (DKMS with Secure Boot signing,
  udev rules, HID-BPF, per-device service, persistent settings via udev).

### Plugins
- **FR-60** Each manufacturer's support is a plugin separate from the app: manifest, driver,
  adapter, catalog, images.
- **FR-61** Plugins are installed, updated and removed separately; the app shows their version,
  origin (official / third-party) and status.
- **FR-62** Official plugins are developed and released together with the app, but loaded through
  the same mechanism as external ones.
- **FR-63** A plugin can be written in any language. Rust has an SDK.

### Presets
- **FR-70** Named presets, applied manually from the app (or `simrig-cli apply <preset>`).
  Applying a preset saves its values into the drivers as persistent settings, so they stay active
  with the app closed. No configuration outside the app is required (no game launch options).
- **FR-71** Standard parameters in presets are portable across brands and apply to every component
  of their type, or to a specific component when the rig has several; custom parameters are stored
  per plugin and applied only when that plugin's device is present.
- **FR-72** Persistent settings are applied by the driver when the device is plugged in, without
  the app.
- **FR-73** Preset import/export for sharing.

### Development and diagnostics
- **FR-80** Complete "simulated rig" plugin (base, wheel, pedals, H and sequential shifter, handbrake).
- **FR-81** Record traffic from a real device and replay it in tests.
- **FR-82** Command-line interface with the same capabilities as the core.
- **FR-83** Export a diagnostics bundle for bug reports.

## 5. Non-functional requirements

- **NFR-01 Portability**: all major distros and all desktops, Wayland and X11. Reached gradually
  (see roadmap), but no choice may tie the project to Ubuntu or GNOME.
- **NFR-02 Privileges**: the app always runs as the user. Privileged operations go through a helper
  with a closed set of actions, authorized via polkit.
- **NFR-03 No app background processes**: with the app closed, nothing from simrig runs (no agents,
  no services). Drivers are free to have their own processes: they are not part of the app.
- **NFR-04 Physical safety**: direct-drive bases can hurt. Test effects ramp in and need explicit
  confirmation; a "stop" button zeroes the force; never apply values outside the limits the
  driver declares.
- **NFR-05 Latency**: the app adds no latency because it is not in the data path. Official drivers
  with userspace parts add less than 1 ms at the 99th percentile.
- **NFR-06 Independence**: devices and saved settings work without the app and after uninstalling it.
- **NFR-07 Extensibility**: a new manufacturer = a new plugin, with no change to the app, including
  its brand-specific settings (custom parameters). A new model of a supported manufacturer = data
  only, inside the plugin.
- **NFR-08 Isolation**: a crashing or hung plugin brings down neither the app nor other plugins.
  The app ↔ plugin interface is versioned and the app rejects incompatible plugins.
- **NFR-09 Trust**: privileged driver hooks are executed only from plugins installed system-wide
  (root-owned directories), never from plugins in the user's home.
- **NFR-10 Testability**: all hardware access goes through a replaceable I/O layer, so core and
  plugins can be tested without devices.
- **NFR-11 Languages**: code and documentation in English. UI in English and Italian from v1.
- **NFR-12 Standards**: configuration and data in XDG directories.
- **NFR-13 Licensing**: app GPL-3.0-or-later. Shipped drivers keep their own license (GPL-2.0 for
  kernel modules). Model images carry a per-file license: no manufacturer photos without permission.
- **NFR-14 Privacy**: no telemetry. Works offline.

## 6. Out of scope for v1

- Writing new complex kernel drivers: existing ones are reused.
- Official drivers adding curves and dead zones to devices that do not support them: after v1.
- Automatic per-game preset switching in the app (it would need a background process). A driver
  may offer it on its own if it wants.
- Game telemetry driving LEDs and displays (SimHub-style): later.
- Device firmware updates.
- Automatic driver installation on immutable distros (Silverblue, SteamOS): instructions only.
- Flatpak build with driver installation (the sandbox does not allow it): native packages only.

## 7. Roadmap

| Phase | Content | Done when |
|---|---|---|
| **M0** ✅ | Domain model, plugin protocol + SDK, "simulated rig" plugin, CLI, tests | `simrig-cli` lists and reads the simulated rig running as an external plugin; CI green |
| **M1** | GUI on the simulated rig: rig view, live inputs, parameters, wizard | Full wizard works on the simulated rig |
| **M2** | Fanatec plugin: `hid-fanatecff` + DKMS install + udev rules | From scratch: install the package, the wizard installs the driver and configures the CSL DD; after a reboot, with the app closed, the saved rotation is applied |
| **M3** | Presets (create, apply, import/export), clean uninstall | Switching preset in the app changes the base's behaviour, also after closing the app and replugging |
| **M4** | Catalog-driven generic HID, Logitech, Moza… | Each brand has at least one tester with the hardware; a mixed-brand rig (e.g. Fanatec base + Moza pedals) works end to end |
| **M5** | Multi-distro packages (AUR, OBS, COPR) | Installation verified on Ubuntu, Fedora, Arch |

## 8. Open questions

1. Final project name (`simrig` is the working name until decided).
2. Third-party plugins: for now, same trust level as any installed software.

## 9. Resolved

| Question | Decision |
|---|---|
| GUI toolkit | Slint (architecture D9) |
| Documentation and code language | English (NFR-11) |
| Mixing brands in one rig | allowed without constraints (FR-06, architecture §3.1.1) |
