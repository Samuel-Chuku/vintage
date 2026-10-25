/* Vintage Lab (usevintage.site/simulate). Needs site.js (window.VT).
   A session with three stations. Every step is labelled honestly:
     LIVE     a real request to the Vintage verifier, on Zcash mainnet, made now
     SANDBOX  computed in this browser from real mainnet block heights (the consensus rule, applied locally)
     REPLAY   a real answer recorded from our wallet on mainnet during the demo (21 to 23 Oct 2026)
   Gold appears only on a LIVE pass, or on a verdict computed from a LIVE age. The session ID is made
   here and never sent anywhere. */
(function () {
  'use strict';
  var VT = window.VT;
  if (!VT || !document.getElementById('lab')) return;
  var $ = VT.$, $$ = VT.$$, esc = VT.esc, fmt = VT.fmt, RM = VT.RM;
  var EXP = 'https://mainnet.zcashexplorer.app/transactions/';
  var T1 = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7';
  var D = 'df151ee2e670d683018edd85bb1bf8eca2ccefe3ff464853368c1ff949db9402';
  var TAINT = 'a4c7622f2e3e39ce0cd5a65fc4417f490568990a6f90fda5579a924a0d83c143';
  var FUND = 'ef383512f19ff8356d96a0f81dd6cb62ec3909f2b8d8f4e5a632523af34d50ff';
  var EVENT = 'vintage-demo-2026-10';
  /* The published demo link bundle (demo/mainnet-link-bundle.json in the repository). It discloses one
     retired, single-use receipt account's viewing key, by design. */
  var BUNDLE = { version: 1, network: 'main',
    ufvk: 'uview1rq8lu4gf209vsql7hgg7menlqrgg7484j379y3dw6n2cx2657cdcnwjmj5fm5d0culxwqql3gr76e5fm5uv0m9zn9xa27f0lvvl068g60ghq7jp5a6j6dm8zgug9d37yr3vt55lcaduz65ga0vg2aeuny04utdn9jk7yjdhuv380xlla7drzd3q2m0ewfum59chneqrmje06h3vfv6736ykvy57q2wqe6rhks88mve7uacak45l8c9kzmz3zstftaplc36mqx9vnmq9hpupq7x6t4t5p6n656yymkdsr5w2xqx9v770jj5lelxnltskahgtp6979zlx6aw7zfph9auexvpgrmek22f98g33q8nqkzt8hckswdpwqvndpv96na2r4h8xt44dlsj4ag253vt8su3f2ttqk4zrhrmdxqcdahymwvyt9sl3l7tw07ufmxkukguw9lvdhfyzznqf2ahusku85ueyycy094shd6unjh99xecqfp4y3',
    deposit: D, funding: [{ txid: T1, outputs: [1, 2] }] };
  var FEE = 10000; /* zat: what the demo's real sends paid */
  var STEP = 100000; /* zat: the deposit stepper moves 0.001 ZEC */
  /* The demo wallet's two real notes (BUILD_LOG, 21 Oct): 0.015 ZEC before the event, 0.005 ZEC after it. */
  var NOTES = [
    { zat: 1500000, h: 3525666, tx: FUND, label: 'Received before the event' },
    { zat: 500000, h: 3525726, tx: TAINT, label: 'Received after the event, from the event wallet', tainted: true }
  ];

  function zec(z) { return (z / 1e8).toFixed(4) + ' ZEC'; }
  function short(tx) { return tx.slice(0, 8) + '…' + tx.slice(-4); }
  function link(tx, label) { return '<a href="' + EXP + tx + '" rel="noopener noreferrer">' + esc(label || short(tx)) + '</a>'; }
  function clock(d) { var p = function (n) { return (n < 10 ? '0' : '') + n; }; return p(d.getHours()) + ':' + p(d.getMinutes()) + ':' + p(d.getSeconds()); }
  function wait(ms) { return new Promise(function (r) { setTimeout(r, RM ? 0 : ms); }); }
  var MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  function utc(t) { var d = new Date(t * 1000), p = function (n) { return (n < 10 ? '0' : '') + n; }; return d.getUTCDate() + ' ' + MONTHS[d.getUTCMonth()] + ' ' + d.getUTCFullYear() + ', ' + p(d.getUTCHours()) + ':' + p(d.getUTCMinutes()) + ' UTC'; }
  function card(kind, label, title, body) {
    return '<div class="v ' + kind + '"><div class="label">' + label + '</div>' + (title ? '<h3 class="display">' + esc(title) + '</h3>' : '') + (body ? '<p>' + body + '</p>' : '') + '</div>';
  }

  /* ---------------- session ---------------- */
  var sess = null;
  var startBox = $('#labstart'), sessBox = $('#labsession'), doneBox = $('#labdone');
  var slog = $('#slog'), logn = $('#logn');
  function newId() {
    var a = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789', r = new Uint8Array(4), s = '';
    (window.crypto || {}).getRandomValues ? crypto.getRandomValues(r) : r.forEach(function (_, i) { r[i] = Math.random() * 256; });
    for (var i = 0; i < 4; i++) s += a[r[i] % a.length];
    return 'VT-' + s;
  }
  var TAG = { live: 'Live', sand: 'Sandbox', replay: 'Replay', info: 'Session' };
  function log(kind, html) {
    if (!sess) return;
    sess.actions++;
    if (kind === 'live') sess.live++;
    var li = document.createElement('li');
    li.innerHTML = '<span class="t">' + clock(new Date()) + '</span><span class="lab-tag ' + kind + '">' + TAG[kind] + '</span><span class="m">' + html + '</span>';
    slog.insertBefore(li, slog.firstChild);
    logn.textContent = sess.actions;
    $('#slast').innerHTML = '<span class="lab-tag ' + kind + '">' + TAG[kind] + '</span> ' + html;
  }
  function tick() {
    var s = Math.floor((Date.now() - sess.t0) / 1000), p = function (n) { return (n < 10 ? '0' : '') + n; };
    $('#selapsed').textContent = p(Math.floor(s / 60)) + ':' + p(s % 60);
  }
  function start() {
    sess = { id: newId(), t0: Date.now(), actions: 0, live: 0, blocked: {}, proven: {}, timer: null };
    slog.innerHTML = ''; logn.textContent = '0';
    $('#sid').textContent = sess.id; tick();
    sess.timer = setInterval(tick, 1000);
    startBox.hidden = true; doneBox.hidden = true; sessBox.hidden = false;
    reset1(); reset2(); reset3(); reset4(); select(1);
    log('info', 'Session ' + sess.id + ' started' + (VT.tip ? ' at mainnet block ' + fmt(VT.tip) : '') + '.');
    sessBox.scrollIntoView({ block: 'start', behavior: RM ? 'auto' : 'smooth' });
  }
  function end() {
    if (!sess) return;
    clearInterval(sess.timer); tick();
    var blocked = Object.keys(sess.blocked).length;
    $('#doneid').textContent = sess.id;
    $('#donefacts').innerHTML = [
      [$('#selapsed').textContent, 'Session length'], [String(sess.actions), 'Actions'],
      [String(sess.live), 'Live checks on mainnet'], [blocked + ' / ' + ATTACKS.length, 'Attacks blocked by the expected rule'],
      [String(Object.keys(sess.proven).length), 'Honest results proven live']
    ].map(function (f) { return '<div class="fact"><b>' + esc(f[0]) + '</b><span>' + esc(f[1]) + '</span></div>'; }).join('');
    sess = null;
    sessBox.hidden = true; doneBox.hidden = false;
    doneBox.scrollIntoView({ block: 'center', behavior: RM ? 'auto' : 'smooth' });
  }
  $('#labgo').addEventListener('click', start);
  $('#logcopy').addEventListener('click', function () {
    var b = this, lines = $$('li', slog).reverse().map(function (li) { return li.innerText.replace(/\s+/g, ' ').trim(); });
    var text = 'Vintage Lab ' + $('#sid').textContent + '\n' + lines.join('\n') + '\n';
    var ok = function () { b.textContent = 'Copied'; setTimeout(function () { b.textContent = 'Copy log'; }, 1600); };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(ok, function () { b.textContent = 'Copy failed'; });
  });
  $('#labagain').addEventListener('click', start);
  $('#labend').addEventListener('click', end);
  VT.onTip(function (t) { $('#stip').textContent = fmt(t); });

  /* ---------------- stations (tabs) ---------------- */
  var tabs = $$('.lab-st'), panels = $$('.lab-panel');
  function select(n) {
    tabs.forEach(function (t, i) { t.setAttribute('aria-selected', i === n - 1 ? 'true' : 'false'); t.tabIndex = i === n - 1 ? 0 : -1; });
    panels.forEach(function (p, i) { p.hidden = i !== n - 1; });
  }
  tabs.forEach(function (t, i) {
    t.addEventListener('click', function () { select(i + 1); });
    t.addEventListener('keydown', function (e) {
      var d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
      if (!d) return;
      e.preventDefault();
      var n = (i + d + tabs.length) % tabs.length; select(n + 1); tabs[n].focus();
    });
  });
  function setChip(el, text, state) { el.textContent = text; el.className = 'lab-chip' + (state ? ' ' + state : ''); }

  /* ---------------- station 1: get your deposit through (sandbox, real heights) ---------------- */
  var anchor = $('#anchor'), anchorOut = $('#anchorout'), notesBox = $('#notes'), marks1 = $('#marks1');
  var amtOut = $('#amt'), exstat = $('#exstat'), flow1 = $('#flow1'), res1 = $('#res1'), sendBtn = $('#sendbtn');
  var ev1 = { h: 3525709, id: EVENT, name: 'the demo event' }, amount = 1000000, sending = false;
  function pct(h, lo, hi) { return Math.max(0, Math.min(100, (h - lo) / (hi - lo) * 100)); }
  function spendable() { var a = +anchor.value; return NOTES.filter(function (n) { return n.h <= a; }); }
  function maxAmount() { return Math.max(0, spendable().reduce(function (s, n) { return s + n.zat; }, 0) - FEE); }
  function holding(text) {
    exstat.className = 'lab-exstat hold';
    exstat.innerHTML = '<span class="label">Status</span><b>Holding</b><span>' + text + '</span>';
  }
  function draw1() {
    var a = +anchor.value, lo = +anchor.min, hi = +anchor.max;
    notesBox.innerHTML = NOTES.map(function (n) {
      var ok = n.h <= a;
      return '<div class="lab-note' + (ok ? '' : ' locked') + (n.tainted ? ' tainted' : '') + '">' +
        '<span class="lab-box" aria-hidden="true">' + (ok ? '✓' : '×') + '</span>' +
        '<span><b>' + zec(n.zat) + '</b> <span class="label">block ' + fmt(n.h) + '</span><br><span class="s">' + esc(n.label) + '. ' +
        (ok ? 'In your anchor\'s pool state: spendable.' : 'Arrived after your anchor: not in that pool state, so it can\'t be spent.') + '</span></span></div>';
    }).join('');
    var em = ev1.h >= lo && ev1.h <= hi
      ? '<span class="mk ev" style="left:' + pct(ev1.h, lo, hi) + '%"><i></i><em>Event ' + fmt(ev1.h) + '</em></span>'
      : '<span class="mk ev far" style="left:0%"><i></i><em>← Event ' + fmt(ev1.h) + '</em></span>';
    marks1.innerHTML = NOTES.map(function (n) {
      return '<span class="mk note' + (n.tainted ? ' t' : '') + '" style="left:' + pct(n.h, lo, hi) + '%"><i></i><em>' + zec(n.zat).replace(' ZEC', '') + '</em></span>';
    }).join('') + em +
      '<span class="past" style="width:' + pct(Math.min(a, ev1.h - 1), lo, hi) + '%"></span>';
    var d = ev1.h - a;
    anchorOut.innerHTML = 'Block <b>' + fmt(a) + '</b> · ' + (d > 0 ? fmt(d) + ' block' + (d === 1 ? '' : 's') + ' before the event' : (d === 0 ? 'the event block itself' : fmt(-d) + ' block' + (d === -1 ? '' : 's') + ' after the event')) +
      '<br><span class="lab-preview">Preview: ' + (maxAmount() <= 0 ? 'nothing spendable at this anchor' : d > 0 ? 'the deposit would clear' : 'the deposit would stay in review') + '</span>';
    var max = maxAmount();
    if (amount > max) amount = max - (max % STEP);
    if (amount <= 0 && max > 0) amount = Math.min(STEP, max);
    amtOut.textContent = zec(Math.max(0, amount));
    $('#amtminus').disabled = sending || amount <= STEP;
    $('#amtplus').disabled = sending || amount + STEP > max;
    sendBtn.disabled = sending || max <= 0;
    if (!sending && !flow1.children.length) holding('Every shielded deposit since ' + ev1.name + ' at block ' + fmt(ev1.h) + ' waits for review.');
  }
  function reset1() { anchor.value = 3525740; amount = 1000000; flow1.innerHTML = ''; res1.innerHTML = ''; sending = false; draw1(); setChip($('#chip1'), 'Open'); }
  anchor.addEventListener('input', function () { if (!sending) { flow1.innerHTML = ''; res1.innerHTML = ''; } draw1(); });
  anchor.addEventListener('change', function () { var s = spendable(); log('sand', 'Anchor set to block ' + fmt(+anchor.value) + ': ' + s.length + ' of ' + NOTES.length + ' notes spendable.'); });
  $('#amtminus').addEventListener('click', function () { amount = Math.max(STEP, amount - STEP); draw1(); });
  $('#amtplus').addEventListener('click', function () { if (amount + STEP <= maxAmount()) amount += STEP; draw1(); });
  $$('#evchips .chip').forEach(function (c) {
    c.addEventListener('click', function () {
      $$('#evchips .chip').forEach(function (o) { o.setAttribute('aria-pressed', o === c ? 'true' : 'false'); });
      ev1 = { h: +c.dataset.h, id: c.dataset.id, name: c.dataset.id === EVENT ? 'the demo event' : 'the Bitget hack' };
      flow1.innerHTML = ''; res1.innerHTML = ''; draw1();
      log('sand', 'Ledger now screens for ' + ev1.name + ' (block ' + fmt(ev1.h) + ').');
    });
  });

  function step(html) { var li = document.createElement('li'); li.innerHTML = '<span>' + html + '</span>'; flow1.appendChild(li); return li; }
  sendBtn.addEventListener('click', function () {
    if (sending) return;
    var a = +anchor.value, spend = spendable(), amt = amount, pass = a < ev1.h;
    /* Spend the fewest notes that cover the amount, oldest first, as a wallet would. */
    var used = [], sum = 0;
    spend.forEach(function (n) { if (sum < amt + FEE) { used.push(n); sum += n.zat; } });
    sending = true; flow1.innerHTML = ''; res1.innerHTML = ''; draw1();
    exstat.className = 'lab-exstat check';
    exstat.innerHTML = '<span class="label">Status</span><b>Incoming</b><span>A shielded deposit of ' + zec(amt) + ' is on its way.</span>';
    log('sand', 'Sent ' + zec(amt) + ' at anchor ' + fmt(a) + ', spending ' + used.map(function (n) { return zec(n.zat); }).join(' + ') + '.');
    step('<b>Built</b> at anchor ' + fmt(a) + '. Spends ' + used.map(function (n) { return zec(n.zat) + ' from block ' + fmt(n.h); }).join(' and ') + '.');
    wait(700).then(function () {
      step('<b>Mined.</b> Consensus checks every spent note is in the pool state at block ' + fmt(a) + '.');
      return wait(700);
    }).then(function () {
      step('<b>Ledger asks the verifier:</b> were these coins in the pool before block ' + fmt(ev1.h) + '?');
      return wait(800);
    }).then(function () {
      step((pass ? '<b>Yes.</b> The anchor is ' + fmt(ev1.h - a) + ' blocks before the event. Deposit credited.' : '<b>Not provable.</b> The anchor is not before the event. The deposit stays in review, as it would without Vintage.') +
        ' <span class="lab-preview">Matches the preview.</span>');
      sending = false; draw1();
      if (pass) {
        exstat.className = 'lab-exstat ok';
        exstat.innerHTML = '<span class="label">Status</span><b>Credited</b><span>' + zec(amt) + ', cleared against ' + ev1.name + '.</span>';
      } else holding('Your deposit of ' + zec(amt) + ' waits for review with all the others.');
      log('sand', pass ? 'Ledger credited the deposit: anchor before the event.' : 'Ledger kept holding the deposit: anchor not before the event.');
      offerLive(pass, a, used);
    });
  });

  /* The same situation, really mined on mainnet: our Vintage payment (passes the demo event, fails
     Bitget) or the transfer anchored after the event. */
  function offerLive(pass, a, used) {
    var tx = pass || ev1.id !== EVENT ? T1 : TAINT;
    var why = tx === T1
      ? 'Our real Vintage payment, ' + link(T1) + ', spent the 0.015 ZEC note at anchor 3,525,696.'
      : 'A real transfer anchored after the event, ' + link(TAINT) + ', at anchor 3,525,722.';
    res1.innerHTML = card(pass ? 'sandbox ok' : 'sandbox no', 'Sandbox · ' + (pass ? 'credited' : 'held'),
      pass ? 'Your deposit cleared.' : 'Your deposit stays in review.',
      (pass ? 'Your anchor, block ' + fmt(a) + ', is before ' + ev1.name + ', so every coin it spends existed before it.' :
        (a >= ev1.h ? 'Your anchor, block ' + fmt(a) + ', is not before ' + ev1.name + '.' : '') +
        (ev1.h < NOTES[0].h ? ' Your coins all arrived after ' + ev1.name + ', so no anchor can date them before it.' : '') +
        (used.some(function (n) { return n.tainted; }) ? ' And it spends the coins from the event wallet, which can only be spent at an anchor after they arrived.' : '')) +
      ' This was computed in your browser from real block heights.') +
      '<div class="row lab-golive"><button type="button" class="tkt primary" id="golive"><span class="lbl">Check the real one live</span><span class="stub" aria-hidden="true">→</span></button><span class="s">' + why + '</span></div>' +
      '<div id="live1" aria-live="polite"></div>';
    $('#golive').addEventListener('click', function () {
      var box = $('#live1'), b = this; b.disabled = true;
      box.innerHTML = card('check', 'Live · checking', 'Asking the verifier on mainnet now.');
      var t0 = Date.now();
      VT.api('/verify/' + tx + '?before=' + encodeURIComponent(ev1.id)).then(function (r) {
        b.disabled = false;
        var d = r.body, secs = ((Date.now() - t0) / 1000).toFixed(2);
        if (!r.ok || !d) { box.innerHTML = card('idle', 'Live · not checked', '', 'Couldn\'t reach the verifier just now. Try again in a minute.'); return; }
        var age = d.age ? ' Coins in the pool by block ' + fmt(d.age.existed_by_height) + ' (' + utc(d.age.existed_by_time) + ').' : '';
        if (d.verdict === 'PASS') {
          if (sess) sess.proven.deposit = true;
          box.innerHTML = card('pass', 'Live · mainnet · ' + secs + ' s', 'Pass. These coins existed before block ' + fmt(d.before.height) + '.',
            link(tx) + ' checked now against ' + fmt(d.evidence.distinct_roots) + ' pool states.' + age +
            ' This payment was the deposit into a NEAR Intents swap: <a href="https://basescan.org/tx/0xc1f7ac737d646c31d00539006887bf507488a5f9115bffe5d433eb1cdb9aa7a4" rel="noopener noreferrer">3.668896 USDC on Base</a>.');
        } else {
          box.innerHTML = card('fail', 'Live · mainnet · ' + secs + ' s', 'Not provable before block ' + fmt(d.before.height) + '.', link(tx) + ': ' + esc(d.summary) + age);
        }
        log('live', 'GET /verify/' + short(tx) + ' before ' + esc(ev1.id) + ' → ' + d.verdict + ' in ' + secs + ' s.');
        setChip($('#chip1'), 'Done', 'done');
      });
    });
  }

  /* ---------------- station 2: move the event (live age) ---------------- */
  var age2 = $('#age2'), move2 = $('#move2'), evh = $('#evh'), marks2 = $('#marks2'), v2 = $('#v2'), snap2 = $('#snap2'), conf2 = $('#conf2');
  var dated = null; /* { tx, A, time, events } */
  function reset2() { dated = null; age2.innerHTML = ''; move2.hidden = true; $('#dtx').value = ''; setChip($('#chip2'), 'Open'); $$('#txchips .chip').forEach(function (c) { c.setAttribute('aria-pressed', 'false'); }); }
  function range2(H) {
    var A = dated.A, lo = Math.min(A, H), hi = Math.max(A, H), pad = Math.max(40, Math.round((hi - lo) * 0.25));
    evh.min = lo - pad; evh.max = hi + pad; evh.value = H;
  }
  function draw2() {
    var A = dated.A, H = +evh.value, lo = +evh.min, hi = +evh.max, pass = A < H;
    marks2.innerHTML = '<span class="mk age" style="left:' + pct(A, lo, hi) + '%"><i></i><em>Coins ' + fmt(A) + '</em></span>' +
      dated.events.map(function (e) { return e.height >= lo && e.height <= hi ? '<span class="mk reg" style="left:' + pct(e.height, lo, hi) + '%"><i></i></span>' : ''; }).join('') +
      '<span class="past' + (pass ? ' gold' : '') + '" style="left:' + pct(Math.min(A, H), lo, hi) + '%;width:' + (pct(Math.max(A, H), lo, hi) - pct(Math.min(A, H), lo, hi)) + '%"></span>';
    v2.innerHTML = pass
      ? card('pass', 'Computed from the live age', 'Predates block ' + fmt(H) + '.', 'In the pool by block ' + fmt(A) + ', ' + fmt(H - A) + ' block' + (H - A === 1 ? '' : 's') + ' before it.')
      : card('fail', 'Computed from the live age', 'Not provable before block ' + fmt(H) + '.', 'In the pool by block ' + fmt(A) + (A === H ? ', the event block itself.' : ', ' + fmt(A - H) + ' block' + (A - H === 1 ? '' : 's') + ' after it.') + ' Says nothing either way: not a flag.');
    conf2.textContent = '';
  }
  function date(tx) {
    if (!/^[0-9a-f]{64}$/.test(tx)) { age2.innerHTML = '<p class="err">A txid is 64 characters, 0 to 9 and a to f.</p>'; return; }
    age2.innerHTML = card('check', 'Live · checking', 'Finding where its anchor sits in the chain.');
    move2.hidden = true;
    var t0 = Date.now();
    VT.api('/age/' + tx).then(function (r) {
      var d = r.body, secs = ((Date.now() - t0) / 1000).toFixed(2);
      if (!r.ok) {
        age2.innerHTML = card('idle', 'Live · not dated', '', r.status === 404 && d && d.error ? 'No mainnet transaction with that txid.' : r.status === 0 ? 'Couldn\'t reach the verifier. Try again in a minute.' : esc((d && d.error) || 'The verifier couldn\'t date this transaction.'));
        return;
      }
      log('live', 'GET /age/' + short(tx) + ' → ' + (d.age ? 'in the pool by block ' + fmt(d.age.existed_by_height) : d.verdict) + ' in ' + secs + ' s.');
      if (d.verdict !== 'DATED') { age2.innerHTML = card('fail', 'Live · not provable', 'This transaction can\'t be dated.', 'It has ' + esc((d.reasons || []).join(', ')) + '. Vintage only dates shielded Ironwood coins.'); return; }
      dated = { tx: tx, A: d.age.existed_by_height, time: d.age.existed_by_time, events: d.events || [] };
      age2.innerHTML = '<div class="lab-agecard"><span class="lab-tag live">Live · ' + secs + ' s</span><b>In the pool by ' + esc(utc(dated.time)) + '</b><span class="s">Every coin ' + link(tx) + ' spends was in the shielded pool by block ' + fmt(dated.A) + '.</span></div>';
      snap2.innerHTML = '<span class="label">Jump to</span>' + dated.events.map(function (e) {
        return '<button type="button" class="chip" data-h="' + e.height + '">' + esc(String(e.label).split(':')[0]) + ' · ' + fmt(e.height) + '</button>';
      }).join('') + '<button type="button" class="chip" data-h="' + (dated.A + 1) + '">The next block · ' + fmt(dated.A + 1) + '</button>';
      $$('.chip', snap2).forEach(function (c) { c.addEventListener('click', function () { range2(+c.dataset.h); draw2(); log('sand', 'Event moved to block ' + fmt(+c.dataset.h) + ': ' + (dated.A < +c.dataset.h ? 'predates' : 'not provable') + '.'); }); });
      var first = dated.events.filter(function (e) { return e.height > dated.A; })[0];
      range2(first ? first.height : dated.A + 13);
      move2.hidden = false; draw2();
      setChip($('#chip2'), 'Done', 'done');
    });
  }
  $$('#txchips .chip').forEach(function (c) {
    c.addEventListener('click', function () {
      $$('#txchips .chip').forEach(function (o) { o.setAttribute('aria-pressed', o === c ? 'true' : 'false'); });
      $('#dtx').value = c.dataset.tx; date(c.dataset.tx);
    });
  });
  $('#dateform').addEventListener('submit', function (e) { e.preventDefault(); $$('#txchips .chip').forEach(function (o) { o.setAttribute('aria-pressed', 'false'); }); date($('#dtx').value.trim().toLowerCase()); });
  evh.addEventListener('input', function () { if (dated) draw2(); });
  evh.addEventListener('change', function () { if (dated) log('sand', 'Event dragged to block ' + fmt(+evh.value) + ': ' + (dated.A < +evh.value ? 'predates' : 'not provable') + '.'); });
  $('#confirm2').addEventListener('click', function () {
    if (!dated) return;
    var H = +evh.value, predicted = dated.A < H ? 'PASS' : 'FAIL', t0 = Date.now();
    conf2.textContent = 'Asking the verifier…';
    VT.api('/verify/' + dated.tx + '?before=' + H).then(function (r) {
      var secs = ((Date.now() - t0) / 1000).toFixed(2);
      if (!r.ok || !r.body) { conf2.textContent = r.body && r.body.error ? r.body.error : 'Couldn\'t reach the verifier.'; return; }
      var got = r.body.verdict;
      conf2.textContent = (got === predicted ? 'Verifier agrees: ' : 'Verifier says: ') + got + ' in ' + secs + ' s';
      if (sess && got === predicted && got === 'PASS') sess.proven.age = true;
      log('live', 'GET /verify/' + short(dated.tx) + ' before ' + fmt(H) + ' → ' + got + (got === predicted ? ', as predicted' : ', prediction was ' + predicted) + '.');
    });
  });

  /* ---------------- station 3: attack the verifier ---------------- */
  var ATTACKS = [
    { title: 'Pass off tainted coins', tries: 'Deposit coins that arrived after the event, and ask for a pass against it.',
      run: function () { return VT.api('/verify/' + TAINT + '?before=' + EVENT); }, call: 'GET /verify/' + short(TAINT) + ' before the demo event',
      expect: { verdict: 'FAIL', matched: true, rule: 'FAIL: its anchor is a pool state from the event or later' } },
    { title: 'Reuse a pass against an older event', tries: 'The Vintage payment passes the demo event. Present it against the Bitget hack, which came first.',
      run: function () { return VT.api('/verify/' + T1 + '?before=bitget-2026-09'); }, call: 'GET /verify/' + short(T1) + ' before Bitget',
      expect: { verdict: 'FAIL', matched: true, rule: 'FAIL: its anchor is a pool state from after the Bitget event' } },
    { title: 'Borrow someone else\'s link', tries: 'Send the real receipt-account link, but name the tainted transfer as the deposit it covers.',
      run: function () { var b = JSON.parse(JSON.stringify(BUNDLE)); b.deposit = TAINT; return VT.api('/verify-link', { body: { bundle: b, before: EVENT } }); }, call: 'POST /verify-link with a swapped deposit',
      expect: { verdict: 'FAIL', reason: /not covered by the disclosed payments/, rule: 'FAIL: the deposit spends coins the disclosed payment never sent' } },
    { title: 'Date coins before the pool existed', tries: 'Ask for a pass against block 3,428,000, before Ironwood launched.',
      run: function () { return VT.api('/verify/' + T1 + '?before=3428000'); }, call: 'GET /verify/' + short(T1) + ' before 3,428,000',
      expect: { status: 400, reason: /after Ironwood's activation/, rule: '400: the height must be after Ironwood\'s activation' } },
    { title: 'Spend new coins at an old anchor', tries: 'Build a deposit at anchor 3,525,696 that also spends the 0.005 ZEC received at block 3,525,726.',
      replay: 'NO WITNESS: Tree does not contain a root at address Address { level: Level(0), index: 824517 }',
      call: 'our wallet on mainnet, 21 Oct', expect: { rule: 'Refused: the note has no witness at that anchor' } }
  ];
  var atBox = $('#attacks');
  function reset3() {
    atBox.innerHTML = ATTACKS.map(function (a, i) {
      return '<div class="lab-attack" id="at' + i + '"><div class="top"><span class="label">Attack 0' + (i + 1) + '</span><span class="lab-chip">Untested</span></div>' +
        '<h3>' + esc(a.title) + '</h3><p>' + esc(a.tries) + '</p><p class="lab-expect"><span class="label">Expected</span> ' + esc(a.expect.rule) + '</p>' +
        '<button type="button" class="tkt"><span class="lbl">Run attack</span><span class="stub" aria-hidden="true">→</span></button><div class="out" aria-live="polite"></div></div>';
    }).join('');
    $$('.lab-attack', atBox).forEach(function (el, i) { $('button', el).addEventListener('click', function () { attack(i, el, this); }); });
    setChip($('#chip3'), '0 / ' + ATTACKS.length + ' blocked');
  }
  function attack(i, el, btn) {
    var a = ATTACKS[i], out = $('.out', el), chip = $('.lab-chip', el), t0 = Date.now();
    btn.disabled = true; out.innerHTML = '<p class="s">Trying…</p>';
    /* blocked: the attack failed. expected: it failed by the rule it was meant to test. */
    var done = function (blocked, label, reason, kind, expected) {
      btn.disabled = false;
      var secs = ((Date.now() - t0) / 1000).toFixed(2);
      out.innerHTML = '<div class="lab-reason"><span class="lab-tag ' + kind + '">' + (kind === 'live' ? 'Live · ' + secs + ' s' : 'Replay') + '</span><b>' + esc(label) + '</b><code>' + esc(reason) + '</code>' +
        (blocked ? '<span class="lab-match">' + (expected ? '✓ Blocked by the expected rule' : '× Blocked, but not by the expected rule') + '</span>' : '') + '</div>';
      blocked = blocked && expected;
      setChip(chip, blocked ? 'Blocked' : 'Unexpected', blocked ? 'done' : 'warn');
      el.classList.toggle('blocked', blocked);
      if (sess && blocked) sess.blocked[i] = true;
      var n = sess ? Object.keys(sess.blocked).length : 0;
      setChip($('#chip3'), n + ' / ' + ATTACKS.length + ' blocked', n === ATTACKS.length ? 'done' : '');
      log(kind, esc(a.title) + ': ' + esc(a.call) + ' → ' + esc(label) + '.');
    };
    if (a.replay) { wait(500).then(function () { done(true, 'Refused to build', a.replay, 'replay', true); }); return; }
    var x = a.expect;
    a.run().then(function (r) {
      var d = r.body;
      if (r.status === 0 || !d) return done(false, 'Not checked', 'Couldn\'t reach the verifier. Try again in a minute.', 'live', false);
      if (!r.ok) return done(true, 'Rejected (' + r.status + ')', d.error || 'error', 'live', r.status === x.status && (!x.reason || x.reason.test(d.error || '')));
      var ok = d.verdict === x.verdict && (!x.reason || x.reason.test(d.summary || '')) && (!x.matched || !!(d.matched_heights || []).length);
      done(d.verdict !== 'PASS', d.verdict, d.summary || '', 'live', ok);
    });
  }
  /* ---------------- station 4: paid after the event ---------------- */
  /* The demo's fresh-coins path, as it happened on mainnet: the payer's Vintage payment T1 (anchor
     3,525,696, mined 3,527,813) paid a single-use receipt account 2 x 0.003 ZEC; the receipt account's
     deposit D (mined 3,527,827) spent both notes into a NEAR Intents swap. */
  var track4 = $('#track4'), a4a = $('#a4a'), a4b = $('#a4b'), a4c = $('#a4c'), q4b = $('#q4b'), q4c = $('#q4c');
  function light(keys) { $$('li', track4).forEach(function (li) { li.classList.toggle('on', keys.indexOf(li.dataset.k) >= 0); }); }
  function press(group, btn) { $$('.chip', group).forEach(function (o) { o.setAttribute('aria-pressed', o === btn ? 'true' : 'false'); }); }
  function reset4() {
    [$('#q4a'), q4b, q4c].forEach(function (g) { $$('.chip', g).forEach(function (o) { o.setAttribute('aria-pressed', 'false'); o.disabled = false; }); });
    a4a.innerHTML = a4b.innerHTML = a4c.innerHTML = ''; q4b.hidden = q4c.hidden = true; light(['event']);
    setChip($('#chip4'), 'Open');
  }
  /* One live call into an answer box: a checking card, then whatever `show` renders from the response. */
  function liveInto(box, call, label, show) {
    box.innerHTML = card('check', 'Live · checking', 'Asking the verifier on mainnet now.');
    var t0 = Date.now();
    call().then(function (r) {
      var secs = ((Date.now() - t0) / 1000).toFixed(2);
      if (!r.ok || !r.body) { box.innerHTML = card('idle', 'Live · not checked', '', 'Couldn\'t reach the verifier just now. Try again in a minute.'); return; }
      log('live', label + ' → ' + r.body.verdict + ' in ' + secs + ' s.');
      box.innerHTML = show(r.body, secs);
    });
  }
  $$('#q4a .chip').forEach(function (c) {
    c.addEventListener('click', function () {
      press($('#q4a'), c);
      a4b.innerHTML = a4c.innerHTML = ''; q4c.hidden = true;
      $$('.chip', q4b).forEach(function (o) { o.setAttribute('aria-pressed', 'false'); });
      $$('.chip', q4c).forEach(function (o) { o.setAttribute('aria-pressed', 'false'); });
      if (c.dataset.v === 'plain') {
        q4b.hidden = true; light(['event', 'paid', 'dep']);
        log('sand', 'Paid with an ordinary payment after the event.');
        liveInto(a4a, function () { return VT.api('/verify/' + D + '?before=' + EVENT); }, 'GET /verify/' + short(D) + ' before the demo event', function (d, secs) {
          return card('fail', 'Live · mainnet · ' + secs + ' s', 'Your deposit can\'t be dated before the event.',
            'Coins you received after the event can only be dated after it, whoever paid you. Here, the real deposit ' + link(D) + ': its coins were in the pool by block ' +
            fmt(d.age ? d.age.existed_by_height : 3527822) + '. It stays in review. Pick the Vintage payment to see the way through.');
        });
      } else {
        q4b.hidden = false; light(['anchor', 'event', 'paid']);
        log('sand', 'Paid with a Vintage payment from a holder of old coins.');
        liveInto(a4a, function () { return VT.api('/verify/' + T1 + '?before=' + EVENT); }, 'GET /verify/' + short(T1) + ' before the demo event', function (d, secs) {
          return d.verdict === 'PASS'
            ? card('pass', 'Live · mainnet · ' + secs + ' s', 'The payment itself predates the event.',
              'Your payer sent ' + link(T1) + ' at an anchor before the event, so every coin in it existed before block ' + fmt(d.before.height) + '. It reached you at block 3,527,813, after the event, so your own coins are new. Now: where do they land?')
            : card('fail', 'Live · mainnet · ' + secs + ' s', 'Not provable.', esc(d.summary));
        });
      }
    });
  });
  $$('.chip', q4b).forEach(function (c) {
    c.addEventListener('click', function () {
      press(q4b, c); a4c.innerHTML = '';
      $$('.chip', q4c).forEach(function (o) { o.setAttribute('aria-pressed', 'false'); });
      if (c.dataset.v === 'main') {
        q4c.hidden = true;
        log('sand', 'Received into the main account.');
        a4b.innerHTML = card('sandbox no', 'Sandbox', 'This works, but costs you more than it should.',
          'To tie a later deposit to this payment, the exchange has to see the viewing key of the account that received it. For your main account, that key shows every payment you ever received there and every amount. A fresh single-use account keeps that to this one payment.');
      } else {
        q4c.hidden = false; light(['anchor', 'event', 'paid', 'dep']);
        log('sand', 'Received into a fresh receipt account; deposited from it.');
        a4b.innerHTML = '<div class="lab-agecard"><span class="lab-tag replay">Replay · mainnet</span><b>Two notes in, one deposit out</b><span class="s">The receipt account received 2 × 0.003 ZEC (outputs 1 and 2 of ' + link(T1) +
          ', block 3,527,813). Its deposit ' + link(D) + ', mined at block 3,527,827, spent both and paid 0.0045 ZEC into a NEAR Intents swap.</span></div>';
      }
    });
  });
  $$('.chip', q4c).forEach(function (c) {
    c.addEventListener('click', function () {
      press(q4c, c);
      var v = c.dataset.v;
      if (v === 'age') {
        log('sand', 'Exchange checks the deposit\'s age only.');
        liveInto(a4c, function () { return VT.api('/verify/' + D + '?before=' + EVENT); }, 'GET /verify/' + short(D) + ' before the demo event', function (d, secs) {
          return card('fail', 'Live · mainnet · ' + secs + ' s', 'Age alone can\'t clear it.',
            'The deposit\'s coins arrived after the event (in the pool by block ' + fmt(d.age ? d.age.existed_by_height : 3527822) + '), so its own age proves nothing about it. It needs the link back to the payment.');
        });
      } else if (v === 'key') {
        log('sand', 'Exchange checks the link with the receipt account\'s viewing key.');
        liveInto(a4c, function () { return VT.api('/verify-link', { body: { bundle: BUNDLE, before: EVENT } }); }, 'POST /verify-link for ' + short(D), function (d, secs) {
          if (d.verdict !== 'PASS') return card('fail', 'Live · mainnet · ' + secs + ' s', 'Link not proven.', esc(d.summary));
          setChip($('#chip4'), 'Done', 'done');
          if (sess) sess.proven.link = true;
          var ins = (d.inputs || []).map(function (i) { return 'input ' + i.action + ' ← output ' + (i.funded_by ? i.funded_by.output + ' (' + fmt(i.funded_by.value_zat) + ' zat)' : '?'); }).join(' · ');
          return card('pass', 'Live · mainnet · ' + secs + ' s', 'Link pass. Every coin came from a payment that predates the event.',
            esc(ins) + '. The swap settled: <a href="https://basescan.org/tx/0xf25a0201f6521cf7aca2174b6f3f30bc5cf6a45b95a96447612a95d1580ffc22" rel="noopener noreferrer">5.52164 USDC on Base</a>.') +
            '<div class="lab-two lab-disc"><div><span class="label">The key disclosed</span><ul class="ticks"><li data-g="●">The payment into the receipt account, and its amounts</li><li data-g="●">The receipt address</li><li data-g="●">The deposit</li></ul></div>' +
            '<div><span class="label">Still private</span><ul class="ticks"><li data-g="✓">Every other account you hold</li><li data-g="✓">Who paid the payer, and their history</li><li data-g="✓">Spending power: a viewing key can\'t spend</li></ul></div></div>';
        });
      } else {
        log('replay', 'Exchange checks the link with zero-knowledge proofs (research prototype).');
        setChip($('#chip4'), 'Done', 'done');
        a4c.innerHTML = '<div class="v idle lab-zk"><div class="row" style="gap:var(--vt-2)"><span class="lab-tag replay">Replay</span><span class="lab-tag sand">Research · unreviewed</span></div>' +
          '<h3 class="display">Linked, with no key at all.</h3>' +
          '<p>One proof per input, made with Zcash\'s existing spend circuit against a tree of only the payment\'s outputs. Our mainnet run: both inputs of ' + link(D) + ' proven to be outputs of ' + link(T1) +
          ', 300,000 zat each, checked in 8.0 s. Six forged versions were rejected.</p>' +
          '<p class="s">Disclosed: each input\'s value. Not disclosed: any key, which output, addresses, memos. This runs from the command line, not in the browser: <a href="https://github.com/Samuel-Chuku/vintage/tree/main/research/zk-link" rel="noopener noreferrer">research/zk-link</a>, proof file <span class="mono">demo/mainnet-zk-link.json</span>.</p></div>';
      }
    });
  });
})();
