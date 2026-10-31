---
title: Cammus, Asetek, VRS and other direct drive bases on Linux
nav: Other wheel bases
order: 7
status: untested
summary: Cammus C5 and C12, Asetek, VRS DirectForce Pro, FFBeast, Turtle Beach VelocityOne Race, OpenFFBoard
description: >-
  Force feedback on Linux for Cammus C5 and C12, Asetek Invicta, Forte, La Prima and Tony Kanaan,
  VRS DirectForce Pro, FFBeast, Turtle Beach VelocityOne Race and OpenFFBoard wheel bases, set
  up by SimRig.
lede: >-
  Wheel bases that speak the standard force feedback protocol work on Linux with one driver.
  SimRig sets it up, shows them live and sets their strength.
card: { label: "Wheel base", art: [base.svg], driver: "hid-universal-pidff, in Linux from 6.15", tested: "Not yet" }
---

## Models

| Brand | Models |
|---|---|
| Cammus | C5, C12 |
| Asetek | Invicta, Forte, La Prima, Tony Kanaan |
| VRS | DirectForce Pro |
| Turtle Beach | VelocityOne Race |
| Open source | FFBeast, OpenFFBoard |
{: .table-scroll}

## Driver

These bases use the USB standard for force feedback (PID). Linux drives them with
[hid-universal-pidff](https://github.com/JacKeTUs/universal-pidff), part of the kernel since
6.15; on older kernels SimRig builds it with DKMS. The OpenFFBoard needs nothing more than the
kernel's own driver.

## What you can set

- **Force feedback strength** and the **centering spring**, through the same interface games
  use (a game can change them too).
- **Test force feedback**, to check the base before opening a game.
- The wheel, the pedals plugged into the base and every button, live.

## Limits

- Rotation, centering and the rest of the base's settings stay in its own software or on the
  base: SimRig cannot change them. The steering angle is shown over a typical rotation (1080°
  for Cammus and Turtle Beach, 900° for the others).
- Pedals on the base are set up by moving them.
- The driver does not say which rim is mounted.
