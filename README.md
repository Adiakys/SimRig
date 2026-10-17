# SimRig

**Set up your sim-racing rig on Linux, whatever the brand.** Wheel base, steering wheel, pedals,
shifter and handbrake in one app: SimRig installs the drivers your devices need, shows every
device moving live, lets you tune and calibrate them and save presets, then gets out of the way.
Your devices keep working in every game with SimRig closed.

![SimRig: the rig live, a wheel base's settings, setting up a shifter, adding a device, plugins and drivers, first run](docs/images/simrig.gif)

- **Plug in, see it move.** Every device is drawn as it stands on your rig and moves as you
  use it: the wheel turns, the gear engages, the handbrake pulls.
- **Drivers without the terminal.** When a device needs a driver, SimRig installs it (one
  password) and it keeps working with the app closed, after updates and reboots.
- **Any mix of brands.** A base from one brand, pedals from another, a no-name USB shifter:
  they all sit in the same rig.
- **No root, no background process.** SimRig runs as you, asks for the password only to install
  a driver, and leaves nothing running when you close it.
- **Honest about your hardware.** What a device or its driver cannot do is shown greyed out
  with the reason, never hidden.

> **Early development.** SimRig works, but most devices have not been tested on real hardware
> yet: see the table below, and please report what works and what doesn't.

## Supported devices

| Brand | Devices | Driver | Status |
|---|---|---|---|
| **Fanatec** | Wheel bases: CSL DD / GT DD Pro / ClubSport DD, CSL Elite, ClubSport V2 and V2.5, Podium DD1 and DD2, CSR Elite, Porsche 911 Turbo S. Their steering wheels. Pedals on the base, CSL Elite and CSL load cell pedals, ClubSport V3 | [hid-fanatecff](https://github.com/gotzl/hid-fanatecff), installed by SimRig (or yours, if you have it) | **Partial**: tested on a CSL DD (live inputs, rotation, tuning menu); other models untested |
| **Any USB device** | Shifters (H-pattern, sequential) and handbrakes without a brand, or whose descriptor is useless: mapped by moving them | SimRig's generic USB driver | **Untested** on real hardware (works on simulated devices) |
| **Moza** | Wheel bases R3 to R21 (rotation, force feedback, base settings), pedals on the base, SR-P and CRP pedals, HBP handbrake. Shifters and wheels' own settings not yet | [hid-universal-pidff](https://github.com/JacKeTUs/universal-pidff), in the kernel from 6.15 (built by SimRig on older kernels); settings over the serial port, with the protocol worked out by [Boxflat](https://github.com/Lawstorant/boxflat) | **Untested** on real hardware (works on simulated devices) |
| **Logitech** | G29, G27, G25, Driving Force GT/Pro, MOMO, G923 (PlayStation and Xbox), G920, G PRO, RS50: rotation, force feedback settings, pedals on the wheel | in the kernel; SimRig adds [new-lg4ff](https://github.com/berarma/new-lg4ff) for the G29 family and the [TrueForce driver](https://github.com/mescon/logitech-trueforce-linux-driver) for the G PRO and RS50, only when one of them is plugged in | **Untested** on real hardware (works on simulated devices) |
| **Thrustmaster** | T300 RS (and T-GT II), T248, T128, TX, TS-PC Racer, TS-XW: rotation, force feedback strength, effect levels, pedals on the base. Not the T818 | [hid-tmff2](https://github.com/Kimplul/hid-tmff2), installed by SimRig (or yours, if you have it) | **Untested** on real hardware (works on simulated devices) |
| **Simagic** | Wheel bases, pedals, shifters, handbrakes | to be evaluated | Planned |
| **PXN** | V10, V12, V12 Lite, VD series | [hid-universal-pidff](https://github.com/JacKeTUs/universal-pidff), in the kernel from 6.15 | Planned |

**Full**: tested on the hardware · **Partial**: tested on some models or some features ·
**Untested**: works in tests, not tried on real hardware yet · **Planned**: not supported yet

Features depend on the device and its driver: SimRig shows, for every setting, whether your
hardware supports it.

## Install

Download the package for your distribution from the
[latest release](https://github.com/Adiakys/SimRig/releases/latest) and install it:

```sh
# Debian, Ubuntu, Linux Mint, Pop!_OS
sudo apt install ./simrig_*_amd64.deb

# Fedora, openSUSE
sudo dnf install ./simrig-*.x86_64.rpm

# Arch, Manjaro, EndeavourOS
sudo pacman -U ./simrig-*-x86_64.pkg.tar.zst

# Any other distribution
tar xzf simrig-*-x86_64-linux.tar.gz && sudo ./simrig-*-x86_64-linux/install.sh
```

Then open **SimRig** from your applications and follow the first setup: it checks your system,
finds your devices and installs what they need.

**Requirements**: an x86-64 Linux with polkit (`pkexec`). Drivers built for your kernel (like
Fanatec's) also need DKMS, a compiler and your kernel's headers: SimRig tells you what is
missing.

**Uninstall**: remove the package as usual. The drivers SimRig installed are kept, so your
devices keep working; remove them first from **Plugins** in the app, or afterwards with
`sudo /var/lib/simrig/remove-drivers --all` (on Debian and Ubuntu, `apt purge simrig` does it).

## How it works

SimRig is a client of your drivers, like a game. Each brand is supported by a **plugin** that
knows its devices and, when needed, ships a **driver**: SimRig installs the driver when you add
the device, and the driver does the work, in every game, with SimRig closed. The app only reads
and changes settings through the same interfaces games use, so it never adds latency and never
needs to run in the background.

Settings you change apply at once. **Save as default** stores them in the driver or in the
device, so they come back every time you plug it in. **Presets** save the whole rig and switch
it in one click.

## Drivers

SimRig builds on open source drivers maintained by their own communities. They are bundled
pinned to a tested version, installed on your machine by the app when you add a device, and
credited with their licenses in [THIRD-PARTY.md](THIRD-PARTY.md).

| Plugin | Driver | Upstream | License |
|---|---|---|---|
| Fanatec | hid-fanatecff, kernel module (force feedback, tuning menu, LEDs, display), built with DKMS | [github.com/gotzl/hid-fanatecff](https://github.com/gotzl/hid-fanatecff) | GPL-2.0 |
| Moza | hid-universal-pidff, kernel module (force feedback): the kernel's from Linux 6.15, built with DKMS on older kernels. udev rules for the devices' serial port (settings) | [github.com/JacKeTUs/universal-pidff](https://github.com/JacKeTUs/universal-pidff); serial protocol from [github.com/Lawstorant/boxflat](https://github.com/Lawstorant/boxflat) | GPL-2.0; GPL-3.0 |
| Logitech | the kernel's drivers (G920, G923 for Xbox); new-lg4ff, kernel module for the G29 family (more effects, gain, autocenter, effect levels, LEDs); the TrueForce driver, kernel module for the G PRO and RS50. Both built with DKMS only for the wheels plugged in | [github.com/berarma/new-lg4ff](https://github.com/berarma/new-lg4ff); [github.com/mescon/logitech-trueforce-linux-driver](https://github.com/mescon/logitech-trueforce-linux-driver) | GPL-2.0 |
| Thrustmaster | hid-tmff2, kernel module (force feedback, rotation, gain, effect levels), with hid-tminit, which starts the wheels up in place of the kernel's hid-thrustmaster; built with DKMS | [github.com/Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2) | GPL-2.0-or-later |
| Generic USB | SimRig's own userspace service (hidraw in, uinput out) | this repository | GPL-3.0-or-later |

A driver you installed yourself (a distribution package, a newer version built by hand) is
kept as it is: SimRig only adds its access rules next to it.

## Contributing

Bug reports, tests on your hardware and plugins for new brands are welcome. To build and run
SimRig from the sources, see [docs/development.md](docs/development.md); for its design,
[docs/architecture.md](docs/architecture.md).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE). Bundled third-party components (drivers, fonts) keep
their own licenses: see [THIRD-PARTY.md](THIRD-PARTY.md).

Product and brand names (Fanatec, Moza, Logitech, Thrustmaster…) belong to their owners and are
used only to say which hardware SimRig works with. SimRig is not affiliated with, endorsed by or
sponsored by any of them.
