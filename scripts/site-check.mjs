// Site checks with headless Chrome over the DevTools protocol. Node ≥ 22 (built-in WebSocket); no deps.
//
//   node scripts/site-check.mjs <url> <outdir> [--verify <txid> <chip-text>]
//
// For each of: desktop light, desktop dark, mobile (390px) light, mobile dark:
//   - every network request the page makes (method, URL, initiator type) → requests-<view>.json
//   - console errors and JS exceptions
//   - horizontal overflow: documentElement.scrollWidth vs clientWidth
//   - em dashes (U+2014) in shown text, titles, aria-labels, placeholders, alt text, <title>
//   - a full-page screenshot → <view>.png
// With --verify, it also types the txid, picks the event chip and runs the verifier, waits for a verdict,
// and screenshots the verifier window.

import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [url, outdir, ...rest] = process.argv.slice(2);
if (!url || !outdir) { console.error('usage: site-check.mjs <url> <outdir> [--verify <txid> <chip-text>]'); process.exit(2); }
const vi = rest.indexOf('--verify');
const verify = vi >= 0 ? { txid: rest[vi + 1], chip: rest[vi + 2] } : null;
mkdirSync(outdir, { recursive: true });

const port = 9300 + Math.floor(Math.random() * 500);
const profile = mkdtempSync(join(tmpdir(), 'vt-chrome-'));
const chrome = spawn('google-chrome', [
  '--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
  '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-sync',
  '--disable-background-networking', '--disable-component-update', '--disable-default-apps',
  '--metrics-recording-only', '--disable-domain-reliability', 'about:blank',
], { stdio: 'ignore' });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function wsUrl() {
  for (let i = 0; i < 50; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = list.find((t) => t.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(200);
  }
  throw new Error('chrome did not start');
}

const ws = new WebSocket(await wsUrl());
await new Promise((r) => ws.addEventListener('open', r, { once: true }));
let id = 0;
const pending = new Map();
const listeners = [];
ws.addEventListener('message', (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) { const { res, rej } = pending.get(msg.id); pending.delete(msg.id); msg.error ? rej(new Error(JSON.stringify(msg.error))) : res(msg.result); }
  else listeners.forEach((f) => f(msg));
});
const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pending.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expr) => (await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })).result.value;

let requests = [], problems = [];
listeners.push((m) => {
  if (m.method === 'Network.requestWillBeSent') requests.push({ method: m.params.request.method, url: m.params.request.url, type: m.params.type || '', initiator: m.params.initiator?.type || '' });
  if (m.method === 'Runtime.exceptionThrown') problems.push('exception: ' + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
  if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') problems.push('console.error: ' + m.params.args.map((a) => a.value ?? a.description).join(' '));
  if (m.method === 'Log.entryAdded' && m.params.entry.level === 'error') problems.push('log: ' + m.params.entry.text + (m.params.entry.url ? ' ' + m.params.entry.url : ''));
});
await send('Page.enable'); await send('Network.enable'); await send('Runtime.enable'); await send('Log.enable');
await send('Network.setCacheDisabled', { cacheDisabled: true });

const PROBE = `(() => {
  const d = document.documentElement;
  const texts = [document.title, document.body.innerText];
  document.querySelectorAll('[title],[aria-label],[placeholder],[alt]').forEach((e) =>
    ['title', 'aria-label', 'placeholder', 'alt'].forEach((a) => e.getAttribute(a) && texts.push(e.getAttribute(a))));
  const all = texts.join('\\n');
  const dashes = [];
  all.split('\\n').forEach((l) => { if (l.includes('\\u2014')) dashes.push(l.trim().slice(0, 120)); });
  const wide = [];
  document.querySelectorAll('body *').forEach((e) => { const r = e.getBoundingClientRect(); if (r.right > d.clientWidth + 1 && getComputedStyle(e).position !== 'fixed') wide.push((e.id ? '#' + e.id : e.tagName.toLowerCase() + '.' + [...e.classList].join('.')) + ' right=' + Math.round(r.right)); });
  return { scrollWidth: d.scrollWidth, clientWidth: d.clientWidth, overflow: d.scrollWidth > d.clientWidth, dashes, wide: wide.slice(0, 8), theme: d.getAttribute('data-theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark (system)' : 'light (system)') };
})()`;

async function view(name, width, height, mobile, scheme) {
  requests = []; problems = [];
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }] });
  await send('Page.navigate', { url });
  await sleep(4500); // load, API calls, intro motion
  // Trigger every reveal so the whole page is in its end state for the screenshot.
  await evaluate(`document.querySelectorAll('.reveal').forEach(e => e.classList.add('in')); window.scrollTo(0, document.body.scrollHeight); 0`);
  await sleep(1500);
  await evaluate('window.scrollTo(0,0); 0');
  await sleep(300);
  const probe = await evaluate(PROBE);
  // What a visitor sees: the first screen, then each section scrolled into a tall viewport. (Full-page
  // capture resizes the window mid-animation and draws a false overlap in the hero, so it isn't used.)
  const fold = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(outdir, `${name}.png`), Buffer.from(fold.data, 'base64'));
  const tall = mobile ? 1400 : 1500;
  await send('Emulation.setDeviceMetricsOverride', { width, height: tall, deviceScaleFactor: 1, mobile });
  const ids = await evaluate(`[...document.querySelectorAll('main > section, footer')].map((e, i) => { if (!e.id) e.id = 'band' + i; return e.id; })`);
  for (const sid of ids) {
    // Clip coordinates are document coordinates, so add the scroll offset back.
    const r = await evaluate(`(() => { const e = document.getElementById(${JSON.stringify(sid)}); window.scrollTo(0, Math.max(0, e.getBoundingClientRect().top + scrollY - 16)); const b = e.getBoundingClientRect(); const top = Math.max(0, b.top - 16); return { y: top + scrollY, h: Math.min(${tall} - top, b.height + 32) }; })()`);
    await sleep(700);
    const s2 = await send('Page.captureScreenshot', { format: 'png', clip: { x: 0, y: r.y, width, height: Math.max(40, r.h), scale: 1 } });
    writeFileSync(join(outdir, `${name}--${sid}.png`), Buffer.from(s2.data, 'base64'));
  }
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile });
  writeFileSync(join(outdir, `requests-${name}.json`), JSON.stringify(requests, null, 1));
  const origins = [...new Set(requests.map((r) => (r.url.startsWith('data:') ? 'data: (inline, no network)' : new URL(r.url).origin)))];
  console.log(`\n== ${name} (${width}px, ${scheme})`);
  console.log(`  requests: ${requests.length}, origins: ${origins.join(', ')}`);
  console.log(`  theme: ${probe.theme}; scrollWidth ${probe.scrollWidth} vs clientWidth ${probe.clientWidth} → ${probe.overflow ? 'HORIZONTAL OVERFLOW' : 'no horizontal scroll'}`);
  if (probe.wide.length) console.log('  wider than viewport: ' + probe.wide.join('; '));
  console.log(`  em dashes in shown text: ${probe.dashes.length}${probe.dashes.length ? ' → ' + probe.dashes.join(' | ') : ''}`);
  console.log(`  console errors / exceptions: ${problems.length}${problems.length ? ' → ' + problems.join(' | ') : ''}`);
  return { origins, probe, problems };
}

const results = {};
results['desktop-light'] = await view('desktop-light', 1280, 900, false, 'light');
results['desktop-dark'] = await view('desktop-dark', 1280, 900, false, 'dark');
results['mobile-light'] = await view('mobile-light', 390, 844, true, 'light');
results['mobile-dark'] = await view('mobile-dark', 390, 844, true, 'dark');

if (verify) {
  requests = []; problems = [];
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
  await send('Page.navigate', { url });
  await sleep(4000);
  const picked = await evaluate(`(() => {
    const chip = [...document.querySelectorAll('#vform .chip[aria-pressed]')].find(c => c.textContent.includes(${JSON.stringify(verify.chip)}));
    if (!chip) return 'no chip matching ' + ${JSON.stringify(verify.chip)};
    chip.click();
    document.getElementById('txid').value = ${JSON.stringify(verify.txid)};
    document.getElementById('vform').requestSubmit();
    document.getElementById('verify').scrollIntoView();
    return 'picked ' + chip.textContent;
  })()`);
  console.log(`\n== verify flow: ${picked}`);
  let state = '';
  for (let i = 0; i < 120; i++) {
    state = await evaluate(`document.getElementById('verdict').className`);
    if (/pass|fail|idle/.test(state) && !/check/.test(state) && i > 2) break;
    await sleep(500);
  }
  await sleep(2500); // seal stamp
  const v = await evaluate(`({ cls: document.getElementById('verdict').className, text: document.getElementById('verdict').innerText.replace(/\\s+/g, ' '), meta: document.getElementById('vmeta').innerText, tk: document.getElementById('tk1').innerText + ' | ' + document.getElementById('tk2').innerText })`);
  console.log(`  verdict class: ${v.cls}\n  text: ${v.text}\n  ticker: ${v.tk}\n  meta: ${v.meta}`);
  const box = await evaluate(`(() => { const r = document.querySelector('#verify .stack').getBoundingClientRect(); return { x: r.left + scrollX - 24, y: r.top + scrollY - 24, width: r.width + 48, height: r.height + 48 }; })()`);
  const shot = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip: { ...box, scale: 1 } });
  writeFileSync(join(outdir, 'verify-flow.png'), Buffer.from(shot.data, 'base64'));
  const origins = [...new Set(requests.map((r) => (r.url.startsWith('data:') ? 'data: (inline, no network)' : new URL(r.url).origin)))];
  console.log(`  requests during the flow: ${requests.length}, origins: ${origins.join(', ')}`);
  writeFileSync(join(outdir, 'requests-verify-flow.json'), JSON.stringify(requests, null, 1));
  console.log(`  console errors / exceptions: ${problems.length}${problems.length ? ' → ' + problems.join(' | ') : ''}`);
}

ws.close();
chrome.kill();
