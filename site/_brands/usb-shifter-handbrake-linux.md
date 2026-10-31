---
title: USB shifters and handbrakes on Linux
nav: USB shifters and handbrakes
order: 8
status: full
summary: Any USB shifter (H-pattern or sequential) or handbrake, even without a brand or rejected by the kernel
description: >-
  Make any USB H-pattern shifter, sequential shifter or handbrake work on Linux, even an
  unbranded one or one the kernel rejects: SimRig maps it by moving it and presents it to every
  game as a proper device.
lede: >-
  An unbranded USB shifter or handbrake, or one Linux does not see right, set up by moving it.
  Tested on an H-pattern shifter and a handbrake.
card: { label: "Shifter", art: [shifter-plate.svg, shifter-knob.svg], driver: "SimRig's generic USB service", tested: "H-pattern shifter, handbrake" }
---

Many shifters and handbrakes sold for sim racing are generic USB devices: no brand, no
software for Linux, sometimes a USB descriptor so broken that the kernel rejects them. SimRig's
generic USB plugin sets them up by watching them move, then presents them to games as a clean
joystick.

## How it is set up

1. Plug it in and open **Add device** in SimRig. If it is not listed, turn on **Show every USB
   device**, then choose it.
2. Say what it is: **H-pattern shifter** (4 to 8 forward gears, reverse left-up or right-down),
   **sequential shifter** or **handbrake**.
3. Leave it at rest, so SimRig learns what to ignore.
4. Move it when asked: each gear and reverse in turn, up and down for a sequential shifter, or
   pull the handbrake fully.
5. Check the mapping, then save.

## Driver

A small service of SimRig's, started when a set-up device is plugged in, turns it into a joystick
that every game sees: "SimRig shifter", "SimRig sequential shifter" or "SimRig handbrake". It
works with SimRig closed. No kernel driver is built, so no Secure Boot step either.

## What you can set

- **Shifters**: the reverse position; the rest is changed by setting it up again.
- **Handbrakes** with an analog lever: calibration, dead zone, response curve, and the mode,
  **analog** or **button** with its threshold, for games that want a handbrake as a button. A
  handbrake with a switch works as a button.

## Limits

Pedals and wheels are not supported by this plugin yet.
