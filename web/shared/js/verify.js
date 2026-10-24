/* Vintage verifier (app.usevintage.site): live checks against the Vintage API. Needs site.js (window.VT). */
(function () {
  'use strict';
  var VT = window.VT, $ = VT.$, $$ = VT.$$, fmt = VT.fmt, esc = VT.esc, RM = VT.RM;
  var IRONWOOD = 3428143; /* NU6.3 activation on mainnet: nothing in Ironwood predates it */
  var EXAMPLE_TX = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7'; /* the mainnet demo deposit */
  var EXAMPLE_EVENT = 'vintage-demo-2026-10';
  var form = $('#vform'), input = $('#txid'), err = $('#txerr'), verdict = $('#verdict'), vseal = $('#vseal');
  var tk1 = $('#tk1'), tk2 = $('#tk2'), vmeta = $('#vmeta'), chips = $('#chips'), custom = $('#customchip');
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
    if (VT.tip) tk2.textContent = 'Block ' + fmt(VT.tip);
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
    var m = /^#(?:tx=([0-9a-f]{64})&)?before=([a-z0-9-]+)$/i.exec(location.hash);
    if (m) {
      history.replaceState(null, '', location.pathname);
      if (m[1]) example(m[1].toLowerCase(), m[2]);
      else { var c = $('.chip[data-before="' + m[2] + '"]', chips); if (c) { selectChip(c); input.focus(); } }
    }
  });
})();
