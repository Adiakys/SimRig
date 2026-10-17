// Renders the core's generic device images with their live state, following
// assets/devices/devices.toml. Used by the mockups; the Slint app implements the same rules.
const DEVICES = {
  pedals: { view: [600, 380], pedal: { size: [120, 300], hinge: [60, 290] },
            hinges: { 3: { clutch: [175, 290], brake: [300, 290], throttle: [425, 290] },
                      2: { brake: [238, 290], throttle: [362, 290] }, 1: { throttle: [300, 290] } } },
  shifter: { view: [320, 320], gate: { o: [56, 264], u: [208, 0], v: [0, -208] }, knob: { size: [100, 100] } },
  handbrake: { view: [320, 380], pivot: [170, 236], pulledAngle: 34 },
};

/** Slots of an H-pattern gate: {label, a, b}, plus the columns. */
function hGate(gears, reverse) {
  const pairs = Math.ceil(gears / 2);
  let cols, offset = 0, rev;
  if (reverse === 'left_up') { cols = pairs + 1; offset = 1; rev = { col: 0, up: true }; }
  else if (gears % 2 === 1) { cols = pairs; rev = { col: pairs - 1, up: false }; }
  else { cols = pairs + 1; rev = { col: pairs, up: false }; }
  const a = c => (c + 0.5) / cols;
  const slots = [];
  for (let g = 1; g <= gears; g++) {
    const col = Math.floor((g - 1) / 2) + offset;
    slots.push({ label: String(g), col, a: a(col), b: g % 2 ? 0.92 : 0.08 });
  }
  slots.push({ label: 'R', col: rev.col, a: a(rev.col), b: rev.up ? 0.92 : 0.08 });
  return { cols, slots, a };
}

function mapGate(g, a, b) {
  return [g.o[0] + a * g.u[0] + b * g.v[0], g.o[1] + a * g.u[1] + b * g.v[1]];
}

/** SVG overlay for a shifter: gate, labels and knob on the engaged gear ('N' for neutral). */
function shifterOverlay(assets, gears, reverse, engaged, accent) {
  const d = DEVICES.shifter, g = d.gate, gate = hGate(gears, reverse);
  const P = (a, b) => mapGate(g, a, b).map(n => n.toFixed(1)).join(' ');
  let paths = `M${P(gate.a(0), 0.5)}L${P(gate.a(gate.cols - 1), 0.5)}`;
  for (let c = 0; c < gate.cols; c++) {
    const inCol = gate.slots.filter(s => s.col === c);
    const top = inCol.some(s => s.b > 0.5) ? 0.92 : 0.5, bottom = inCol.some(s => s.b < 0.5) ? 0.08 : 0.5;
    paths += `M${P(gate.a(c), top)}L${P(gate.a(c), bottom)}`;
  }
  const slot = gate.slots.find(s => s.label === engaged);
  const at = slot ? mapGate(g, slot.a, slot.b) : mapGate(g, 0.5, 0.5);
  const k = d.knob.size[0];
  const labels = gate.slots.map(s => {
    const p = mapGate(g, s.a, s.b > 0.5 ? 1.065 : -0.105);
    const on = s.label === engaged;
    return `<text x="${p[0].toFixed(1)}" y="${(p[1] + 4).toFixed(1)}" text-anchor="middle" font-family="Martian Mono" font-size="19" font-weight="${on ? 700 : 500}" fill="${on ? accent : '#9aa0a6'}">${s.label}</text>`;
  }).join('');
  return `<svg viewBox="0 0 ${d.view[0]} ${d.view[1]}" class="layer" aria-hidden="true">
    <path d="${paths}" fill="none" stroke="#141516" stroke-width="14" stroke-linecap="round" stroke-linejoin="round"/>
    <path d="${paths}" fill="none" stroke="#5d6268" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.8"/>
    ${labels}
    <image href="${assets}shifter-knob.svg" x="${(at[0] - k / 2).toFixed(1)}" y="${(at[1] - k / 2).toFixed(1)}" width="${k}" height="${k}"/>
  </svg>`;
}

/** Pedal layers over the frame, for the pedals the component declares (they do not move). */
function pedalLayers(assets, values) {
  const d = DEVICES.pedals, s = d.pedal;
  const roles = ['clutch', 'brake', 'throttle'].filter(r => r in values);
  const hinges = d.hinges[roles.length];
  return roles.map(role => {
    const h = hinges[role];
    const box = `left:${(h[0] - s.hinge[0]) / d.view[0] * 100}%;top:${(h[1] - s.hinge[1]) / d.view[1] * 100}%;width:${s.size[0] / d.view[0] * 100}%;height:${s.size[1] / d.view[1] * 100}%`;
    return `<img class="pedal" src="${assets}pedal.svg" alt="" style="${box}">`;
  }).join('');
}

/** Rotation of the handbrake lever for a pull of 0..1. */
function handbrakeTransform(pull) {
  const d = DEVICES.handbrake;
  return `transform-origin:${d.pivot[0] / d.view[0] * 100}% ${d.pivot[1] / d.view[1] * 100}%;transform:rotate(${(pull * d.pulledAngle).toFixed(1)}deg)`;
}
