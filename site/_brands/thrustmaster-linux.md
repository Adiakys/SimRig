---
title: Thrustmaster T300 RS, T248, TX, T150 and TMX on Linux
nav: Thrustmaster
order: 4
status: untested
summary: T300 RS, T-GT II, T248, T128, TX, TS-PC Racer, TS-XW, T150, TMX, T500 RS, T80
description: >-
  Force feedback for a Thrustmaster T300 RS, T-GT II, T248, T128, TX, TS-PC Racer, TS-XW, T150
  or TMX on Linux: SimRig installs hid-tmff2 or hid-t150 and sets rotation and force feedback.
lede: >-
  Thrustmaster wheels need a driver of their own on Linux for force feedback. SimRig installs the
  right one and sets rotation and force feedback.
card: { label: "Steering wheel", art: [wheel-gt.svg], driver: "hid-tmff2 or hid-t150, built with DKMS", tested: "Not yet" }
---

## Models

| Models | Force feedback | Driver |
|---|---|---|
| T300 RS, T-GT II, T248, T128, TX, TS-PC Racer, TS-XW | Yes | [hid-tmff2](https://github.com/Kimplul/hid-tmff2) |
| T150, TMX | Yes | [hid-t150](https://github.com/scarburato/t150_driver) |
| T500 RS | No: no Linux driver drives it yet; inputs only | in the kernel |
| T80, T80 (H) | The wheel has none; inputs only | in the kernel |
{: .table-scroll}

The pedals on the base come with the wheel. The T818 is not supported.

## Drivers

- **hid-tmff2**, by Kim Kuparinen and contributors, with hid-tminit, which starts the wheel up
  in its PC mode. SimRig builds it with DKMS and keeps the kernel's own Thrustmaster driver out
  of its way.
- **hid-t150**, by Dario Pagani, built only when a T150 or TMX is plugged in.

The Xbox editions are taken back from the gamepad driver, so they are seen as wheels. If you
installed one of these drivers yourself, SimRig uses yours as it is.

## What you can set

- **Rotation**, up to 1080° (900° on some models), and **force feedback strength**.
- The **centering spring**, the strength of the games' friction effects, and of their spring
  and damper effects (hid-tmff2 wheels).
- **Test force feedback**, to check the wheel before opening a game.

Saved settings become the driver's start-up options, so they come back with SimRig closed.

## Limits

- The TMX must already be in its PC mode: hid-tminit does not start it up.
- The driver does not say which rim is mounted, and buttons are shown by number.
- The T500 RS keeps its own rotation, 1080°; the T80 is fixed at 270°.
- Wheels center themselves when plugged in.

## Other tools

[Oversteer](https://github.com/berarma/oversteer) works with the same drivers for rotation and
force feedback. See [other Linux tools]({{ '/alternatives/' | relative_url }}).
