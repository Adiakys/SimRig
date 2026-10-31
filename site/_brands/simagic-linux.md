---
title: Simagic Alpha and EVO wheel bases on Linux
nav: Simagic
order: 5
status: untested
summary: Wheel bases Alpha Mini, Alpha, Alpha U and the EVO series, with pedals on the base
description: >-
  Use a Simagic Alpha, Alpha Mini, Alpha U, EVO Sport, EVO or EVO Pro wheel base on Linux with
  rotation, force feedback and base settings: SimRig installs simagic-ff only for the bases that
  need it.
lede: >-
  Simagic direct drive bases on Linux, with their rotation, force feedback and base settings.
card: { label: "Wheel base", art: [base.svg], driver: "simagic-ff, built with DKMS", tested: "Not yet" }
---

## Models

| Type | Models |
|---|---|
| Wheel bases | Alpha Mini, Alpha, Alpha U, EVO Sport, EVO, EVO Pro |
| Pedals | Pedals plugged into the base |
{: .table-scroll}

## Driver

The EVO bases, and the Alpha bases on firmware v159 or newer, need
[simagic-ff](https://github.com/JacKeTUs/simagic-ff), an open source kernel driver by Oleg
Makarenko and contributors. SimRig builds it with DKMS only for those bases.

An Alpha on a firmware older than v159 works with the kernel's own driver: force feedback, but
no settings. Use Simagic's software to change them, or update the firmware.

## What you can set

- **Rotation**, from 90° to 2520°, and **force feedback strength**.
- The natural damper, friction and inertia, and the strength of the games' friction, inertia,
  spring and damper effects.
- **The base's settings**: feedback detail, force feedback filter, natural centering, the ring
  light, slew rate limit, center damper, wheel speed limit, end stop angle and strength.
- **Test force feedback**, to check the base before opening a game.

Settings are re-applied at every plug-in. Rotation is not: games set it as they start.

## Limits

- Rotation changes only with the wheel near the center (within half the new rotation), or the
  base would lose force feedback until it is plugged in again: SimRig says so.
- Centering is set in Simagic's software, not in SimRig yet.
- Pedal sets, shifters and handbrakes on their own USB are not supported yet.
