---
title: Moza wheel bases, pedals and shifters on Linux
nav: Moza
order: 2
status: partial
tested: an R21 Ultra base, a CS Pro steering wheel, CRP2 pedals, an HBP handbrake and an SGP shifter
summary: Wheel bases R3 to R21 and the steering wheel on them, SR-P and CRP pedals, HBP handbrake, SGP and HGP shifters
description: >-
  Use a Moza R3, R5, R9, R12, R16 or R21 wheel base on Linux with force feedback and base
  settings, the steering wheel on it with its buttons and settings, SR-P and CRP pedals, the HBP
  handbrake and the SGP and HGP shifters. Tested on an R21 Ultra and a CS Pro wheel.
lede: >-
  Moza bases on Linux with their force feedback and settings, the steering wheel on them, and
  Moza pedals, handbrake and shifters, in one rig.
card: { label: "Wheel base", art: [base.svg], driver: "hid-universal-pidff, in Linux from 6.15", tested: "R21 Ultra, CS Pro, CRP2, HBP, SGP" }
---

## Models

| Type | Models |
|---|---|
| Wheel bases | R3, R5, R9, R12, R16, R21 |
| Steering wheels | The wheel mounted on the base, CS Pro tested |
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

## The steering wheel

The wheel mounted on the base shows up with its buttons, hat switches and analog paddles, live,
and its settings:

- **Analog paddle mode**: buttons, a combined axis, or separate axes, with the **combined axis
  split** (the clutch point).
- **LED brightness** for the RPM lights, and **button LED brightness**.
- **Stick as directional pad**, and each **rotary encoder** as buttons or a knob.

## Pedals, handbrake and shifters

- **Pedals** on the base and SR-P and CRP pedals are recognized with their layout; mapping them
  by hand is offered too.
- **HBP handbrake**: its position, live.
- **SGP sequential shifter** and **HGP H-pattern shifter**, on their own USB or plugged into the
  base. Plugged into the base, they and the HBP show up by themselves within a second or two, and
  go when unplugged. The SGP brings its extra buttons, and its up and down follow its direction
  setting.
- Mapping by hand stays on offer for any of them that responds wrong.

## Limits

- Pedal calibration, dead zones and curves are not available through SimRig yet.
- **Reset settings** is not available.

## Other tools

[Boxflat](https://github.com/Lawstorant/boxflat) is the complete settings app for Moza on Linux,
wheels and dashboards included. It and SimRig can be used side by side. See
[other Linux tools]({{ '/alternatives/' | relative_url }}).
