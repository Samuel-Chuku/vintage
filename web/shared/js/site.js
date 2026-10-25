/* Vintage site script, on every page. No dependencies. The only network calls go to the Vintage API.
   Each part runs only if its elements are on the page. Shared helpers are exposed as window.VT for
   verify.js (the app) and sim.js (the simulation). */
(function () {
  'use strict';
  var root = document.documentElement;
  var RM = matchMedia('(prefers-reduced-motion: reduce)').matches;
  var API = 'https://api.usevintage.site';
  var SITES = ['https://usevintage.site', 'https://app.usevintage.site'];
  function $(s, c) { return (c || document).querySelector(s); }
  function $$(s, c) { return Array.prototype.slice.call((c || document).querySelectorAll(s)); }
  function fmt(n) { return Math.round(n).toLocaleString('en-US'); }
  function esc(t) { return String(t).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  /* Every API call: no cookies, no referrer, no cache. Resolves to {ok, status, body}; status 0 = unreachable. */
  function api(path, opts) {
    var o = { credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store' };
    if (opts && opts.body) { o.method = 'POST'; o.headers = { 'content-type': 'application/json' }; o.body = JSON.stringify(opts.body); }
    return fetch(API + path, o).then(function (r) {
      return r.json().then(function (j) { return { ok: r.ok, status: r.status, body: j }; }, function () { return { ok: false, status: r.status, body: null }; });
    }, function () { return { ok: false, status: 0, body: null }; });
  }
  function savedTheme() { try { return localStorage.getItem('vt-theme'); } catch (e) { return null; } }

  /* ---- theme ---- */
  var tg = $('#toggle');
  function cur() { var a = root.getAttribute('data-theme'); return a || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'); }
  function paint() { $$('span', tg).forEach(function (s) { s.classList.toggle('on', s.dataset.t === cur()); }); }
  tg.addEventListener('click', function () { var n = cur() === 'dark' ? 'light' : 'dark'; root.setAttribute('data-theme', n); try { localStorage.setItem('vt-theme', n); } catch (e) {} paint(); });
  paint();

  /* ---- full-bleed bands end at the scrollbar, not under it (vintage.css uses --vt-sbw) ---- */
  function sbw() { root.style.setProperty('--vt-sbw', Math.max(0, innerWidth - root.clientWidth) + 'px'); }
  sbw(); addEventListener('resize', sbw);

  /* ---- nav: border after scroll, mobile menu ---- */
  var nav = $('#nav');
  function onScroll() { nav.classList.toggle('scrolled', scrollY > 8); }
  addEventListener('scroll', onScroll, { passive: true }); onScroll();
  var mb = $('#menubtn'), mn = $('#mnav');
  function menu(open) { mb.setAttribute('aria-expanded', open ? 'true' : 'false'); mn.hidden = !open; }
  mb.addEventListener('click', function () { menu(mn.hidden); });
  addEventListener('keydown', function (e) { if (e.key === 'Escape' && !mn.hidden) { menu(false); mb.focus(); } });

  /* ---- navigation: carry the theme across our two sites, and fade out before leaving ----
     usevintage.site and app.usevintage.site keep separate browser storage, so a chosen theme travels in
     the URL fragment (#theme=dark), which is never sent to a server; the next page applies and removes it. */
  document.addEventListener('click', function (e) {
    var a = e.target.closest('a[href]');
    if (!a || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || a.target === '_blank' || a.hasAttribute('download')) return;
    var u;
    try { u = new URL(a.href, location.href); } catch (err) { return; }
    if (!/^https?:$/.test(u.protocol)) return;
    if (u.origin === location.origin && u.pathname === location.pathname && u.hash) return; /* in-page jump */
    var t = savedTheme();
    if (t && u.origin !== location.origin && SITES.indexOf(u.origin) >= 0) u.hash = (u.hash ? u.hash + '&' : '#') + 'theme=' + t;
    e.preventDefault();
    if (!mn.hidden) menu(false);
    if (RM) { location.href = u.href; return; }
    root.classList.add('leaving');
    setTimeout(function () { location.href = u.href; }, 200);
  });
  /* Coming back through the browser cache: undo the fade. */
  addEventListener('pageshow', function () { root.classList.remove('leaving'); });

  /* ---- headline: release the rise mask once done ---- */
  var mega = $('#mega');
  if (mega) setTimeout(function () { mega.classList.add('done'); }, RM ? 0 : 1300);

  /* ---- tape builder (hero, verifier) ---- */
  /* By default it draws the real mainnet demo payment da9017e0…: anchor 3,525,696, event 3,525,709. */
  function buildTape(tape, o) {
    o = o || {};
    var A = o.anchor || 3525696, E = o.event || 3525709;
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

  /* ---- hero tape (home) ---- */
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
  var tipWaiters = [];
  var VT = window.VT = { api: api, fmt: fmt, esc: esc, $: $, $$: $$, RM: RM, buildTape: buildTape, tip: null,
    onTip: function (f) { if (VT.tip) f(VT.tip); else tipWaiters.push(f); } };
  api('/health').then(function (r) {
    if (!r.ok || !r.body || !r.body.chain_tip) return;
    VT.tip = r.body.chain_tip;
    var lt = $('#livetip');
    if (lt) { lt.textContent = 'Mainnet tip ' + fmt(VT.tip); $('#live').hidden = false; }
    tipWaiters.forEach(function (f) { f(VT.tip); });
  });

  /* ---- home: mark the nav item whose section is on screen ---- */
  if (document.body.dataset.page === 'home' && 'IntersectionObserver' in window) {
    var spy = new IntersectionObserver(function (es) {
      es.forEach(function (e) {
        var key = e.target.dataset.spy;
        $$('[data-nav="' + key + '"]').forEach(function (a) { a.classList.toggle('here', e.isIntersecting); });
      });
    }, { rootMargin: '-40% 0px -55% 0px' });
    $$('[data-spy]').forEach(function (s) { spy.observe(s); });
  }

  /* ---- copy buttons: data-copy="<id of the element to copy>" ---- */
  $$('[data-copy]').forEach(function (b) {
    if (!navigator.clipboard) { b.hidden = true; return; }
    b.addEventListener('click', function () {
      navigator.clipboard.writeText($('#' + b.dataset.copy).textContent).then(function () { b.textContent = 'Copied'; setTimeout(function () { b.textContent = 'Copy'; }, 1600); });
    });
  });

  /* ---- code tabs: buttons [data-tab] in a .tabs group switch [data-panel] siblings ---- */
  $$('.tabs').forEach(function (g) {
    var box = g.parentElement;
    $$('[data-tab]', g).forEach(function (b) {
      b.addEventListener('click', function () {
        $$('[data-tab]', g).forEach(function (o) { o.setAttribute('aria-pressed', o === b ? 'true' : 'false'); });
        $$('[data-panel]', box).forEach(function (p) { p.hidden = p.dataset.panel !== b.dataset.tab; });
        var c = $('[data-copy]', box); if (c) c.dataset.copy = 'code-' + b.dataset.tab;
      });
    });
  });

  /* ---- events page: the live, signed registry ---- */
  var reg = $('#registry');
  if (reg) api('/events').then(function (r) {
    var evs = r.ok && r.body && r.body.events ? r.body.events : [];
    var tb = $('tbody', reg);
    if (!evs.length) { tb.innerHTML = '<tr><td colspan="5">Couldn\'t load the registry right now. The signed file is in the repository: registry/events.json.</td></tr>'; return; }
    tb.innerHTML = evs.map(function (ev) {
      var src = (ev.sources || []).filter(function (s) { return /^https:\/\//.test(s); }).map(function (s, i) { return '<a href="' + esc(s) + '" rel="noopener noreferrer">source ' + (i + 1) + '</a>'; }).join(' ');
      var check = 'https://app.usevintage.site/#before=' + encodeURIComponent(ev.id);
      return '<tr><td><span class="tag ink">' + esc(String(ev.type).replace('_', ' ')) + '</span></td><td>' + esc(ev.label) + (src ? '<br>' + src : '') +
        '</td><td class="num">' + fmt(ev.height) + '</td><td class="num">' + esc(ev.id) + '</td><td class="num"><a href="' + check + '">Check a deposit</a></td></tr>';
    }).join('');
    var meta = $('#regmeta');
    if (meta) meta.textContent = 'Signature verified by the API · public key ' + esc(String(r.body.public_key).slice(0, 8)) + '…' + esc(String(r.body.public_key).slice(-4));
  });

  /* ---- home page: check any txid's age, inline (GET /age). Neutral styling: gold is kept for a
     proven PASS against an event, which happens in the verifier. ---- */
  var hform = $('#hcheck');
  if (hform) {
    var htx = $('#htx'), hres = $('#hres'), hterr = $('#hterr'), hnext = $('#hnext'), hbusy = false;
    var MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    var utc = function (t) {
      var d = new Date(t * 1000), p = function (n) { return (n < 10 ? '0' : '') + n; };
      return d.getUTCDate() + ' ' + MONTHS[d.getUTCMonth()] + ' ' + d.getUTCFullYear() + ', ' + p(d.getUTCHours()) + ':' + p(d.getUTCMinutes()) + ' UTC';
    };
    var shortName = function (label) { var s = String(label).split(':')[0]; return s.length > 20 ? s.split(' ')[0] : s; };
    var show = function (kind, label, title, body) {
      hres.className = 'v ' + kind;
      hres.innerHTML = '<div class="label">' + esc(label) + '</div>' + (title ? '<h3 class="display">' + esc(title) + '</h3>' : '') + (body ? '<p>' + esc(body) + '</p>' : '');
    };
    var check = function (tx) {
      if (hbusy) return;
      hnext.hidden = true;
      if (!/^[0-9a-f]{64}$/.test(tx)) { hterr.hidden = false; htx.setAttribute('aria-invalid', 'true'); htx.focus(); return; }
      hterr.hidden = true; htx.removeAttribute('aria-invalid');
      hbusy = true;
      show('check', 'Checking', 'Finding where its anchor sits in the chain.');
      api('/age/' + tx).then(function (r) {
        hbusy = false;
        var d = r.body;
        if (!r.ok) {
          if (r.status === 0) show('idle', 'Not checked', '', "Couldn't reach the verifier. Try again in a minute, or run vintage age on your own machine.");
          else if (r.status === 404 && !(d && d.error)) show('idle', 'Not available', '', "Age lookups aren't on the verifier yet. Open the verifier to check against an event.");
          else if (r.status === 404) show('idle', 'Not found', '', "No mainnet transaction with that txid, or it hasn't reached the server yet.");
          else show('idle', 'Not checked', '', (d && d.error) || 'The verifier could not date this transaction.');
        } else if (d.verdict !== 'DATED') {
          show('idle', 'Not provable', "This transaction can't be dated.", 'It has ' + ((d.reasons || []).join(', ') || 'inputs Vintage cannot read') + '. Vintage only dates shielded Ironwood coins.');
        } else {
          var a = d.age;
          show('idle', d.provisional ? 'Dated · not mined yet' : 'Dated · ' + d.elapsed_secs.toFixed(2) + ' s', 'In the pool by ' + utc(a.existed_by_time) + '.',
            'Every coin it spends was in the shielded pool ' + (a.exact ? 'by' : 'at least by') + ' block ' + fmt(a.existed_by_height) + '.');
          var evs = d.events || [];
          if (evs.length) {
            var ul = document.createElement('ul'); ul.className = 'ticks';
            evs.forEach(function (e) {
              var li = document.createElement('li');
              li.dataset.g = e.predates ? '✓' : '×';
              li.textContent = (e.predates ? 'Predates ' : 'After ') + shortName(e.label) + ', block ' + fmt(e.height);
              ul.appendChild(li);
            });
            hres.appendChild(ul);
          }
        }
        if (r.status !== 0) {
          hnext.innerHTML = 'Check it against one event or any block in the <a href="https://app.usevintage.site/#tx=' + tx + '">verifier</a>.';
          hnext.hidden = false;
        }
      });
    };
    hform.addEventListener('submit', function (e) { e.preventDefault(); check(htx.value.trim().toLowerCase()); });
    $('#hexample').addEventListener('click', function () {
      htx.value = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7';
      check(htx.value);
    });
  }

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
