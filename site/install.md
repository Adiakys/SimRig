---
title: Install SimRig on Linux
heading: Install SimRig
permalink: /install/
description: >-
  Install SimRig, the sim-racing rig app for Linux, on Debian, Ubuntu, Linux Mint, Pop!_OS,
  Fedora, openSUSE, Arch, Manjaro, EndeavourOS or any other distribution.
lede: >-
  Packages for the main distributions, and a tarball for the rest. Updates are offered in the
  app itself.
---

<a class="button" href="{{ site.download }}">Download the latest release</a>

Download the package for your distribution from the [latest release]({{ site.download }}), then
install it:

| Distribution | Package | Install with |
|---|---|---|
| Debian, Ubuntu, Linux Mint, Pop!_OS | `simrig_*_amd64.deb` | `sudo apt install ./simrig_*_amd64.deb` |
| Fedora | `simrig-*.x86_64.rpm` | `sudo dnf install ./simrig-*.x86_64.rpm` |
| openSUSE | `simrig-*.x86_64.rpm` | `sudo zypper install ./simrig-*.x86_64.rpm` |
| Arch, Manjaro, EndeavourOS, CachyOS | `simrig-*-x86_64.pkg.tar.zst` | `sudo pacman -U ./simrig-*-x86_64.pkg.tar.zst` |
| Any other | `simrig-*-x86_64-linux.tar.gz` | see below |
{: .table-scroll}

On any other distribution:

```sh
tar xzf simrig-*-x86_64-linux.tar.gz && sudo ./simrig-*-x86_64-linux/install.sh
```

Then open **SimRig** from your applications. The first setup checks your system, finds your
devices and installs what they need.

## Requirements

- An x86-64 Linux with polkit (`pkexec`).
- Drivers built for your kernel (like Fanatec's or Thrustmaster's) also need DKMS, a compiler
  and your kernel's headers. SimRig says what is missing for the devices you have.

## Updates

SimRig shows a new release in its top bar and installs it with one click and your password.
Pre-releases are offered when chosen in Settings.

## Uninstall

Remove the package as usual. The drivers SimRig installed are kept, so your devices keep
working. To remove them too, do it first from **Plugins** in the app, or afterwards with
`sudo /var/lib/simrig/remove-drivers --all` (on Debian and Ubuntu, `apt purge simrig` does it).

## Why not Flatpak?

A Flatpak sandbox cannot install kernel drivers or the rules that give games access to the
devices, which is most of what SimRig does for a rig. SimRig is shipped as native packages for
that reason.

## From the sources

To build SimRig yourself, see [docs/development.md]({{ site.repo }}/blob/main/docs/development.md).
