---
title: Fanatec wheels on Linux
nav: Fanatec
order: 1
status: partial
tested: a CSL DD (live inputs, rotation, tuning menu)
summary: CSL DD, GT DD Pro, ClubSport DD, Podium DD1 and DD2, CSL Elite, ClubSport V2 and V2.5, their wheels and pedals
description: >-
  Use a Fanatec CSL DD, GT DD Pro, ClubSport DD, Podium DD1/DD2, CSL Elite or ClubSport wheel
  base on Linux: SimRig installs the hid-fanatecff driver and sets rotation, force feedback and
  the base's tuning menu, with the wheel and pedals live.
lede: >-
  SimRig installs the Fanatec driver for you and gives every base its tuning menu on Linux:
  rotation, force feedback, the wheel's paddles and the pedals, live.
card: { label: "Wheel base", art: [base.svg], driver: "hid-fanatecff, built with DKMS", tested: "CSL DD" }
---

## Models

| Type | Models |
|---|---|
| Wheel bases | CSL DD, GT DD Pro, ClubSport DD, CSL Elite, ClubSport V2 and V2.5, Podium DD1 and DD2, CSR Elite, Porsche 911 Turbo S |
| Steering wheels | Those on the bases above. Known by name: CSL P1 V2, CSL Elite WRC, McLaren GT3, ClubSport F1 IS, Formula V2, Podium 911 GT3 R |
| Pedals | Pedals plugged into the base; CSL Elite Pedals, CSL Pedals (load cell), ClubSport Pedals V3 on their own USB |
{: .table-scroll}

## Driver

Fanatec bases need [hid-fanatecff](https://github.com/gotzl/hid-fanatecff), an open source
kernel driver by gotzl and contributors. SimRig bundles it at a pinned version and builds it for
your kernel with DKMS when you add the base, so it is rebuilt after every kernel update. That
needs DKMS, a compiler and your kernel's headers: SimRig says what is missing.

If you installed hid-fanatecff yourself (from the AUR, by hand), SimRig uses yours as it is and
only adds its access rules.

## What you can set

- **Rotation**, from 90° up to the base's maximum (2520° on the CSL DD and Podium bases, 1080°
  on the CSL Elite, 900° on the CSR Elite and ClubSport V2), kept as the driver's start-up
  rotation.
- **Force feedback strength**, the natural damper, friction and inertia, and the strength of
  the games' spring and damper effects.
- **The tuning menu** of the base: force effect strength and intensity, force feedback scaling,
  interpolation filter, full force, drift mode, sensitivity, brake force of load cell pedals,
  the brake level indicator, and the tuning slot (1 to 5). **Reset settings** puts the current
  slot back to its defaults.
- **The steering wheel**: the analog paddles' mode (clutch bite point, clutch and handbrake,
  brake and throttle, or analog axes) and vibration. The rim's buttons are shown by name.
- **Test force feedback**, to check the base turns before opening a game.

## Limits

- The settings stay disabled until the base sends its tuning menu: if they do, open the tuning
  menu on the wheel and change any value.
- Centering is done from the base's own menu, not from SimRig.
- The rim's LEDs and display are driven by games.
- Fanatec pedals calibrate themselves; dead zones and curves are set in the game.

## Other tools

[Oversteer](https://github.com/berarma/oversteer) also works with hid-fanatecff for rotation and
force feedback once the driver is installed. See [other Linux tools]({{ '/alternatives/' | relative_url }}).
