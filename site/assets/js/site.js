// The site's live parts: the rig in the hero, the driver install, the settings panel, the stages
// of adding a device, the questions and the install command. Everything reads without it.
(() => {
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const wide = matchMedia("(min-width: 961px)");

  // Entrances, once, staggered within each group.
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add("is-in"); io.unobserve(e.target); }
  }, { rootMargin: "0px 0px -10% 0px", threshold: 0.08 });
  $$("[data-reveal]").forEach((el) => {
    const sibs = $$(":scope > [data-reveal]", el.parentElement);
    el.style.setProperty("--d", `${Math.min(sibs.indexOf(el), 5) * 80}ms`);
    io.observe(el);
  });

  // ---- Hero: the reel advances with the scroll, the devices move. ----
  const hero = $(".hero");
  const track = $(".reel__track");
  let heroVisible = true;
  if (hero && track) {
    const step = () => {
      const r = hero.getBoundingClientRect();
      const run = hero.offsetHeight - innerHeight;
      const p = run > 0 ? Math.min(1, Math.max(0, -r.top / run)) : 0;
      track.style.setProperty("--i", Math.min(3, Math.floor(p * 4)));
    };
    // On a phone the stage is not pinned: the reel turns by itself.
    let turn = 0;
    const narrow = () => !wide.matches;
    addEventListener("scroll", () => { if (!narrow()) step(); }, { passive: true });
    addEventListener("resize", () => { if (!narrow()) step(); });
    if (!narrow()) step();
    if (!still) setInterval(() => { if (narrow() && heroVisible) track.style.setProperty("--i", (turn = (turn + 1) % 4)); }, 3500);
    new IntersectionObserver(([e]) => { heroVisible = e.isIntersecting; }).observe(hero);
  }

  // The steering wheel: drag it, or use its slider; at rest it drives by itself.
  const rim = $("[data-wheel-rim]");
  const art = $("[data-wheel-drag]");
  const wheelInput = $("[data-wheel-input]");
  const wheelValue = $("[data-wheel-value]");
  let angle = 0;
  let touchedAt = still ? Infinity : -1e9;
  const setAngle = (a) => {
    angle = Math.max(-450, Math.min(450, a));
    rim.parentElement.style.setProperty("--a", angle.toFixed(1));
    wheelValue.value = `${Math.round(angle)}°`;
    wheelInput.value = Math.round(angle);
  };
  if (rim) {
    let from = null;
    art.addEventListener("pointerdown", (e) => {
      from = { x: e.clientX, a: angle };
      art.setPointerCapture(e.pointerId);
      art.classList.add("is-grabbed");
      touchedAt = performance.now();
    });
    art.addEventListener("pointermove", (e) => {
      if (!from) return;
      setAngle(from.a + (e.clientX - from.x) * 1.4);
      touchedAt = performance.now();
    });
    const drop = () => { from = null; art.classList.remove("is-grabbed"); };
    art.addEventListener("pointerup", drop);
    art.addEventListener("pointercancel", drop);
    wheelInput.addEventListener("input", () => { setAngle(+wheelInput.value); touchedAt = performance.now(); });
  }

  // Pedals, shifter and handbrake run a lap; the wheel joins in when left alone.
  const pedals = ["clutch", "brake", "throttle"].map((k) => {
    const out = $(`[data-pedal="${k}"]`);
    return out && { out, fill: out.parentElement.querySelector(".bar__fill") };
  });
  const knob = $("[data-knob]");
  const gearOut = $("[data-gear]");
  const lever = $("[data-lever]");
  const leverOut = $("[data-handbrake]");
  const leverState = $("[data-handbrake-state]");
  // Gate slots in the plate's units (devices.toml): 6+R, reverse right and down.
  const slots = { N: [160, 160], 1: [82, 73], 2: [82, 247], 3: [134, 73], 4: [134, 247], 5: [186, 73], 6: [186, 247], R: [238, 247] };
  const lap = ["N", 1, "N", 2, "N", 3, "N", 4, "N", 3, "N", 2];
  const setGear = (g) => {
    const [x, y] = slots[g];
    knob.parentElement.style.setProperty("--kx", x);
    knob.parentElement.style.setProperty("--ky", y);
    gearOut.value = g;
  };
  const setPedal = (i, v) => {
    if (!pedals[i]) return;
    pedals[i].fill.style.setProperty("--v", v.toFixed(3));
    pedals[i].out.value = `${Math.round(v * 100)}%`;
  };
  const setLever = (v) => {
    lever.parentElement.style.setProperty("--h", v.toFixed(3));
    leverOut.value = `${Math.round(v * 100)}%`;
    leverState.textContent = v > 0.05 ? "Pulled" : "At rest";
  };
  if (knob) {
    if (still) {
      setAngle(196); setPedal(0, 0.62); setPedal(1, 0.02); setPedal(2, 0.86); setGear(1); setLever(0.38);
    } else {
      let gi = 0;
      setInterval(() => { if (heroVisible) setGear(lap[(gi = (gi + 1) % lap.length)]); }, 1100);
      const frame = (t) => {
        if (heroVisible) {
          const s = t / 1000;
          const throttle = Math.max(0, Math.sin(s * 0.9)) ** 0.7;
          const brake = Math.max(0, -Math.sin(s * 0.9)) ** 1.6 * 0.9;
          setPedal(2, throttle);
          setPedal(1, brake);
          setPedal(0, Math.max(0, Math.sin(s * 2.2 + 1)) ** 6 * 0.95);
          const pull = Math.max(0, Math.sin(s * 0.7 - 1)) ** 3;
          setLever(pull);
          if (performance.now() - touchedAt > 2500) setAngle(Math.sin(s * 0.55) * 210 + Math.sin(s * 1.7) * 18);
        }
        requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    }
  }

  // ---- The driver install, in the middle row. ----
  const install = $("[data-install]");
  if (install) {
    const button = $("[data-install-button]", install);
    const label = $("[data-install-label]", install);
    const sub = $("[data-install-sub]", install);
    const first = sub.textContent;
    button.addEventListener("click", () => {
      if (install.classList.contains("is-busy")) return;
      if (install.classList.contains("is-done")) {
        install.classList.remove("is-done");
        label.textContent = "Install the driver";
        sub.textContent = first;
        return;
      }
      install.classList.add("is-busy");
      label.textContent = "Installing";
      sub.textContent = "Building hid-fanatecff with DKMS";
      setTimeout(() => {
        install.classList.remove("is-busy");
        install.classList.add("is-done");
        label.textContent = "Set up";
        sub.textContent = "Fanatec · driver installed, kept after kernel updates";
      }, still ? 300 : 2300);
    });
  }

  // ---- The settings panel and its notes. ----
  const panel = $("[data-panel]");
  if (panel) {
    const rot = $("[data-rot]", panel);
    const rotOut = $("[data-rot-value]", panel);
    const quick = $$("[data-rot-quick]", panel);
    const setRot = (v) => {
      rot.value = v;
      rotOut.value = `${v}°`;
      quick.forEach((b) => b.setAttribute("aria-pressed", String(+b.dataset.rotQuick === +v)));
    };
    rot.addEventListener("input", () => setRot(rot.value));
    quick.forEach((b) => b.addEventListener("click", () => setRot(b.dataset.rotQuick)));
    const ffb = $("[data-ffb]", panel);
    ffb.addEventListener("input", () => { $("[data-ffb-value]", panel).value = `${ffb.value}%`; });
    const adv = $("[data-advanced]", panel);
    adv.addEventListener("click", () => {
      const open = adv.getAttribute("aria-expanded") !== "true";
      adv.setAttribute("aria-expanded", String(open));
      $("[data-advanced-list]", panel).hidden = !open;
      place();
    });

    // Notes sit level with what they explain, in the margins, on wide screens.
    const notes = $(".notes");
    const grid = panel.parentElement;
    const left = new Set(["rotation", "ffb", "advanced"]);
    function place() {
      if (!wide.matches) { notes.classList.remove("is-placed"); return; }
      notes.classList.add("is-placed");
      const g = grid.getBoundingClientRect();
      for (const n of $$(".note", notes)) {
        const t = $(`[data-target="${n.dataset.note}"]`, panel);
        n.classList.toggle("is-left", left.has(n.dataset.note));
        n.classList.toggle("is-right", !left.has(n.dataset.note));
        n.style.top = `${t.getBoundingClientRect().top - g.top}px`;
      }
    }
    addEventListener("resize", place);
    wide.addEventListener("change", place);
    document.fonts?.ready.then(place);
    place();
    for (const n of $$(".note", notes)) {
      const t = () => $(`[data-target="${n.dataset.note}"]`, panel);
      const on = () => t().classList.add("is-lit");
      const off = () => t().classList.remove("is-lit");
      n.addEventListener("mouseenter", on);
      n.addEventListener("mouseleave", off);
      n.addEventListener("focus", on);
      n.addEventListener("blur", off);
    }
  }

  // ---- Adding a device: the slider drives the clause and the screen. ----
  const stages = $("[data-stages]");
  if (stages) {
    const input = $("[data-stage-input]", stages);
    const names = $$(".stages__names li", stages);
    const show = () => {
      const i = +input.value;
      $$("[data-stage-clause]", stages).forEach((p) => { p.hidden = +p.dataset.stageClause !== i; });
      $$("[data-stage-ui]", stages).forEach((p) => { p.hidden = +p.dataset.stageUi !== i; });
      names.forEach((n, k) => n.classList.toggle("is-current", k === i));
      input.setAttribute("aria-valuetext", names[i].textContent);
      $("[data-stage-slider]", stages).style.setProperty("--p", i / 3);
    };
    // The scroll walks through the stages while the section is pinned; the slider and the
    // names move the scroll to their stage, so the two always agree.
    const pin = $(".stages__pin", stages);
    const run = () => stages.offsetHeight - pin.offsetHeight;
    let last = -1;
    const follow = () => {
      const r = run();
      const p = r > 0 ? Math.min(1, Math.max(0, -stages.getBoundingClientRect().top / r)) : 0;
      const i = Math.min(3, Math.floor(p * 4));
      if (i !== last) { last = i; input.value = i; show(); }
    };
    const go = (i) => {
      input.value = i;
      show();
      last = i;
      const r = run();
      const top = stages.getBoundingClientRect().top + scrollY;
      if (r > 0) scrollTo({ top: top + (r * (i + 0.5)) / 4, behavior: still ? "auto" : "smooth" });
    };
    input.addEventListener("input", () => go(+input.value));
    addEventListener("scroll", follow, { passive: true });
    addEventListener("resize", follow);
    names.forEach((n, k) => { n.style.cursor = "pointer"; n.addEventListener("click", () => go(k)); });
    show();
  }

  // ---- Questions: the index selects one answer. ----
  const faq = $("[data-faq]");
  if (faq) {
    const tabs = $$('[role="tab"]', faq);
    const select = (tab, focus) => {
      tabs.forEach((t, k) => {
        const on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        $(`#a${k + 1}`).hidden = !on;
        $(`#i${k + 1}`).hidden = !on;
      });
      if (focus) tab.focus();
    };
    tabs.forEach((t, k) => {
      t.addEventListener("click", () => select(t));
      t.addEventListener("mouseenter", () => { if (wide.matches) select(t); });
      t.addEventListener("keydown", (e) => {
        const d = { ArrowDown: 1, ArrowUp: -1 }[e.key];
        if (d) { e.preventDefault(); select(tabs[(k + d + tabs.length) % tabs.length], true); }
      });
    });
  }

  // ---- The install command for the chosen distribution. ----
  const distro = $("[data-distro]");
  if (distro) {
    const cmds = {
      deb: "sudo apt install ./simrig_*_amd64.deb",
      rpm: "sudo dnf install ./simrig-*.x86_64.rpm",
      suse: "sudo zypper install ./simrig-*.x86_64.rpm",
      arch: "sudo pacman -U ./simrig-*-x86_64.pkg.tar.zst",
      tar: "tar xzf simrig-*-x86_64-linux.tar.gz && sudo ./simrig-*-x86_64-linux/install.sh",
    };
    distro.addEventListener("change", () => { $("[data-cmd]").textContent = cmds[distro.value]; });
  }
})();
