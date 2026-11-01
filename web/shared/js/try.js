/* /try/: start a disposable sandbox (Beam) and show its terminal beside the command list.
   The launcher answers POST (no body, so the browser sends no preflight) with
   {"url": "<terminal>", "expires_at": <unix seconds>} or {"error": "...", "message": "<sentence>"}.
   Nothing about the visitor is sent: no cookies, no referrer, no body. */
(function () {
  var box = document.getElementById('sandbox');
  if (!box) return;
  var LAUNCH = box.dataset.launcher || '';
  var btn = document.getElementById('sb-start'), msg = document.getElementById('sb-msg');
  var idle = document.getElementById('sb-idle'), frame = document.getElementById('sb-frame');
  var clock = document.getElementById('sb-clock');
  var lbl = btn.querySelector('.lbl'), timer = null;

  if (!LAUNCH) {
    btn.disabled = true;
    msg.textContent = 'Sandboxes open soon. Meanwhile, every command on the left works on your own machine.';
    return;
  }

  function show(text) { msg.textContent = text; }
  function closeSandbox(text) {
    if (timer) { clearInterval(timer); timer = null; }
    frame.removeAttribute('src'); frame.hidden = true;
    idle.hidden = false; clock.hidden = true;
    box.classList.remove('busy');
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

  btn.addEventListener('click', function () {
    btn.disabled = true; box.classList.add('busy');
    show('Starting a sandbox. This takes about 10 to 30 seconds.');
    fetch(LAUNCH, { method: 'POST', credentials: 'omit', referrerPolicy: 'no-referrer', cache: 'no-store' })
      .then(function (r) { return r.json().catch(function () { return {}; }); })
      .then(function (j) {
        box.classList.remove('busy');
        if (!j.url) {
          btn.disabled = false;
          show(j.message || 'Couldn\'t start a sandbox just now. Try again in a minute, or run the commands on your own machine.');
          return;
        }
        idle.hidden = true; frame.hidden = false; frame.src = j.url;
        countdown(j.expires_at || (Date.now() / 1000 + 900));
      }, function () {
        box.classList.remove('busy'); btn.disabled = false;
        show('Couldn\'t reach the sandbox service. Try again in a minute, or run the commands on your own machine.');
      });
  });
})();
