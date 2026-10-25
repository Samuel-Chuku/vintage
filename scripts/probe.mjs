// One-off DevTools probe: node scripts/probe.mjs <url> <width> <js-expression>
// Loads the page at that viewport width and prints the JSON value of the expression.
import { spawn } from 'node:child_process';
import { mkdtempSync , rmSync} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [url, width, expr] = process.argv.slice(2);
const port = 9800 + Math.floor(Math.random() * 150);
const PROFILE = mkdtempSync(join(tmpdir(), 'vt-p-'));
const chrome = spawn('google-chrome', ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${PROFILE}`, '--no-first-run', 'about:blank'], { stdio: 'ignore' });
// The profile is throwaway: remove it when Chrome exits, and on any crash or Ctrl-C.
const dropProfile = () => { try { chrome.kill('SIGKILL'); } catch {} try { rmSync(PROFILE, { recursive: true, force: true }); } catch {} };
process.on('exit', dropProfile);
// On a crash or Ctrl-C, stop Chrome, wait for it and its helper processes to exit, then delete.
const bail = async (code, e) => {
  if (e) console.error(e);
  await new Promise((r) => { if (chrome.exitCode !== null || chrome.signalCode) return r(); chrome.once('exit', r); chrome.kill(); setTimeout(r, 3000); });
  await new Promise((r) => setTimeout(r, 300));
  dropProfile(); process.exit(code);
};
process.on('SIGINT', () => bail(130)); process.on('SIGTERM', () => bail(143));
process.on('uncaughtException', (e) => bail(1, e)); process.on('unhandledRejection', (e) => bail(1, e));

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
ws.close(); await new Promise((r) => { chrome.once('exit', r); chrome.kill(); }); await new Promise((r) => setTimeout(r, 300)); dropProfile();
