# Third-party components

SimRig is licensed under GPL-3.0-or-later (see [LICENSE](LICENSE)). It ships these components,
each under its own license.

## Bundled with the plugins

| Component | Where | Authors | License | Version |
|---|---|---|---|---|
| [hid-fanatecff](https://github.com/gotzl/hid-fanatecff): Linux kernel driver for Fanatec devices | `plugins/fanatec/driver/hid-fanatecff` (git submodule) | Matthias Gorzellik (gotzl) and contributors | GPL-2.0 ([LICENSE](plugins/fanatec/driver/hid-fanatecff/LICENSE)) | see [`hid-fanatecff.version`](plugins/fanatec/driver/hid-fanatecff.version) |
| [universal-pidff](https://github.com/JacKeTUs/universal-pidff): Linux kernel driver for direct drive bases (Moza and others), upstream in Linux 6.15 | `plugins/moza/driver/universal-pidff` and `plugins/pxn/driver/universal-pidff` (git submodules) | Oleg Makarenko (JacKeTUs), Tomasz Pakuła and contributors | GPL-2.0 ([LICENSE](plugins/moza/driver/universal-pidff/LICENSE)) | see [`universal-pidff.version`](plugins/moza/driver/universal-pidff.version) |
| [new-lg4ff](https://github.com/berarma/new-lg4ff): Linux kernel driver for the Logitech G29 family | `plugins/logitech/driver/new-lg4ff` (git submodule) | Bernat Arlandis (berarma), Simon Wood and the authors of the kernel's hid-logitech | GPL-2.0 ([LICENSE](plugins/logitech/driver/new-lg4ff/LICENSE)) | see [`new-lg4ff.version`](plugins/logitech/driver/new-lg4ff.version) |
| [logitech-trueforce-linux-driver](https://github.com/mescon/logitech-trueforce-linux-driver): Linux kernel driver for the Logitech G PRO and RS50 | `plugins/logitech/driver/logitech-trueforce` (git submodule; the packages ship only its kernel module, `mainline/`, and its license) | mescon and contributors (see its CREDITS.md), on the kernel's hid-logitech-hidpp | GPL-2.0 ([COPYING](plugins/logitech/driver/logitech-trueforce/COPYING)) | see [`logitech-trueforce.version`](plugins/logitech/driver/logitech-trueforce.version) |
| [hid-tmff2](https://github.com/Kimplul/hid-tmff2): Linux kernel driver for Thrustmaster wheels, with [hid-tminit](https://github.com/scarburato/hid-tminit) (its nested submodule) | `plugins/thrustmaster/driver/hid-tmff2` (git submodule) | Kim Kuparinen (Kimplul), Dario Pagani (scarburato) and contributors | GPL-2.0-or-later, as its sources declare (the repository's [LICENSE](plugins/thrustmaster/driver/hid-tmff2/LICENSE) file is GPL-3.0) | see [`hid-tmff2.version`](plugins/thrustmaster/driver/hid-tmff2.version) |

The kernel drivers are separate programs: each is distributed as its unmodified source code and
built on the user's machine by DKMS when its plugin installs its driver. SimRig talks to them
only through the kernel's interfaces (sysfs, evdev), like a game does: they are aggregated with
SimRig, not combined into one work. Any change SimRig makes to them is published under GPL-2.0
and offered upstream.

## Derived from other projects

| What | Where | From | License |
|---|---|---|---|
| The Moza serial protocol: frame format, checksum, command table and value scales | `plugins/moza/src/protocol.rs`, `plugins/moza/src/settings.rs` | [Boxflat](https://github.com/Lawstorant/boxflat) by Tomasz Pakuła (`moza-protocol.md`, `data/serial.yml`, the base panel) | GPL-3.0, like SimRig |
| The PXN VD control protocol: reports, commands and the base settings block | `plugins/pxn/src/protocol.rs`, `plugins/pxn/src/settings.rs` | [Pitwall](https://github.com/unionyxx/pitwall) by unionyxx (`pitwall/protocol.py`, the base page) | GPL-3.0-or-later, like SimRig |

## Fonts

| Font | Where | License |
|---|---|---|
| [Barlow](https://github.com/jpt/barlow) by Jeremy Tribby | `assets/fonts/Barlow-*.ttf` | SIL Open Font License 1.1 ([OFL](assets/fonts/OFL-Barlow.txt)) |
| [Martian Mono](https://github.com/evilmartians/mono) by Evil Martians | `assets/fonts/MartianMono-Variable.ttf` | SIL Open Font License 1.1 ([OFL](assets/fonts/OFL-MartianMono.txt)) |

## Rust crates

The Rust libraries SimRig is built with (Slint, serde, evdev, resvg…) are listed in
`Cargo.lock`; each comes under its own license, all compatible with GPL-3.0.

## Trademarks

Product and brand names (such as Fanatec, Moza, Logitech, Thrustmaster, PXN, Simagic) belong to their owners and
are used only to say which hardware SimRig works with. SimRig is not affiliated with, endorsed
by or sponsored by any of them. The device drawings in the app are generic and represent no
manufacturer's product.
