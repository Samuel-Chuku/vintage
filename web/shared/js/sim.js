/* Vintage simulation (usevintage.site/simulate). Needs site.js (window.VT).
   Four short scenarios, each a few steps. Steps are labelled honestly:
     LIVE       a real request to the Vintage verifier, made now, on Zcash mainnet
     REPLAY     real mainnet data recorded during our demo (21 to 23 Oct 2026)
     SIMULATED  an illustrative party (for example a generic exchange) acting on real data
   Gold appears only on a LIVE pass. */
(function () {
  'use strict';
  var VT = window.VT, $ = VT.$, $$ = VT.$$, esc = VT.esc, fmt = VT.fmt, RM = VT.RM;
  var EXP = 'https://mainnet.zcashexplorer.app/transactions/';
  var T1 = 'da9017e0fca0d8bd87def8e3faa7956067f2bd2234cfee91ca7ad4e67095e3a7';
  var D = 'df151ee2e670d683018edd85bb1bf8eca2ccefe3ff464853368c1ff949db9402';
  var TAINT = 'a4c7622f2e3e39ce0cd5a65fc4417f490568990a6f90fda5579a924a0d83c143';
  var EVENT = 'vintage-demo-2026-10';
  /* The published demo link bundle (demo/mainnet-link-bundle.json in the repository). It discloses one
     retired, single-use receipt account's viewing key, by design. */
  var BUNDLE = { version: 1, network: 'main',
    ufvk: 'uview1rq8lu4gf209vsql7hgg7menlqrgg7484j379y3dw6n2cx2657cdcnwjmj5fm5d0culxwqql3gr76e5fm5uv0m9zn9xa27f0lvvl068g60ghq7jp5a6j6dm8zgug9d37yr3vt55lcaduz65ga0vg2aeuny04utdn9jk7yjdhuv380xlla7drzd3q2m0ewfum59chneqrmje06h3vfv6736ykvy57q2wqe6rhks88mve7uacak45l8c9kzmz3zstftaplc36mqx9vnmq9hpupq7x6t4t5p6n656yymkdsr5w2xqx9v770jj5lelxnltskahgtp6979zlx6aw7zfph9auexvpgrmek22f98g33q8nqkzt8hckswdpwqvndpv96na2r4h8xt44dlsj4ag253vt8su3f2ttqk4zrhrmdxqcdahymwvyt9sl3l7tw07ufmxkukguw9lvdhfyzznqf2ahusku85ueyycy094shd6unjh99xecqfp4y3',
    deposit: D, funding: [{ txid: T1, outputs: [1, 2] }] };

  function link(tx, label) { return '<a href="' + EXP + tx + '" rel="noopener noreferrer">' + esc(label || tx.slice(0, 8) + '…' + tx.slice(-4)) + '</a>'; }
  function base(h, label) { return '<a href="https://basescan.org/tx/' + h + '" rel="noopener noreferrer">' + esc(label) + '</a>'; }

  var S = {
    exchange: {
      title: 'Exchange deposit', rail: 'Exchange',
      learn: {
        knows: ['Every coin in the deposit existed before block 3,525,709.', 'The anchor is old, so you chose to prove it.', 'The deposit amount, as with any deposit.'],
        hidden: ['Your balance, your history, your other coins.', 'Who paid you, or when.', 'Any key. Age proofs never need one.']
      },
      steps: [
        { tag: 'replay', title: 'A hack lands in the shielded pool', flow: 'Event at block 3,525,709',
          text: 'Coins we flagged as "stolen" enter the shielded pool at block 3,525,709: our staged demo event, recorded on mainnet on 21 Oct, standing in for a real hack. From now on the exchange can\'t tell whether any shielded deposit carries them, so it holds them all for review.',
          actors: { you: '0.020 ZEC shielded', chain: 'Event at block 3,525,709', verifier: 'Idle', rail: 'Holding shielded deposits for review' },
          evidence: 'event   ' + EVENT + ' (custom)\nheight  3,525,709\ninflow  ' + link('e66bb554e869a376e56753c03ba3637ec08a439fe409ac53600d9473e68d3577') },
        { tag: 'replay', title: 'Your wallet sorts your coins', flow: 'You: vintage status', on: ['you'],
          text: 'Your coins arrived at different times. Vintage shows which of them existed before the event: 0.015 ZEC did, and 0.005 ZEC (received at block 3,525,726) did not.',
          actors: { you: '0.015 ZEC provable · 0.005 too new' },
          evidence: '$ vintage status --wallet honest\n' + EVENT + '  (block 3525709)\n  ✓ 0.01500000 ZEC provably predates it (anchor block 3525696)' },
        { tag: 'replay', title: 'You deposit with an older anchor', flow: 'You → Zcash mainnet: the deposit', on: ['you', 'chain'],
          text: 'The deposit names the pool as it stood at block 3,525,696, 13 blocks before the event. Coins that arrived later are not in that snapshot, so the wallet cannot spend them in this transaction, and consensus checks it. (This real payment went to NEAR Intents; here it stands in for an exchange deposit.)',
          actors: { chain: 'Deposit ' + T1.slice(0, 8) + '… mined at 3,527,813' },
          evidence: '$ vintage send --before ' + EVENT + ' …\nanchor H_a:    3525696\n  ✓ usable       0.01500000 ZEC  mined 3525666\n  ✗ too new      0.00500000 ZEC  mined 3525726\ntxid  ' + link(T1, T1) },
        { tag: 'live', title: 'The exchange asks the verifier', flow: 'Exchange → Vintage verifier: GET /verify', on: ['verifier', 'rail'],
          text: 'One request with the deposit\'s txid and the event. The verifier reads the anchor and rules out every pool state from the event to the chain tip. This request runs now, against mainnet.',
          actors: { rail: 'Waiting for the verdict' },
          live: { kind: 'verify', tx: T1, before: EVENT } },
        { tag: 'simulated', title: 'Credited, not held', flow: 'Exchange: credit the deposit', on: ['rail'],
          text: 'A pass means none of these coins can be the hack\'s coins, so the exchange skips event-related review and credits the deposit. Without it: held for review. One forum user\'s exit was still held after 50 days.',
          actors: { rail: 'Credited. Review skipped.' } }
      ]
    },
    near: {
      title: 'NEAR Intents swap', rail: 'NEAR Intents',
      learn: {
        knows: ['The deposit amount and a fresh refund address.', 'With a check: every coin predates the event.'],
        hidden: ['Your other coins, your balance, your history.', 'Any key.']
      },
      steps: [
        { tag: 'replay', title: 'Ask for a quote', flow: 'You → NEAR Intents: quote 0.003 ZEC for USDC', on: ['you', 'rail'],
          text: 'At 11:18 UTC on 23 Oct we asked NEAR Intents for a real quote: 0.003 ZEC to USDC on Base. It gave a deposit address and an expected 3.613948 USDC.',
          actors: { you: '0.015 ZEC provable', verifier: 'Idle', rail: 'Deposit address t1f5uGK4…v95Jw' },
          evidence: 'depositAddress  t1f5uGK4SZuBHi1XzR7LXiNJWT5sWRv95Jw\namountIn        0.003 ZEC\namountOut       3.613948 USDC (min 3.577808)\ndeadline        2026-10-27T11:18:22Z' },
        { tag: 'replay', title: 'Pay with a Vintage payment', flow: 'You → Zcash mainnet: the deposit', on: ['you', 'chain'],
          text: 'At 12:14 UTC the honest wallet paid the deposit address with a Vintage payment, anchored at block 3,525,696, before the event at 3,525,709. The same transaction also funded a receipt account (see Fresh coins).',
          actors: { chain: 'Broadcast ' + T1.slice(0, 8) + '…' },
          evidence: 'txid    ' + link(T1, T1) + '\nanchor  3,525,696 (2,116 blocks old)\npays    0.003 ZEC → t1f5uGK4…v95Jw' },
        { tag: 'replay', title: 'Mined, deposit seen', flow: 'Zcash mainnet → NEAR Intents: deposit detected', on: ['chain', 'rail'],
          text: 'Mined at block 3,527,813 at 12:17 UTC. NEAR Intents saw the deposit and started the swap.',
          actors: { chain: 'Mined at 3,527,813', rail: 'PENDING_DEPOSIT → PROCESSING' },
          evidence: '12:15:24  PENDING_DEPOSIT\n12:17:32  mined at 3,527,813' },
        { tag: 'live', title: 'Check the deposit', flow: 'Rail → Vintage verifier: GET /verify', on: ['verifier', 'rail'],
          text: 'NEAR Intents did not call Vintage. This is the check a rail like its screening could add, run now against mainnet on the real deposit.',
          actors: {}, live: { kind: 'verify', tx: T1, before: EVENT } },
        { tag: 'replay', title: 'USDC arrives', flow: 'NEAR Intents → You: 3.668896 USDC on Base', on: ['rail', 'you'],
          text: 'SUCCESS at 12:23 UTC, about five and a half minutes after mining: 3.668896 USDC on Base, more than quoted.',
          actors: { rail: 'SUCCESS', you: '+3.668896 USDC on Base' },
          evidence: 'status  SUCCESS 2026-10-23T12:23:05Z\nout     3.668896 USDC\nbase    ' + base('0xc1f7ac737d646c31d00539006887bf507488a5f9115bffe5d433eb1cdb9aa7a4', '0xc1f7ac73…a7a4') }
      ]
    },
    fresh: {
      title: 'Fresh coins', rail: 'Exit rail',
      learn: {
        knows: ['The receipt account\'s viewing key, therefore the funding payments, the amounts received, the receipt address, and the deposit.', 'The viewing key cannot spend.'],
        hidden: ['Nothing from your other accounts.', 'Rotate to a fresh receipt account for every use.']
      },
      steps: [
        { tag: 'replay', title: 'Coins received after the event', flow: 'The problem', on: ['you'],
          text: 'Coins you received after the event can\'t be dated by age: any later coin could descend from the event\'s coins. They need a different proof: where they came from.',
          actors: { you: 'Fresh coins: no age proof', chain: 'Event at block 3,525,709', verifier: 'Idle', rail: 'Holding' } },
        { tag: 'replay', title: 'The payer sends a Vintage payment', flow: 'Payer → your receipt account: 2 notes', on: ['you', 'chain'],
          text: 'You ask for a Vintage payment with vintage request; the memo says VINTAGE before=3525709. The payer\'s Vintage payment sends two notes of 0.003 ZEC to a single-use receipt account with its own seed.',
          actors: { you: 'Receipt account: 2 × 0.003 ZEC', chain: T1.slice(0, 8) + '… outputs 1 and 2' },
          evidence: '$ vintage request --to <receipt address> --amount 0.003 --before ' + EVENT + '\nmemo  "VINTAGE before=3525709"\npaid  ' + link(T1) + ' outputs 1 and 2' },
        { tag: 'replay', title: 'Deposit from the receipt account', flow: 'You → Zcash mainnet: deposit 0.0045 ZEC', on: ['you', 'chain'],
          text: 'The deposit spends both receipt notes, so the wallet adds no dummy input, and pays 0.0045 ZEC to a NEAR Intents deposit address. Mined at block 3,527,827.',
          actors: { chain: 'Deposit ' + D.slice(0, 8) + '… mined at 3,527,827' },
          evidence: 'txid    ' + link(D, D) + '\ninputs  ' + T1.slice(0, 8) + '…:1, ' + T1.slice(0, 8) + '…:2\npays    0.0045 ZEC → t1bGQ9fC…H86dV' },
        { tag: 'live', title: 'The rail checks the link', flow: 'Rail → Vintage verifier: POST /verify-link', on: ['verifier', 'rail'],
          text: 'The rail checks that every input of the deposit is an output of the Vintage payment, and that the payment itself passes. This request runs now, with our published demo bundle. (A rail can run the same check itself with vintage link verify, so the key goes to nobody else.)',
          actors: { rail: 'Waiting for the verdict' }, live: { kind: 'link' } },
        { tag: 'replay', title: 'Settled', flow: 'NEAR Intents → You: 5.52164 USDC on Base', on: ['rail', 'you'],
          text: 'The swap settled at 12:51 UTC: 5.52164 USDC on Base. Fresh coins, proven clean, with one single-use account disclosed and nothing else.',
          actors: { rail: 'SUCCESS', you: '+5.52164 USDC on Base' },
          evidence: 'status  SUCCESS 2026-10-23T12:51:09Z\nout     5.52164 USDC\nbase    ' + base('0xf25a0201f6521cf7aca2174b6f3f30bc5cf6a45b95a96447612a95d1580ffc22', '0xf25a0201…fc22') }
      ]
    },
    tainted: {
      title: 'Tainted coins', rail: 'Exchange',
      learn: {
        knows: ['A fail is no information: the exchange applies its normal process.', 'Vintage never flags anyone.'],
        hidden: ['Nothing is revealed by a fail either.']
      },
      steps: [
        { tag: 'replay', title: 'Tainted coins arrive', flow: 'Flagged wallet → You: 0.005 ZEC', on: ['chain', 'you'],
          text: 'After the event, 0.005 ZEC from the event wallet lands in your wallet at block 3,525,726. Taint-based screening could now flag you. Vintage won\'t.',
          actors: { you: '0.015 old · 0.005 tainted', chain: 'Transfer ' + TAINT.slice(0, 8) + '… at 3,525,726', verifier: 'Idle', rail: 'Screening' },
          evidence: 'txid    ' + link(TAINT, TAINT) + '\nmined   3,525,726 (after the event at 3,525,709)' },
        { tag: 'replay', title: 'Try to pass them off as old', flow: 'You: vintage send 0.016 ZEC', on: ['you'],
          text: 'Asking for 0.016 ZEC needs the tainted note too. The wallet refuses: those coins are not in the pre-event snapshot, so no valid proof for them exists.',
          actors: { you: 'Refused: only 0.015 ZEC is old enough' },
          evidence: 'Error: could not build the transaction: Insufficient balance\n(have 1500000, need 1610000 including fee)\nOnly notes received at or before block 3525696 can be used;\nthis wallet has 0.01500000 ZEC of them.' },
        { tag: 'live', title: 'Deposit them without Vintage', flow: 'Exchange → Vintage verifier: GET /verify', on: ['verifier', 'rail'],
          text: 'An ordinary spend of the tainted coins, checked now against mainnet. Its anchor is a pool state from after the event, so the verdict is "not provable".',
          actors: { rail: 'Checking' }, live: { kind: 'verify', tx: TAINT, before: EVENT } },
        { tag: 'replay', title: 'Your old coins still pass', flow: 'Dusting does nothing', on: ['you', 'rail'],
          text: 'The 0.015 ZEC from before the event still proves its age: that is the deposit in the first scenario. Sending you tainted dust can\'t make your old coins look new.',
          actors: { you: '0.015 ZEC still provable', rail: 'Fail = no information; normal process' } }
      ]
    }
  };

  var ACT = [['you', 'You'], ['chain', 'Zcash mainnet'], ['verifier', 'Vintage verifier'], ['rail', '']];
  var root = $('#sim'), tabs = $('#simtabs'), stepLbl = $('#simstep'), title = $('#simtitle');
  var actors = $('#actors'), flow = $('#flow'), narr = $('#narr'), rail = $('#rail');
  var bBack = $('#back'), bNext = $('#next'), bPlay = $('#play'), bRestart = $('#restart');
  var knows = $('#knows'), hidden = $('#hidden');
  var cur = 'exchange', at = 0, timer = null, results = {}, pending = false;

  function stateAt(sc, i) {
    var st = { you: '', chain: '', verifier: '', rail: '' };
    for (var k = 0; k <= i; k++) {
      var a = sc.steps[k].actors || {};
      for (var key in a) if (Object.prototype.hasOwnProperty.call(a, key)) st[key] = a[key];
      var res = results[cur + ':' + k];
      if (k <= i && res && res.actors) for (var r in res.actors) st[r] = res.actors[r];
    }
    return st;
  }

  function render() {
    var sc = S[cur], s = sc.steps[at];
    title.textContent = sc.title;
    stepLbl.textContent = 'Step ' + (at + 1) + ' of ' + sc.steps.length;
    var st = stateAt(sc, at), on = s.on || [];
    actors.innerHTML = ACT.map(function (a) {
      var nm = a[0] === 'rail' ? sc.rail : a[1];
      return '<div class="actor' + (on.indexOf(a[0]) >= 0 ? ' on' : '') + '"><span class="nm">' + esc(nm) + '</span><span class="st8">' + esc(st[a[0]] || ' ') + '</span></div>';
    }).join('');
    flow.textContent = '▸ ' + s.flow;
    var badge = { live: '<span class="badge live">● Live · mainnet, now</span>', replay: '<span class="badge replay">Replay · recorded on mainnet</span>', simulated: '<span class="badge replay">Simulated party · real data</span>' }[s.tag];
    narr.innerHTML = badge + '<h3 class="display">' + esc(s.title) + '</h3><p>' + esc(s.text) + '</p>' +
      (s.evidence ? '<pre class="evid">' + s.evidence + '</pre>' : '') + (s.live ? '<div id="livebox"></div>' : '');
    rail.innerHTML = sc.steps.map(function (x, i) {
      return '<button type="button" aria-label="Step ' + (i + 1) + ': ' + esc(x.title) + '"' + (i === at ? ' aria-current="step"' : '') + (i < at ? ' class="done"' : '') + ' data-i="' + i + '">' + (i + 1) + '</button>';
    }).join('');
    bBack.disabled = at === 0;
    $('.lbl', bNext).textContent = at === sc.steps.length - 1 ? 'Next scenario' : 'Next step';
    knows.innerHTML = sc.learn.knows.map(function (t) { return '<li>' + esc(t) + '</li>'; }).join('');
    hidden.innerHTML = sc.learn.hidden.map(function (t) { return '<li>' + esc(t) + '</li>'; }).join('');
    if (s.live) live(s.live);
  }

  function verdict(kind, label, head, body) {
    return '<div class="v ' + kind + '"><div class="label">' + esc(label) + '</div>' + (head ? '<h3 class="display">' + esc(head) + '</h3>' : '') + (body ? '<p>' + esc(body) + '</p>' : '') + '</div>';
  }

  function live(spec) {
    var key = cur + ':' + at, box = $('#livebox');
    if (results[key]) { box.innerHTML = results[key].html; return; }
    pending = true;
    box.innerHTML = '<div class="v check"><div class="label">Checking</div><h3 class="display">Asking the verifier now.</h3><div class="seg-bar">' + new Array(25).join('<i></i>') + '</div></div>';
    var cells = $$('.seg-bar i', box), k = 0;
    var loop = setInterval(function () { k = (k + 1) % (cells.length + 1); cells.forEach(function (c, i) { c.classList.toggle('f', i < k); }); }, RM ? 1000 : 80);
    var req = spec.kind === 'link' ? VT.api('/verify-link', { body: { bundle: BUNDLE, before: EVENT } })
      : VT.api('/verify/' + spec.tx + '?before=' + encodeURIComponent(spec.before));
    var sentAt = Date.now();
    req.then(function (r) {
      clearInterval(loop);
      var d = r.body, out, acts = {};
      if (!r.ok || !d) {
        out = verdict('idle', 'Not checked', '', "Couldn't reach the verifier just now. The recorded result for this step is in the repository's demo/README.md. Try again in a minute.");
        acts = { verifier: 'Unreachable' };
      } else if (spec.kind === 'link') {
        var ins = (d.inputs || []).map(function (i) { return 'input ' + i.action + ' ← ' + (i.funded_by ? 'output ' + i.funded_by.output + ' (' + fmt(i.funded_by.value_zat) + ' zat)' : 'not covered'); }).join(' · ');
        out = (d.verdict === 'PASS' ? verdict('pass', 'Link pass', 'Every coin came from a payment that predates the event.', ins)
          : verdict('fail', 'Not provable', d.summary, ins));
        acts = { verifier: d.verdict === 'PASS' ? 'LINK PASS in ' + d.elapsed_secs.toFixed(2) + ' s' : 'Not provable', rail: d.verdict === 'PASS' ? 'Link verified' : 'No information' };
      } else if (d.verdict === 'PASS') {
        out = verdict('pass', 'Pass', 'These coins existed before block ' + fmt(d.before.height) + '.', fmt(d.evidence.distinct_roots) + ' pool states ruled out, block ' + fmt(d.before.height) + ' to the tip at ' + fmt(d.chain_tip) + '.');
        acts = { verifier: 'PASS in ' + d.elapsed_secs.toFixed(2) + ' s', rail: 'Verdict: PASS' };
      } else {
        var why = d.matched_heights ? 'Its anchor is the pool state at block ' + fmt(d.matched_heights[0]) + ', after the event.' : 'It has ' + (d.reasons || []).join(', ') + '.';
        out = verdict('fail', 'Not provable', "This deposit can't be dated before block " + fmt(d.before.height) + '.', why);
        acts = { verifier: 'Not provable', rail: 'Verdict: not provable' };
      }
      out += '<p class="cap">Answered by api.usevintage.site in ' + ((Date.now() - sentAt) / 1000).toFixed(2) + ' s. <button type="button" class="quiet" id="again">Run again</button></p>';
      results[key] = { html: out, actors: acts };
      pending = false;
      if (cur + ':' + at === key) { render(); }
      if (timer) schedule();
    });
  }

  $('#simtabs').addEventListener('click', function (e) {
    var b = e.target.closest('[data-sc]'); if (!b) return;
    go(b.dataset.sc, 0); stop();
  });
  rail.addEventListener('click', function (e) { var b = e.target.closest('[data-i]'); if (b) { go(cur, +b.dataset.i); stop(); } });
  narr.addEventListener('click', function (e) { if (e.target.id === 'again') { delete results[cur + ':' + at]; render(); } });

  function go(sc, i) {
    cur = sc; at = i;
    $$('[data-sc]', tabs).forEach(function (b) { b.setAttribute('aria-pressed', b.dataset.sc === sc ? 'true' : 'false'); });
    render();
  }
  var ORDER = ['exchange', 'near', 'fresh', 'tainted'];
  function next() {
    var sc = S[cur];
    if (at < sc.steps.length - 1) go(cur, at + 1);
    else go(ORDER[(ORDER.indexOf(cur) + 1) % ORDER.length], 0);
  }
  function back() { if (at > 0) go(cur, at - 1); }
  function stop() { if (timer) { clearTimeout(timer); timer = null; } $('.lbl', bPlay).textContent = 'Autoplay'; bPlay.setAttribute('aria-pressed', 'false'); }
  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(function () {
      if (pending) { schedule(); return; }
      var sc = S[cur];
      if (cur === 'tainted' && at === sc.steps.length - 1) { stop(); return; }
      next(); schedule();
    }, RM ? 6000 : 5200);
  }
  bNext.addEventListener('click', function () { next(); if (timer) schedule(); });
  bBack.addEventListener('click', function () { back(); stop(); });
  bRestart.addEventListener('click', function () { stop(); go(cur, 0); });
  bPlay.addEventListener('click', function () {
    if (timer) { stop(); return; }
    $('.lbl', bPlay).textContent = 'Pause'; bPlay.setAttribute('aria-pressed', 'true'); schedule();
  });
  root.addEventListener('keydown', function (e) {
    if (e.target.matches('input, textarea')) return;
    if (e.key === 'ArrowRight') { e.preventDefault(); next(); stop(); }
    if (e.key === 'ArrowLeft') { e.preventDefault(); back(); stop(); }
  });

  /* Deep links: /simulate/#near starts that scenario, on load and when the fragment changes. */
  function fromHash(scroll) {
    var h = location.hash.replace('#', '');
    if (S[h]) { stop(); go(h, 0); if (scroll) root.scrollIntoView(); return true; }
    return false;
  }
  if (!fromHash(true)) go('exchange', 0);
  addEventListener('hashchange', function () { fromHash(true); });
})();
