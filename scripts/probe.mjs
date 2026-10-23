// One-off DevTools probe: node scripts/probe.mjs <url> <width> <js-expression>
// Loads the page at that viewport width and prints the JSON value of the expression.
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [url, width, expr] = process.argv.slice(2);
const port = 9800 + Math.floor(Math.random() * 150);
const chrome = spawn('google-chrome', ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), 'vt-p-'))}`, '--no-first-run', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let target;
for (let i = 0; i < 50 && !target; i++) { try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page'); } catch {} await sleep(200); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r, { once: true }));
let id = 0; const pend = new Map();
ws.addEventListener('message', (m) => { const x = JSON.parse(m.data); if (x.id && pend.has(x.id)) { pend.get(x.id)(x.result); pend.delete(x.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await send('Emulation.setDeviceMetricsOverride', { width: +width, height: 844, deviceScaleFactor: 1, mobile: +width < 768 });
// JSOFF=1: page scripts disabled (DevTools evaluation still works). RM=1: prefers-reduced-motion.
if (process.env.JSOFF) await send('Emulation.setScriptExecutionDisabled', { value: true });
if (process.env.RM) await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
await send('Page.navigate', { url });
await sleep(4500);
const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
console.log(JSON.stringify(r.result.value ?? r, null, 1));
// Optional: a screenshot of exactly what is on screen (no full-page resize), e.g. SHOT=/tmp/x.png
if (process.env.SHOT) {
  const { writeFileSync } = await import('node:fs');
  const s = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(process.env.SHOT, Buffer.from(s.data, 'base64'));
}
ws.close(); chrome.kill();
