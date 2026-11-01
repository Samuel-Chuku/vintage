/* /try/: start a disposable sandbox (Beam) and show its terminal beside the command list.
   The launcher answers POST (no body, so the browser sends no preflight) with
   {"url": "<terminal>", "expires_at": <unix seconds>} or {"error": "...", "message": "<sentence>"}.
   Nothing about the visitor is sent: no cookies, no referrer, no body.
   While it starts, the window shows a boot sequence. The launcher answers once, at the end, so the steps
   advance on a timer (they are what it does, in order) and all complete when the terminal has loaded. */
(function () {
  var COPY = '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M5 5h9v9H5zM2 11V2h9" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square"/></svg>';
  var DONE = '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M2 8l4 4 8-8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square"/></svg>';

  /* ---- copy icons: data-cmd holds the full command (the shown one abbreviates txids) ---- */
  document.addEventListener('click', function (e) {
    var b = e.target.closest ? e.target.closest('.cp') : null;
    if (!b || !navigator.clipboard) return;
    navigator.clipboard.writeText(b.dataset.cmd).then(function () {
      b.innerHTML = DONE; b.classList.add('ok'); b.setAttribute('aria-label', 'Copied');
      setTimeout(function () { b.innerHTML = COPY; b.classList.remove('ok'); b.setAttribute('aria-label', 'Copy command'); }, 1500);
    });
  });
  if (!navigator.clipboard) document.documentElement.classList.add('no-clip');

  /* ---- bring your own transaction: build the commands for any txid and event or height ---- */
  var tx = document.getElementById('byo-tx'), ev = document.getElementById('byo-ev'), hh = document.getElementById('byo-h');
  var list = document.getElementById('byo-cmds'), err = document.getElementById('byo-err');
  if (tx && ev && list) {
    var sample = list.innerHTML;
    var esc = function (s) { return s.replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); };
    var row = function (cmd, shown, note) {
      return '<li><div class="cmd-row"><code title="' + esc(cmd) + '">' + esc(shown) + '</code><button class="cp" type="button" data-cmd="' + esc(cmd) +
        '" aria-label="Copy command">' + COPY + '</button></div><span class="exp">' + note + '</span></li>';
    };
    var build = function () {
      var t = tx.value.trim().toLowerCase(), before = ev.value;
      hh.hidden = before !== 'height';
      err.textContent = '';
      if (!t) { list.innerHTML = sample; return; }
      if (!/^[0-9a-f]{64}$/.test(t)) { list.innerHTML = sample; err.textContent = 'A txid is 64 hexadecimal characters (0 to 9, a to f).'; return; }
      if (before === 'height') {
        var h = hh.value.trim();
        if (!/^\d+$/.test(h) || +h <= 3428143) { list.innerHTML = sample; err.textContent = 'Pick a block height after Ironwood activated (3,428,143).'; return; }
        before = h;
      }
      var s = t.slice(0, 8) + '…' + t.slice(-4);
      list.innerHTML =
        row('vintage verify ' + t + ' --before ' + before, 'vintage verify ' + s + ' --before ' + before,
            '<span class="vt plain">PASS / FAIL</span> did its coins exist before that block?') +
        row('vintage age ' + t, 'vintage age ' + s, '<span class="vt plain">DATED</span> the block by which its coins were in the pool') +
        sample;
    };
    tx.addEventListener('input', build); ev.addEventListener('change', build); hh.addEventListener('input', build);
  }

  /* ---- the sandbox ---- */
  var box = document.getElementById('sandbox');
  if (!box) return;
  var LAUNCH = box.dataset.launcher || '';
  var btn = document.getElementById('sb-start'), msg = document.getElementById('sb-msg');
  var idle = document.getElementById('sb-idle'), frame = document.getElementById('sb-frame');
  var boot = document.getElementById('sb-boot'), secs = document.getElementById('sb-secs');
  var steps = boot.querySelectorAll('.sb-steps li'), cells = boot.querySelectorAll('#sb-bar i');
  var note = boot.querySelector('.sb-note'), noteText = note.textContent;
  var clock = document.getElementById('sb-clock'), open = document.getElementById('sb-open');
  var lbl = btn.querySelector('.lbl'), timer = null, ticker = null;
  var STEP_MS = 1400, FILL_MS = 8000, SLOW_MS = 25000, GIVE_UP_MS = 90000;

  if (!LAUNCH) {
    btn.disabled = true;
    msg.textContent = 'Sandboxes open soon. Meanwhile, every command on the left works on your own machine.';
    return;
  }

  function show(text) { msg.textContent = text; }

  /* The boot panel: which step is current, how much of the bar is filled, seconds elapsed. */
  function paint(stepIndex, filled, done) {
    for (var i = 0; i < steps.length; i++) {
      steps[i].className = i < stepIndex || done ? 'done' : (i === stepIndex ? 'cur' : '');
    }
    for (var j = 0; j < cells.length; j++) cells[j].className = j < filled ? 'f' : '';
  }
  function startBoot() {
    var t0 = Date.now();
    idle.hidden = true; boot.hidden = false; box.classList.add('busy');
    note.textContent = noteText;
    paint(0, 0, false);
    ticker = setInterval(function () {
      var ms = Date.now() - t0;
      secs.textContent = (ms / 1000).toFixed(1) + ' s';
      if (ms > SLOW_MS) note.textContent = 'Taking longer than usual. Beam may be starting a machine from cold; this can take up to a minute.';
      /* Hold on the last step and at 90% of the bar until the terminal is really there. */
      paint(Math.min(steps.length - 1, Math.floor(ms / STEP_MS)),
            Math.min(Math.floor(cells.length * 0.9), Math.floor(ms / FILL_MS * cells.length * 0.9)), false);
    }, 100);
  }
  function stopBoot() {
    if (ticker) { clearInterval(ticker); ticker = null; }
    box.classList.remove('busy');
  }

  function closeSandbox(text) {
    if (timer) { clearInterval(timer); timer = null; }
    stopBoot();
    frame.removeAttribute('src'); frame.hidden = true; boot.hidden = true;
    idle.hidden = false; clock.hidden = true; open.hidden = true;
    btn.disabled = false; lbl.textContent = 'Start a new sandbox';
    show(text);
  }
  function countdown(expiresAt) {
    clock.hidden = false;
    function tick() {
      var left = Math.max(0, Math.round(expiresAt - Date.now() / 1000));
      clock.textContent = Math.floor(left / 60) + ':' + ('0' + (left % 60)).slice(-2) + ' left';
      if (!left) closeSandbox('That sandbox closed after 15 minutes and was deleted. Start another any time.');
    }
    tick();
    timer = setInterval(tick, 1000);
  }
  function fail(text) {
    stopBoot();
    boot.hidden = true; idle.hidden = false;
    btn.disabled = false;
    show(text);
  }

  btn.addEventListener('click', function () {
    btn.disabled = true; show('');
    startBoot();
    var ctl = window.AbortController ? new AbortController() : null;
    var giveUp = setTimeout(function () { if (ctl) ctl.abort(); }, GIVE_UP_MS);
    fetch(LAUNCH, { method: 'POST', credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store', signal: ctl ? ctl.signal : undefined })
      .then(function (r) { return r.json().catch(function () { return {}; }); })
      .then(function (j) {
        clearTimeout(giveUp);
        if (!j.url) {
          fail(j.message || 'Couldn\'t start a sandbox just now. Try again in a minute, or run the commands on your own machine.');
          return;
        }
        /* Load the terminal behind the boot panel; swap when it has loaded (or after 10 s regardless). */
        var shown = false;
        function reveal() {
          if (shown) return;
          shown = true;
          stopBoot(); paint(steps.length, cells.length, true);
          setTimeout(function () { boot.hidden = true; frame.hidden = false; }, 300);
        }
        frame.addEventListener('load', reveal, { once: true });
        setTimeout(reveal, 10000);
        frame.src = j.url;
        open.href = j.url; open.hidden = false;
        countdown(j.expires_at || (Date.now() / 1000 + 900));
      }, function () {
        clearTimeout(giveUp);
        fail('Couldn\'t reach the sandbox service. Try again in a minute, or run the commands on your own machine.');
      });
  });
})();
