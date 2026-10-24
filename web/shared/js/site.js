/* Vintage site script, shared by the landing page and the app. No dependencies. The only network
   calls go to the Vintage API. Each part runs only if its elements are on the page. */
(function () {
  'use strict';
  var root = document.documentElement;
  var RM = matchMedia('(prefers-reduced-motion: reduce)').matches;
  var API = 'https://api.usevintage.site';
  var IRONWOOD = 3428143; /* NU6.3 activation on mainnet: nothing in Ironwood predates it */
  function $(s, c) { return (c || document).querySelector(s); }
  function $$(s, c) { return Array.prototype.slice.call((c || document).querySelectorAll(s)); }
  function fmt(n) { return Math.round(n).toLocaleString('en-US'); }
  function esc(t) { return String(t).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  /* Every API call: no cookies, no referrer, no cache. Resolves to {ok, status, body}; status 0 = unreachable. */
  function api(path) {
    return fetch(API + path, { credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store' }).then(function (r) {
      return r.json().then(function (j) { return { ok: r.ok, status: r.status, body: j }; }, function () { return { ok: false, status: r.status, body: null }; });
    }, function () { return { ok: false, status: 0, body: null }; });
  }

  /* ---- theme ---- */
  var tg = $('#toggle');
  function cur() { var a = root.getAttribute('data-theme'); return a || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'); }
  function paint() { $$('span', tg).forEach(function (s) { s.classList.toggle('on', s.dataset.t === cur()); }); }
  tg.addEventListener('click', function () { var n = cur() === 'dark' ? 'light' : 'dark'; root.setAttribute('data-theme', n); try { localStorage.setItem('vt-theme', n); } catch (e) {} paint(); });
  paint();

  /* ---- full-bleed bands end at the scrollbar, not under it (vintage.css uses --vt-sbw) ---- */
  function sbw() { root.style.setProperty('--vt-sbw', Math.max(0, innerWidth - root.clientWidth) + 'px'); }
  sbw(); addEventListener('resize', sbw);

  /* ---- nav border after scroll ---- */
  var nav = $('#nav');
  function onScroll() { nav.classList.toggle('scrolled', scrollY > 8); }
  addEventListener('scroll', onScroll, { passive: true }); onScroll();

  /* ---- headline: release the rise mask once done ---- */
  var mega = $('#mega');
  if (mega) setTimeout(function () { mega.classList.add('done'); }, RM ? 0 : 1300);

  /* ---- tape builder (hero and verifier share it) ---- */
  /* It draws the real mainnet demo payment da9017e0…: anchor 3,525,696, event 3,525,709, mined 3,527,813. */
  var A = 3525696, E = 3525709;
  function buildTape(tape, o) {
    o = o || {};
    var box = $('.cells', tape), w = tape.clientWidth - (o.mini ? 24 : 48);
    var mob = innerWidth < 768, cw = o.mini ? 16 : (mob ? 16 : 28);
    var n = Math.max(12, Math.min(o.mini ? 32 : 56, Math.floor(w / cw)));
    var ia = Math.round(n * .26), ii = Math.round(n * .5), per = (E - A) / (ii - ia);
    var els = [];
    box.innerHTML = '';
    function chip(t) { var c = document.createElement('span'); c.className = 'chip'; c.textContent = t; return c; }
    for (var i = 0; i < n; i++) {
      var b = document.createElement('div'); b.className = 'blk';
      if (i === ii) {
        b.classList.add('inc');
        var u = document.createElement('div'); u.className = 'up';
        u.appendChild(chip(o.mini || mob ? '◆ Event' : '◆ Event · block ' + fmt(E))); b.appendChild(u);
      }
      if (i === ia) {
        var d = document.createElement('div'); d.className = 'dn';
        d.appendChild(document.createElement('b'));
        d.appendChild(chip(o.mini || mob ? 'Anchor' : 'Your coins · ' + fmt(A))); b.appendChild(d);
      }
      var mid = Math.round((ia + ii) / 2);
      if (!o.mini && !mob && i % 8 === 4 && Math.abs(i - ia) > 3 && Math.abs(i - ii) > 3 && Math.abs(i - mid) > 3 && i < n - 3) {
        var h = document.createElement('span'); h.className = 'hl label'; h.textContent = fmt(A + (i - ia) * per); b.appendChild(h);
      }
      if (!o.mini && i === n - 1) { var t = document.createElement('span'); t.className = 'hl label'; t.style.color = 'var(--vt-ink)'; t.textContent = '▸ Tip'; b.appendChild(t); }
      box.appendChild(b); els.push(b);
    }
    function x(i) { var r = els[i].getBoundingClientRect(), R = tape.getBoundingClientRect(); return r.left - R.left + r.width / 2; }
    return {
      els: els, ia: ia, ii: ii, n: n, x: x,
      show: function (instant) { els.forEach(function (e, k) { if (instant) e.classList.add('on'); else setTimeout(function () { e.classList.add('on'); }, k * 18); }); },
      reset: function () { els.forEach(function (e) { e.classList.remove('past', 'g', 'anc'); }); },
      /* scan from tip back to the event; then call done() */
      scan: function (ms, onStep, done) {
        var scan = $('.scanl', tape), from = n - 1, steps = from - ii, k = 0;
        if (RM) { els.forEach(function (e, j) { if (j > ii) e.classList.add('past'); }); if (onStep) onStep(1); done(); return; }
        scan.style.opacity = 1;
        var iv = setInterval(function () {
          var at = from - k; scan.style.left = (x(at) - 1) + 'px';
          if (at < n - 1) els[at + 1].classList.add('past');
          if (onStep) onStep(Math.min(1, k / steps));
          if (at <= ii) { clearInterval(iv); scan.style.opacity = 0; done(); }
          k++;
        }, Math.max(24, ms / steps));
      },
      pass: function (done) {
        els[ia].classList.add('anc');
        els.forEach(function (e, j) { if (j > ii) e.classList.remove('past'); });
        for (var g = ia; g < ii; g++) (function (g) { setTimeout(function () { els[g].classList.add('g'); }, RM ? 0 : (g - ia) * 45); })(g);
        setTimeout(done || function () {}, RM ? 0 : (ii - ia) * 45 + 150);
      }
    };
  }

  /* ---- hero tape (landing) ---- */
  var heroTape = $('#tape');
  if (heroTape) {
    var heroSeal = $('.seal', heroTape), hero = buildTape(heroTape);
    var placeSeal = function () { var mid = Math.round((hero.ia + hero.ii) / 2); heroSeal.style.left = (hero.x(mid) - (innerWidth < 768 ? 44 : 56)) + 'px'; };
    if (RM) { hero.show(true); hero.pass(); placeSeal(); heroSeal.style.opacity = 1; }
    else {
      hero.show(false);
      setTimeout(function () {
        hero.scan(1100, null, function () { setTimeout(function () { hero.pass(function () { placeSeal(); heroSeal.classList.add('go'); }); }, 300); });
      }, hero.n * 18 + 300);
    }
  }

  /* ---- live chain tip ---- */
  var liveTip = null, onTip = null;
  api('/health').then(function (r) {
    if (!r.ok || !r.body || !r.body.chain_tip) return;
    liveTip = r.body.chain_tip;
    var lt = $('#livetip');
    if (lt) { lt.textContent = 'Mainnet tip ' + fmt(liveTip); $('#live').hidden = false; }
    if (onTip) onTip();
  });

  /* ---- verifier (app): live, against the Vintage API ---- */
  var form = $('#vform');
  if (form) (function () {
    var EXAMPLE_TX = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7'; /* the mainnet demo deposit */
    var EXAMPLE_EVENT = 'vintage-demo-2026-10';
    var input = $('#txid'), err = $('#txerr'), verdict = $('#verdict'), vseal = $('#vseal');
    var tk1 = $('#tk1'), tk2 = $('#tk2'), vmeta = $('#vmeta'), chips = $('#chips'), custom = $('#customchip');
    var cfield = $('#customfield'), cinput = $('#customh'), herr = $('#herr');
    var vt = buildTape($('#vtape'), { mini: true }); vt.show(true);
    var busy = false;
    onTip = function () { if (!busy) tk2.textContent = 'Block ' + fmt(liveTip); };
    if (liveTip) onTip();

    function selectChip(c) {
      $$('.chip[aria-pressed]', chips).forEach(function (o) { o.setAttribute('aria-pressed', o === c ? 'true' : 'false'); });
      cfield.hidden = c !== custom;
      if (c === custom) cinput.focus(); else herr.hidden = true;
    }
    chips.addEventListener('click', function (e) { var c = e.target.closest('.chip[aria-pressed]'); if (c) selectChip(c); });
    /* A short name for a registry event: the label before any colon, or its first word if that is long. */
    function shortName(label) { var s = String(label).split(':')[0]; return s.length > 20 ? s.split(' ')[0] : s; }

    var pasteBtn = $('#paste');
    if (!navigator.clipboard || !navigator.clipboard.readText) pasteBtn.hidden = true;
    else pasteBtn.addEventListener('click', function () { navigator.clipboard.readText().then(function (t) { input.value = t.trim(); input.focus(); }).catch(function () {}); });

    function setVerdict(kind, label, title, body) {
      verdict.className = 'v ' + kind;
      verdict.innerHTML = '<div class="label">' + esc(label) + '</div>' + (title ? '<h3 class="display">' + esc(title) + '</h3>' : '') + (body ? '<p' + (kind === 'idle' ? ' style="font-family:var(--vt-font-mono);font-size:14px"' : '') + '>' + esc(body) + '</p>' : '');
    }
    function bar() { var b = document.createElement('div'); b.className = 'seg-bar'; for (var i = 0; i < 24; i++) b.appendChild(document.createElement('i')); verdict.appendChild(b); return $$('i', b); }

    /* What the user picked: { before, h, name } or null (with the error shown). */
    function picked() {
      var c = $('.chip[aria-pressed="true"]', chips);
      if (!c) return null;
      if (c !== custom) return { before: c.dataset.before, h: +c.dataset.h, name: c.dataset.name };
      var v = cinput.value.trim().replace(/[,\s]/g, '');
      if (!/^[0-9]+$/.test(v) || +v <= IRONWOOD) { herr.hidden = false; cinput.setAttribute('aria-invalid', 'true'); cinput.focus(); return null; }
      herr.hidden = true; cinput.removeAttribute('aria-invalid');
      return { before: v, h: +v, name: null };
    }
    function eventPhrase(ev) { return ev.name ? ev.name + ' at block ' + fmt(ev.h) : 'block ' + fmt(ev.h); }

    /* Back to the empty state: an old verdict (above all a gold one) must never sit next to new input. */
    function idle() {
      setVerdict('idle', 'Awaiting txid', '', "> Paste a Zcash txid. We'll check whether every coin it spends was already in the shielded pool before the event you pick.");
      tk1.textContent = 'Roots checked 0';
      if (liveTip) tk2.textContent = 'Block ' + fmt(liveTip);
    }

    function run(tx) {
      if (busy) return;
      vseal.classList.remove('go'); vseal.style.opacity = 0; vt.reset(); vmeta.textContent = '';
      if (!/^[0-9a-f]{64}$/i.test(tx)) { idle(); err.hidden = false; input.setAttribute('aria-invalid', 'true'); input.focus(); return; }
      err.hidden = true; input.removeAttribute('aria-invalid');
      var ev = picked();
      if (!ev) { idle(); return; }
      busy = true;
      setVerdict('check', 'Checking', 'Reading the tree, block by block.');
      var cells = bar(), k = 0, resp = null, scanned = false;
      tk1.textContent = 'Roots checked …';
      /* The seg-bar is the one thing that loops, and only while the request is pending. */
      var loop = setInterval(function () { k = (k + 1) % (cells.length + 1); cells.forEach(function (c, i) { c.classList.toggle('f', i < k); }); }, RM ? 1000 : 80);
      vt.scan(RM ? 0 : 1600, function (p) {
        if (liveTip) tk2.textContent = 'Block ' + fmt(liveTip - (liveTip - ev.h) * p);
      }, function () { scanned = true; finish(); });
      api('/verify/' + tx + '?before=' + encodeURIComponent(ev.before)).then(function (r) { resp = r; finish(); });

      function finish() {
        if (!scanned || !resp) return;
        clearInterval(loop);
        cells.forEach(function (c) { c.classList.add('f'); });
        var d = resp.body;
        if (!resp.ok) {
          vt.reset();
          if (resp.status === 0) setVerdict('idle', 'Not checked', '', "Couldn't reach the verifier. Try again in a minute, or run vintage verify on your own machine to involve nobody.");
          else if (resp.status === 404) setVerdict('idle', 'Not found', '', "No mainnet transaction with that txid, or it hasn't reached the server yet. Testnet transactions aren't checked here.");
          else setVerdict('idle', 'Not checked', '', (d && d.error) || 'The verifier could not check this transaction.');
          if (liveTip) tk2.textContent = 'Block ' + fmt(liveTip);
          busy = false; return;
        }
        var roots = d.evidence ? d.evidence.distinct_roots : 0;
        tk1.textContent = 'Roots checked ' + fmt(roots) + ' / ' + fmt(roots);
        tk2.textContent = 'Block ' + fmt(d.chain_tip);
        vmeta.textContent = (roots ? fmt(roots) + ' roots · ' : '') + d.elapsed_secs.toFixed(2) + ' s · mainnet · live';
        if (d.provisional) {
          vt.reset();
          setVerdict('idle', 'Not mined yet', '', "This transaction isn't in a block yet, so its anchor could still change. Check again once it's mined.");
          busy = false; return;
        }
        if (d.verdict === 'PASS') {
          vt.pass(function () {
            setVerdict('pass', 'Pass', 'These coins existed before ' + eventPhrase(ev) + '.', 'This proves nothing about who sent them or how much, only when they last moved.');
            $('#vsealtext').textContent = 'PROVEN OLDER THAN BLOCK ' + fmt(ev.h) + ' · MAINNET ·';
            vseal.style.opacity = ''; if (RM) vseal.style.opacity = 1; else vseal.classList.add('go');
            busy = false;
          });
          return;
        }
        if (d.verdict === 'FAIL' && d.matched_heights) {
          var after = Math.min(vt.n - 2, vt.ii + 6);
          vt.els[after].classList.add('anc');
          for (var j = vt.ii + 1; j < after; j++) vt.els[j].classList.add('past');
          var lo = d.matched_heights[0];
          setVerdict('fail', 'Not provable', "This deposit can't be dated before " + eventPhrase(ev) + '.',
            d.root_existed_before_h
              ? 'Its anchor is the state of the pool at block ' + fmt(lo) + ", which hadn't changed since before the event, so its coins may still predate it. A spend anchored earlier would prove it."
              : 'Its anchor is the state of the pool at block ' + fmt(lo) + ', after the event, so its coins were last moved after it.');
        } else {
          vt.reset();
          var reasons = (d.reasons || []).join(', ');
          setVerdict('fail', 'Not provable', "This deposit can't be dated.", 'It has ' + (reasons || 'inputs Vintage cannot read') + '. Vintage can only date shielded Ironwood coins.');
        }
        busy = false;
      }
    }
    form.addEventListener('submit', function (e) { e.preventDefault(); run(input.value.trim().toLowerCase()); });

    function example(tx, eventId) {
      var c = $('.chip[data-before="' + eventId + '"]', chips);
      if (!c) { setVerdict('idle', 'Events unavailable', '', "Couldn't load the event list, so the example can't run. Try again in a minute."); return; }
      input.value = tx;
      selectChip(c);
      run(tx);
    }
    $('#example').addEventListener('click', function () { example(EXAMPLE_TX, EXAMPLE_EVENT); });

    api('/events').then(function (r) {
      var events = r.ok && r.body && r.body.events ? r.body.events : [];
      events.forEach(function (ev, i) {
        var c = document.createElement('button');
        c.type = 'button'; c.className = 'chip';
        c.setAttribute('aria-pressed', i === 0 ? 'true' : 'false');
        c.dataset.before = ev.id; c.dataset.h = ev.height; c.dataset.name = shortName(ev.label);
        c.title = ev.label;
        c.textContent = shortName(ev.label) + ' · ' + fmt(ev.height);
        chips.insertBefore(c, custom);
      });
      if (!events.length) {
        selectChip(custom);
        setVerdict('idle', 'Events unavailable', '', "Couldn't load the event list from the verifier. You can still check against a custom block height.");
      }
      /* A link like app.usevintage.site/#tx=<txid>&before=<event-id> runs that check. The fragment is
         never sent to any server; it is cleared from the address bar once read. */
      var m = /^#tx=([0-9a-f]{64})&before=([a-z0-9-]+)$/i.exec(location.hash);
      if (m) {
        history.replaceState(null, '', location.pathname);
        example(m[1].toLowerCase(), m[2]);
      }
    });
  })();

  /* ---- copy code ---- */
  var cc = $('#copycode');
  if (cc && navigator.clipboard) cc.addEventListener('click', function () { navigator.clipboard.writeText($('#code').textContent).then(function () { cc.textContent = 'Copied'; setTimeout(function () { cc.textContent = 'Copy'; }, 1600); }); });

  /* ---- counter: the API's anonymous total of checks ---- */
  var box = $('#counter'), bandIn = false, counted = false;
  function roll() { if (!counted) return; $$('.cell', box).forEach(function (c, ix) { var h = $('span', c).offsetHeight; setTimeout(function () { $('.strip', c).style.transform = 'translateY(-' + (+c.dataset.v * h) + 'px)'; }, RM ? 0 : ix * 120); }); }
  if (box) api('/stats').then(function (r) {
    if (!(r.ok && r.body && typeof r.body.total_checks === 'number')) { box.setAttribute('aria-label', 'Counter unavailable'); $('#countcap').textContent = 'Counter unavailable right now'; return; }
    var str = String(r.body.total_checks);
    str.split('').forEach(function (ch, idx) {
      if (idx > 0 && (str.length - idx) % 3 === 0) { var cm = document.createElement('span'); cm.className = 'comma'; cm.textContent = ','; box.appendChild(cm); }
      var cell = document.createElement('div'); cell.className = 'cell'; cell.dataset.v = ch;
      var s = document.createElement('div'); s.className = 'strip';
      for (var x = 0; x < 10; x++) { var sp = document.createElement('span'); sp.textContent = x; s.appendChild(sp); }
      cell.appendChild(s); box.appendChild(cell);
    });
    box.setAttribute('aria-label', fmt(r.body.total_checks) + ' checks run since launch');
    counted = true;
    if (bandIn) roll();
  });

  /* ---- reveal once ---- */
  function bandSeen() { bandIn = true; roll(); }
  if (!('IntersectionObserver' in window)) { $$('.reveal').forEach(function (el) { el.classList.add('in'); }); bandSeen(); return; }
  var io = new IntersectionObserver(function (es) {
    es.forEach(function (e) { if (e.isIntersecting) { e.target.classList.add('in'); if (box && e.target.contains(box)) bandSeen(); io.unobserve(e.target); } });
  }, { threshold: .12 });
  $$('.reveal').forEach(function (el) { io.observe(el); });
})();
