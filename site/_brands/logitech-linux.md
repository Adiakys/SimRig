---
title: Logitech G29, G923, G920, G PRO and RS50 on Linux
nav: Logitech
order: 3
status: untested
summary: G29, G923, G920, G27, G25, Driving Force GT and Pro, MOMO, G PRO, RS50, WingMan
description: >-
  Set up a Logitech G29, G923, G920, G27, G25, Driving Force GT, G PRO or RS50 on Linux:
  rotation, force feedback, combined pedals and TrueForce. SimRig adds new-lg4ff and the
  TrueForce driver only for the wheels that need them.
lede: >-
  Every Logitech wheel from the WingMan to the RS50, with the right driver for each one and the
  settings it offers on Linux.
card: { label: "Steering wheel", art: [wheel-round.svg], driver: "The kernel's, new-lg4ff, TrueForce", tested: "Not yet" }
---

## Models

| Family | Models | Driver |
|---|---|---|
| G29 family | G29, G923 (PlayStation), G27, G25, Driving Force GT, Driving Force Pro, Driving Force, MOMO, Speed Force Wireless, WingMan Formula Force GP | in the kernel; SimRig adds [new-lg4ff](https://github.com/berarma/new-lg4ff) for the full set of settings |
| Xbox and PC | G920, G923 (Xbox) | in the kernel |
| TrueForce | G PRO, RS50 | the [TrueForce driver](https://github.com/mescon/logitech-trueforce-linux-driver), built by SimRig |
| Classic | WingMan Formula GP, WingMan Formula Force | in the kernel |
{: .table-scroll}

The pedals on the wheel come with it. Buttons are shown by name on the G29, G923, G920, G PRO and
RS50.

## Drivers

Logitech wheels work with the kernel's own driver. For more, SimRig builds two open source
drivers with DKMS, and only when one of their wheels is plugged in:

- **new-lg4ff** by Bernat Arlandis, for the G29 family: more force feedback effects, combined
  pedals, the LEDs, and the model presented to older games.
- **The TrueForce driver** by mescon and contributors, for the G PRO and RS50.

The Xbox editions of the G923 and RS50 start in a mode Linux cannot use. SimRig switches them
to their PC mode at every plug-in; that needs `usb-modeswitch` installed.

## What you can set

**G29 family**

- **Rotation**, from 40° up to 900° (270° on the oldest models), and **force feedback
  strength**.
- With new-lg4ff: the strength of the games' spring, damper and friction effects, the centering
  spring, force feedback on the LEDs, **combined pedals** (throttle and brake, or throttle and
  clutch, on one axis for games that need it), and the **model presented to games**, for older
  games that know only an earlier wheel.

**G920 and G923 (Xbox)**: rotation from 180°, force feedback strength and the centering spring.

**G PRO and RS50**: rotation up to 1080° or 2700°, force feedback strength, natural damper,
TrueForce strength and the force feedback filter.

On every wheel, **Test force feedback** checks it before opening a game. Settings are re-applied
at every plug-in.

## Limits

- TrueForce in games running through Proton also needs that project's companion apps
  (logi-launch), which SimRig does not install.
- The WingMan wheels have a fixed rotation of 180°.
- Wheels center themselves when plugged in.

## Other tools

[Oversteer](https://github.com/berarma/oversteer) has long managed Logitech wheels on Linux and
works with the same drivers. See [other Linux tools]({{ '/alternatives/' | relative_url }}).
