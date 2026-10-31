---
title: Moza wheel bases, pedals and shifters on Linux
nav: Moza
order: 2
status: partial
tested: CRP2 pedals (live inputs)
summary: Wheel bases R3 to R21, SR-P and CRP pedals, HBP handbrake, SGP and HGP shifters
description: >-
  Use a Moza R3, R5, R9, R12, R16 or R21 wheel base on Linux with force feedback and base
  settings, plus SR-P and CRP pedals, the HBP handbrake and the SGP and HGP shifters. SimRig sets
  them up and builds the force feedback driver on kernels older than 6.15.
lede: >-
  Moza bases on Linux with their force feedback and settings, next to Moza pedals, handbrake and
  shifters, in one rig.
card: { label: "Wheel base", art: [base.svg], driver: "hid-universal-pidff, in Linux from 6.15", tested: "CRP2 pedals" }
---

## Models

| Type | Models |
|---|---|
| Wheel bases | R3, R5, R9, R12, R16, R21 |
| Pedals | Pedals plugged into the base; SR-P and CRP pedals on their own USB |
| Handbrake | HBP |
| Shifters | SGP sequential, HGP H-pattern, on their own USB or plugged into the base |
{: .table-scroll}

Moza does not publish its USB product ids, so SimRig recognizes each device by the name it gives
over USB. Hubs, stalks and dashboards are not supported yet.

## Driver

Force feedback on Moza bases comes from
[hid-universal-pidff](https://github.com/JacKeTUs/universal-pidff), part of Linux since 6.15. On
older kernels SimRig builds it with DKMS when the base is added. Either way, SimRig adds the
access rules for the base's serial port, where its settings travel.

## What you can set

- **Rotation**, from 90° to 2700°, and **force feedback strength**.
- **The base's settings**: torque output, maximum wheel speed, natural damper, friction and
  inertia, centering spring, road sensitivity, the strength of the games' friction, inertia,
  spring and damper effects.
- **Protection**: hands-off protection, steering wheel inertia, speed damping and the speed it
  starts from, end stop stiffness.
- **Set center**: hold the wheel straight and its position becomes the center.
- **Test force feedback**.

The base stores what it is sent, so settings come back with the base, with SimRig closed. The
serial protocol comes from [Boxflat](https://github.com/Lawstorant/boxflat), credited in
SimRig's third-party notices.

## Pedals, handbrake and shifters

- **Pedals** on the base and SR-P and CRP pedals are recognized with their layout; mapping them
  by hand is offered too.
- **HBP handbrake**: its position, live.
- **SGP sequential shifter** and **HGP H-pattern shifter** on their own USB are recognized. An
  HGP plugged into the base appears by itself once a gear is engaged; an SGP plugged into the
  base is set up by moving it. If the shifter's direction was reversed in its settings, map it
  by hand.

## Limits

- The settings of Moza steering wheels (the rims) are not offered yet.
- Pedal calibration, dead zones and curves are not available through SimRig yet.
- **Reset settings** is not available.

## Other tools

[Boxflat](https://github.com/Lawstorant/boxflat) is the complete settings app for Moza on Linux,
wheels and dashboards included. It and SimRig can be used side by side. See
[other Linux tools]({{ '/alternatives/' | relative_url }}).
