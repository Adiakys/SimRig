# Third-party components

SimRig is licensed under GPL-3.0-or-later (see [LICENSE](LICENSE)). It ships these components,
each under its own license.

## Bundled with the plugins

| Component | Where | Authors | License | Version |
|---|---|---|---|---|
| [hid-fanatecff](https://github.com/gotzl/hid-fanatecff): Linux kernel driver for Fanatec devices | `plugins/fanatec/driver/hid-fanatecff` (git submodule) | Matthias Gorzellik (gotzl) and contributors | GPL-2.0 ([LICENSE](plugins/fanatec/driver/hid-fanatecff/LICENSE)) | see [`hid-fanatecff.version`](plugins/fanatec/driver/hid-fanatecff.version) |

`hid-fanatecff` is a separate program: it is distributed as its unmodified source code and built
on the user's machine by DKMS when the Fanatec plugin installs its driver. SimRig talks to it
only through the kernel's interfaces (sysfs, evdev), like a game does: the two are aggregated,
not combined into one work. Any change SimRig makes to it is published under GPL-2.0 and offered
upstream.

## Fonts

| Font | Where | License |
|---|---|---|
| [Barlow](https://github.com/jpt/barlow) by Jeremy Tribby | `assets/fonts/Barlow-*.ttf` | SIL Open Font License 1.1 ([OFL](assets/fonts/OFL-Barlow.txt)) |
| [Martian Mono](https://github.com/evilmartians/mono) by Evil Martians | `assets/fonts/MartianMono-Variable.ttf` | SIL Open Font License 1.1 ([OFL](assets/fonts/OFL-MartianMono.txt)) |

## Rust crates

The Rust libraries SimRig is built with (Slint, serde, evdev, resvg…) are listed in
`Cargo.lock`; each comes under its own license, all compatible with GPL-3.0.

## Trademarks

Product and brand names (such as Fanatec, Moza, Simagic, Logitech) belong to their owners and
are used only to say which hardware SimRig works with. SimRig is not affiliated with, endorsed
by or sponsored by any of them. The device drawings in the app are generic and represent no
manufacturer's product.
