---
name: SimRig
description: Configure a whole sim-racing rig on Linux, any brand, from one place.
colors:
  light-bg: "#f7f6f3"
  light-surface: "#fcfcf9"
  light-raised: "#efeee9"
  light-line: "#dbd9d4"
  light-line-strong: "#b9b7b1"
  light-text: "#201e18"
  light-text-2: "#5a5851"
  light-text-3: "#73716b"
  light-accent-ink: "#8a6500"
  light-accent-edge: "#c7a01e"
  dark-bg: "#151411"
  dark-surface: "#1d1c18"
  dark-raised: "#272521"
  dark-line: "#34332e"
  dark-line-strong: "#4f4d46"
  dark-text: "#eeede8"
  dark-text-2: "#b0aea7"
  dark-text-3: "#88867f"
  chrome: "#f3c726"
  chrome-dark: "#efc63a"
  on-chrome: "#231e0d"
  light-throttle: "#1b9247"
  light-brake: "#cc3430"
  light-clutch: "#1f74bf"
  light-warning: "#cc6600"
  light-error: "#c9302d"
  light-success: "#2d8949"
  dark-throttle: "#55c975"
  dark-brake: "#f66d62"
  dark-clutch: "#67aaed"
  dark-warning: "#fc9e47"
  dark-error: "#f66d62"
  dark-success: "#62c37a"
  device-body: "#303236"
  device-edge: "#5d6268"
  device-metal: "#9aa0a6"
typography:
  title:
    fontFamily: "Barlow, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.01em"
  heading:
    fontFamily: "Barlow, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.25
  body:
    fontFamily: "Barlow, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.45
  small:
    fontFamily: "Barlow, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: "Barlow, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "0.09em"
  value-xl:
    fontFamily: "Martian Mono, monospace"
    fontSize: "72px"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "-0.02em"
  value-l:
    fontFamily: "Martian Mono, monospace"
    fontSize: "40px"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "-0.02em"
  value-m:
    fontFamily: "Martian Mono, monospace"
    fontSize: "20px"
    fontWeight: 500
    lineHeight: 1.1
rounded:
  xs: "4px"
  sm: "6px"
  md: "10px"
  lg: "14px"
  full: "999px"
spacing:
  2xs: "4px"
  xs: "8px"
  sm: "12px"
  md: "16px"
  lg: "24px"
  xl: "32px"
  2xl: "48px"
  3xl: "64px"
components:
  button-primary:
    backgroundColor: "{colors.chrome}"
    textColor: "{colors.on-chrome}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "10px 18px"
    height: "40px"
  button-secondary-light:
    backgroundColor: "{colors.light-raised}"
    textColor: "{colors.light-text}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "40px"
  button-secondary-dark:
    backgroundColor: "{colors.dark-raised}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.sm}"
    padding: "10px 16px"
    height: "40px"
  topbar-light:
    backgroundColor: "{colors.light-surface}"
    textColor: "{colors.light-text}"
    height: "44px"
  topbar-dark:
    backgroundColor: "{colors.dark-surface}"
    textColor: "{colors.dark-text}"
    height: "44px"
  input-light:
    backgroundColor: "{colors.light-surface}"
    textColor: "{colors.light-text}"
    rounded: "{rounded.sm}"
    height: "40px"
  input-dark:
    backgroundColor: "{colors.dark-raised}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.sm}"
    height: "40px"
---

# Design System: SimRig

## 1. Overview

**Creative North Star: "The Timing Board"**

A race timing board tells everyone the one thing that matters, from far away, in a typeface you
can trust, and nothing else. SimRig works the same way. The home is the rig itself, drawn as it
stands in the room, with each device moving live and one big number next to it. Everything else
(chrome, settings, procedures) is quiet graphite and paper around those numbers, until the user
asks for more.

The system is **Restrained**: tinted neutrals carry almost every surface, and a single chrome
yellow marks what to press next and what is selected. The telemetry colours of the pedals
(throttle green, brake red, clutch blue) are data, not decoration, and never stand alone. Density
is low by default and grows only inside the panels an expert opens. Both themes are first-class:
the dark one for the dim room at the rig, the light one for the desk.

It explicitly rejects the anti-references of PRODUCT.md: the **aggressive RGB gamer** look (neon,
glows, cut corners, fake carbon fibre), the **overloaded cockpit** (dozens of numbers at once),
the **generic SaaS dashboard** (identical cards, purple accents, giant hero metrics) and the
**2000s configuration panel** (walls of tabs, checkboxes and dropdowns).

**Key Characteristics:**
- The rig is the layout: one card per device, placed like the real thing, not a uniform grid.
- One live number per device, in Martian Mono, readable from the seat.
- One accent, chrome yellow, for the next action and the current selection only.
- Unsupported items stay visible, disabled, with their reason in plain words.
- Motion only for information: devices move live; screens change in 150 to 250 ms.

**Motion.** Live device animation follows the hardware at display rate, with no easing of its
own (it is data). Interface transitions use ease-out-quart (`cubic-bezier(0.25, 1, 0.5, 1)`):
120 ms for feedback, 200 ms for state changes, 320 ms for the home-to-settings transition where
the device image travels to its new place. No bounce, no staggered page loads, no idle loops.
With reduced motion, interface transitions become instant cross-fades; live devices keep moving.

**Layout.** Fixed spacing scale on a 4 px base. Desktop windows from 1280×720 up; the home
recomposes (not shrinks) below 1280 px wide by stacking the right column under the centre.

## 2. Colors

Graphite and paper neutrals tinted towards the chrome yellow (OKLCH hue 92, chroma under 0.012),
with one accent and three data colours.

### Primary
- **Chrome Yellow** (oklch(84.5% 0.165 92), `chrome`): primary buttons, the selected item,
  focus rings in the dark theme, the engaged gear. Text on it is always **On-Chrome** ink
  (contrast 10.3:1). In the light theme its edge is drawn with **Chrome Edge** so a yellow button
  never dissolves into the paper, and accent text uses **Chrome Ink** (oklch(53% 0.115 85),
  4.9:1).

### Secondary
- **Telemetry Green** (`light-throttle` / `dark-throttle`): the throttle, and success.
- **Telemetry Red** (`light-brake` / `dark-brake`): the brake, and errors.
- **Telemetry Blue** (`light-clutch` / `dark-clutch`): the clutch.

### Tertiary
- **Pit Orange** (`light-warning` / `dark-warning`): warnings only, always with the triangle
  icon. Never used for a primary action, so it cannot be mistaken for the chrome accent.

### Neutral
- **Paper / Night** (`light-bg` / `dark-bg`): the canvas. Dark is oklch(19%), never black.
- **Sheet** (`light-surface` / `dark-surface`): top bar, side panels, inputs on paper.
- **Raised** (`light-raised` / `dark-raised`): secondary buttons, hovered zones, the selected row.
- **Hairline** (`light-line` / `dark-line`): 1 px dividers between groups.
- **Rule** (`light-line-strong` / `dark-line-strong`): control outlines, slider tracks.
- **Ink** (`light-text` / `dark-text`): primary text and live values (15:1 and more).
- **Ink 2** (`light-text-2` / `dark-text-2`): secondary text, reasons of unsupported items (6:1 and more).
- **Ink 3** (`light-text-3` / `dark-text-3`): captions and placeholders (4.5:1).
- **Device graphite** (`device-body`, `device-edge`, `device-metal`): the device drawings. One
  palette for both themes: dark bodies with a lighter edge read on paper and on night alike.

### Named Rules
**The One Signal Rule.** Chrome yellow is used on at most 10% of any screen and always means
"this is selected" or "do this next". If two yellow things compete on a screen, one is wrong.

**The Never Colour Alone Rule.** Every pedal bar carries its name; every state carries an icon or
a word. A colour-blind user reads the whole rig without the data colours.

## 3. Typography

**Body Font:** Barlow (with system-ui, sans-serif)
**Label/Mono Font:** Martian Mono (with monospace), live values only

**Character:** Barlow comes from road signage, low-contrast and slightly condensed, so it sounds
technical and sporty without shouting. Martian Mono, used narrow (width 87.5), gives live values
fixed-width digits that do not jitter while they change, like a timing board.

### Hierarchy
- **Title** (600, 28 px, 1.15): screen titles ("Wheel base", "Add a device"). One per screen.
- **Heading** (600, 20 px, 1.25): section headings inside settings and flows.
- **Body** (400, 16 px, 1.45): every label and paragraph; prose capped at 65ch.
- **Small** (400, 14 px, 1.4): helper text, reasons, secondary metadata.
- **Label** (600, 12 px, 0.09em, uppercase): captions over live values ("STEERING", "BRAKE") and
  group eyebrows. Never longer than three words.
- **Value XL** (Martian Mono 600, 72 px): the engaged gear.
- **Value L** (Martian Mono 600, 40 px): the steering angle, the main value of a device.
- **Value M** (Martian Mono 500, 20 px): pedal percentages, secondary live values, slider readouts.

### Named Rules
**The Mono Is Data Rule.** Martian Mono appears only on values that come from a device or go to
it. Never on labels, buttons or headings.

## 4. Elevation

Flat by default. Depth comes from tonal layers (Paper, Sheet, Raised), lighter as they rise in the
dark theme. Shadows exist only for things that float over content: menus, the preset picker,
toasts.

### Shadow Vocabulary
- **Float** (`box-shadow: 0 12px 32px -12px rgba(32,30,24,0.28), 0 2px 6px -2px rgba(32,30,24,0.12)`):
  menus and pickers in the light theme. In the dark theme the same layers use a 1 px Rule border
  and no visible shadow.

### Named Rules
**The Quiet Card Rule.** Each device on the home sits in its own card: Sheet fill, 1 px Hairline,
14 px radius, 24 px apart. Cards never nest and never carry shadows; a hovered or focused card
turns Raised with a Rule border, to say "this opens".

## 5. Components

### Buttons
- **Shape:** gently squared (6 px).
- **Primary:** Chrome Yellow with On-Chrome text, 40 px high, 18 px side padding; at most one per
  view. In the light theme a 1 px Chrome Edge outline.
- **Hover / Focus:** hover darkens the fill by 4% lightness in 120 ms; focus shows a 2 px ring
  2 px outside the button (Chrome Ink in light, Chrome Yellow in dark). Active presses 1 px down.
- **Secondary:** Raised fill, Ink text, 1 px Rule outline in the light theme.
- **Quiet:** text only with an icon, for the top bar and inline actions; Raised on hover.
- **Add actions:** never a bare "+" icon. The icon names what is added (a wheel for a device, a
  list for a preset) with a small plus at its bottom right.
- **Privileged:** a button that will ask for the password carries a small lock icon and, below
  it, one line saying why ("Asks for your password once to install the driver").

### Chips
- **Style:** 28 px high, full radius, Raised fill, Small text. Used for persistence ("Saved in
  the base", "Until unplugged") and for the plugin a component belongs to.
- **State:** a selected filter chip uses Chrome Yellow fill; status chips never do.

### Cards / Containers
- **Corner Style:** 10 px for panels, 14 px for the step container of flows.
- **Background:** Sheet on Paper (light), Surface on Night (dark).
- **Shadow Strategy:** none at rest (see Elevation).
- **Border:** 1 px Hairline.
- **Internal Padding:** 24 px; 16 px between rows inside.

### Inputs / Fields
- **Style:** 40 px, 6 px radius, Sheet fill and Rule outline (light) or Raised fill (dark).
- **Slider:** 4 px Rule track, Ink fill to the value, 18 px thumb; the value sits at the right in
  Value M with its unit; optional quick-value chips under it (360°, 540°, 900°, 1080°).
- **Segmented control:** for 2 to 4 exclusive choices (6+R / 7+R, Analog / Button).
- **Focus:** the same 2 px ring as buttons.
- **Error / Disabled:** errors show a red outline and the message under the field with the error
  icon. Unsupported items are rendered disabled (Ink 3 controls) with the reason in Ink 2 right
  under the label, never in a tooltip only.

### Navigation
- **Rig header:** above the device cards, a single line: "Rig", the preset picker (one click to
  switch preset) and its state on the left, the overall device state on the right. No bottom bar.
- **Top bar:** 44 px, Sheet, Hairline below. Left: the mark and "SimRig". Right: three Quiet
  buttons with icon and label: Settings, Plugins, and **Add device**, the only one with a Chrome
  outline because it is the way into the most important flow.
- **Back:** always in the same place: a Quiet back button at the top left, on its own row right
  under the top bar, its icon aligned with the logo ("Rig" from a settings screen or a flow,
  "Back" between steps). The row may hold the step indicator on the right. Never at the bottom.
  Esc goes back.
- **Flows:** a step header ("Step 2 of 4 · Driver") instead of tabs; one primary action at the
  bottom right, secondary at the bottom left.

### The Live Device (signature component)
The core's generic drawings (`assets/devices`), animated from live inputs: the rim turns with the
steering angle, the pedals stay still (their travel is in the pedal bars next to them), the shifter knob sits on
the engaged slot of a gate drawn from the configuration (6+R, 7+R, sequential), the handbrake
lever turns on its pivot. Only the pedals the component declares are drawn. Next to each drawing
sits one **Live Value** (Label caption, Value L or XL) and at most two secondary values. A device
whose plugin provides its own images uses them with the same animation rules.

### Chips and report link of a device
The chips at the top of a device's settings say only what needs attention: the driver when
something is to be done (not installed, needs a restart, an update; Pit Orange with the triangle)
and, for a model not confirmed on real hardware, **Untested** (neutral). Nothing that is shown
elsewhere (the plugin's name is under the title, the firmware among the live values) and nothing
that only says all is well ("Driver active", "Tested"). At the right end of the same row, a Quiet button with the
external-link icon, **Report how it works**, opens the filled-in hardware report; its tooltip
says what the report holds. Nothing pops up to ask for a report.

### Inputs view
A steering wheel's screen has two views, **Settings** and **Inputs**, picked with a segmented
control at the top of the right column, so the rim's controls never mix with its settings. The
Inputs view shows only what the rim sends (steering and pedals have their own screens): a line
of counts (Label caption, Value M), the **last input** in a Raised band (its number in Value M
mono, its name, its kernel code in Ink 3), then the buttons as a grid of numbered tiles (Chrome
Yellow with a dot while pressed, a 1 px Ink 2 outline on the last one, a Pit Orange outline and
"held 41 s" under a button held over 30 s, with a warning notice), the hat switch as four arms,
the encoders with their steps, and the analog paddles as bars. Numbers are the ones games read.
On the home, the control just used shows for 1.5 s under the steering value as a pill (a Chrome
dot, its number, its name), fading in and out in 200 ms.

## 6. Do's and Don'ts

### Do:
- **Do** place device cards like the physical rig: base and wheel at the top, pedals below,
  shifter and handbrake on the right, with 24 px between cards.
- **Do** keep the preset picker in the rig header, one click away.
- **Do** keep one primary (Chrome Yellow) action per view, bottom right in flows.
- **Do** write the reason of every unsupported item in plain words under its label, in Ink 2.
- **Do** say before asking: one line explaining why the password is needed, right above the button.
- **Do** use Martian Mono for live and device values, Barlow for everything else.
- **Do** check both themes for every screen; live values must reach 7:1 on their background.

### Don't:
- **Don't** repeat information on a screen, or add a chip or badge that only says all is well
  ("Active", "Tested", "OK"): status appears when something needs attention.
- **Don't** use the **aggressive RGB gamer** look: no neon, no glows, no cut corners, no fake
  carbon fibre, no RGB gradients.
- **Don't** build an **overloaded cockpit**: no more than one big value and two small ones per
  device on the home.
- **Don't** make a **generic SaaS dashboard**: no identical card grids, no purple, no giant hero
  metrics with sparkline decorations.
- **Don't** fall back to the **2000s configuration panel**: no walls of tabs, checkboxes and
  dropdowns; group settings in short sections with Essentials first and Advanced collapsed.
- **Don't** hide unsupported items, and don't use colour alone for any state.
- **Don't** open modals for flows: adding a device and the first run are full screens.
- **Don't** use border stripes thicker than 1 px as accents, gradient text or glass panels.
- **Don't** use em dashes in interface copy.
