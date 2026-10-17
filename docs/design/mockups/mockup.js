// SimRig mockups: shared pieces and the screens. Static poses of the live rig.
const A = '../../../assets/devices/';
const params = new URLSearchParams(location.search);
const THEME = params.get('theme') || 'light';
document.documentElement.dataset.theme = THEME;

const ICON = {
  gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2.8l1.7 2.4 2.8-.8.8 2.8 2.4 1.7-1.3 2.6 1.3 2.6-2.4 1.7-.8 2.8-2.8-.8L12 21.2l-1.7-2.4-2.8.8-.8-2.8-2.4-1.7 1.3-2.6-1.3-2.6 2.4-1.7.8-2.8 2.8.8z"/>',
  plug: '<path d="M9 2.5v5M15 2.5v5M6 7.5h12v3.5a6 6 0 0 1-12 0zM12 17v4.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  back: '<path d="M14.5 6l-6 6 6 6"/>',
  down: '<path d="M6 9.5l6 6 6-6"/>',
  right: '<path d="M9.5 6l6 6-6 6"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  warn: '<path d="M12 4l9 16H3z"/><path d="M12 10v4.5M12 17.4v.1"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.6v.1"/>',
  lock: '<rect x="5" y="11" width="14" height="9.5" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
  x: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>',
  usb: '<path d="M12 3v14M12 17a2.2 2.2 0 1 0 .01 0M12 7l-4 4v2M12 9.5l4-3V5M6.5 13h3v-3h-3zM15 3.5h2v2h-2z"/>',
  sliders: '<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>',
};
const icon = (name, cls = '') => `<svg class="icon ${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICON[name]}</svg>`;

const MARK = `<svg class="mark" viewBox="0 0 22 22" aria-hidden="true"><rect width="22" height="22" rx="5" fill="var(--accent)"/>
  <circle cx="11" cy="11.6" r="6.3" fill="none" stroke="var(--on-accent)" stroke-width="2.2"/>
  <path d="M4.6 11.6h12.8" stroke="var(--on-accent)" stroke-width="2.2"/><rect x="9.9" y="3.4" width="2.2" height="3.6" rx=".6" fill="var(--on-accent)"/></svg>`;

function topbar(active = '') {
  const item = (id, ic, label, extra = '') =>
    `<span class="btn quiet ${extra}" ${active === id ? 'style="background:var(--raised)"' : ''}>${icon(ic)}${label}</span>`;
  return `<header class="topbar"><div class="brand">${MARK}SimRig</div>
    ${item('settings', 'gear', 'Impostazioni')}${item('plugins', 'plug', 'Plugin')}${item('add', 'plus', 'Nuovo dispositivo', 'outline')}</header>`;
}

// ---------------------------------------------------------------- live devices
const DEV = {
  base: [380, 360], wheel: [400, 400], pedals: [600, 380], shifter: [320, 320], handbrake: [320, 380],
};
function box(kind, width, extra = '') {
  const [w, h] = DEV[kind];
  return `style="width:${width}px;height:${(width * h / w).toFixed(1)}px;${extra}"`;
}
const dev = {
  base: (w) => `<div class="dev" ${box('base', w)}><img src="${A}base.svg" alt=""></div>`,
  wheel: (w, shape, angle) => `<div class="dev" ${box('wheel', w)}><img src="${A}wheel-${shape}.svg" alt="" style="transform:rotate(${angle}deg)"></div>`,
  pedals: (w, values, cls = '') => `<div class="dev ${cls}" ${box('pedals', w)}><img src="${A}pedals-frame.svg" alt="">${pedalLayers(A, values)}</div>`,
  shifter: (w, gears, reverse, gear, cls = '') => `<div class="dev ${cls}" ${box('shifter', w)}><img src="${A}shifter-plate.svg" alt="">${shifterOverlay(A, gears, reverse, gear, getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#efc63a')}</div>`,
  handbrake: (w, pull, cls = '') => `<div class="dev ${cls}" ${box('handbrake', w)}><img src="${A}handbrake-body.svg" alt=""><img src="${A}handbrake-lever.svg" alt="" style="${handbrakeTransform(pull)}"></div>`,
};

// Degrees stay attached to the number at full size; other units are smaller.
/** The back row: always right under the top bar, aligned with the logo; `right` holds e.g. the steps. */
const backRow = (label, right = '') => `<div class="backrow">${label ? `<span class="btn quiet back">${icon('back')}${label}</span>` : ''}<span style="margin-left:auto">${right}</span></div>`;

const value = (v, unit = '', cls = 'value-l') => unit === '°' ? `<span class="${cls}">${v}°</span>` : `<span class="${cls}">${v}${unit ? `<span class="unit">${unit}</span>` : ''}</span>`;

/** Steering position within the rotation: a track with a centre tick and a marker. */
function steerTrack(angle, rotation, width = 200) {
  const x = 50 + angle / (rotation / 2) * 50;
  return `<div style="position:relative;width:${width}px;height:14px;margin-top:10px">
    <div style="position:absolute;left:0;right:0;top:6px;height:2px;background:var(--track);border-radius:1px"></div>
    <div style="position:absolute;left:50%;top:1px;width:2px;height:12px;margin-left:-1px;background:var(--rule)"></div>
    <div style="position:absolute;left:${x}%;top:0;width:4px;height:14px;margin-left:-2px;background:var(--ink);border-radius:2px"></div></div>`;
}

/** Vertical pedal bar: name, percentage, fill in the pedal's data colour. */
function pedalBar(name, pct, color) {
  return `<div style="display:grid;justify-items:center;gap:8px;width:56px">
    <span class="value-m">${pct}<span class="unit">%</span></span>
    <div style="position:relative;width:12px;height:118px;border-radius:6px;background:var(--track);overflow:hidden">
      <div style="position:absolute;left:0;right:0;bottom:0;height:${pct}%;background:var(--${color});border-radius:6px"></div></div>
    <span class="label">${name}</span></div>`;
}

// ---------------------------------------------------------------- screens
const screens = {};

function zoneHead(kind, name, plugin) {
  return `<div style="display:flex;align-items:baseline;gap:10px;flex-wrap:wrap"><span class="label">${kind}</span>
    <span style="font-weight:500">${name}</span><span class="small">${plugin}</span></div>`;
}

const card = (inner, extra = '', hover = false) =>
  `<section class="card${hover ? ' hover' : ''}" style="${extra}">${inner}</section>`;

/** The home header: where the rig stands and which preset is active. */
const rigHeader = (preset, status) => `<div style="grid-column:1 / -1;display:flex;align-items:center;gap:16px">
  <h1 class="heading">Rig</h1>
  <span class="preset">${preset ? `<span class="label">Preset</span>${preset}` : '<span class="label">Preset</span><span class="ink3">Nessuno</span>'}${icon('down', 'sm')}</span>
  ${preset ? '<span class="small">Applicato e salvato nei driver</span>' : ''}
  <span style="margin-left:auto">${status}</span></div>`;

screens.home = () => `<div class="app" style="grid-template-rows:44px 1fr">${topbar()}
  <main style="display:grid;grid-template-columns:0.9fr 1.3fr 1fr;grid-template-rows:auto 1fr 0.92fr;gap:24px;padding:24px 32px 32px;min-height:0">
    ${rigHeader('GT3', `<span class="chip ok">${icon('check', 'sm')}5 dispositivi pronti</span>`)}
    ${card(`${zoneHead('Base', 'CSL DD', 'Fanatec')}
      <div style="display:grid;justify-items:center">${dev.base(196)}</div>
      <div style="display:flex;gap:24px;align-items:flex-end">
        <div><div class="label">Forza</div>${value('70', '%', 'value-m')}</div>
        <div><div class="label">Rotazione</div>${value('540', '°', 'value-m')}</div>
        <span class="chip ok" style="margin-left:auto">${icon('check', 'sm')}Pronta</span></div>`, 'display:grid;align-content:space-between')}
    ${card(`<div style="position:absolute;left:20px;top:18px">${zoneHead('Volante', 'Formula V2.5', 'Fanatec')}</div>
      <div style="display:grid;justify-items:center;padding-top:24px">${dev.wheel(270, 'formula', -38)}</div>
      <div style="padding-right:4px"><div class="label">Sterzo</div>${value('−38', '°')}${steerTrack(-38, 540, 150)}
        <div class="small" style="margin-top:8px">su 540° di rotazione</div></div>
      <span class="small" style="position:absolute;right:20px;bottom:14px;display:flex;align-items:center;gap:4px;color:var(--ink)">Apri impostazioni${icon('right', 'sm')}</span>`,
      'display:grid;grid-template-columns:1fr auto;align-items:center;gap:8px;position:relative', true)}
    ${card(`<div style="position:absolute;left:20px;top:18px">${zoneHead('Cambio', 'Shifter USB', 'Generico')}</div>
      <div style="padding-top:28px">${dev.shifter(210, 6, 'right_down', '3')}</div>
      <div style="text-align:right;padding-right:4px"><div class="label">Marcia</div>${value('3', '', 'value-xl')}<div class="small" style="margin-top:6px">H 6+R</div></div>`,
      'display:grid;grid-template-columns:1fr auto;align-items:center;position:relative')}
    ${card(`<div style="position:absolute;left:20px;top:18px">${zoneHead('Pedaliera', 'SR-P Lite', 'MOZA')}</div>
      <div style="display:grid;justify-items:center;padding-top:22px">${dev.pedals(390, { clutch: 0, brake: 0.03, throttle: 0.67 })}</div>
      <div style="display:flex;gap:8px;padding-right:8px">${pedalBar('Frizione', 0, 'clutch')}${pedalBar('Freno', 3, 'brake')}${pedalBar('Gas', 67, 'throttle')}</div>`,
      'grid-column:1 / span 2;display:grid;grid-template-columns:1fr auto;align-items:center;gap:24px;position:relative')}
    ${card(`<div style="position:absolute;left:20px;top:18px">${zoneHead('Freno a mano', 'Handbrake USB', 'Generico')}</div>
      <div style="padding-top:26px">${dev.handbrake(150, 0)}</div>
      <div style="text-align:right;padding-right:4px"><div class="label">Posizione</div>${value('0', '%')}<div class="small" style="margin-top:6px">Rilasciato</div></div>`,
      'display:grid;grid-template-columns:auto 1fr;align-items:center;gap:12px;position:relative')}
  </main></div>`;

screens['home-empty'] = () => `<div class="app" style="grid-template-rows:44px 1fr">${topbar()}
  <main style="display:grid;grid-template-rows:auto 1fr;gap:24px;padding:24px 32px 32px;min-height:0">
    ${rigHeader('', '<span class="chip">Nessun dispositivo</span>')}
    <div style="display:grid;grid-template-columns:1.1fr 1fr;align-items:center;gap:48px;padding:0 56px">
      <div style="display:grid;grid-template-columns:1fr 1fr;grid-template-rows:auto auto;gap:20px;align-items:end;justify-items:center">
        ${dev.wheel(240, 'round', 0).replace('class="dev"', 'class="dev ghost"')}${dev.shifter(170, 6, 'right_down', 'N', 'ghost')}
        ${dev.pedals(300, { clutch: 0, brake: 0, throttle: 0 }, 'ghost')}${dev.handbrake(120, 0, 'ghost')}</div>
      <div style="max-width:46ch">
        <h2 class="title">Nessun dispositivo collegato</h2>
        <p class="ink2" style="margin:12px 0 28px">Collega volante, pedaliera o un'altra periferica alla porta USB: compaiono qui da soli e si muovono quando li usi.</p>
        <div style="display:flex;gap:12px;align-items:center"><span class="btn primary">${icon('plus')}Aggiungi un dispositivo</span>
          <span class="btn quiet">È collegato ma non compare?</span></div>
        <div class="notice info" style="margin-top:32px">${icon('info')}<span>Hai già un rig configurato? Le impostazioni restano nei driver anche quando i dispositivi sono scollegati.</span></div>
      </div></div>
  </main></div>`;

function settingRow(label, control, opts = {}) {
  const { help = '', chip = '', disabled = false } = opts;
  return `<div style="display:grid;grid-template-columns:220px 1fr;gap:24px;padding:12px 0;border-top:1px solid var(--line)">
    <div><div style="font-weight:500;${disabled ? 'color:var(--ink3)' : ''}">${label}</div>${help ? `<div class="small" style="margin-top:4px">${help}</div>` : ''}</div>
    <div>${control}${chip ? `<div style="margin-top:10px">${chip}</div>` : ''}</div></div>`;
}
function slider(pct, readout, disabled = false) {
  return `<div class="slider ${disabled ? 'disabled' : ''}"><div class="track"><div class="fill" style="width:${pct}%"></div><div class="thumb" style="left:${pct}%"></div></div>
    <span class="value-m" style="min-width:84px;text-align:right;${disabled ? 'color:var(--ink3)' : ''}">${readout}</span></div>`;
}

screens.component = () => `<div class="app" style="grid-template-rows:44px auto 1fr">${topbar()}${backRow('Rig')}
  <main style="display:grid;grid-template-columns:minmax(380px,0.8fr) 1.2fr;min-height:0">
    <aside style="padding:8px 28px 20px;display:grid;grid-template-rows:auto 1fr auto;border-right:1px solid var(--line)">
      <div><h1 class="title">Base</h1>
        <div style="display:flex;gap:10px;align-items:center;margin-top:6px"><span style="font-weight:500">CSL DD</span><span class="small">Fanatec</span></div></div>
      <div style="display:grid;place-items:center">${dev.base(300)}</div>
      <div style="display:grid;grid-template-columns:auto 1fr;gap:28px;align-items:end">
        <div><div class="label">Sterzo</div>${value('−38', '°')}${steerTrack(-38, 540, 150)}</div>
        <div><div class="label">Forza attuale</div>${value('70', '%', 'value-m')}</div></div>
    </aside>
    <section style="overflow:hidden;padding:8px 40px 0;display:grid;grid-template-rows:auto 1fr auto">
      <div style="display:flex;gap:8px;flex-wrap:wrap"><span class="chip ok">${icon('check', 'sm')}Driver hid-fanatec attivo</span><span class="chip">Firmware 1.4</span></div>
      <div style="overflow:hidden;padding-top:8px">
        <h2 class="heading" style="margin:4px 0 4px">Essenziali</h2>
        ${settingRow('Rotazione', `${slider(19.8, '540°')}<div class="quick"><span>360</span><span class="on">540</span><span>900</span><span>1080</span><span>2520</span></div>`, { help: 'Quanto gira il volante da un fine corsa all’altro.', chip: '<span class="chip">Ripristinata al collegamento</span>' })}
        ${settingRow('Forza del force feedback', slider(70, '70%'), { chip: '<span class="chip">Salvata nella base</span>' })}
        ${settingRow('Smorzamento naturale', slider(12, '12%'))}
        <div style="display:flex;align-items:center;gap:10px;padding:12px 0;border-top:1px solid var(--line);font-weight:500">${icon('right', 'sm')}Avanzate<span class="small">6 regolazioni</span></div>
        <h2 class="heading" style="margin:14px 0 4px">Regolazioni Fanatec</h2>
        ${settingRow('Scala del force feedback', '<span class="seg"><span>Lineare</span><span class="on">Completa</span></span>')}
        ${settingRow('Inerzia naturale', slider(0, '–', true), { help: 'Non disponibile su questo hardware: la CSL DD non ha un filtro d’inerzia.', disabled: true })}
        <h2 class="heading" style="margin:14px 0 4px">Procedure guidate</h2>
        <div style="display:flex;gap:12px;padding:12px 0;border-top:1px solid var(--line)"><span class="btn secondary small">Imposta il centro</span><span class="btn secondary small">Prova del force feedback</span><span class="small" style="align-self:center">Il volante si muove da solo: tienilo saldo.</span></div>
      </div>
      <div style="display:flex;align-items:center;gap:16px;padding:16px 0 20px;border-top:1px solid var(--line)">
        <span class="btn quiet">Ripristina i valori</span>
        <div style="margin-left:auto;display:grid;justify-items:end;gap:6px"><span class="btn primary">${icon('lock', 'sm')}Salva come predefinito</span>
          <span class="lockline">Chiede la password una volta, per salvarlo nel sistema</span></div></div>
    </section>
  </main></div>`;

function flow(step, title, body, footer) {
  const names = ['Collega', 'Driver', 'Configura', 'Fatto'];
  const steps = names.map((n, i) => `<span class="${i + 1 < step ? 'done' : i + 1 === step ? 'cur' : ''}"><span class="n">${i + 1 < step ? '✓' : i + 1}</span>${n}</span>`).join('<span class="sep"></span>');
  return `<div class="app" style="grid-template-rows:44px auto 1fr">${topbar('add')}${backRow('Rig', `<span class="steps">${steps}</span>`)}
  <main style="display:grid;grid-template-rows:1fr auto;padding:0 28px;min-height:0">
    <div style="padding-top:8px;min-height:0"><h1 class="title">${title}</h1><div style="padding-top:40px">${body}</div></div>
    <div style="display:flex;align-items:center;gap:16px;padding:16px 0 20px;border-top:1px solid var(--line)">${footer}</div>
  </main></div>`;
}

const thumb = (inner, w = 96, h = 72) => `<div style="width:${w}px;height:${h}px;display:grid;place-items:center;border-radius:10px;background:var(--raised)">${inner}</div>`;

screens['add-1-connect'] = () => flow(1, 'Aggiungi un dispositivo', `
  <div style="display:grid;grid-template-columns:0.9fr 1.1fr;gap:64px;align-items:center">
    <div style="display:grid;justify-items:start;gap:20px">
      <div style="position:relative;width:200px;height:200px;display:grid;place-items:center">
        <div style="position:absolute;inset:0;border-radius:50%;border:1px solid var(--line)"></div>
        <div style="position:absolute;inset:30px;border-radius:50%;border:1px solid var(--rule)"></div>
        <div style="position:absolute;inset:62px;border-radius:50%;background:var(--raised)"></div>
        <span style="position:relative;color:var(--ink)">${icon('usb').replace('class="icon ', 'style="width:40px;height:40px" class="icon ')}</span></div>
      <p class="ink2" style="margin:0;max-width:40ch">Collegare il dispositivo a una porta USB. I dispositivi noti sono riconosciuti automaticamente; quelli che nessun plugin conosce si configurano come dispositivo generico.</p></div>
    <div><div class="label" style="margin-bottom:12px">Trovati ora</div>
      <div style="display:grid;grid-template-columns:auto 1fr auto;gap:16px 20px;align-items:center;padding:16px 0;border-top:1px solid var(--line)">
        ${thumb(dev.shifter(64, 6, 'right_down', 'N'))}
        <div><div style="font-weight:500">USB Game Controller</div><div class="small">USB 1234:0001 · nessun plugin lo riconosce</div></div>
        <span class="btn primary">Configura come generico</span></div>
      <div style="display:grid;grid-template-columns:auto 1fr auto;gap:16px 20px;align-items:center;padding:16px 0;border-top:1px solid var(--line)">
        ${thumb(dev.pedals(84, { clutch: 0, brake: 0, throttle: 0 }))}
        <div><div style="font-weight:500">SR-P Lite</div><div class="small">MOZA · già nel tuo rig</div></div>
        <span class="chip ok">${icon('check', 'sm')}Pronto</span></div>
      <div style="padding:16px 0;border-top:1px solid var(--line)" class="small">Collega un altro dispositivo per aggiungerlo a questa lista.</div>
    </div></div>`,
  `<span class="btn quiet">Il dispositivo non compare?</span>`);

screens['add-2-driver'] = () => flow(2, 'Serve un piccolo driver', `
  <div style="display:grid;grid-template-columns:auto 1fr;gap:56px;align-items:start;padding-top:28px">
    ${thumb(dev.shifter(150, 6, 'right_down', 'N'), 220, 200)}
    <div style="max-width:60ch">
      <p style="margin:0 0 20px">Il plugin <strong>USB generico</strong> installa un servizio che traduce i segnali di <strong>USB Game Controller</strong> in un joystick che tutti i giochi riconoscono.</p>
      <div style="display:grid;gap:14px;margin-bottom:24px">
        <div style="display:flex;gap:10px">${icon('check')}<span>Funziona anche con SimRig chiuso.</span></div>
        <div style="display:flex;gap:10px">${icon('check')}<span>Si avvia solo quando il dispositivo è collegato.</span></div>
        <div style="display:flex;gap:10px">${icon('check')}<span>Incluso nell'app: non si scarica nulla. Lo rimuovi quando vuoi.</span></div></div>
      <div class="notice info">${icon('lock')}<span><strong>La password è richiesta una volta</strong>, perché il driver si installa nel sistema. SimRig in sé non ha mai bisogno di privilegi.</span></div>
    </div></div>`,
  `<span class="btn quiet">Non ora</span><div style="margin-left:auto"><span class="btn primary">${icon('lock', 'sm')}Installa il driver</span></div>`);

screens['add-3-setup'] = () => flow(3, 'Muovi la leva in 3ª', `
  <div style="display:grid;grid-template-columns:auto 1fr;gap:64px;align-items:center">
    <div style="position:relative">${dev.shifter(340, 6, 'right_down', 'N')}
      <div style="position:absolute;left:${(56 + 0.5 / 4 * 208 + 208 / 4 * 2 - 208 / 8) / 320 * 100}%;top:${(264 - 0.92 * 208) / 320 * 100}%;width:56px;height:56px;margin:-28px 0 0 -28px;border-radius:50%;border:2px dashed var(--accent);"></div></div>
    <div style="max-width:52ch">
      <div style="display:flex;gap:12px;align-items:center"><span class="chip"><span class="dot" style="color:var(--accent)"></span>In attesa del dispositivo</span><span class="small">Marcia 3 di 7</span></div>
      <p class="ink2" style="margin:16px 0 28px">Portare la leva nella posizione evidenziata e tenerla ferma: la posizione è rilevata automaticamente e si passa alla successiva.</p>
      <table class="data"><tr><th>Posizione</th><th>Ingresso rilevato</th></tr>
        <tr><td>Marcia 1</td><td class="mono">byte 2 = 0x01</td></tr><tr><td>Marcia 2</td><td class="mono">byte 2 = 0x02</td></tr>
        <tr><td style="color:var(--ink3)">Marcia 3</td><td class="mono" style="color:var(--ink3)">in attesa</td></tr></table>
      <p class="small" style="margin-top:20px">Passo definito dal plugin USB generico.</p></div></div>`,
  `<span class="btn quiet">Annulla</span><span class="small" style="margin-left:auto">Avanza da solo quando rileva la marcia</span>`);

screens['add-4-done'] = () => flow(4, 'Il cambio è pronto', `
  <div style="display:grid;grid-template-columns:auto 1fr;gap:64px;align-items:center">
    ${dev.shifter(320, 6, 'right_down', '2')}
    <div style="max-width:50ch">
      <div style="display:flex;align-items:baseline;gap:16px"><div><div class="label">Marcia</div>${value('2', '', 'value-xl')}</div><span class="chip ok">${icon('check', 'sm')}Funziona</span></div>
      <p class="ink2" style="margin:20px 0">Prova a cambiare marcia: la vedi qui in tempo reale. Nei giochi compare come joystick con un pulsante per marcia, anche con SimRig chiuso.</p>
      <div class="notice ok">${icon('check')}<span><strong>Configurazione salvata nel driver.</strong> 6 marce più retromarcia; la cambi quando vuoi dalle impostazioni del cambio.</span></div></div></div>`,
  `<span class="btn secondary">Apri le impostazioni</span><div style="margin-left:auto"><span class="btn primary">Torna al rig</span></div>`);

function firstRun(step, body, footer) {
  return `<div class="app" style="grid-template-rows:44px auto 1fr"><header class="topbar"><div class="brand">${MARK}SimRig</div></header>
  ${backRow(step > 1 ? 'Indietro' : '', `<span class="small">Prima configurazione · ${step} di 3</span>`)}
  <main style="display:grid;grid-template-rows:1fr auto;padding:0 28px;min-height:0">${body}
    <div style="display:flex;align-items:center;gap:16px;padding:16px 0 20px;border-top:1px solid var(--line)">${footer}</div></main></div>`;
}
const sysRow = (ic, color, label, val, note = '') => `<div style="display:grid;grid-template-columns:auto 1fr auto;gap:12px;padding:12px 0;border-top:1px solid var(--line);align-items:start">
  <span style="color:var(--${color})">${icon(ic)}</span><div><div>${label}</div>${note ? `<div class="small" style="margin-top:2px">${note}</div>` : ''}</div><span class="small">${val}</span></div>`;

screens['first-1-welcome'] = () => firstRun(1, `
  <div style="display:grid;grid-template-columns:1.1fr 0.9fr;gap:80px;align-items:center">
    <div><div style="display:flex;gap:14px;align-items:center">${MARK.replace('class="mark"', 'class="mark" style="width:44px;height:44px"')}</div>
      <h1 class="title" style="font-size:40px;margin-top:28px">Configurazione del rig</h1>
      <p class="ink2" style="font-size:18px;margin:16px 0 0;max-width:44ch">Base, volante, pedali, cambio e freno a mano di qualsiasi marca. I driver necessari sono installati automaticamente e restano attivi a SimRig chiuso.</p>
      <div style="display:grid;gap:10px;margin-top:32px" class="small">
        <div style="display:flex;gap:8px;align-items:center">${icon('check', 'sm')}Funziona con qualsiasi marca, anche mischiate.</div>
        <div style="display:flex;gap:8px;align-items:center">${icon('check', 'sm')}Quando chiudi SimRig non resta niente in esecuzione.</div></div></div>
    <div><div class="label" style="margin-bottom:8px">Il tuo sistema</div>
      ${sysRow('check', 'ok', 'Ubuntu 26.04 LTS', 'ok')}${sysRow('check', 'ok', 'Kernel 7.0', 'ok')}
      ${sysRow('info', 'ink2', 'Secure Boot attivo', 'nota', 'I driver vengono firmati. Al primo riavvio può servire la conferma di una chiave, con istruzioni passo passo.')}
      ${sysRow('check', 'ok', 'Sessione Wayland', 'ok')}</div></div>`,
  `<span class="small">Tema e lingua seguono il sistema: li cambi in Impostazioni.</span><div style="margin-left:auto"><span class="btn primary">Avvia</span></div>`);

screens['first-2-devices'] = () => firstRun(2, `
  <div style="padding-top:8px"><h1 class="title">4 dispositivi trovati</h1>
    <p class="ink2" style="margin:10px 0 24px">I dispositivi collegati in seguito compaiono qui.</p>
    <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:0 56px">
      ${[['CSL DD', 'Fanatec · base', dev.base(78), '<span class="chip warn">' + icon('warn', 'sm') + 'Serve un driver</span>'],
         ['Formula V2.5', 'Fanatec · volante, montato sulla base', dev.wheel(66, 'formula', 0), '<span class="chip">Con la base</span>'],
         ['SR-P Lite', 'MOZA · pedaliera', dev.pedals(88, { clutch: 0, brake: 0, throttle: 0 }), '<span class="chip ok">' + icon('check', 'sm') + 'Pronta</span>'],
         ['USB Game Controller', 'USB 1234:0001 · non riconosciuto', '', '<span class="chip">Da configurare</span>']]
        .map(([n, s, img, st]) => `<div style="display:grid;grid-template-columns:auto 1fr auto;gap:16px;align-items:center;padding:14px 0;border-top:1px solid var(--line)">${thumb(img)}<div><div style="font-weight:500">${n}</div><div class="small">${s}</div></div>${st}</div>`).join('')}
    </div>
    <div class="notice info" style="margin-top:28px;max-width:76ch">${icon('lock')}<span><strong>Una sola password per tutti i driver.</strong> Si installano nel sistema e funzionano anche con SimRig chiuso. I dispositivi da configurare hanno poi una procedura guidata.</span></div></div>`,
  `<span class="btn quiet">Salta per ora</span><div style="margin-left:auto;display:grid;justify-items:end;gap:6px"><span class="btn primary">${icon('lock', 'sm')}Installa 1 driver</span></div>`);

const toggle = (on) => `<span style="display:inline-block;width:40px;height:24px;border-radius:12px;background:${on ? 'var(--accent)' : 'var(--rule)'};position:relative;flex:none">
  <span style="position:absolute;top:3px;left:${on ? 19 : 3}px;width:18px;height:18px;border-radius:50%;background:${on ? 'var(--on-accent)' : 'var(--surface)'}"></span></span>`;

screens['first-3-rig'] = () => firstRun(3, `
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:72px;padding-top:8px">
    <div><h1 class="title">Composizione del rig</h1><p class="ink2" style="margin:10px 0 20px">Solo ciò che l'hardware non può rilevare.</p>
      <div style="display:grid;grid-template-columns:1fr auto;gap:16px;align-items:center;padding:16px 0;border-top:1px solid var(--line)">
        <div><div style="font-weight:500">Pedali collegati alla base</div><div class="small" style="margin-top:4px">La CSL DD ha un ingresso per i pedali. Con una pedaliera separata (SR-P Lite) l'ingresso è escluso dal rig.</div></div>${toggle(false)}</div>
      <div style="display:grid;grid-template-columns:1fr auto;gap:16px;align-items:center;padding:16px 0;border-top:1px solid var(--line)">
        <div><div style="font-weight:500">Frizione</div><div class="small" style="margin-top:4px">La SR-P Lite può avere 2 o 3 pedali.</div></div><span class="seg"><span>2 pedali</span><span class="on">3 pedali</span></span></div>
      <div class="label" style="margin:28px 0 12px">Anteprima dal vivo</div>
      <div class="card" style="display:flex;align-items:flex-end;justify-content:space-around;gap:16px;padding:20px">
        ${dev.base(110)}${dev.wheel(120, 'formula', 12)}${dev.pedals(210, { clutch: 0, brake: 0.4, throttle: 0 })}${dev.shifter(104, 6, 'right_down', 'N')}</div></div>
    <div><div class="label" style="margin:6px 0 12px">Da calibrare</div>
      ${[['Pedaliera', 'Premi e rilascia ogni pedale quando richiesto.', 'Calibra', true], ['USB Game Controller', 'Scegli il tipo (cambio, freno a mano, pedaliera) e muovi i comandi quando richiesto.', 'Configura', false], ['Base', 'Centro e rotazione: va bene così per iniziare.', 'Fatto', false]]
        .map(([n, s, b, primary]) => `<div style="display:grid;grid-template-columns:1fr auto;gap:16px;align-items:center;padding:16px 0;border-top:1px solid var(--line)"><div><div style="font-weight:500">${n}</div><div class="small" style="margin-top:4px">${s}</div></div>${b === 'Fatto' ? `<span class="chip ok">${icon('check', 'sm')}Fatto</span>` : `<span class="btn ${primary ? 'secondary' : 'secondary'} small">${b}</span>`}</div>`).join('')}</div></div>`,
  `<div style="margin-left:auto"><span class="btn primary">Vai al rig</span></div>`);

function render() {
  const name = params.get('screen') || 'home';
  document.title = `SimRig mockup: ${name}`;
  document.body.innerHTML = screens[name]();
}

// ---------------------------------------------------------------- style guide board
screens['style-guide'] = () => {
  const sw = (name, varName, note) => `<div style="display:grid;gap:6px"><div style="height:56px;border-radius:10px;background:var(${varName});border:1px solid var(--line)"></div>
    <div style="font-weight:500;font-size:14px">${name}</div><div class="small" style="font-size:12px">${note}</div></div>`;
  const section = (title, inner) => `<section style="padding:28px 0;border-top:1px solid var(--line)"><div class="label" style="margin-bottom:18px">${title}</div>${inner}</section>`;
  return `<div style="padding:40px 56px 56px">
    <div style="display:flex;align-items:center;gap:16px">${MARK.replace('class="mark"', 'class="mark" style="width:40px;height:40px"')}<div><h1 class="title">SimRig</h1><div class="small">Guida di stile · tema ${THEME === 'dark' ? 'scuro' : 'chiaro'}</div></div>
      <div style="margin-left:auto;max-width:52ch" class="small">"The Timing Board": il rig al centro, un valore grande per dispositivo, grafite e carta intorno, un solo accento giallo cromo per ciò che va premuto.</div></div>
    ${section('Colori', `<div style="display:grid;grid-template-columns:repeat(8,1fr);gap:16px">
      ${sw('Cromo', '--accent', 'Azione e selezione')}${sw('Inchiostro', '--ink', 'Testo e valori')}${sw('Inchiostro 2', '--ink2', 'Testo secondario')}${sw('Inchiostro 3', '--ink3', 'Didascalie')}
      ${sw('Fondo', '--bg', 'Canvas')}${sw('Foglio', '--surface', 'Card, barre')}${sw('Rialzato', '--raised', 'Hover, secondari')}${sw('Filo', '--line', 'Divisori')}
      ${sw('Gas', '--throttle', 'Dato, e successo')}${sw('Freno', '--brake', 'Dato, ed errore')}${sw('Frizione', '--clutch', 'Dato')}${sw('Avviso', '--warn', 'Sempre col triangolo')}</div>`)}
    ${section('Caratteri', `<div style="display:grid;grid-template-columns:1.2fr 1fr;gap:48px;align-items:end">
      <div style="display:grid;gap:14px"><div class="title">Titolo · Barlow 600 28</div><div class="heading">Sezione · Barlow 600 20</div>
        <div>Testo · Barlow 400 16. Dispositivo collegato: riconosciuto automaticamente.</div><div class="small">Piccolo · Barlow 400 14, aiuti e motivi</div><div class="label">Etichetta · 600 12 maiuscolo</div></div>
      <div style="display:flex;gap:40px;align-items:flex-end"><div><div class="label">Marcia</div>${value('3', '', 'value-xl')}</div><div><div class="label">Sterzo</div>${value('−38', '°')}</div><div><div class="label">Freno</div>${value('42', '%', 'value-m')}</div></div></div>
      <div class="small" style="margin-top:14px">Martian Mono solo per i valori dei dispositivi: cifre a larghezza fissa, non ballano mentre cambiano.</div>`)}
    ${section('Componenti', `<div style="display:grid;grid-template-columns:1fr 1fr;gap:40px">
      <div style="display:grid;gap:16px;align-content:start">
        <div style="display:flex;gap:12px;align-items:center;flex-wrap:wrap"><span class="btn primary">Primario</span><span class="btn secondary">Secondario</span><span class="btn quiet">Discreto</span><span class="btn disabled">Disattivato</span><span class="btn primary focus">Focus</span></div>
        <div style="display:flex;gap:12px;align-items:center"><span class="btn primary">${icon('lock', 'sm')}Installa il driver</span><span class="lockline">Chiede la password una volta</span></div>
        <div style="display:flex;gap:8px;flex-wrap:wrap"><span class="chip ok">${icon('check', 'sm')}Pronto</span><span class="chip warn">${icon('warn', 'sm')}Serve un driver</span><span class="chip">Salvata nella base</span><span class="chip">Fino allo scollegamento</span></div>
        <div style="display:flex;gap:16px;align-items:center"><span class="seg"><span class="on">6+R</span><span>7+R</span></span><span class="seg"><span class="on">Analogico</span><span>Pulsante</span></span>${toggle(true)}${toggle(false)}</div>
        <div class="notice warn">${icon('warn')}<span><strong>Il driver va ricompilato.</strong> Il kernel è stato aggiornato: ci vuole un minuto.</span></div></div>
      <div style="display:grid;gap:0;align-content:start">
        ${settingRow('Rotazione', `${slider(19.8, '540°')}<div class="quick"><span>360</span><span class="on">540</span><span>900</span><span>1080</span></div>`, { chip: '<span class="chip">Ripristinata al collegamento</span>' })}
        ${settingRow('Inerzia naturale', slider(0, '–', true), { help: 'Non disponibile su questo hardware: la base non ha un filtro d’inerzia.', disabled: true })}</div></div>`)}
    ${section('Dispositivi del core', `<div style="display:grid;grid-template-columns:repeat(4,1fr);gap:24px;align-items:end;justify-items:center">
      <div style="display:grid;justify-items:center;gap:8px">${dev.base(190)}<span class="small">Base, tre quarti</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.wheel(170, 'round', -20)}<span class="small">Volante tondo</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.wheel(170, 'gt', 0)}<span class="small">Volante GT</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.wheel(170, 'formula', 15)}<span class="small">Volante formula</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.pedals(250, { clutch: 0, brake: 0.55, throttle: 0 })}<span class="small">Pedaliera a 3 pedali, freno premuto</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.pedals(250, { brake: 0, throttle: 0.8 })}<span class="small">2 pedali: la frizione non c'è</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.shifter(170, 6, 'right_down', '3')}<span class="small">Cambio 6+R, in 3ª</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.shifter(170, 7, 'right_down', 'R')}<span class="small">Cambio 7+R, in retromarcia</span></div>
      <div style="display:grid;justify-items:center;gap:8px">${dev.handbrake(140, 0.7)}<span class="small">Freno a mano tirato</span></div></div>
      <div class="small" style="margin-top:16px">Disegni generici del core, senza marchi. I plugin possono fornirne di propri per i loro modelli, con le stesse regole di animazione.</div>`)}
  </div>`;
};
