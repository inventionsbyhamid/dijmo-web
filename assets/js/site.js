/* Dijmo: the keystroke. Plain ES2020, no dependencies.
   Sound is off until the visitor turns it on. Haptics answer only the visitor's own actions. */
(() => {
  'use strict';

  const d = document;
  const root = d.documentElement;
  root.classList.add('js');

  const reduceQuery = matchMedia('(prefers-reduced-motion: reduce)');
  let still = reduceQuery.matches;
  const listen = (q, fn) => (q.addEventListener ? q.addEventListener('change', fn) : q.addListener(fn));
  listen(reduceQuery, (e) => { still = e.matches; });

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const rand = (a, b) => a + Math.random() * (b - a);
  const store = {
    get(k) { try { return localStorage.getItem(k); } catch (_) { return null; } },
    set(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* private mode */ } },
  };

  // Browsers ignore vibrate() and audio until the visitor has touched the page.
  let touched = false;
  for (const type of ['pointerdown', 'keydown', 'touchstart']) {
    addEventListener(type, () => { touched = true; }, { capture: true, passive: true });
  }

  /* ---------------------------------------------------------------- sound */

  const Sound = (() => {
    const KEY = 'dijmo:sound';
    // Where each sound sits in the sprite: [start, length] in seconds.
    const MAP = {
      k0: [0.12, 0.0647], k1: [0.3047, 0.0647], k2: [0.4894, 0.0597], k3: [0.6691, 0.0516],
      k4: [0.8407, 0.0646], k5: [1.0254, 0.0567], k6: [1.2021, 0.0634], k7: [1.3855, 0.0622],
      thock: [1.5677, 0.147], tick: [1.8346, 0.0554],
    };
    const FIRST_ONSET = 0.1283; // where k0 first rises above 0.02 in the source
    const AC = window.AudioContext || window.webkitAudioContext;

    let on = store.get(KEY) === 'on';
    let ctx = null;
    let master = null;
    let buffer = null;
    let failed = false;
    let shift = 0;
    let noise = null;
    let lastKey = -1;
    let voices = 0;
    let fetched = null;
    let decoding = null;

    const urls = () => {
      const probe = d.createElement('audio');
      const webm = probe.canPlayType && probe.canPlayType('audio/webm; codecs="opus"');
      const list = ['/assets/sounds/sfx.m4a'];
      if (webm) list.unshift('/assets/sounds/sfx.webm');
      return list;
    };

    // Fetch the bytes early (no audio context needed), decode after a gesture.
    function prefetch() {
      if (!fetched) {
        const list = urls();
        fetched = list.reduce(
          (p, url) => p.catch(() => fetch(url).then((r) => {
            if (!r.ok) throw new Error(String(r.status));
            return r.arrayBuffer().then((bytes) => ({ bytes, url }));
          })),
          Promise.reject(new Error('start')),
        );
        fetched.catch(() => {});
      }
      return fetched;
    }

    const decode = (bytes) => new Promise((res, rej) => {
      // The callback form still works on older Safari.
      const p = ctx.decodeAudioData(bytes, res, rej);
      if (p && p.catch) p.catch(rej);
    });

    // AAC adds encoder delay on some decoders: find the first onset and correct for it.
    function measureShift(buf) {
      const ch = buf.getChannelData(0);
      const end = Math.min(ch.length, Math.floor(buf.sampleRate * 0.5));
      for (let i = 0; i < end; i++) {
        if (Math.abs(ch[i]) > 0.02) {
          const s = i / buf.sampleRate - FIRST_ONSET;
          return Math.max(-0.01, Math.min(0.08, s));
        }
      }
      return 0;
    }

    function load() {
      if (decoding) return decoding;
      decoding = prefetch()
        .then(async ({ bytes, url }) => {
          try {
            return await decode(bytes);
          } catch (_) {
            // WebM said "maybe" but would not decode: try AAC.
            if (!url.endsWith('.m4a')) {
              const r = await fetch('/assets/sounds/sfx.m4a');
              return decode(await r.arrayBuffer());
            }
            throw _;
          }
        })
        .then((buf) => { buffer = buf; shift = measureShift(buf); })
        .catch(() => { failed = true; });
      return decoding;
    }

    function context() {
      if (ctx || !AC) return ctx;
      try {
        // Ambient audio follows the iPhone's silent switch.
        if (navigator.audioSession) navigator.audioSession.type = 'ambient';
      } catch (_) { /* not supported */ }
      try {
        ctx = new AC({ latencyHint: 'interactive' });
      } catch (_) {
        ctx = new AC();
      }
      master = ctx.createGain();
      master.gain.value = 0.6;
      master.connect(ctx.destination);
      return ctx;
    }

    function wake() {
      if (!on || !AC) return;
      context();
      if (ctx.state !== 'running') ctx.resume().catch(() => {});
      load();
    }

    function ready() {
      wake();
      return decoding || Promise.resolve();
    }

    // A refined synth for when the samples can't load.
    function synth(kind, gain, rate) {
      const c = ctx;
      const t = c.currentTime + 0.002;
      if (!noise) {
        noise = c.createBuffer(1, Math.floor(c.sampleRate * 0.08), c.sampleRate);
        const data = noise.getChannelData(0);
        for (let i = 0; i < data.length; i++) data[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / data.length, 5);
      }
      const out = c.createGain();
      out.gain.value = gain;
      out.connect(master);
      const click = (level, freq, q, dur) => {
        const src = c.createBufferSource();
        src.buffer = noise;
        src.playbackRate.value = rate * rand(0.85, 1.15);
        const bp = c.createBiquadFilter();
        bp.type = 'bandpass';
        bp.frequency.value = freq;
        bp.Q.value = q;
        const g = c.createGain();
        g.gain.setValueAtTime(level, t);
        g.gain.exponentialRampToValueAtTime(0.0008, t + dur);
        src.connect(bp).connect(g).connect(out);
        src.start(t);
        src.stop(t + dur + 0.02);
      };
      const body = (level, from, to, dur) => {
        const o = c.createOscillator();
        o.type = 'sine';
        o.frequency.setValueAtTime(from * rate, t);
        o.frequency.exponentialRampToValueAtTime(to * rate, t + dur * 0.8);
        const g = c.createGain();
        g.gain.setValueAtTime(0.0008, t);
        g.gain.exponentialRampToValueAtTime(level, t + 0.004);
        g.gain.exponentialRampToValueAtTime(0.0008, t + dur);
        o.connect(g).connect(out);
        o.start(t);
        o.stop(t + dur + 0.02);
      };
      if (kind === 'thock') {
        body(0.7, 150, 52, 0.2);
        click(0.25, 900, 0.9, 0.06);
      } else if (kind === 'tick') {
        click(0.12, 7000, 2.5, 0.025);
      } else {
        click(0.45, rand(1900, 3000), 1.3, 0.045);
        body(0.1, rand(170, 220), 90, 0.05);
      }
    }

    function play(name, gain, rate) {
      if (!on || !ctx || ctx.state !== 'running' || voices > 14) return;
      const kind = name[0] === 'k' ? 'key' : name;
      if (!buffer) {
        if (failed) synth(kind, gain, rate);
        return;
      }
      const [at, dur] = MAP[name];
      const src = ctx.createBufferSource();
      src.buffer = buffer;
      src.playbackRate.value = rate;
      const g = ctx.createGain();
      g.gain.value = gain;
      src.connect(g).connect(master);
      voices++;
      src.onended = () => { voices--; };
      src.start(ctx.currentTime, Math.max(0, at + shift - 0.003), dur + 0.01);
    }

    function key(soft = 1) {
      let i = Math.floor(Math.random() * 8);
      if (i === lastKey) i = (i + 1 + Math.floor(Math.random() * 7)) % 8;
      lastKey = i;
      play('k' + i, soft * rand(0.85, 1.15), rand(0.96, 1.04));
    }

    function set(value) {
      on = value;
      store.set(KEY, on ? 'on' : 'off');
      if (on) wake();
      else if (ctx && ctx.state === 'running') ctx.suspend().catch(() => {});
    }

    if (on && AC) {
      // Remembered from last time: get the bytes ready, start on the first gesture.
      const idle = window.requestIdleCallback || ((fn) => setTimeout(fn, 1200));
      idle(() => prefetch());
      const first = () => {
        wake();
        for (const type of ['pointerdown', 'keydown', 'touchend']) removeEventListener(type, first, true);
      };
      for (const type of ['pointerdown', 'keydown', 'touchend']) addEventListener(type, first, true);
    }

    return {
      get supported() { return Boolean(AC); },
      get on() { return on; },
      set,
      ready,
      key,
      thock: (soft = 1) => play('thock', soft * rand(0.92, 1.05), rand(0.97, 1.02)),
      tick: (soft = 1) => play('tick', 0.42 * soft * rand(0.85, 1.15), rand(0.95, 1.05)),
    };
  })();

  /* -------------------------------------------------------------- haptics */

  const Haptic = (() => {
    const canVibrate = typeof navigator.vibrate === 'function';
    // iOS 17.4+ has a native switch control; toggling it plays the system haptic (iOS 18+).
    const canSwitch = !canVibrate && 'switch' in HTMLInputElement.prototype;
    let label = null;

    function tap(ms) {
      if (still || !touched || !canVibrate) return;
      try { navigator.vibrate(ms); } catch (_) { /* blocked */ }
    }

    function enter() {
      if (still) return;
      if (canVibrate) { tap(18); return; }
      if (!canSwitch) return;
      if (!label) {
        label = d.createElement('label');
        label.className = 'haptic';
        label.setAttribute('aria-hidden', 'true');
        const box = d.createElement('input');
        box.type = 'checkbox';
        box.setAttribute('switch', '');
        box.tabIndex = -1;
        label.append(box);
        d.body.append(label);
      }
      label.click();
    }

    return { tap, enter };
  })();

  /* ---------------------------------------------------------- sound button */

  const soundButton = d.querySelector('[data-sound]');
  if (soundButton && Sound.supported) {
    const state = soundButton.querySelector('[data-sound-state]');
    const paint = () => {
      soundButton.setAttribute('aria-pressed', String(Sound.on));
      if (state) state.textContent = Sound.on ? 'on' : 'off';
    };
    paint();
    soundButton.hidden = false;
    soundButton.addEventListener('click', () => {
      Sound.set(!Sound.on);
      paint();
      if (Sound.on) Sound.ready().then(() => Sound.key(0.9));
    });
  }

  /* --------------------------------------------------------------- typing */

  const pause = (ch) => (ch === ' ' ? rand(70, 150) : rand(40, 95) + (Math.random() < 0.08 ? 150 : 0));

  // Types a heading letter by letter. Screen readers get the whole sentence at once.
  function typer(el) {
    const text = el.textContent.trim();
    const spoken = d.createElement('span');
    spoken.className = 'sr-only';
    spoken.textContent = text;
    const shown = d.createElement('span');
    shown.setAttribute('aria-hidden', 'true');
    const chars = Array.from(text, (c) => {
      const s = d.createElement('span');
      s.className = 'ch';
      s.textContent = c;
      shown.append(s);
      return s;
    });
    const caret = d.createElement('span');
    caret.className = 'caret';
    el.replaceChildren(spoken, shown);
    return {
      async run(soft) {
        if (still) {
          for (const s of chars) s.classList.add('on');
          shown.append(caret);
          return;
        }
        el.classList.add('is-typing');
        shown.prepend(caret);
        for (const s of chars) {
          s.classList.add('on');
          s.after(caret);
          if (s.textContent !== ' ') Sound.key(soft);
          await sleep(pause(s.textContent));
        }
        el.classList.remove('is-typing');
      },
    };
  }

  /* ------------------------------------------------------- story moments */

  const typers = new Map();
  for (const el of d.querySelectorAll('[data-type], [data-ask]')) typers.set(el, typer(el));
  const typerIn = (el, sel) => typers.get(el.querySelector(sel) || el);

  const at = (ms, fn) => setTimeout(fn, still ? 0 : ms);

  const moments = {
    problem() {
      [250, 550, 850, 1150].forEach((ms) => at(ms + 60, () => Sound.key(0.5)));
      at(1950, () => Sound.tick(0.8));
    },
    turn(el) {
      at(250, () => typerIn(el, '[data-type]').run(0.6));
    },
    record() {
      at(320, () => Sound.tick(0.7));
    },
    async say(el) {
      await typerIn(el, '[data-ask]').run(0.45);
      if (!still) await sleep(1300);
      el.classList.add('is-done');
      Sound.key(0.7);
      at(260, () => Sound.tick(0.8));
    },
    post() {
      at(200, () => Sound.tick(0.7));
    },
    promise() {
      at(2300, () => Sound.key(0.85));
    },
    outcome() {
      at(1700, () => Sound.tick());
    },
  };

  function fire(el) {
    el.classList.add('is-on');
    Haptic.tap(3);
    const fn = moments[el.dataset.moment];
    if (fn) fn(el);
  }

  const momentEls = d.querySelectorAll('[data-moment]');
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        io.unobserve(e.target);
        fire(e.target);
      }
    }, { rootMargin: '0px 0px -30% 0px' });
    momentEls.forEach((el) => io.observe(el));
  } else {
    momentEls.forEach(fire);
  }

  /* ------------------------------------------------------------ hero demo */

  function heroDemo(box) {
    const frame = box.querySelector('[data-frame]');
    const track = box.querySelector('[data-track]');
    const sweep = box.querySelector('[data-sweep]');
    const line = box.querySelector('[data-line]');
    const toggle = d.querySelector('[data-demo-toggle]');

    if (still) {
      frame.classList.add('is-vertical');
      return;
    }

    let paused = false;
    let visible = true;
    let release = null;
    const blocked = () => paused || !visible || d.hidden;
    const resume = () => {
      if (release && !blocked()) { const r = release; release = null; r(); }
    };
    const wait = async (ms) => {
      await sleep(ms);
      while (blocked()) await new Promise((r) => { release = r; });
    };

    if ('IntersectionObserver' in window) {
      new IntersectionObserver(([e]) => {
        visible = e.isIntersecting;
        box.classList.toggle('is-still', !visible || paused);
        resume();
      }).observe(box);
    }
    d.addEventListener('visibilitychange', resume);

    if (toggle) {
      toggle.hidden = false;
      toggle.addEventListener('click', () => {
        paused = !paused;
        toggle.textContent = paused ? 'Play the demo' : 'Pause the demo';
        box.classList.toggle('is-still', paused);
        resume();
      });
    }

    const run = (back) => {
      sweep.classList.remove('is-forward', 'is-back');
      void sweep.offsetWidth; // restart the animation
      sweep.classList.add(back ? 'is-back' : 'is-forward');
    };

    const steps = [
      { say: 'make it vertical for tiktok', land: 330, apply: () => frame.classList.add('is-vertical') },
      { say: 'add captions', land: 380, apply: () => frame.classList.add('has-captions') },
      {
        say: 'cut the long pause',
        land: 120,
        apply: async () => {
          track.classList.add('is-preview');
          await wait(650);
          track.classList.add('is-cut');
          Sound.tick(0.6);
        },
      },
      { say: 'warmer, like film', land: 60, apply: () => frame.classList.add('is-warm') },
      {
        say: 'undo all',
        back: true,
        land: 60,
        apply: () => {
          frame.classList.remove('is-vertical', 'has-captions', 'is-warm');
          track.classList.remove('is-preview', 'is-cut');
        },
      },
    ];

    async function type(text) {
      box.classList.add('is-typing');
      for (let i = 1; i <= text.length; i++) {
        line.textContent = text.slice(0, i);
        if (text[i - 1] !== ' ') Sound.key(0.4);
        await wait(pause(text[i - 1]));
      }
      box.classList.remove('is-typing');
    }

    async function erase() {
      const text = line.textContent;
      box.classList.add('is-typing');
      for (let i = text.length; i > 0; i -= 3) {
        line.textContent = text.slice(0, i);
        await wait(16);
      }
      line.textContent = '';
      box.classList.remove('is-typing');
    }

    (async () => {
      let first = true;
      for (;;) {
        for (const step of steps) {
          if (first) {
            await wait(1200); // the first request is already typed in the HTML
            first = false;
          } else {
            await type(step.say);
            await wait(rand(380, 520));
          }
          Sound.thock(0.45);
          run(step.back);
          await wait(step.land);
          Sound.tick(0.7);
          await step.apply();
          await wait(step.back ? 1500 : 2000);
          await erase();
          await wait(420);
        }
      }
    })();
  }

  const demo = d.querySelector('[data-demo]');
  if (demo) heroDemo(demo);

  /* ------------------------------------------------------ the waitlist line */

  const WORDS = {
    sent: 'Check your inbox. Open the link we sent to finish joining.',
    invalid: 'That email doesn’t look right. Check it and try again.',
    slow_down: 'Too many tries in a short time. Wait a few minutes, then try again.',
    error: 'Something went wrong on our side. Try again in a moment.',
    offline: 'That didn’t go through. Check your connection and try again.',
  };
  const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

  function joinForm(form) {
    const input = form.querySelector('[data-entry]');
    const status = form.querySelector('[data-join-status]');
    const mirror = form.querySelector('[data-mirror]');
    const caret = form.querySelector('[data-entry-caret]');
    const honeypot = form.elements.namedItem('website');
    const timing = form.elements.namedItem('t');
    let busy = false;

    form.noValidate = true;

    // A thick caret needs the caret position, which email inputs don't expose.
    // inputmode="email" keeps the email keyboard; the server checks the address anyway.
    try { input.type = 'text'; } catch (_) { /* keep email */ }
    const thickCaret = input.selectionStart !== null && mirror && caret;
    if (thickCaret) form.classList.add('has-caret');

    function place() {
      if (!thickCaret) return;
      const focused = d.activeElement === input;
      if (!focused || input.selectionStart !== input.selectionEnd || input.readOnly) {
        caret.classList.remove('on', 'blink');
        return;
      }
      mirror.textContent = input.value.slice(0, input.selectionStart);
      const x = Math.min(mirror.offsetWidth - input.scrollLeft, input.clientWidth);
      caret.style.transform = `translateX(${Math.max(0, x)}px)`;
      caret.classList.remove('blink');
      void caret.offsetWidth;
      caret.classList.add('on', 'blink');
    }

    for (const type of ['input', 'focus', 'click', 'keyup', 'select', 'scroll']) input.addEventListener(type, place);
    d.addEventListener('selectionchange', () => { if (d.activeElement === input) place(); });
    input.addEventListener('blur', () => caret && caret.classList.remove('on', 'blink'));
    addEventListener('resize', place, { passive: true });

    const tell = (word) => {
      status.textContent = '';
      requestAnimationFrame(() => { status.textContent = WORDS[word] || ''; });
    };

    input.addEventListener('input', (e) => {
      const deleting = e.inputType && e.inputType.startsWith('delete');
      Sound.key(deleting ? 0.6 : 1);
      Haptic.tap(5);
      if (input.getAttribute('aria-invalid')) {
        input.removeAttribute('aria-invalid');
        status.textContent = '';
      }
    });

    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      if (busy || form.classList.contains('is-sent')) return;
      const email = input.value.trim();
      if (!EMAIL.test(email) || email.length > 254) {
        input.setAttribute('aria-invalid', 'true');
        tell('invalid');
        Sound.key(0.6);
        input.focus();
        return;
      }

      // Enter lands: the thock, the tap, the playhead runs the line.
      busy = true;
      Sound.thock(1);
      Haptic.enter();
      form.classList.add('is-sending');

      let outcome = 'error';
      try {
        const res = await fetch(form.action, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
          credentials: 'same-origin',
          body: JSON.stringify({
            email,
            website: honeypot ? honeypot.value : '',
            t: timing ? timing.value : '',
          }),
        });
        const data = await res.json().catch(() => ({}));
        if (res.ok && data.ok) outcome = 'sent';
        else if (res.status === 429 || data.error === 'slow_down') outcome = 'slow_down';
        else if (res.status === 422 || data.error === 'invalid') outcome = 'invalid';
      } catch (_) {
        outcome = 'offline';
      }

      busy = false;
      form.classList.remove('is-sending');
      if (outcome === 'sent') {
        form.classList.add('is-sent');
        input.readOnly = true;
        input.blur();
        Sound.tick();
      } else if (outcome === 'invalid') {
        input.setAttribute('aria-invalid', 'true');
      }
      tell(outcome);
    });
  }

  const form = d.querySelector('[data-join]');
  if (form) joinForm(form);

  /* ---------------------------------------------- a small celebration edit */

  const party = d.querySelector('[data-celebrate]');
  if (party && !still) {
    setTimeout(() => {
      party.classList.add('is-on');
      const sweep = party.querySelector('[data-sweep]');
      if (sweep) sweep.classList.add('is-forward');
      Sound.thock(0.6);
    }, 500);
  }
})();
