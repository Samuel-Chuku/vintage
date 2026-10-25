/* Vintage verifier (app.usevintage.site): live checks against the Vintage API. Needs site.js (window.VT). */
(function () {
  'use strict';
  var VT = window.VT, $ = VT.$, $$ = VT.$$, fmt = VT.fmt, esc = VT.esc, RM = VT.RM;
  var IRONWOOD = 3428143; /* NU6.3 activation on mainnet: nothing in Ironwood predates it */
  var EXAMPLE_TX = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7'; /* the mainnet demo deposit */
  var EXAMPLE_EVENT = 'vintage-demo-2026-10';
  var form = $('#vform'), input = $('#txid'), err = $('#txerr'), verdict = $('#verdict'), vseal = $('#vseal');
  var tk1 = $('#tk1'), tk2 = $('#tk2'), vmeta = $('#vmeta'), chips = $('#chips'), custom = $('#customchip'), agechip = $('#agechip');
  var cfield = $('#customfield'), cinput = $('#customh'), herr = $('#herr');
  var vt = VT.buildTape($('#vtape'), { mini: true }); vt.show(true);
  var busy = false;
  VT.onTip(function (t) { if (!busy) tk2.textContent = 'Block ' + fmt(t); });

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
  /* Bring a finished result into view when it is below the screen (phones), without moving a visible one. */
  function reveal() { var r = verdict.getBoundingClientRect(); if (r.bottom > window.innerHeight || r.top < 0) verdict.scrollIntoView({ block: 'nearest', behavior: RM ? 'auto' : 'smooth' }); }
  function bar() { var b = document.createElement('div'); b.className = 'seg-bar'; for (var i = 0; i < 24; i++) b.appendChild(document.createElement('i')); verdict.appendChild(b); return $$('i', b); }

  /* What the user picked: { before, h, name } or null (with the error shown). */
  function picked() {
    var c = $('.chip[aria-pressed="true"]', chips);
    if (!c) return null;
    if (c === agechip) return { age: true };
    if (c !== custom) return { before: c.dataset.before, h: +c.dataset.h, name: c.dataset.name };
    var v = cinput.value.trim().replace(/[,\s]/g, '');
    if (!/^[0-9]+$/.test(v) || +v <= IRONWOOD) { herr.hidden = false; cinput.setAttribute('aria-invalid', 'true'); cinput.focus(); return null; }
    herr.hidden = true; cinput.removeAttribute('aria-invalid');
    return { before: v, h: +v, name: null };
  }
  function eventPhrase(ev) { return ev.name ? ev.name + ' at block ' + fmt(ev.h) : 'block ' + fmt(ev.h); }
  var MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  /* Block time (unix seconds) as "21 Oct 2026, 16:03 UTC". */
  function utc(t) {
    var d = new Date(t * 1000), p = function (n) { return (n < 10 ? '0' : '') + n; };
    return d.getUTCDate() + ' ' + MONTHS[d.getUTCMonth()] + ' ' + d.getUTCFullYear() + ', ' + p(d.getUTCHours()) + ':' + p(d.getUTCMinutes()) + ' UTC';
  }
  /* "in the pool by block 3,525,693 (21 Oct 2026, 16:03 UTC)" from a report's age. */
  function agePhrase(a) { return 'in the pool ' + (a.exact ? 'by' : 'at least by') + ' block ' + fmt(a.existed_by_height) + ' (' + utc(a.existed_by_time) + ')'; }

  /* Back to the empty state: an old verdict (above all a gold one) must never sit next to new input. */
  function idle() {
    setVerdict('idle', 'Awaiting txid', '', '> Paste a Zcash txid, then pick an event to check against, or Age only to see how old its coins are.');
    tk1.textContent = 'Roots checked 0';
    if (VT.tip) tk2.textContent = 'Block ' + fmt(VT.tip);
  }

  function run(tx) {
    if (busy) return;
    vseal.classList.remove('go'); vseal.style.opacity = 0; vt.reset(); vmeta.textContent = '';
    if (!/^[0-9a-f]{64}$/i.test(tx)) { idle(); err.hidden = false; input.setAttribute('aria-invalid', 'true'); input.focus(); return; }
    err.hidden = true; input.removeAttribute('aria-invalid');
    var ev = picked();
    if (!ev) { idle(); return; }
    if (ev.age) { dateTx(tx); return; }
    busy = true;
    setVerdict('check', 'Checking', 'Reading the tree, block by block.');
    var cells = bar(), k = 0, resp = null, scanned = false;
    tk1.textContent = 'Roots checked …';
    /* The seg-bar is the one thing that loops, and only while the request is pending. */
    var loop = setInterval(function () { k = (k + 1) % (cells.length + 1); cells.forEach(function (c, i) { c.classList.toggle('f', i < k); }); }, RM ? 1000 : 80);
    vt.scan(RM ? 0 : 1600, function (p) {
      if (VT.tip) tk2.textContent = 'Block ' + fmt(VT.tip - (VT.tip - ev.h) * p);
    }, function () { scanned = true; finish(); });
    VT.api('/verify/' + tx + '?before=' + encodeURIComponent(ev.before)).then(function (r) { resp = r; finish(); });

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
        if (VT.tip) tk2.textContent = 'Block ' + fmt(VT.tip);
        busy = false; reveal(); return;
      }
      var roots = d.evidence ? d.evidence.distinct_roots : 0;
      tk1.textContent = 'Roots checked ' + fmt(roots) + ' / ' + fmt(roots);
      tk2.textContent = 'Block ' + fmt(d.chain_tip);
      vmeta.textContent = (roots ? fmt(roots) + ' roots · ' : '') + d.elapsed_secs.toFixed(2) + ' s · mainnet · live';
      if (d.provisional) {
        vt.reset();
        setVerdict('idle', 'Not mined yet', '', "This transaction isn't in a block yet, so its anchor could still change. Check again once it's mined.");
        busy = false; reveal(); return;
      }
      if (d.verdict === 'PASS') {
        vt.pass(function () {
          setVerdict('pass', 'Pass', 'These coins existed before ' + eventPhrase(ev) + '.', (d.age ? 'Proven age: ' + agePhrase(d.age) + '. ' : '') + 'This proves nothing about who sent them or how much, only when they last moved.');
          $('#vsealtext').textContent = 'PROVEN OLDER THAN BLOCK ' + fmt(ev.h) + ' · MAINNET ·';
          vseal.style.opacity = ''; if (RM) vseal.style.opacity = 1; else vseal.classList.add('go');
          busy = false;
          reveal();
        });
        return;
      }
      if (d.verdict === 'FAIL' && d.matched_heights) {
        var after = Math.min(vt.n - 2, vt.ii + 6);
        vt.els[after].classList.add('anc');
        for (var j = vt.ii + 1; j < after; j++) vt.els[j].classList.add('past');
        var lo = d.age ? d.age.existed_by_height : d.matched_heights[0];
        setVerdict('fail', 'Not provable', "This deposit can't be dated before " + eventPhrase(ev) + '.',
          'Its coins were ' + (d.age ? agePhrase(d.age) : 'in the pool by block ' + fmt(lo)) + ', ' + (lo === ev.h ? 'the event block itself' : fmt(lo - ev.h) + ' block' + (lo - ev.h === 1 ? '' : 's') + ' after the event') + ', so this deposit says nothing about it either way. Not a flag.');
      } else {
        vt.reset();
        var reasons = (d.reasons || []).join(', ');
        setVerdict('fail', 'Not provable', "This deposit can't be dated.", 'It has ' + (reasons || 'inputs Vintage cannot read') + '. Vintage can only date shielded Ironwood coins.');
      }
      busy = false;
      reveal();
    }
  }
  form.addEventListener('submit', function (e) { e.preventDefault(); run(input.value.trim().toLowerCase()); });

  /* "Age only": no event. Asks GET /age and lists every registry event the coins predate. Neutral
     styling: gold is kept for a proven PASS against an event the user chose. */
  function dateTx(tx) {
    busy = true;
    setVerdict('check', 'Checking', 'Finding where its anchor sits in the chain.');
    var cells = bar(), k = 0;
    var loop = setInterval(function () { k = (k + 1) % (cells.length + 1); cells.forEach(function (c, i) { c.classList.toggle('f', i < k); }); }, RM ? 1000 : 80);
    VT.api('/age/' + tx).then(function (resp) {
      clearInterval(loop);
      var d = resp.body;
      busy = false;
      if (!resp.ok) {
        if (resp.status === 0) setVerdict('idle', 'Not checked', '', "Couldn't reach the verifier. Try again in a minute, or run vintage age on your own machine to involve nobody.");
        /* The API's own 404 always carries an error message; a bare 404 means this verifier predates age lookups. */
        else if (resp.status === 404 && !(d && d.error)) setVerdict('idle', 'Not available', '', "This verifier doesn't offer age lookups yet. Pick an event instead, or run vintage age on your own machine.");
        else if (resp.status === 404) setVerdict('idle', 'Not found', '', "No mainnet transaction with that txid, or it hasn't reached the server yet. Testnet transactions aren't checked here.");
        else setVerdict('idle', 'Not checked', '', (d && d.error) || 'The verifier could not date this transaction.');
        return;
      }
      vmeta.textContent = d.elapsed_secs.toFixed(2) + ' s · mainnet · live';
      tk1.textContent = d.age ? 'Anchor at block ' + fmt(d.age.existed_by_height) : 'Anchor not located';
      tk2.textContent = 'Block ' + fmt(d.chain_tip);
      if (d.verdict !== 'DATED') {
        setVerdict('idle', 'Not provable', "This transaction can't be dated.", 'It has ' + ((d.reasons || []).join(', ') || 'inputs Vintage cannot read') + '. Vintage can only date shielded Ironwood coins.');
        return;
      }
      var a = d.age, evs = d.events || [];
      setVerdict('idle', d.provisional ? 'Dated · not mined yet' : 'Dated', 'Coins ' + agePhrase(a) + '.', 'Every coin this transaction spends was already in the shielded pool by then.');
      if (evs.length) {
        var ul = document.createElement('ul'); ul.className = 'ticks';
        evs.forEach(function (e) {
          var li = document.createElement('li');
          li.dataset.g = e.predates ? '✓' : '×';
          li.textContent = (e.predates ? 'Predates ' : 'After ') + shortName(e.label) + ' at block ' + fmt(e.height) + '.';
          ul.appendChild(li);
        });
        verdict.appendChild(ul);
      }
      reveal();
    });
  }

  function example(tx, eventId) {
    var c = $('.chip[data-before="' + eventId + '"]', chips);
    if (!c) { setVerdict('idle', 'Events unavailable', '', "Couldn't load the event list, so the example can't run. Try again in a minute."); return; }
    input.value = tx;
    selectChip(c);
    run(tx);
  }
  $('#example').addEventListener('click', function () { example(EXAMPLE_TX, EXAMPLE_EVENT); });

  VT.api('/events').then(function (r) {
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
    /* Links into the app: #tx=<txid>&before=<event-id> runs that check; #before=<event-id> picks the
       event. The fragment is never sent to any server, and it is cleared once read. */
    var g = /^#tx=([0-9a-f]{64})(&age)?$/i.exec(location.hash);
    if (g) {
      history.replaceState(null, '', location.pathname);
      input.value = g[1].toLowerCase();
      if (g[2]) { selectChip(agechip); run(input.value); }
      else { setVerdict('idle', 'Txid filled in', '', '> Pick an event to check against, or Age only, then press Verify deposit.'); chips.scrollIntoView({ block: 'center' }); }
    }
    var m = /^#(?:tx=([0-9a-f]{64})&)?before=([a-z0-9-]+)$/i.exec(location.hash);
    if (m) {
      history.replaceState(null, '', location.pathname);
      if (m[1]) example(m[1].toLowerCase(), m[2]);
      else { var c = $('.chip[data-before="' + m[2] + '"]', chips); if (c) { selectChip(c); input.focus(); } }
    }
  });
})();
