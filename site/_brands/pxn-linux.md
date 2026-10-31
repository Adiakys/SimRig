---
title: PXN VD4, VD6, V10 and V12 on Linux
nav: PXN
order: 6
status: partial
tested: a VD4 (live inputs, rotation, base settings, force feedback in games)
summary: VD6 and VD4 with their settings; V10, V12, V12 Lite and Lite Star GT987
description: >-
  Use a PXN VD4, VD6, V10, V12, V12 Lite or Lite Star GT987 on Linux with force feedback, and
  set the VD bases' rotation, torque and force feedback settings with SimRig.
lede: >-
  PXN wheels on Linux with force feedback, and the VD bases' own settings in the same app as
  the rest of the rig.
card: { label: "Wheel base", art: [base.svg], driver: "hid-universal-pidff, in Linux from 6.15", tested: "VD4" }
---

## Models

| Models | What SimRig does |
|---|---|
| VD6, VD4 | Live inputs, force feedback, rotation and the base's settings |
| V10, V12, V12 Lite, Lite Star GT987 | Live inputs and force feedback strength; their other settings stay on the base |
{: .table-scroll}

Pedals plugged into the base are set up by moving them.

## Driver

Force feedback comes from
[hid-universal-pidff](https://github.com/JacKeTUs/universal-pidff), part of Linux since 6.15.
On older kernels SimRig builds it with DKMS. The VD4 is not in that driver: it gets force
feedback from Linux 6.15 on.

## What you can set (VD6 and VD4)

- **Rotation**, from 90° to 2700°, and **force feedback strength**.
- **The base's settings**: torque output, maximum wheel speed, centering spring, end stop force,
  road feel, force feedback filter, the natural damper, friction and inertia, and the strength
  of the games' vibration, inertia, friction, spring and damper effects.
- **Protection**: hands-off protection and reversed forces.
- **Test force feedback**, to check the base before opening a game.

The settings protocol of the VD bases comes from
[Pitwall](https://github.com/unionyxx/pitwall), credited in SimRig's third-party notices.

## Limits

- Whether a VD base keeps its settings when unplugged is not confirmed yet, so SimRig shows them
  as lasting until then.
- The center is set on the base, not in SimRig yet.
- The rim's buttons are shown by number.

## Other tools

[Pitwall](https://github.com/unionyxx/pitwall) goes deeper on the VD bases: road feel
equalizer, pedal curves, lights and a button map. See
[other Linux tools]({{ '/alternatives/' | relative_url }}).
