// Renders the promo: headless Chrome draws each frame (main.js render(t)), ffmpeg encodes them.
//
//   node render.mjs                     the whole video, 60 fps -> out/video.mp4 (no sound)
//   node render.mjs --fps 30 --from 20 --to 30 --out out/part.mp4
//   node render.mjs --stills 1,5.5,12   PNG frames at those times -> out/stills/
//   node render.mjs --cues              only out/cues.json (for audio.py)
import puppeteer from 'puppeteer-core';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const CHROME = process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const args = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => {
  if (a.startsWith('--')) acc.push([a.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : true]);
  return acc;
}, []));
const out = path.join(here, 'out');
fs.mkdirSync(out, { recursive: true });

async function openPage() {
  const browser = await puppeteer.launch({
    executablePath: CHROME, headless: 'new',
    args: ['--force-device-scale-factor=1', '--allow-file-access-from-files', '--font-render-hinting=none', '--hide-scrollbars'],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
  page.on('console', m => { if (m.type() === 'warning' || m.type() === 'error') console.error('[page]', m.text()); });
  page.on('pageerror', e => console.error('[page error]', e.message));
  await page.goto(pathToFileURL(path.join(here, 'index.html')).href);
  await page.evaluate(async () => {
    // load every font face before the first frame
    await Promise.all(['15px "Ubuntu-Light"', '15px "Hack"', '15px "NotoEmoji"'].map(f => document.fonts.load(f, 'abc✔')));
    await document.fonts.ready;
    window.PROMO.measure();
  });
  return { browser, page };
}

async function cues() {
  const { browser, page } = await openPage();
  const c = await page.evaluate(() => ({ duration: PROMO.DUR, markers: PROMO.MARKERS, sfx: PROMO.CUES }));
  fs.writeFileSync(path.join(out, 'cues.json'), JSON.stringify(c, null, 1));
  await browser.close();
  console.log(`cues: ${c.sfx.length} sound effects`);
  return c;
}

async function stills(times) {
  const dir = path.join(out, 'stills');
  fs.mkdirSync(dir, { recursive: true });
  const { browser, page } = await openPage();
  for (const t of times) {
    await page.evaluate(t => PROMO.render(t), t);
    const f = path.join(dir, `t${String(t.toFixed(2)).padStart(6, '0')}.png`);
    await page.screenshot({ path: f });
    console.log(f);
  }
  await browser.close();
}

async function segment(i, f0, f1, fps, file) {
  const { browser, page } = await openPage();
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', '-',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '15', '-pix_fmt', 'yuv420p', '-r', String(fps), file], { stdio: ['pipe', 'inherit', 'inherit'] });
  const done = new Promise((res, rej) => ff.on('close', c => (c === 0 ? res() : rej(new Error(`ffmpeg ${c}`)))));
  for (let f = f0; f < f1; f++) {
    await page.evaluate(t => PROMO.render(t), f / fps);
    const buf = await page.screenshot({ type: 'jpeg', quality: 96, optimizeForSpeed: true });
    if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r));
    if (i === 0 && (f - f0) % 60 === 0) process.stdout.write(`\r  worker 0: ${Math.round((f - f0) / (f1 - f0) * 100)}%   `);
  }
  ff.stdin.end();
  await done;
  await browser.close();
}

async function video() {
  const fps = Number(args.fps || 60);
  const c = await cues();
  const from = Number(args.from || 0), to = Number(args.to || c.duration);
  const F0 = Math.round(from * fps), F1 = Math.round(to * fps);
  const workers = Number(args.workers || 6);
  const per = Math.ceil((F1 - F0) / workers);
  const t0 = Date.now();
  const parts = [];
  await Promise.all([...Array(workers).keys()].map(i => {
    const a = F0 + i * per, b = Math.min(F1, a + per);
    if (a >= b) return null;
    const file = path.join(out, `seg${i}.mp4`);
    parts[i] = file;
    return segment(i, a, b, fps, file);
  }));
  const list = path.join(out, 'segments.txt');
  fs.writeFileSync(list, parts.filter(Boolean).map(p => `file '${p}'`).join('\n'));
  const file = args.out ? path.resolve(args.out) : path.join(out, 'video.mp4');
  await new Promise((res, rej) => spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', list, '-c', 'copy', file], { stdio: 'inherit' })
    .on('close', c => (c === 0 ? res() : rej(new Error('concat')))));
  parts.filter(Boolean).forEach(p => fs.unlinkSync(p));
  console.log(`\n${file}: ${F1 - F0} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}

if (args.stills) await stills(String(args.stills).split(',').map(Number));
else if (args.cues) await cues();
else await video();
