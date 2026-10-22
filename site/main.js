// Pina Studio, direction C: "After Hours".
// One AudioContext, one analyser on the sum. What it hears is split into five bands (sub to air)
// that light the screenshots on screen and the hairline at the top. Nothing loads or plays until
// the visitor clicks something that says it will.
(() => {
  "use strict";

  const doc = document.documentElement;
  doc.classList.add("js");
  doc.dataset.sound = "off";

  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const lerp = (a, b, t) => a + (b - a) * t;
  const smooth = (t) => t * t * (3 - 2 * t);

  const motionQuery = matchMedia("(prefers-reduced-motion: reduce)");
  let reduced = motionQuery.matches;

  const announcer = $("[data-announce]");
  const announce = (msg) => {
    if (!announcer) return;
    announcer.textContent = "";
    setTimeout(() => (announcer.textContent = msg), 40);
  };

  /* ------------------------------------------------------------------ frames */
  // Each task returns true when it wants another frame. Scrolling and sound wake the loop.
  const tasks = [];
  let raf = 0;
  const frame = (now) => {
    raf = 0;
    let again = false;
    for (const t of tasks) if (t(now)) again = true;
    if (again) raf = requestAnimationFrame(frame);
  };
  const wake = () => {
    if (!raf) raf = requestAnimationFrame(frame);
  };

  /* ------------------------------------------------------------------- audio */
  const BANDS = [
    // low Hz, high Hz, gain (music falls off with frequency; lift the top so air can light up)
    [30, 100, 1.0],
    [100, 500, 1.0],
    [500, 2000, 1.12],
    [2000, 6000, 1.32],
    [6000, 16000, 1.65],
  ];
  const BAND_RGB = [
    [255, 74, 46],
    [255, 159, 28],
    [233, 242, 107],
    [63, 224, 197],
    [75, 123, 255],
  ];

  const audio = { ctx: null, master: null, analyser: null, data: null, binHz: 0, levels: [0, 0, 0, 0, 0], level: 0 };
  let state = "off"; // off | loading | on
  let program = null; // "song" (Corner Store Blues, the whole song) | "md" (a Listen player's stems)

  function getCtx() {
    if (!audio.ctx) {
      const AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return null;
      const c = new AC();
      audio.ctx = c;
      audio.master = c.createGain();
      audio.analyser = c.createAnalyser();
      audio.analyser.fftSize = 4096;
      audio.analyser.smoothingTimeConstant = 0.68;
      audio.analyser.minDecibels = -96;
      audio.analyser.maxDecibels = -22;
      audio.master.connect(audio.analyser);
      audio.analyser.connect(c.destination);
      audio.data = new Uint8Array(audio.analyser.frequencyBinCount);
      audio.binHz = c.sampleRate / audio.analyser.fftSize;
    }
    if (audio.ctx.state === "suspended") audio.ctx.resume();
    return audio.ctx;
  }

  function rampTo(param, value, seconds) {
    const now = audio.ctx.currentTime;
    param.cancelScheduledValues(now);
    param.setValueAtTime(param.value, now);
    param.linearRampToValueAtTime(value, now + seconds);
  }

  /* Corner Store Blues: a streamed <audio> through the same graph */
  const song = { el: null, gain: null, pauseTimer: 0 };

  function songPlay(at) {
    const c = getCtx();
    if (!c) {
      announce("This browser can't play audio here.");
      return;
    }
    if (!song.el) {
      const el = new Audio();
      el.preload = "auto";
      el.loop = true;
      el.src = "media/corner-store-blues.m4a";
      song.el = el;
      const node = c.createMediaElementSource(el);
      song.gain = c.createGain();
      song.gain.gain.value = 0;
      node.connect(song.gain);
      song.gain.connect(audio.master);
      el.addEventListener("playing", () => {
        if (program === "song" && state !== "off") setSound("on");
      });
      el.addEventListener("waiting", () => {
        if (program === "song" && state === "on") setSound("loading");
      });
      el.addEventListener("error", () => {
        if (program !== "song") return;
        setSound("off");
        announce("Corner Store Blues couldn't be loaded. Try again in a moment.");
      });
    }
    players.forEach((p) => mdPause(p, true));
    program = "song";
    clearTimeout(song.pauseTimer);
    if (typeof at === "number") song.el.currentTime = at;
    setSound(song.el.readyState >= 3 && !song.el.paused ? "on" : "loading");
    const p = song.el.play();
    if (p && p.catch) {
      p.catch(() => {
        if (program === "song" && song.el.paused) setSound("off");
      });
    }
    rampTo(song.gain.gain, 1, 0.9);
  }

  function songPause() {
    if (!song.el || song.el.paused) return;
    rampTo(song.gain.gain, 0, 0.3);
    clearTimeout(song.pauseTimer);
    song.pauseTimer = setTimeout(() => song.el.pause(), 330);
  }

  function stopAll() {
    songPause();
    players.forEach((p) => mdPause(p, true));
    setSound("off");
  }

  /* --------------------------------------------------------- sound UI state */
  const toggles = $$("[data-sound-toggle]");
  const labels = {
    long: $$('[data-sound-label="long"]'),
    short: $$('[data-sound-label="short"]'),
    replay: $$('[data-sound-label="replay"]'),
    sub: $$("[data-sound-sub]"),
  };
  const setText = (els, text) => els.forEach((el) => el.textContent !== text && (el.textContent = text));

  function setSound(next) {
    state = next;
    doc.dataset.sound = next;
    doc.dataset.program = program || "";
    const on = next !== "off";
    const name = program === "md" && md ? md.name : "Corner Store Blues";
    toggles.forEach((b) => {
      const pressed = b.dataset.soundToggle === "song" ? on && program === "song" : on;
      b.setAttribute("aria-pressed", String(pressed));
    });
    setText(labels.long, next === "off" ? "Enter with sound" : next === "loading" ? "Loading…" : "Sound on");
    setText(labels.sub, next === "off" ? "Plays Corner Store Blues, 2:23" : next === "loading" ? name : `Playing ${name}`);
    setText(labels.short, next === "off" ? "Sound off" : next === "loading" ? "Loading" : "Sound on");
    setText(
      labels.replay,
      on && program === "song" ? (next === "loading" ? "Loading the song…" : "Playing under it") : "Play the song under it"
    );
    if (next === "on") announce(`Playing ${name}.`);
    wake();
  }

  toggles.forEach((b) =>
    b.addEventListener("click", () => {
      if (b.dataset.soundToggle === "song") {
        if (program === "song" && state !== "off") stopAll();
        else songPlay(rp.pinned && rp.active >= 0 ? CHAPTERS[rp.active].t : undefined);
        return;
      }
      if (state !== "off") stopAll();
      else songPlay();
    })
  );

  /* ------------------------------------------------------ bands and painting */
  const hosts = new Set(); // [data-react] elements on screen
  const bars = $$(".bars").map((g) => $$("i", g));

  function analyse() {
    const a = audio.analyser;
    const d = audio.data;
    a.getByteFrequencyData(d);
    let total = 0;
    for (let i = 0; i < 5; i++) {
      const [lo, hi, gain] = BANDS[i];
      const s = Math.max(1, Math.floor(lo / audio.binHz));
      const e = Math.min(d.length - 1, Math.ceil(hi / audio.binHz));
      let sum = 0;
      let peak = 0;
      for (let j = s; j <= e; j++) {
        sum += d[j];
        if (d[j] > peak) peak = d[j];
      }
      let v = (((sum / (e - s + 1)) * 0.6 + peak * 0.4) / 255) * gain;
      v = clamp((v - 0.2) / 0.6);
      v = smooth(v);
      const cur = audio.levels[i];
      audio.levels[i] = v > cur ? cur + (v - cur) * 0.6 : cur + (v - cur) * 0.12;
      total += audio.levels[i];
    }
    audio.level = total / 5;
  }

  function paintHost(el) {
    const L = audio.levels;
    const s = el.style;
    s.setProperty("--sub", L[0].toFixed(3));
    s.setProperty("--low", L[1].toFixed(3));
    s.setProperty("--mid", L[2].toFixed(3));
    s.setProperty("--pres", L[3].toFixed(3));
    s.setProperty("--air", L[4].toFixed(3));
    s.setProperty("--level", audio.level.toFixed(3));
  }

  function paintBars(reset) {
    const L = audio.levels;
    for (const g of bars) {
      for (let i = 0; i < g.length; i++) g[i].style.transform = reset ? "" : `scaleY(${(0.2 + L[i] * 0.8).toFixed(3)})`;
    }
  }

  const STATIC_LEVELS = [0.5, 0.42, 0.34, 0.4, 0.46];

  tasks.push(function audioTask() {
    const live = state === "on" && audio.analyser;
    if (live && !reduced) {
      analyse();
      hosts.forEach(paintHost);
      paintBars(false);
      hairDraw();
      return true;
    }
    if (live && reduced) {
      // Reduced motion: a steady glow while something plays, no flicker.
      audio.levels = STATIC_LEVELS.slice();
      audio.level = 0.42;
      hosts.forEach(paintHost);
      paintBars(false);
      hairDraw(true);
      return false;
    }
    let any = false;
    for (let i = 0; i < 5; i++) {
      audio.levels[i] *= 0.86;
      if (audio.levels[i] > 0.004) any = true;
      else audio.levels[i] = 0;
    }
    audio.level *= 0.86;
    hosts.forEach(paintHost);
    paintBars(!any);
    return any;
  });

  const reactIO = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          hosts.add(e.target);
          paintHost(e.target);
        } else hosts.delete(e.target);
      }
    },
    { rootMargin: "15% 0px" }
  );
  $$("[data-react]").forEach((el) => reactIO.observe(el));

  /* -------------------------------------------------------- the hairline */
  const hair = { cv: $(".hairline__canvas"), g: null, w: 0, h: 0, cols: [], mask: null };

  function bandColor(f) {
    // Blend between band centres on a log scale
    const centres = BANDS.map(([lo, hi]) => Math.log(Math.sqrt(lo * hi)));
    const x = Math.log(f);
    if (x <= centres[0]) return BAND_RGB[0];
    for (let i = 0; i < 4; i++) {
      if (x <= centres[i + 1]) {
        const t = smooth((x - centres[i]) / (centres[i + 1] - centres[i]));
        return BAND_RGB[i].map((c, k) => Math.round(lerp(c, BAND_RGB[i + 1][k], t)));
      }
    }
    return BAND_RGB[4];
  }

  function hairSize() {
    if (!hair.cv) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const cssW = window.innerWidth;
    const cssH = 28;
    hair.cv.width = Math.round(cssW * dpr);
    hair.cv.height = Math.round(cssH * dpr);
    hair.w = hair.cv.width;
    hair.h = hair.cv.height;
    hair.g = hair.cv.getContext("2d");
    const n = Math.max(64, Math.round(cssW / 3));
    hair.cols = [];
    for (let i = 0; i < n; i++) {
      const f = 30 * Math.pow(16000 / 30, (i + 0.5) / n);
      const [r, g, b] = bandColor(f);
      hair.cols.push({ f, color: `rgb(${r},${g},${b})`, band: f < 100 ? 0 : f < 500 ? 1 : f < 2000 ? 2 : f < 6000 ? 3 : 4 });
    }
    const m = hair.g.createLinearGradient(0, 0, 0, hair.h);
    const px = (v) => clamp((v * dpr) / hair.h);
    m.addColorStop(0, "rgba(0,0,0,1)");
    m.addColorStop(px(2), "rgba(0,0,0,1)");
    m.addColorStop(px(3), "rgba(0,0,0,.34)");
    m.addColorStop(px(10), "rgba(0,0,0,.12)");
    m.addColorStop(1, "rgba(0,0,0,0)");
    hair.mask = m;
  }

  function hairDraw(still) {
    const { g, w, h, cols } = hair;
    if (!g || !audio.data) return;
    g.globalCompositeOperation = "source-over";
    g.clearRect(0, 0, w, h);
    const cw = w / cols.length;
    const d = audio.data;
    const last = d.length - 1;
    for (let i = 0; i < cols.length; i++) {
      const c = cols[i];
      let v;
      if (still) v = STATIC_LEVELS[c.band] * 0.8;
      else {
        const bin = Math.min(last, Math.max(1, Math.round(c.f / audio.binHz)));
        v = (d[bin - 1] + d[bin] * 2 + d[Math.min(last, bin + 1)]) / (4 * 255);
        v = clamp((v - 0.22) / 0.62) * BANDS[c.band][2];
      }
      if (v <= 0.01) continue;
      g.globalAlpha = clamp(v);
      g.fillStyle = c.color;
      g.fillRect(i * cw, 0, cw + 0.6, h);
    }
    g.globalAlpha = 1;
    g.globalCompositeOperation = "destination-in";
    g.fillStyle = hair.mask;
    g.fillRect(0, 0, w, h);
  }

  /* -------------------------------------------------------- session replay */
  // Where each step lands in the song (95 BPM: a bar is 2.5263 s) and what the camera looks at:
  // [image, rect at the start of the step, rect at its end], rects as fractions [x, y, w, h].
  const CHAPTERS = [
    { t: 0, shot: ["arrange", [0.06, 0, 0.88, 0.66], [0.33, 0, 0.32, 0.11]], cap: "Arrange · the transport: 95 BPM, C minor" },
    { t: 10.105, shot: ["arrange", [0.17, 0.04, 0.6, 0.42], [0.18, 0.1, 0.42, 0.26]], cap: "Arrange · Kick, Snare and Hats" },
    { t: 50.526, shot: ["arrange", [0.41, 0.03, 0.59, 0.44], [0.6, 0.04, 0.38, 0.3]], cap: "Arrange · the Hook at bar 21" },
    { t: 70.737, shot: ["arrange", [0.14, 0.44, 0.46, 0.34], [0.54, 0.46, 0.46, 0.34]], cap: "Arrange · 808, Bass and Keys into Verse 2" },
    { t: 101.053, shot: ["mixer", [0, 0, 1, 1], [0.74, 0.22, 0.26, 0.3]], cap: "Mixer · Corner Store Blues" },
    { t: 121.263, shot: ["eq", [0.05, 0.06, 0.9, 0.88], [0, 0, 1, 1]], cap: "Channel EQ on the Kick, over a live spectrum" },
    { t: 143, shot: ["modulation", [0, 0, 0.5, 1], [0.5, 0, 0.5, 1]], cap: "Modulation on the Kick · five modulators" },
  ];
  const SPB = 60 / 95; // seconds per beat

  const rp = {
    root: $("#replay"),
    track: $("[data-replay-track]"),
    stage: $("[data-stage]"),
    chapters: $$(".chapter"),
    ticks: $$("[data-goto]"),
    fill: $("[data-fill]"),
    caption: $("[data-stage-caption]"),
    lcdBar: $('[data-lcd="bar"]'),
    lcdTime: $('[data-lcd="time"]'),
    layers: {},
    bleeds: {},
    active: -1,
    shot: "",
    pinned: false,
    visible: false,
    sw: 0,
    sh: 0,
    lastText: "",
  };

  $$("[data-layer]").forEach((el) => {
    const img = $("img", el);
    rp.layers[el.dataset.layer] = { el, img, w: +img.getAttribute("width"), h: +img.getAttribute("height"), cur: null };
  });
  $$("[data-bleed]").forEach((img) => (rp.bleeds[img.dataset.bleed] = img));
  rp.chapters.forEach((ch) => {
    ch._lines = $$("[data-line]", ch);
    const n = ch._lines.length;
    ch._thr = ch._lines.map((_, k) => (k === 0 ? 0 : k === n - 1 ? 0.66 : 0.08 + (k - 1) * (0.44 / Math.max(1, n - 2))));
  });

  function fit(sw, sh, w, h, r) {
    const k = Math.min(sw / (r[2] * w), sh / (r[3] * h));
    const iw = w * k;
    const ih = h * k;
    let x = sw / 2 - (r[0] + r[2] / 2) * iw;
    let y = sh / 2 - (r[1] + r[3] / 2) * ih;
    x = iw >= sw ? clamp(x, sw - iw, 0) : (sw - iw) / 2;
    y = ih >= sh ? clamp(y, sh - ih, 0) : (sh - ih) / 2;
    return [x, y, k];
  }
  const place = (img, t) => {
    img.style.transform = `translate3d(${t[0].toFixed(2)}px, ${t[1].toFixed(2)}px, 0) scale(${t[2].toFixed(5)})`;
  };

  function layoutStills() {
    $$("[data-still]").forEach((st) => {
      const img = $("img", st);
      const sw = st.clientWidth;
      const sh = st.clientHeight;
      if (!sw) return;
      const w = +img.getAttribute("width");
      const h = +img.getAttribute("height");
      img.style.width = `${w}px`;
      img.style.height = `${h}px`;
      place(img, fit(sw, sh, w, h, st.dataset.rect.split(",").map(Number)));
    });
  }

  function replayLayout() {
    if (!rp.root) return;
    const want = !reduced && window.innerWidth >= 1100 && window.innerHeight >= 680;
    rp.root.classList.toggle("is-pinned", want);
    rp.pinned = want;
    layoutStills();
    if (want) {
      rp.sw = rp.stage.clientWidth;
      rp.sh = rp.stage.clientHeight;
      for (const name in rp.layers) {
        const L = rp.layers[name];
        L.img.style.width = `${L.w}px`;
        L.img.style.height = `${L.h}px`;
        const first = CHAPTERS.find((c) => c.shot[0] === name);
        L.cur = fit(rp.sw, rp.sh, L.w, L.h, first.shot[1]);
        place(L.img, L.cur);
      }
      rp.active = -1;
      rp.shot = "";
    } else {
      rp.chapters.forEach((c) => c.classList.remove("is-active", "is-past"));
    }
    wake();
  }

  function fmtBar(t) {
    const ticks = Math.floor((t / SPB) * 960);
    const bar = Math.floor(ticks / 3840) + 1;
    const beat = Math.floor((ticks % 3840) / 960) + 1;
    const div = Math.floor((ticks % 960) / 240) + 1;
    const tick = (ticks % 240) + 1;
    return `${bar}.${beat}.${div}.${String(tick).padStart(3, "0")}`;
  }
  function fmtTime(t) {
    const m = Math.floor(t / 60);
    const s = t - m * 60;
    return `${m}:${s.toFixed(3).padStart(6, "0")}`;
  }

  function setChapter(i) {
    rp.active = i;
    rp.chapters.forEach((c, k) => {
      c.classList.toggle("is-active", k === i);
      c.classList.toggle("is-past", k < i);
    });
    rp.ticks.forEach((b, k) => {
      if (k === i) b.setAttribute("aria-current", "step");
      else b.removeAttribute("aria-current");
      b.classList.toggle("is-done", k < i);
    });
    const name = CHAPTERS[i].shot[0];
    if (rp.shot !== name) {
      rp.shot = name;
      for (const n in rp.layers) rp.layers[n].el.classList.toggle("is-on", n === name);
      for (const n in rp.bleeds) rp.bleeds[n].classList.toggle("is-on", n === name);
      const L = rp.layers[name];
      L.cur = null; // snap: no fly-in from a stale view
    }
    if (rp.caption) rp.caption.textContent = CHAPTERS[i].cap;
  }

  tasks.push(function replayTask() {
    if (!rp.pinned || !rp.visible) return false;
    const r = rp.track.getBoundingClientRect();
    const total = r.height - window.innerHeight;
    const p = clamp(-r.top / total);
    const N = CHAPTERS.length;
    const x = p * N;
    const i = Math.min(N - 1, Math.floor(x));
    const u = clamp(x - i);
    if (i !== rp.active) setChapter(i);

    // The step's lines appear as the scroll moves through it
    const ch = rp.chapters[i];
    let current = -1;
    ch._lines.forEach((el, k) => {
      const on = u >= ch._thr[k];
      el.classList.toggle("is-on", on);
      if (on && el.classList.contains("cmd")) current = k;
    });
    const resultOn = u >= ch._thr[ch._thr.length - 1];
    ch._lines.forEach((el, k) => el.classList.toggle("is-current", k === current && !resultOn));

    // Camera
    const c = CHAPTERS[i];
    const L = rp.layers[c.shot[0]];
    const e = smooth(clamp(u * 1.12));
    const rect = c.shot[1].map((v, k) => lerp(v, c.shot[2][k], e));
    const target = fit(rp.sw, rp.sh, L.w, L.h, rect);
    let settled = true;
    if (!L.cur) L.cur = target.slice();
    else {
      for (let k = 0; k < 3; k++) {
        const dlt = target[k] - L.cur[k];
        L.cur[k] += dlt * 0.14;
        if (Math.abs(dlt) > (k === 2 ? 0.0004 : 0.3)) settled = false;
      }
    }
    place(L.img, L.cur);

    // Transport readout: the playhead walks to where each step lands
    const t1 = i < N - 1 ? CHAPTERS[i + 1].t : c.t;
    const t = lerp(c.t, t1, u);
    const bar = fmtBar(t);
    if (bar !== rp.lastText) {
      rp.lastText = bar;
      rp.lcdBar.textContent = bar;
      rp.lcdTime.textContent = fmtTime(t);
    }
    rp.fill.style.setProperty("--p", p.toFixed(4));
    return !settled;
  });

  if (rp.track) {
    new IntersectionObserver(([e]) => {
      rp.visible = e.isIntersecting;
      if (rp.visible) wake();
    }).observe(rp.track);
  }

  rp.ticks.forEach((b) =>
    b.addEventListener("click", () => {
      const i = +b.dataset.goto;
      if (rp.pinned) {
        const top = rp.track.getBoundingClientRect().top + window.scrollY;
        const total = rp.track.offsetHeight - window.innerHeight;
        window.scrollTo({ top: top + ((i + 0.42) / CHAPTERS.length) * total, behavior: reduced ? "auto" : "smooth" });
      } else {
        document.getElementById(`step-${i + 1}`).scrollIntoView({ block: "start" });
      }
      if (program === "song" && state !== "off" && song.el) song.el.currentTime = CHAPTERS[i].t;
    })
  );

  /* ------------------------------------------------------ syntax in commands */
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  $$("code.cmd").forEach((el) => {
    const src = el.textContent;
    const m = src.match(/^(\S+)\s+(\S+)([\s\S]*)$/);
    if (!m) return;
    let rest = esc(m[3]);
    rest = rest.replace(/("op":")([a-z_]+)(")/g, '$1<span class="k-op">$2</span>$3');
    rest = rest.replace(/^(\s+)(song)\b/, '$1<span class="k-str">$2</span>');
    rest = rest.replace(/^(\s+)("[^"]*")$/, '$1<span class="k-str">$2</span>');
    rest = rest.replace(/ ("[^"]*")$/, ' <span class="k-str">$1</span>');
    // Let long JSON break after a comma rather than inside a number
    if (rest.includes("{")) rest = rest.replace(/,/g, ",<wbr>").replace(/:\{/g, ":<wbr>{");
    el.innerHTML = `<span class="k-bin">${esc(m[1])}</span> <span class="k-sub">${esc(m[2])}</span>${rest}`;
  });

  /* ------------------------------------------------- the stems players (Listen) */
  // One for each song, from its data attributes: its stems (files in data-dir), the loop's
  // length in seconds, its tempo and the bar the loop starts at. One plays at a time.
  const players = $$("[data-md]").map((root) => {
    const stems = root.dataset.stems.split(" ");
    const beat = 60 / +root.dataset.bpm;
    return {
      root,
      stems,
      name: root.dataset.name,
      dir: root.dataset.dir,
      loop: +root.dataset.loop,
      beat,
      bar: beat * 4,
      first: +(root.dataset.firstBar || 1),
      play: $("[data-md-play]", root),
      status: $("[data-md-status]", root),
      lane: $("[data-md-lane]", root),
      head: $("[data-md-head]", root),
      barEl: $("[data-md-bar]", root),
      time: $("[data-md-time]", root),
      rows: $$(".md__row", root),
      cards: $$(".stem", root),
      meters: $$(".stem__meter i", root),
      seeks: $$("[data-md-seek]", root),
      buffers: null,
      loading: null,
      gains: [],
      analysers: [],
      levels: stems.map(() => 0),
      srcs: [],
      bus: null,
      startAt: 0,
      offset: 0,
      playing: false,
      mute: new Set(),
      solo: new Set(),
      buf: null,
    };
  });
  let md = players[0] || null; // the player playing, or played last

  const mdSay = (p, text, isError) => {
    p.status.textContent = text;
    p.status.classList.toggle("is-error", !!isError);
  };

  const decode = (c, data) =>
    new Promise((resolve, reject) => {
      const pr = c.decodeAudioData(data, resolve, reject);
      if (pr && pr.then) pr.then(resolve, reject);
    });

  function mdLoad(p) {
    if (p.buffers) return Promise.resolve(p.buffers);
    if (p.loading) return p.loading;
    const c = getCtx();
    const n = p.stems.length;
    let done = 0;
    p.root.classList.add("is-loading");
    p.play.setAttribute("aria-label", `Loading ${p.name}, click to cancel`);
    mdSay(p, `Loading the stems, 0 of ${n}…`);
    p.loading = Promise.all(
      p.stems.map((name) =>
        fetch(`${p.dir}/${name}.m4a`)
          .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(String(r.status)))))
          .then((data) => decode(c, data))
          .then((buffer) => {
            done += 1;
            mdSay(p, `Loading the stems, ${done} of ${n}…`);
            return buffer;
          })
      )
    )
      .then((buffers) => {
        p.buffers = buffers;
        p.root.classList.remove("is-loading");
        mdGraph(p);
        return buffers;
      })
      .catch((err) => {
        p.loading = null;
        p.root.classList.remove("is-loading");
        p.play.setAttribute("aria-label", `Play ${p.name}`);
        mdSay(p, "The stems couldn't be loaded. Check your connection and press play to try again.", true);
        throw err;
      });
    return p.loading;
  }

  function mdGraph(p) {
    const c = audio.ctx;
    p.bus = c.createGain();
    p.bus.gain.value = 0;
    p.bus.connect(audio.master);
    p.buf = new Float32Array(1024);
    p.stems.forEach((_, i) => {
      const g = c.createGain();
      const a = c.createAnalyser();
      a.fftSize = 1024;
      g.connect(a);
      g.connect(p.bus);
      p.gains[i] = g;
      p.analysers[i] = a;
    });
    mdApply(p, true);
  }

  function mdStart(p, offset) {
    const c = audio.ctx;
    const t0 = c.currentTime + 0.05;
    p.srcs = p.buffers.map((buffer, i) => {
      const s = c.createBufferSource();
      s.buffer = buffer;
      s.loop = true;
      s.loopStart = 0;
      s.loopEnd = Math.min(p.loop, buffer.duration);
      s.connect(p.gains[i]);
      s.start(t0, offset % p.loop);
      return s;
    });
    p.startAt = t0 - offset;
    p.playing = true;
    p.bus.gain.cancelScheduledValues(c.currentTime);
    p.bus.gain.setValueAtTime(0, c.currentTime);
    p.bus.gain.linearRampToValueAtTime(0, t0);
    p.bus.gain.linearRampToValueAtTime(1, t0 + 0.03);
  }

  function mdKill(p, at) {
    p.srcs.forEach((s) => {
      try {
        s.stop(at);
      } catch (e) {
        /* already stopped */
      }
    });
    p.srcs = [];
  }

  const mdPos = (p) => {
    if (!p.playing) return p.offset;
    const t = audio.ctx.currentTime - p.startAt;
    return ((t % p.loop) + p.loop) % p.loop;
  };

  function mdPaint(p, t) {
    p.head.style.transform = `translateX(${((t / p.loop) * 100).toFixed(3)}%)`;
    const bar = Math.floor(t / p.bar) + p.first;
    const beat = Math.floor((t % p.bar) / p.beat) + 1;
    p.barEl.textContent = `${bar}.${beat}`;
    p.time.textContent = `0:${t.toFixed(1).padStart(4, "0")}`;
    let cur = 0;
    p.seeks.forEach((b, k) => {
      if (t >= +b.dataset.mdSeek - 0.01) cur = k;
    });
    p.seeks.forEach((b, k) => b.setAttribute("aria-current", String(k === cur)));
  }

  async function mdPlay(p, from) {
    if (!getCtx()) {
      mdSay(p, "This browser can't play audio here.", true);
      return;
    }
    if (typeof from === "number") p.offset = from;
    if (program === "song") songPause();
    players.forEach((o) => o !== p && mdPause(o, true));
    program = "md";
    md = p;
    setSound("loading");
    try {
      await mdLoad(p);
    } catch (e) {
      if (program === "md" && md === p) {
        program = null;
        setSound("off");
      }
      return;
    }
    if (program !== "md" || md !== p || p.playing) return; // the visitor changed their mind meanwhile
    mdStart(p, p.offset);
    p.root.classList.add("is-playing");
    p.play.setAttribute("aria-label", `Pause ${p.name}`);
    mdSay(p, "Playing, and looping. Mute or solo a stem; the light follows what's left.");
    setSound("on");
    wake();
  }

  function mdPause(p, silent) {
    if (p.root.classList.contains("is-loading") && program === "md" && md === p) program = null;
    if (!p.playing) return;
    p.offset = mdPos(p);
    const c = audio.ctx;
    p.bus.gain.cancelScheduledValues(c.currentTime);
    p.bus.gain.setTargetAtTime(0, c.currentTime, 0.012);
    mdKill(p, c.currentTime + 0.08);
    p.playing = false;
    p.root.classList.remove("is-playing");
    p.play.setAttribute("aria-label", `Play ${p.name}`);
    p.levels = p.stems.map(() => 0);
    p.meters.forEach((m) => m.style.setProperty("--m", "0"));
    mdSay(p, "Paused at bar " + (Math.floor(p.offset / p.bar) + p.first) + ".");
    if (!silent) setSound("off");
  }

  function mdSeek(p, t) {
    t = clamp(t, 0, p.loop - 0.02);
    if (p.playing) {
      mdKill(p, audio.ctx.currentTime);
      mdStart(p, t);
    } else {
      p.offset = t;
      mdPaint(p, t);
    }
  }

  function mdApply(p, instant) {
    const on = p.stems.map((_, i) => (p.solo.size ? p.solo.has(i) : !p.mute.has(i)));
    on.forEach((o, i) => {
      const g = p.gains[i];
      if (g) {
        if (instant) g.gain.value = o ? 1 : 0;
        else g.gain.setTargetAtTime(o ? 1 : 0, audio.ctx.currentTime, 0.015);
      }
      p.cards[i].classList.toggle("is-off", !o);
    });
    p.rows.forEach((r) => r.classList.toggle("is-off", !on[+r.dataset.row]));
  }

  players.forEach((p) => {
    p.play.addEventListener("click", () => {
      if (p.root.classList.contains("is-loading")) {
        program = null;
        setSound("off");
        mdSay(p, "Cancelled. Press play when you're ready.");
        return;
      }
      if (p.playing) mdPause(p);
      else mdPlay(p);
    });
    p.seeks.forEach((b) =>
      b.addEventListener("click", () => {
        const t = +b.dataset.mdSeek;
        if (p.playing) mdSeek(p, t);
        else mdPlay(p, t);
        mdPaint(p, t);
      })
    );
    // Click the lanes to play from there, like clicking the ruler in the app
    p.lane.addEventListener("pointerdown", (e) => {
      const r = p.lane.getBoundingClientRect();
      const t = clamp(((e.clientX - r.left) / r.width) * p.loop, 0, p.loop - 0.02);
      if (p.playing) mdSeek(p, t);
      else if (!p.root.classList.contains("is-loading")) {
        mdPaint(p, t);
        mdPlay(p, t);
      }
    });
    $$("[data-mute]", p.root).forEach((b) =>
      b.addEventListener("click", () => {
        const i = +b.dataset.mute;
        if (p.mute.has(i)) p.mute.delete(i);
        else p.mute.add(i);
        b.setAttribute("aria-pressed", String(p.mute.has(i)));
        mdApply(p, false);
      })
    );
    $$("[data-solo]", p.root).forEach((b) =>
      b.addEventListener("click", () => {
        const i = +b.dataset.solo;
        if (p.solo.has(i)) p.solo.delete(i);
        else p.solo.add(i);
        b.setAttribute("aria-pressed", String(p.solo.has(i)));
        mdApply(p, false);
      })
    );
    // Space plays, like in the app, while focus is inside the player
    p.root.addEventListener("keydown", (e) => {
      if (e.code !== "Space" || e.target.closest("button")) return;
      e.preventDefault();
      p.play.click();
    });
    // Switching to the other song stops this one
    new MutationObserver(() => {
      if (!p.root.hidden) return;
      if (p.root.classList.contains("is-loading") && md === p && program === "md") {
        program = null;
        setSound("off");
      }
      if (p.playing) mdPause(p);
    }).observe(p.root, { attributes: true, attributeFilter: ["hidden"] });
    mdPaint(p, 0);
  });

  tasks.push(function mdTask() {
    let any = false;
    for (const p of players) {
      if (!p.playing) continue;
      any = true;
      mdPaint(p, mdPos(p));
      for (let i = 0; i < p.analysers.length; i++) {
        p.analysers[i].getFloatTimeDomainData(p.buf);
        let s = 0;
        for (let k = 0; k < p.buf.length; k += 2) s += p.buf[k] * p.buf[k];
        const rms = Math.sqrt(s / (p.buf.length / 2));
        const v = clamp((20 * Math.log10(rms + 1e-6) + 46) / 40);
        p.levels[i] = v > p.levels[i] ? v : p.levels[i] * 0.9;
        p.meters[i].style.setProperty("--m", p.levels[i].toFixed(3));
      }
    }
    return any;
  });

  /* ----------------------------------------------------------------- copy */
  const CHECK = '<path d="M5 12.5l4.5 4.5L19 7.5"/>';
  function copyFallback(text) {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (e) {
      ok = false;
    }
    ta.remove();
    return ok;
  }
  $$("[data-copy]").forEach((b) => {
    const label = $(".copy-label", b);
    const svg = $(".copy-state svg", b);
    const icon = svg ? svg.innerHTML : "";
    b.setAttribute("aria-label", `Copy: ${b.dataset.copy}`);
    b.addEventListener("click", async () => {
      const text = b.dataset.copy;
      let ok = false;
      try {
        await navigator.clipboard.writeText(text);
        ok = true;
      } catch (e) {
        ok = copyFallback(text);
      }
      clearTimeout(b._t);
      if (ok) {
        b.classList.add("is-copied");
        if (label) label.textContent = "Copied";
        if (svg) svg.innerHTML = CHECK;
        announce("Copied to the clipboard.");
      } else {
        if (label) label.textContent = "Select it";
        const code = $("code", b);
        const range = document.createRange();
        range.selectNodeContents(code);
        const sel = getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
      }
      b._t = setTimeout(() => {
        b.classList.remove("is-copied");
        if (label) label.textContent = "Copy";
        if (svg) svg.innerHTML = icon;
      }, 1800);
    });
  });

  /* ----------------------------------------------------------------- tabs */
  $$("[data-tabs]").forEach((root) => {
    const list = $('[role="tablist"]', root);
    const tabs = $$('[role="tab"]', root);
    const select = (tab, focus) => {
      tabs.forEach((t) => {
        const on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
      if (focus) tab.focus();
      const l = tab.offsetLeft - list.offsetLeft;
      if (l < list.scrollLeft || l + tab.offsetWidth > list.scrollLeft + list.clientWidth) {
        list.scrollTo({ left: l - 16, behavior: reduced ? "auto" : "smooth" });
      }
    };
    tabs.forEach((t, i) => {
      t.addEventListener("click", () => select(t));
      t.addEventListener("keydown", (e) => {
        let j = null;
        if (e.key === "ArrowRight") j = (i + 1) % tabs.length;
        else if (e.key === "ArrowLeft") j = (i - 1 + tabs.length) % tabs.length;
        else if (e.key === "Home") j = 0;
        else if (e.key === "End") j = tabs.length - 1;
        if (j === null) return;
        e.preventDefault();
        select(tabs[j], true);
      });
    });
    // Warm the hidden panels' images before anyone switches to them
    new IntersectionObserver(
      ([e], obs) => {
        if (!e.isIntersecting) return;
        $$("img[loading='lazy']", root).forEach((img) => (img.loading = "eager"));
        obs.disconnect();
      },
      { rootMargin: "100% 0px" }
    ).observe(root);
  });

  /* --------------------------------------------------------------- reveal */
  const revealIO = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.classList.add("is-in");
        revealIO.unobserve(e.target);
      }
    },
    { rootMargin: "0px 0px -6% 0px", threshold: 0.06 }
  );
  $$(".reveal").forEach((el) => {
    const sibs = Array.from(el.parentElement.children).filter((c) => c.classList.contains("reveal"));
    const k = sibs.indexOf(el);
    if (k > 0 && !el.style.transitionDelay) el.style.transitionDelay = `${Math.min(k, 5) * 70}ms`;
    revealIO.observe(el);
  });

  /* ------------------------------------------------------------------ nav */
  const nav = $("[data-nav]");
  const navLinks = $$(".nav__links a");
  const onScroll = () => {
    nav.classList.toggle("is-scrolled", window.scrollY > 8);
    wake();
  };
  window.addEventListener("scroll", onScroll, { passive: true });
  const sectionIO = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        navLinks.forEach((a) => a.classList.toggle("is-current", a.getAttribute("href") === `#${e.target.id}`));
      }
    },
    { rootMargin: "-45% 0px -50% 0px" }
  );
  ["replay", "agents", "craft", "listen", "studio", "open", "download"].forEach((id) => {
    const el = document.getElementById(id);
    if (el) sectionIO.observe(el);
  });

  /* --------------------------------------------------------------- finale */
  const finale = $("[data-finale]");
  let finaleVisible = false;
  if (finale) {
    new IntersectionObserver(([e]) => {
      finaleVisible = e.isIntersecting;
      if (finaleVisible) wake();
    }).observe(finale);
  }
  tasks.push(function finaleTask() {
    if (!finale || !finaleVisible || reduced) return false;
    const r = finale.getBoundingClientRect();
    const p = clamp((window.innerHeight - r.top) / (r.height * 0.85));
    finale.style.setProperty("--flood", (0.12 + smooth(p) * 0.88).toFixed(3));
    return false;
  });

  /* ---------------------------------------------------------------- setup */
  let resizeTimer = 0;
  window.addEventListener("resize", () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      replayLayout();
      hairSize();
    }, 120);
  });
  const onMotion = (e) => {
    reduced = e.matches;
    replayLayout();
  };
  if (motionQuery.addEventListener) motionQuery.addEventListener("change", onMotion);
  else if (motionQuery.addListener) motionQuery.addListener(onMotion);

  hairSize();
  replayLayout();
  onScroll();
  setSound("off");
})();
