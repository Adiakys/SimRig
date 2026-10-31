---
title: SimRig, Oversteer, Boxflat and Pitwall compared
heading: Other Linux tools for sim racing
permalink: /alternatives/
description: >-
  How SimRig compares with Oversteer, Boxflat and Pitwall, the other open source apps for
  racing wheels on Linux, and when each one is the better choice.
lede: >-
  SimRig is not the only open source app for racing wheels on Linux. Here is what each one is
  for, so you can pick what fits your rig.
---

| | SimRig | Oversteer | Boxflat | Pitwall |
|---|---|---|---|---|
| Brands | Fanatec, Moza, Logitech, Thrustmaster, PXN, Simagic, standard force feedback bases, any USB shifter or handbrake | Logitech, Thrustmaster, Fanatec (with their drivers) | Moza | PXN VD series |
| Installs the drivers | Yes, with one password | No | No (not needed on Linux 6.15 and later) | No |
| Several brands in one rig | Yes, with presets for the whole rig | One wheel at a time | Moza devices | One base |
| Brand's own settings | Moza bases, PXN VD bases, Fanatec tuning menu, Simagic bases | What the kernel driver offers | The whole Moza range, wheels and dashboards included | The whole VD base, pedal curves, lights, button map |
{: .table-scroll}

## Oversteer

[Oversteer](https://github.com/berarma/oversteer) is the long-standing steering wheel manager
for Linux: rotation, force feedback gain and autocenter, combined pedals, profiles, and tools to
test the wheel. It works with the drivers already on your system and does not install any. It
is packaged for Flathub, the AUR and others.

**Choose Oversteer** when you want its force feedback meter, its range overlay or its
performance tests. **Choose SimRig** when the wheel needs a driver you do not have yet, or the
rig mixes devices and brands.

## Boxflat

[Boxflat](https://github.com/Lawstorant/boxflat) is the complete settings app for Moza hardware
on Linux: base, wheels, pedals, shifters, handbrake, dashboard and more. SimRig's Moza plugin
speaks the serial protocol that Boxflat worked out, and credits it.

**Choose Boxflat** for the settings of Moza wheels and dashboards, which SimRig does not offer
yet. **Choose SimRig** when the Moza devices share the rig with other brands, or on a kernel
older than 6.15, where SimRig builds the force feedback driver for you.

## Pitwall

[Pitwall](https://github.com/unionyxx/pitwall) is a configurator for PXN VD bases: every base
setting, a road feel equalizer, pedal response curves, lights and a button map. SimRig's PXN
plugin uses the protocol Pitwall worked out, and credits it.

**Choose Pitwall** for the deepest control of a PXN VD base. **Choose SimRig** for the other PXN
wheels (V10, V12) or a rig with more than one brand.

## Using them together

They can live side by side: all of them talk to the same kernel drivers, and settings saved in
the device stay there whichever app wrote them. Close one before changing the same setting in
another.
