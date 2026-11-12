'use strict';
// The whole promo is a pure function of time: render(t) draws the frame at t seconds.
// render.mjs steps t frame by frame in headless Chrome; audio.py reads CUES for the sound.

const DUR = 115;
const MARKERS = { chaos_end: 10, logo_end: 14, full_from: 22, outro: 104, final: 108 };

// ------------------------------------------------------------------ helpers
const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, u) => a + (b - a) * u;
const P = (t, a, b) => clamp((t - a) / (b - a));
const E = {
  io: u => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2),
  out: u => 1 - Math.pow(1 - u, 3),
  out5: u => 1 - Math.pow(1 - u, 5),
  in: u => u * u * u,
  back: u => { const c1 = 1.9, c3 = c1 + 1; return 1 + c3 * Math.pow(u - 1, 3) + c1 * Math.pow(u - 1, 2); },
  expo: u => (u >= 1 ? 1 : 1 - Math.pow(2, -10 * u)),
};
const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const rnd = i => { const x = Math.sin(i * 12.9898 + 78.233) * 43758.5453; return x - Math.floor(x); };
const typed = (text, t, t0, t1) => text.slice(0, Math.floor(text.length * P(t, t0, t1)));
const $ = id => document.getElementById(id);
const rgb = (c, a = 1) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

// ------------------------------------------------------------------ the story's data
const SCOPES = {
  API: { c: [86, 156, 214], at: 21.6 },
  WEB: { c: [214, 122, 78], at: 22.1 },
  INFRA: { c: [106, 190, 112], at: 22.5 },
};
const SESS = [
  { id: 'login', name: 'Fix login redirect', scope: 'API', time: '14:32' },
  { id: 'checkout', name: 'Checkout page redesign', scope: 'WEB', time: '14:29' },
  { id: 'rate', name: 'Rate limit middleware', scope: 'API', time: '13:05', tag: 'done' },
  { id: 'cart', name: 'Flaky cart test', scope: 'WEB', time: '12:40', tag: 'blocked' },
  { id: 'token', name: 'Auth token refresh', scope: 'API', time: '11:48', tag: 'needs' },
  { id: 'ci', name: 'CI cache speedup', scope: 'INFRA', time: '10:15', tag: 'done' },
  { id: 'dark', name: 'Dark mode toggle', scope: 'WEB', time: 'Nov 11 18:02', tag: 'done' },
  { id: 'pg', name: 'Postgres 17 upgrade', scope: 'INFRA', time: 'Nov 10 09:30', tag: 'needs' },
  { id: 'docs', name: 'API docs examples', scope: 'API', time: 'Nov 3 16:20', tag: 'blocked' },
];
const ORDERS = { id: 'orders', scope: 'API' };
const FIRST_PROMPT = 'orders page: let people export what they see as CSV, keep the current filters';
const FOLLOW_UP = 'also add a header row with the filter summary';
const TEMPLATE_TEXT = 'Write focused tests for the code changed in this session: one test per behavior, edge cases included. Run them and fix what fails.';
const Q1 = 'Why stream the rows instead of building the CSV in memory?';
const Q2 = 'And does that still hold with the filters applied?';
const SEARCH_Q = 'the one where we fixed auth';
const FLY0 = 23.0, FLYSTEP = 0.36, FLYDUR = 0.55;

// Key moments, shared by picture and sound.
const T = {
  plusClick: 20.8, typeScope: [21.0, 21.5], api: 30.4, web: 33.4, api2: 35.4,
  newClick: 36.2, type1: [36.6, 37.9], send1: 38.0, ordersBorn: 38.1, rename: 39.6,
  loginDone: 42.6, ordersDone: 44.6,
  type2: [46.6, 47.8], send2: 47.9, ordersDone2: 51.6,
  info1: 52.3, askClick: 52.7, typeQ1: [52.9, 54.5], sendQ1: 54.7, toolQ1: 55.3, ansQ1: [55.6, 56.9],
  typeQ2: [57.3, 58.4], sendQ2: 58.5, toolQ2: 59.0, ansQ2: [59.2, 60.4],
  changes: 62.5, mdClick: 64.6, mdOpen: 64.85, srcClick: 66.5, rendClick: 67.2, info2: 68.3, tick: 70.4,
  tmpl: 73.3, tmplOpen: 73.6, tmplClick: 74.5, useClick: 76.2, send3: 77.2, tmplDone: 82.0,
  all: 78.3, searchClick: 78.5, typeS: [78.7, 80.1], sendS: 80.4, picks: 81.4, clearS: 84.3,
  thType: [85.0, 86.3], thSend: 86.5, thTool: 86.85, thReply: 87.1,
};
// Restyling by asking Claude: each chip is a prompt typed while the window is wide, then a wipe.
const THEME_PROMPT = 'make scope look like One Dark Pro';
const CHIPS = [
  { text: 'now try Dracula', type: [88.15, 88.75], send: 88.85, to: 'dracula' },
  { text: 'GitHub Dark, please', type: [89.35, 89.95], send: 90.05, to: 'github' },
  { text: 'now white, just two colors', type: [90.5, 91.2], send: 91.3, to: 'paper' },
  { text: 'go full Matrix: black and green only', type: [92.1, 92.95], send: 93.05, to: 'matrix' },
  { text: 'back to classic', type: [93.95, 94.4], send: 94.5, to: 'classic' },
];
const WIPES = [{ t: T.thReply + 0.15, to: 'onedark' }, ...CHIPS.map(c => ({ t: c.send + 0.35, to: c.to }))];
const THEME_NAMES = { onedark: 'One Dark Pro', dracula: 'Dracula', github: 'GitHub Dark', paper: 'Two-tone white', matrix: 'Matrix', classic: 'classic' };

// Dictate a messy prompt, then Refine it with three answers.
const DIC = {
  micOn: 96.2, micOff: 98.2, text: [98.6, 99.2], refine: 99.7, questions: 100.3,
  answers: [100.8, 101.25, 101.7], go: 102.15, refined: 102.7, send: 104.0,
};
const MESSY = "ok so the orders export thing… people say it's slow when they pick like a whole year, and finance wants it in excel, or xlsx, not sure which, and maybe just email it to them instead of waiting? oh and the dates are UTC which confuses everyone so maybe their timezone… and don't break the CSV one";
const REFINED = `Improve the Orders export:
1. Add XLSX next to CSV; keep the CSV export unchanged.
2. Exports over 50k rows: build them in the background and email the user a download link.
3. Show dates in the user's profile timezone, with the zone in the header row.
Add tests for each, and keep the current filters behavior.`;
const QUESTIONS = [
  ['Which spreadsheet format should the export add?', ['XLSX only', 'CSV and XLSX'], 1],
  ['How should big exports reach people?', ['Email a download link', 'Stream in the browser'], 0],
  ['Which timezone should dates use?', ["The user's profile timezone", 'Keep UTC, label it'], 0],
];

// ------------------------------------------------------------------ captions
// mode: title (top, during the chaos), center (big, mid-screen), top (above the window), left (split)
const CAPS = [
  { t0: 0.35, t1: 1.95, mode: 'title', h: 'One Claude session. *Easy.*' },
  { t0: 2.05, t1: 3.35, mode: 'title', h: 'Then another.' },
  { t0: 3.4, t1: 5.3, mode: 'title', h: 'And another. And another…' },
  { t0: 7.3, t1: 9.95, mode: 'center', h: "The bottleneck isn't Claude.", s2: 'It’s *context switching.*', s2at: 7.95 },
  { t0: 14.7, t1: 19.9, mode: 'top', h: 'Every session. *One window.*', s: 'scope is an IDE for directing Claude Code sessions, not for typing code.' },
  { t0: 20.4, t1: 29.85, mode: 'left', n: '01', k: 'Scopes', h: 'Group your sessions into *scopes.*', s: 'One per project, epic or ticket. Each with its own color, so you always know what belongs where.' },
  { t0: 30.4, t1: 35.85, mode: 'left', n: '02', k: 'Focus', h: 'One scope. *One context.*', s: 'Switch tasks in one click. Each scope reopens where you left it and tells you what needs you.' },
  { t0: 36.4, t1: 40.85, mode: 'left', n: '03', k: 'Auto-naming', h: 'Sessions *name themselves.*', s: 'After the first reply, every session gets a short, clear name.' },
  { t0: 41.2, t1: 45.85, mode: 'left', n: '04', k: 'Status at a glance', h: 'Working, done, or *waiting on you.*', s: 'Each session is tagged when its run ends, with the time it last moved.' },
  { t0: 46.3, t1: 51.85, mode: 'left', n: '05', k: 'Conversation', h: 'Always clear *who said what.*', s: 'Your prompts in blue, Claude in orange, rendered as clean Markdown.' },
  { t0: 52.4, t1: 61.85, mode: 'left', n: '06', k: 'Ask', h: 'Ask side questions. *Stay on track.*', s: 'Answers use the session’s full context and its tools. Keep asking: follow-ups build on the last answer, and nothing is added to the session.' },
  { t0: 62.4, t1: 67.85, mode: 'left', n: '07', k: 'Changes', h: 'Review *every change*, file by file.', s: 'Syntax-highlighted diffs of every file Claude touched. Double-click one to open it: Markdown opens rendered.' },
  { t0: 68.4, t1: 72.85, mode: 'left', n: '08', k: 'Info', h: 'Where it stands, *at a glance.*', s: 'The goal as a checklist, the model, context used and your usage limits.' },
  { t0: 73.4, t1: 77.85, mode: 'left', n: '09', k: 'Templates', h: 'Reuse your *best prompts.*', s: 'Templates, skills and commands for each scope, one click from the prompt.' },
  { t0: 78.4, t1: 83.85, mode: 'left', n: '10', k: 'Search', h: 'Find *any session*, even old ones.', s: 'Filter as you type, or describe it and press Enter: Haiku finds it.' },
  { t0: 84.5, t1: 87.95, mode: 'left', n: '11', k: 'Make it yours', h: 'Restyle it. *Just ask Claude.*', s: 'Sessions can drive scope itself: ask for a theme, and the whole IDE changes, live.' },
  { t0: 88.1, t1: 95.0, mode: 'themes', s: 'The most-used editor themes, one sentence away.' },
  { t0: 95.3, t1: 99.35, mode: 'left', n: '12', k: 'Voice input', h: 'Just *say it.*', s: 'Click 🎤 and talk. Your words are transcribed right on your Mac and land in the prompt.' },
  { t0: 99.5, t1: 104.4, mode: 'left', n: '13', k: 'Refine', h: 'Messy prompt in. *Clear prompt out.*', s: '✨ Refine asks what is unclear, then rewrites it into a structured prompt you review before sending.' },
  { t0: 104.7, t1: 107.75, mode: 'top', h: 'Less juggling. *More shipping.*', s: 'All your Claude Code sessions, organized by scope.' },
];

// ------------------------------------------------------------------ camera (window coordinates)
// Each key: arrive at t after moving for d seconds. f = focus point in the window, s = scale,
// a = where on screen the focus point sits.
const WIDE = { f: [800, 480], s: 0.86, a: [960, 612] };
const CAM = [
  { t: 0, d: 0, ...WIDE },
  { t: 21.1, d: 1.1, f: [156, 250], s: 1.95, a: [1290, 540] },
  { t: 31.0, d: 1.0, f: [560, 400], s: 1.05, a: [1380, 560] },
  { t: 36.9, d: 0.8, f: [704, 500], s: 1.1, a: [1300, 540] },
  { t: 39.1, d: 0.8, f: [156, 170], s: 2.4, a: [1170, 470] },
  { t: 41.9, d: 0.8, f: [156, 330], s: 2.05, a: [1120, 560] },
  { t: 47.0, d: 0.9, f: [704, 470], s: 1.45, a: [1350, 560] },
  { t: 53.1, d: 0.9, f: [1348, 690], s: 1.75, a: [1235, 560] },
  { t: 62.45, d: 0.7, f: [1348, 200], s: 1.75, a: [1250, 420] },
  { t: 64.2, d: 0.8, f: [1348, 640], s: 1.6, a: [1250, 560] },
  { t: 65.8, d: 0.7, f: [704, 380], s: 1.45, a: [1350, 560] },
  { t: 69.1, d: 0.9, f: [1348, 330], s: 1.75, a: [1250, 560] },
  { t: 74.0, d: 0.8, f: [1348, 330], s: 1.7, a: [1250, 560] },
  { t: 75.5, d: 0.8, f: [704, 360], s: 1.45, a: [1350, 560] },
  { t: 77.1, d: 0.6, f: [704, 700], s: 1.45, a: [1350, 560] },
  { t: 79.1, d: 0.8, f: [156, 860], s: 2.1, a: [1260, 700] },
  { t: 81.5, d: 0.8, f: [156, 290], s: 1.95, a: [1260, 520] },
  { t: 85.0, d: 0.8, f: [704, 720], s: 1.4, a: [1350, 560] },
  { t: 88.3, d: 1.0, f: [800, 480], s: 0.9, a: [960, 660] },
  { t: 96.0, d: 0.9, f: [704, 690], s: 1.45, a: [1350, 560] },
  { t: 105.3, d: 1.0, ...WIDE },
];

function camera(t) {
  let k = 0;
  while (k + 1 < CAM.length && CAM[k + 1].t - CAM[k + 1].d <= t) k++;
  const cur = CAM[k];
  const prev = CAM[Math.max(0, k - 1)];
  const u = cur.d ? E.io(P(t, cur.t - cur.d, cur.t)) : 1;
  const ls = lerp(Math.log(prev.s), Math.log(cur.s), u);
  // a little breathing so a held shot never looks frozen
  const drift = 1 + 0.012 * Math.sin((t - cur.t) * 0.5);
  return {
    s: Math.exp(ls) * drift,
    f: [lerp(prev.f[0], cur.f[0], u), lerp(prev.f[1], cur.f[1], u)],
    a: [lerp(prev.a[0], cur.a[0], u), lerp(prev.a[1], cur.a[1], u)],
  };
}

// ------------------------------------------------------------------ cursor (targets are data-k keys)
const CURSOR = [
  { t: 20.25, k: null, x: 980, y: 520, show: true },
  { t: T.plusClick, k: 'tab-plus', click: true },
  { t: 27.5, k: null, x: 230, y: 520 },
  { t: T.api, k: 'tab-API', click: true },
  { t: T.web, k: 'tab-WEB', click: true },
  { t: T.api2, k: 'tab-API', click: true },
  { t: T.newClick, k: 'newbtn', click: true },
  { t: 37.4, k: null, x: 900, y: 760 },
  { t: 46.2, k: null, x: 900, y: 860 },
  { t: T.info1, k: 'rtab-Info', click: true },
  { t: T.askClick, k: 'askfield', click: true },
  { t: T.changes, k: 'rtab-Changes', click: true },
  { t: T.mdClick, k: 'file-orders-export.md', click: true },
  { t: T.mdOpen, k: 'file-orders-export.md', click: true },
  { t: T.srcClick, k: 'md-source', click: true },
  { t: T.rendClick, k: 'md-rendered', click: true },
  { t: T.info2, k: 'rtab-Info', click: true },
  { t: T.tick, k: 'chk-3', click: true },
  { t: T.tmpl, k: 'rtab-Templates', click: true },
  { t: T.tmplClick, k: 'tmpl-write-tests', click: true },
  { t: T.useClick, k: 'use-in-prompt', click: true },
  { t: T.all, k: 'tab-ALL', click: true },
  { t: T.searchClick, k: 'search', click: true },
  { t: 84.0, k: null, x: 230, y: 600, hide: true },
  { t: DIC.micOn, k: 'mic', click: true },
  { t: DIC.micOff, k: 'mic', click: true },
  { t: DIC.refine, k: 'refine', click: true },
  { t: DIC.answers[0], k: 'ans-0-1', click: true },
  { t: DIC.answers[1], k: 'ans-1-0', click: true },
  { t: DIC.answers[2], k: 'ans-2-0', click: true },
  { t: DIC.go, k: 'refine-go', click: true },
  { t: 104.3, k: null, x: 760, y: 560, hide: true },
];
let cursorPos = null; // measured once: render.mjs calls measure() before the first frame

// ------------------------------------------------------------------ sound cues
const CUES = [];
function cue(t, kind, o = {}) { CUES.push({ t, kind, ...o }); }
function buildCues() {
  CUES.length = 0;
  // chaos
  TERMS.forEach((tm, i) => {
    cue(tm.at, i === 0 ? 'pop' : 'whoosh', { gain: i === 0 ? 0.6 : 0.45, pan: (tm.x - 960) / 960 * 0.7, dur: 0.35, freq: 700 });
    if (tm.typeEnd) cue(tm.at + 0.2, 'type', { end: tm.typeEnd, gain: i === 0 ? 0.8 : 0.35, pan: (tm.x - 960) / 960 * 0.6 });
  });
  BUBBLES.forEach(b => cue(b.at, 'alert', { pan: (b.x - 960) / 960 * 0.6 }));
  cue(7.3, 'glitch', { dur: 0.4 }); cue(7.95, 'glitch', { dur: 0.3, gain: 0.8 });
  cue(9.45, 'whoosh', { dur: 0.6, gain: 0.9 });
  cue(13.3, 'whooshDown', { gain: 0.6 }); cue(13.7, 'whoosh', { gain: 0.5 });
  SESS.forEach((s, i) => cue(14.6 + i * 0.12, 'pop', { gain: 0.18, freq: 1100 + i * 60 }));
  // captions
  CAPS.forEach(c => { if (c.mode === 'left' || c.mode === 'top') cue(c.t0, 'whoosh', { gain: 0.28, dur: 0.45, pan: c.mode === 'left' ? -0.4 : 0 }); });
  // camera moves
  CAM.slice(1).forEach(k => { if (k.t > 15) cue(k.t - k.d, 'zoom', { gain: 0.45 }); });
  // clicks
  CURSOR.forEach(k => { if (k.click) cue(k.t, 'click'); });
  // scopes
  cue(T.typeScope[0], 'type', { end: T.typeScope[1], gain: 0.6 });
  Object.values(SCOPES).forEach(s => cue(s.at, 'pop', { freq: 950, gain: 0.7 }));
  SESS.forEach((s, i) => cue(FLY0 + i * FLYSTEP + FLYDUR, 'pop', { freq: 760 + i * 45, gain: 0.45 }));
  // naming
  cue(T.type1[0], 'type', { end: T.type1[1], gain: 0.7 }); cue(T.send1, 'click');
  cue(T.ordersBorn, 'pop', { freq: 800, gain: 0.5 }); cue(T.rename, 'shimmer', { gain: 0.9 });
  // marks
  MARKS.forEach(m => cue(m.t, 'pop', { freq: 1000, gain: 0.45 }));
  cue(T.loginDone, 'ding', { gain: 0.9 });
  // conversation
  cue(T.type2[0], 'type', { end: T.type2[1], gain: 0.7 }); cue(T.send2, 'click');
  // ask
  cue(T.typeQ1[0], 'type', { end: T.typeQ1[1], gain: 0.7 }); cue(T.sendQ1, 'click');
  cue(T.ansQ1[0], 'pop', { freq: 900, gain: 0.5 });
  cue(T.typeQ2[0], 'type', { end: T.typeQ2[1], gain: 0.7 }); cue(T.sendQ2, 'click');
  cue(T.ansQ2[0], 'pop', { freq: 1000, gain: 0.5 });
  // changes, info, templates
  [0, 1, 2].forEach(i => cue(T.changes + 0.15 + i * 0.12, 'pop', { freq: 700 + i * 120, gain: 0.3 }));
  cue(T.tick + 0.05, 'success', { gain: 0.8 });
  cue(T.tmplOpen, 'pop', { freq: 850, gain: 0.4 }); cue(T.useClick + 0.1, 'pop', { freq: 1200, gain: 0.35 });
  cue(T.send3, 'click');
  // search
  cue(T.typeS[0], 'type', { end: T.typeS[1], gain: 0.7 }); cue(T.sendS, 'click');
  [0, 1, 2].forEach(i => cue(T.picks + i * 0.14, 'pop', { freq: 900 + i * 150, gain: 0.45 }));
  cue(T.clearS, 'whooshDown', { gain: 0.3 });
  cue(T.thType[0], 'type', { end: T.thType[1], gain: 0.7 }); cue(T.thSend, 'click');
  CHIPS.forEach(c => { cue(c.type[0], 'type', { end: c.type[1], gain: 0.6 }); cue(c.send, 'click'); });
  WIPES.forEach(w => { cue(w.t - 0.05, 'whoosh', { gain: 0.7, dur: 0.5 }); cue(w.t + 0.1, 'shimmer', { gain: 0.45 }); });
  cue(T.mdOpen + 0.1, 'pop', { freq: 900, gain: 0.4 });
  cue(DIC.micOn + 0.05, 'pop', { freq: 620, gain: 0.5 }); cue(DIC.micOff + 0.05, 'pop', { freq: 480, gain: 0.5 });
  cue(DIC.text[0], 'type', { end: DIC.text[1], every: 0.03, gain: 0.35 });
  cue(DIC.questions, 'pop', { freq: 900, gain: 0.5 }); cue(DIC.refined, 'shimmer', { gain: 0.8 }); cue(DIC.send, 'click');
  cue(106.0, 'success', { gain: 0.6 });
  cue(108.6, 'whoosh', { gain: 0.3 });
  CUES.sort((a, b) => a.t - b.t);
}

// ================================================================== CHAOS (0-10 s)
const TERMS = [
  { x: 960, y: 770, r: 0, at: 0.3, typeEnd: 1.6, title: 'claude — shop',
    lines: ['>p fix the login redirect bug', '⏺ I’ll trace the redirect after sign-in.', '⏺ Read(src/auth/redirect.rs)', '  ⎿d Read 112 lines', '⏺ Update(src/auth/redirect.rs)', '✻o Thinking… (esc to interrupt)'] },
  { x: 520, y: 400, r: -2.5, at: 2.1, typeEnd: 2.9, title: 'claude — shop',
    lines: ['>p add CSV export to orders', '⏺ Search(pattern: "orders")', '  ⎿d Found 23 files', '⏺ Write(src/orders/export.rs)', '✻o Brewing…'] },
  { x: 1400, y: 410, r: 2, at: 2.6, typeEnd: 3.3, title: 'claude — shop/web',
    lines: ['>p why is the cart test flaky?', '⏺ Bash(npm test -- cart --repeat 20)', '  ⎿d 3 failed, 17 passed', 'q Do you want to proceed?', 'q ❯ 1. Yes', '    2. No'] },
  { x: 560, y: 830, r: 1.5, at: 3.3, typeEnd: 3.9, title: 'claude — infra',
    lines: ['>p upgrade postgres to 17', '⏺ Bash(pg_upgrade --check)', '  ⎿d Error: incompatible extension', '⏺ How should I handle pg_trgm?', '>p '] },
  { x: 1380, y: 840, r: -1.8, at: 3.7, typeEnd: 4.3, title: 'claude — shop/web',
    lines: ['>p redesign the checkout page', '⏺ Read(web/Checkout.tsx)', '⏺ Update(web/Checkout.tsx)', '  ⎿d +84 -31', '✻o Thinking…'] },
  { x: 960, y: 340, r: 1, at: 4.2, typeEnd: 4.8, title: 'claude — shop',
    lines: ['>p add rate limiting to the api', '⏺ Update(src/middleware/rate.rs)', '  ⎿d Updated with 42 additions', 'g⏺ Done. Rate limits are on.', '>p '] },
  { x: 300, y: 600, r: -3, at: 4.6, typeEnd: 5.1, title: 'claude — shop',
    lines: ['>p token refresh loops forever', '⏺ Grep("refresh_token")', '⏺ Which expiry do you want?', 'q   1. sliding   2. fixed', '>p '] },
  { x: 1650, y: 620, r: 3, at: 4.9, typeEnd: 5.4, title: 'claude — infra',
    lines: ['>p speed up CI', '⏺ Read(.github/workflows/ci.yml)', '✻o Thinking…'] },
  { x: 960, y: 900, r: -1, at: 5.2, typeEnd: 5.7, title: 'claude — shop',
    lines: ['>p add examples to the API docs', '⏺ Bash(cargo doc)', '  ⎿d Running…'] },
];
const BUBBLES = [
  { at: 5.4, x: 300, y: 215, text: 'Is this one done?' },
  { at: 5.8, x: 1090, y: 215, text: 'Wait… which one was the auth fix?' },
  { at: 6.2, x: 1190, y: 650, text: 'Is it waiting on me?' },
  { at: 6.55, x: 250, y: 650, text: 'What did it even change?' },
  { at: 6.9, x: 700, y: 455, text: 'Where was I?' },
];

function termLine(l) {
  // a leading marker picks the line's color: > prompt, d dim, o orange, q question, g green
  let cls = '';
  if (l.startsWith('>p')) return `<span class="p">&gt;</span>${esc(l.slice(2))}`;
  if (l.startsWith('  ⎿d')) return `<span class="d">  ⎿${esc(l.slice(4))}</span>`;
  if (l.startsWith('✻o')) return `<span class="o">✻${esc(l.slice(2))}</span>`;
  if (l.startsWith('q')) cls = 'q';
  if (l.startsWith('g')) cls = 'g';
  if (cls) return `<span class="${cls}">${esc(l.slice(1))}</span>`;
  if (l.startsWith('⏺')) return `<span class="g">⏺</span>${esc(l.slice(1))}`;
  return esc(l);
}

function renderChaos(t, frame) {
  const el = $('chaos');
  if (t > 10.3) { el.innerHTML = ''; return; }
  const implode = E.in(P(t, 9.45, 10.0));
  const shake = P(t, 5.5, 8.0) * 3 + P(t, 8.0, 9.4) * 6;
  const push = 1 + 0.06 * P(t, 2, 9.4);
  let h = '';
  TERMS.forEach((tm, i) => {
    if (t < tm.at) return;
    const u = E.back(P(t, tm.at, tm.at + 0.4));
    const o = P(t, tm.at, tm.at + 0.15);
    const jx = (rnd(frame * 7 + i) - 0.5) * shake, jy = (rnd(frame * 13 + i) - 0.5) * shake;
    // implode into the center
    const x = lerp((tm.x - 960) * push + 960, 960, implode) + jx;
    const y = lerp((tm.y - 540) * push + 540, 540, implode) + jy;
    const sc = (0.6 + 0.4 * u) * lerp(1, 0.02, implode) * (i === 0 ? 1 : 0.92);
    const rot = tm.r + implode * (i % 2 ? 140 : -140);
    // lines
    const since = t - tm.at;
    const first = tm.lines[0];
    let body = '';
    const promptTxt = first.slice(2);
    const tStart = tm.at + 0.2, tEnd = tm.typeEnd;
    const shown = typed(promptTxt, t, tStart, tEnd);
    body += `<span class="p">&gt;</span> ${esc(shown.trimStart())}${t < tEnd + 0.1 ? '<span style="background:#c9cdd3">&nbsp;</span>' : ''}\n`;
    tm.lines.slice(1).forEach((l, j) => {
      const at = tEnd + 0.25 + j * (i === 0 ? 0.35 : 0.3);
      if (t >= at) body += termLine(l) + '\n';
    });
    // the blinking question in the waiting terminals
    const glitch = (t > 7.3 && t < 7.55) || (t > 7.95 && t < 8.15);
    const gx = glitch ? (rnd(frame + i * 3) - 0.5) * 16 : 0;
    h += `<div class="term" style="left:${x - 300}px;top:${y - 180}px;opacity:${o * (1 - P(t, 9.85, 10.0))};
      transform:translateX(${gx}px) rotate(${rot}deg) scale(${sc});z-index:${i + 1};
      ${glitch ? 'filter:drop-shadow(6px 0 0 rgba(255,0,80,.6)) drop-shadow(-6px 0 0 rgba(0,220,255,.6));' : ''}">
      <div class="tb"><div class="lights"><i></i><i></i><i></i></div><div class="ttl">${tm.title} — 100×30</div></div>
      <div class="tc">${body}</div></div>`;
    void since;
  });
  BUBBLES.forEach((b, i) => {
    if (t < b.at) return;
    const u = E.back(P(t, b.at, b.at + 0.35));
    const x = lerp(b.x, 960, implode), y = lerp(b.y, 540, implode);
    const bob = Math.sin((t - b.at) * 3 + i) * 4;
    h += `<div class="bubble" style="left:${x}px;top:${y + bob}px;z-index:${20 + i};transform:scale(${u * lerp(1, 0, implode)});
      transform-origin:30px 80px;opacity:${P(t, b.at, b.at + 0.1) * (1 - P(t, 7.0, 7.3))}">${esc(b.text)}</div>`;
  });
  // darken for the punchline
  const dark = P(t, 7.1, 7.5) * 0.78 * (1 - P(t, 9.6, 10));
  h += `<div style="position:absolute;inset:0;background:#06080b;opacity:${dark};z-index:30"></div>`;
  el.innerHTML = h;
  el.style.transform = `scale(${1 + 0.03 * P(t, 0, 9.4)})`;
}

// ================================================================== LOGO
function iconSvg(size, draw = 1, dot = 1, glow = 1, id = 'a') {
  const C1 = 2 * Math.PI * 92, C2 = 2 * Math.PI * 150;
  return `<svg width="${size}" height="${size}" viewBox="0 0 512 512">
    <defs>
      <linearGradient id="tile${id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1d2635"/><stop offset="1" stop-color="#0d121a"/></linearGradient>
      <radialGradient id="glw${id}"><stop offset="0" stop-color="#5ae1ff" stop-opacity="${0.9 * glow}"/><stop offset=".35" stop-color="#00aaff" stop-opacity="${0.35 * glow}"/><stop offset="1" stop-color="#00aaff" stop-opacity="0"/></radialGradient>
    </defs>
    <rect x="48" y="48" width="416" height="416" rx="92" fill="url(#tile${id})" stroke="#2a3443" stroke-width="2" opacity="${clamp(draw * 3)}"/>
    <circle cx="256" cy="256" r="150" fill="none" stroke="#5b6573" stroke-width="7" stroke-linecap="round"
      stroke-dasharray="${C2}" stroke-dashoffset="${C2 * (1 - E.io(clamp(draw * 1.2 - 0.2)))}" transform="rotate(-90 256 256)"/>
    <circle cx="256" cy="256" r="92" fill="none" stroke="#5b6573" stroke-width="7" stroke-linecap="round"
      stroke-dasharray="${C1}" stroke-dashoffset="${C1 * (1 - E.io(clamp(draw * 1.2)))}" transform="rotate(90 256 256)"/>
    <circle cx="256" cy="256" r="${90 * dot}" fill="url(#glw${id})"/>
    <circle cx="256" cy="256" r="${15 * dot}" fill="#7fe9ff"/>
  </svg>`;
}

function renderLogo(t) {
  const el = $('logo');
  let h = '';
  // 10-14: the reveal
  if (t >= 9.95 && t < 14.4) {
    const out = E.io(P(t, 13.1, 14.1));
    const draw = P(t, 10.0, 10.9);
    const dot = E.back(P(t, 10.0, 10.35));
    const pulse = 1 + 0.25 * Math.exp(-(t - 10) * 4);
    const sc = (1 - 0.35 * out) * (1 + 0.04 * P(t, 10, 13));
    h += `<div style="position:absolute;inset:0;transform:translateY(${-140 * out}px) scale(${sc});opacity:${1 - out};transform-origin:960px 540px">`;
    // shockwave
    const sw = P(t, 10.0, 11.0);
    if (sw < 1) h += `<div style="position:absolute;left:${960 - 1000 * E.out(sw)}px;top:${400 - 1000 * E.out(sw)}px;width:${2000 * E.out(sw)}px;height:${2000 * E.out(sw)}px;border-radius:50%;border:${3 + 10 * (1 - sw)}px solid rgba(90,225,255,${0.6 * (1 - sw)})"></div>`;
    h += `<div style="position:absolute;left:${960 - 150}px;top:${400 - 150}px">${iconSvg(300, draw, dot, pulse, 'l')}</div>`;
    const letters = 'scope'.split('');
    h += `<div class="wordmark" style="left:0;right:0;text-align:center;top:560px">`;
    letters.forEach((c, i) => {
      const u = E.out5(P(t, 10.45 + i * 0.06, 11.0 + i * 0.06));
      h += `<span style="display:inline-block;transform:translateY(${(1 - u) * 60}px);opacity:${u};filter:blur(${(1 - u) * 10}px)">${c}</span>`;
    });
    h += `</div>`;
    const tg = E.out(P(t, 11.2, 11.8));
    h += `<div class="tagline" style="top:760px;opacity:${tg};transform:translateY(${(1 - tg) * 20}px)">Mission control for your <span class="hl">Claude Code</span> sessions.</div>`;
    h += `</div>`;
  }
  // 88-95: the end card
  if (t >= 108.0) {
    t -= 20; // the end card was timed for 88 s
    const draw = P(t, 88.05, 88.9);
    const dot = E.back(P(t, 88.0, 88.35));
    const pulse = 1 + 0.2 * Math.exp(-(t - 88) * 4) + 0.05 * Math.sin(t * 3);
    const sw = P(t, 88.0, 89.0);
    if (sw < 1) h += `<div style="position:absolute;left:${960 - 900 * E.out(sw)}px;top:${370 - 900 * E.out(sw)}px;width:${1800 * E.out(sw)}px;height:${1800 * E.out(sw)}px;border-radius:50%;border:${2 + 8 * (1 - sw)}px solid rgba(90,225,255,${0.45 * (1 - sw)})"></div>`;
    h += `<div style="position:absolute;left:${960 - 120}px;top:${370 - 120}px">${iconSvg(240, draw, dot, pulse, 'e')}</div>`;
    h += `<div class="wordmark" style="left:0;right:0;text-align:center;top:500px;font-size:128px">`;
    'scope'.split('').forEach((c, i) => {
      const u = E.out5(P(t, 88.35 + i * 0.05, 88.9 + i * 0.05));
      h += `<span style="display:inline-block;transform:translateY(${(1 - u) * 50}px);opacity:${u};filter:blur(${(1 - u) * 10}px)">${c}</span>`;
    });
    h += `</div>`;
    const tg = E.out(P(t, 89.0, 89.6));
    h += `<div class="tagline" style="top:668px;opacity:${tg};transform:translateY(${(1 - tg) * 20}px)">Your Claude Code sessions, <span class="hl">organized.</span></div>`;
    const pl = E.out(P(t, 89.7, 90.3));
    h += `<div class="pill" style="top:770px;opacity:${pl};transform:translateX(-50%) translateY(${(1 - pl) * 20}px)"><span class="g">$</span> brew install --cask guintherkovalski/scope/scope</div>`;
    const ft = E.out(P(t, 90.4, 91.0));
    h += `<div class="tagline" style="top:870px;font-size:22px;color:#6b7684;opacity:${ft}">macOS · built on Claude Code</div>`;
  }
  el.innerHTML = h;
}

// ================================================================== CAPTIONS
function words(str, t, t0, delay0 = 0) {
  // *x* marks a highlighted run
  const parts = [];
  str.split(/(\*[^*]+\*)/).forEach(seg => {
    if (!seg) return;
    const hl = seg.startsWith('*');
    const txt = hl ? seg.slice(1, -1) : seg;
    txt.split(/(\s+)/).forEach(w => { if (w) parts.push({ w, hl }); });
  });
  let i = 0;
  return parts.map(p => {
    if (/^\s+$/.test(p.w)) return p.w;
    const d = t0 + delay0 + (i++) * 0.04;
    const u = E.out5(P(t, d, d + 0.55));
    return `<span class="w ${p.hl ? 'hl' : ''}" style="opacity:${u};transform:translateY(${(1 - u) * 34}px);filter:blur(${(1 - u) * 8}px)">${esc(p.w)}</span>`;
  }).join('');
}

function renderCaps(t) {
  let h = '';
  let leftOn = 0, topOn = 0;
  for (const c of CAPS) {
    if (t < c.t0 - 0.05 || t > c.t1 + 0.05) continue;
    const out = E.in(P(t, c.t1 - 0.35, c.t1));
    const style = `opacity:${1 - out};transform:translateY(${-18 * out}px)`;
    if (c.mode === 'title') {
      const halo = P(t, c.t0 - 0.2, c.t0 + 0.2) * (1 - out);
      h += `<div style="position:absolute;left:260px;top:300px;width:1400px;height:420px;opacity:${halo};
        background:radial-gradient(closest-side,rgba(6,8,11,.88),rgba(6,8,11,.6) 55%,transparent)"></div>`;
      h += `<div class="title" style="top:452px;font-size:88px;${style}">${words(c.h, t, c.t0)}</div>`;
    } else if (c.mode === 'center') {
      const push = 1 + 0.05 * P(t, c.t0, c.t1);
      h += `<div class="title" style="top:360px;font-size:80px;${style};transform:scale(${push})">${words(c.h, t, c.t0)}</div>`;
      h += `<div class="title" style="top:480px;font-size:104px;${style};transform:scale(${push})">${words(c.s2, t, c.s2at)}</div>`;
    } else if (c.mode === 'themes') {
      topOn = Math.max(topOn, P(t, c.t0 - 0.3, c.t0) * (1 - out));
      const cur = themeAt(t).name;
      const wild = cur === 'matrix' || cur === 'paper';
      const names = ['onedark', 'dracula', 'github'].map((k, i) => {
        const u = E.out5(P(t, c.t0 + i * 0.08, c.t0 + 0.5 + i * 0.08));
        const on = cur === k && !wild;
        return `<span class="w ${on ? 'hl' : ''}" style="margin:0 18px;opacity:${u * (on ? 1 : 0.32)};transform:translateY(${(1 - u) * 30}px) scale(${on ? 1.06 : 1})">${THEME_NAMES[k]}</span>`;
      }).join('');
      const su = E.out(P(t, c.t0 + 0.35, c.t0 + 0.9));
      h += `<div class="cap top" style="top:52px;${style}"><div class="h">${names}</div><div class="s" style="opacity:${su}">${wild ? `<span class="hl" style="${cur === 'matrix' ? 'background:linear-gradient(90deg,#00ff41,#8dff9e);-webkit-background-clip:text;background-clip:text' : ''}">Or anything you can describe.</span>` : esc(c.s)}</div></div>`;
    } else if (c.mode === 'top') {
      topOn = Math.max(topOn, P(t, c.t0 - 0.3, c.t0) * (1 - out));
      const su = E.out(P(t, c.t0 + 0.35, c.t0 + 0.9));
      h += `<div class="cap top" style="top:52px;${style}"><div class="h">${words(c.h, t, c.t0)}</div>
        ${c.s ? `<div class="s" style="opacity:${su}">${esc(c.s)}</div>` : ''}</div>`;
    } else {
      leftOn = Math.max(leftOn, P(t, c.t0 - 0.6, c.t0) * (1 - E.in(P(t, c.t1 - 0.1, c.t1 + 0.5))));
      const ku = E.out(P(t, c.t0, c.t0 + 0.5));
      const su = E.out(P(t, c.t0 + 0.4, c.t0 + 1.0));
      h += `<div class="cap" style="left:110px;top:0;bottom:0;width:640px;display:flex;flex-direction:column;justify-content:center;${style}">
        <div class="k" style="opacity:${ku}"><span class="num">${c.n}</span><span class="bar" style="width:${40 * ku}px"></span><span class="hl">${esc(c.k)}</span></div>
        <div class="h">${words(c.h, t, c.t0 + 0.08)}</div>
        <div class="s" style="opacity:${su};transform:translateY(${(1 - su) * 16}px)">${esc(c.s)}</div></div>`;
    }
  }
  $('caps').innerHTML = h;
  return { leftOn, topOn };
}

// ================================================================== IDE: state over time
function scopeAt(s, t) { return t >= FLY0 + SESS.indexOf(s) * FLYSTEP + FLYDUR ? s.scope : null; }

function sessState(id, t) {
  const s = id === 'orders' ? ORDERS : SESS.find(x => x.id === id);
  const st = { name: s.name, working: false, tag: s.tag, time: s.time, queued: 0 };
  if (id === 'login') { st.working = t < T.loginDone; if (!st.working) { st.tag = 'done'; st.time = '14:41'; } }
  if (id === 'checkout') { st.working = true; st.queued = t < 60 ? 2 : 1; }
  if (id === 'orders') {
    st.name = t < T.rename ? FIRST_PROMPT.slice(0, 60) : 'Orders CSV export';
    st.working = (t >= T.ordersBorn && t < T.ordersDone) || (t >= T.send2 && t < T.ordersDone2) || (t >= T.send3 && t < T.tmplDone) || themeWorking(t) || t >= DIC.send;
    st.tag = t >= T.ordersDone ? 'done' : null;
    st.time = t < T.send2 ? '14:41' : '14:43';
  }
  return st;
}

function tabAt(t) {
  if (t < T.api) return 'ALL';
  if (t < T.web) return 'API';
  if (t < T.api2) return 'WEB';
  if (t < T.all) return 'API';
  return 'ALL';
}

function listFor(tab, t) {
  const base = SESS.filter(s => tab === 'ALL' || s.scope === tab).map(s => s.id);
  if (t >= T.ordersBorn && (tab === 'ALL' || tab === 'API')) base.unshift('orders');
  return base;
}

function selectedAt(t) {
  if (t < T.api) return 'login';
  if (t < T.newClick) return 'orch';
  if (t < T.ordersBorn) return null;
  return 'orders';
}

// ------------------------------------------------------------------ small widgets
function spinner(t, size = 14, color = 'var(--text)') {
  const a = (t * 400) % 360;
  const r = size / 2 - 1.5;
  const sweep = 200 + 60 * Math.sin(t * 3);
  const x1 = size / 2 + r * Math.cos(0), y1 = size / 2 + r * Math.sin(0);
  const x2 = size / 2 + r * Math.cos(sweep * Math.PI / 180), y2 = size / 2 + r * Math.sin(sweep * Math.PI / 180);
  return `<svg width="${size}" height="${size}" style="transform:rotate(${a}deg);display:inline-block;vertical-align:middle">
    <path d="M${x1} ${y1} A${r} ${r} 0 ${sweep > 180 ? 1 : 0} 1 ${x2} ${y2}" style="stroke:${color}" stroke-width="1.8" fill="none" stroke-linecap="round"/></svg>`;
}
const TAGS = {
  done: ['✔', 'var(--green)', 'done'],
  needs: ['✋', 'var(--gold)', 'needs you'],
  blocked: ['⚠', 'var(--lightred)', 'blocked'],
};
function tagHtml(tag) {
  if (!tag) return '';
  const [i, c, w] = TAGS[tag];
  return `<span class="tagw"><span style="color:${c}">${i}</span> ${w}</span>`;
}
const check = `<svg width="14" height="14" viewBox="0 0 14 14"><path d="M2.5 7.5 L5.8 10.5 L11.5 3.5" style="stroke:var(--selText)" stroke-width="2" fill="none"/></svg>`;

// ------------------------------------------------------------------ left panel
function tabsHtml(t) {
  const tab = tabAt(t);
  let h = `<span class="slabel ${tab === 'ALL' ? 'on' : ''}" data-k="tab-ALL">SCOPES</span>`;
  for (const [name, sc] of Object.entries(SCOPES)) {
    if (t < sc.at) continue;
    const u = E.back(P(t, sc.at, sc.at + 0.35));
    // a bump each time a session lands on it
    let bump = 0;
    SESS.forEach((s, i) => { if (s.scope === name) { const a = FLY0 + i * FLYSTEP + FLYDUR; bump = Math.max(bump, 1 - P(t, a, a + 0.3)) * (t >= a ? 1 : 0); } });
    h += `<span class="stab ${tab === name ? 'on' : ''}" data-k="tab-${name}" style="--c:${rgb(sc.c)};background:${rgb(sc.c, 90 / 255)};
      transform:scale(${u * (1 + 0.14 * bump)});${bump ? `box-shadow:0 0 ${16 * bump}px ${rgb(sc.c, 0.8)}` : ''}">${name}</span>`;
  }
  if (t >= T.plusClick && t < SCOPES.API.at) {
    const txt = typed('api', t, ...T.typeScope);
    h += `<span style="background:#0a0a0a;border:1px solid var(--selText);border-radius:2.4px;padding:3px 6px;min-width:150px;display:inline-block">${txt ? esc(txt) + '<span class="caret" style="height:15px;width:7px"></span>' : '<span style="color:var(--weak)">Scope name, Enter to create</span>'}</span>`;
  }
  h += `<span class="btn" data-k="tab-plus">+</span>`;
  return `<div class="tabs">${h}</div>`;
}

function rowHtml(id, t, tab, sel, extra = {}) {
  const st = sessState(id, t);
  const s = id === 'orders' ? ORDERS : SESS.find(x => x.id === id);
  let bar = '';
  if (tab === 'ALL') {
    const sc = id === 'orders' ? 'API' : scopeAt(s, t);
    if (sc) {
      const i = SESS.indexOf(s);
      const g = id === 'orders' ? 1 : E.out(P(t, FLY0 + i * FLYSTEP + FLYDUR, FLY0 + i * FLYSTEP + FLYDUR + 0.3));
      bar = `<div class="cbar" style="background:${rgb(SCOPES[sc].c)};transform:scaleY(${g})"></div>`;
    }
  }
  let title = esc(st.name) + (st.queued ? `<span style="color:var(--weak)">  +${st.queued} queued</span>` : '');
  if (id === 'orders' && t >= T.rename - 0.05 && t < T.rename + 1.2) {
    const u = P(t, T.rename, T.rename + 0.9);
    const shown = typed('Orders CSV export', t, T.rename, T.rename + 0.45);
    title = `<span style="background:linear-gradient(90deg,var(--selText) ${u * 140 - 40}%,#5ae1ff ${u * 140 - 20}%,var(--selText) ${u * 140}%);-webkit-background-clip:text;color:transparent">${esc(shown)}</span><span class="caret" style="height:16px;width:2px;opacity:${shown.length < 17 ? 1 : 0}"></span>`;
  }
  const meta = st.working ? '' : `<span data-k="meta-${id}">${tagHtml(st.tag)}<span data-k="time-${id}">${esc(st.time)}</span></span>`;
  const born = id === 'orders' ? E.out(P(t, T.ordersBorn, T.ordersBorn + 0.4)) : 1;
  const style = id === 'orders' ? `max-height:${born * 90}px;opacity:${born};overflow:hidden;${born < 1 ? 'min-height:0;' : ''}` : '';
  return `<div class="row ${sel ? 'sel' : ''}" data-k="row-${id}" style="${style}${extra.style || ''}">${bar}
    <div class="ttl">${title}</div>${extra.note ? `<div class="note">${esc(extra.note)}</div>` : ''}
    <div class="meta">${meta}</div>
    ${st.working ? `<div class="spinwrap" data-k="meta-${id}">${spinner(t, 16, sel ? 'var(--selText)' : 'var(--text)')}</div>` : ''}</div>`;
}

const ORCH = {
  API: { pct: 62, summary: 'Auth and orders API: the login redirect fix, rate limits, token refresh and a CSV export for the Orders page.',
    attn: ['Auth token refresh: pick sliding or fixed expiry before it can go on', 'API docs examples: blocked on a staging API key'],
    next: 'Fix login redirect: verify the redirect on staging, then open the PR' },
  WEB: { pct: 48, summary: 'Storefront work: a new checkout page, dark mode, and a flaky cart test that blocks CI.',
    attn: ['Flaky cart test: the fixture clock is shared between tests'], next: 'Checkout page redesign: two queued prompts for the payment step' },
};
const NOW = {
  login: ['Carrying next through the cookie refresh', 3, 4], checkout: ['Splitting the payment step into its own form', 2, 5],
  rate: ['Per-user limits live behind a flag', 5, 5], token: ['Waiting for an expiry decision', 2, 4],
  docs: ['Needs a staging API key to run the examples', 1, 3], orders: ['Export button and streaming writer done', 3, 5],
  cart: ['Shared fixture clock makes tests order-dependent', 1, 3], dark: ['Toggle and stored preference shipped', 4, 4],
  ci: ['Cache keyed by lockfile, 6 min faster', 3, 3], pg: ['pg_trgm needs a manual upgrade step', 2, 4],
};

function orchBubble(tab, t, sel) {
  const ids = listFor(tab, t);
  const working = ids.filter(id => sessState(id, t).working).length;
  const needs = ids.filter(id => sessState(id, t).tag === 'needs').length;
  const pct = ORCH[tab] ? ORCH[tab].pct : 70;
  return `<div class="orch ${sel ? 'sel' : ''}" data-k="orch">
    <div class="t">Orchestration</div>
    <div class="l">${ids.length} sessions${working ? ` · ${working} working` : ''} · ${pct}%${needs ? ` · ${needs} need${needs > 1 ? '' : 's'} you` : ''}</div>
    ${working ? `<div style="position:absolute;right:12px;top:16px">${spinner(t, 16, sel ? 'var(--selText)' : 'var(--text)')}</div>` : ''}</div>`;
}

const PICKS = [
  ['token', 'Fixed the refresh loop in the auth middleware'],
  ['login', 'Login sent users to a stale page after auth'],
  ['rate', 'Reworked auth headers for per-user limits'],
];

function leftPanel(t) {
  const tab = tabAt(t);
  const sel = selectedAt(t);
  const build = P(t, 13.55, 14.5);
  let list = '';
  const searching = t >= T.typeS[0] && t < T.clearS;
  if (tab !== 'ALL' && t >= T.api) list += orchBubble(tab, t, sel === 'orch');
  if (searching && t >= T.typeS[1] - 0.6) {
    if (t < T.picks) {
      if (t < T.sendS) list += `<div class="searchinfo">No matches. Press Enter to ask Haiku.</div>`;
    } else {
      list += `<div class="searchinfo" style="opacity:${P(t, T.picks, T.picks + 0.3)}">Haiku's picks, best first. Edit the query for live results.</div>`;
      PICKS.forEach(([id, note], i) => {
        const u = E.back(P(t, T.picks + i * 0.14, T.picks + i * 0.14 + 0.4));
        list += rowHtml(id, t, tab, false, { note, style: `opacity:${clamp(u)};transform:translateY(${(1 - u) * 20}px)` });
      });
    }
  } else {
    listFor(tab, t).forEach((id, i) => {
      // rows arrive one by one as the window builds
      const a = 14.6 + i * 0.12;
      const u = t < 16.5 ? E.out(P(t, a, a + 0.4)) : 1;
      const s = u < 1 ? `opacity:${u};transform:translateX(${(1 - u) * -30}px)` : '';
      list += rowHtml(id, t, tab, sel === id, { style: s });
    });
  }
  let sbox;
  if (t >= T.typeS[0] && t < T.clearS) {
    const q = typed(SEARCH_Q, t, ...T.typeS);
    sbox = `<span>${esc(q)}</span>${t < T.picks ? '<span class="caret" style="height:16px;width:2px"></span>' : ''}`;
  } else sbox = `<span class="hint">🔍 Search · Enter asks Haiku</span>`;
  const asking = t >= T.sendS && t < T.picks ? `<div class="searchinfo">${spinner(t, 13)} Asking Haiku…</div>` : '';
  return `<div class="lp" style="opacity:${build};transform:translateX(${(1 - E.out(build)) * -40}px)">
    ${tabsHtml(t)}
    <div class="newbtn" data-k="newbtn">+&nbsp; New session</div>
    <div class="sep"></div>
    <div class="list">${list}</div>
    ${asking}
    <div class="search" data-k="search" ${t >= T.searchClick && t < T.clearS ? 'style="border-color:var(--selText)"' : ''}>${sbox}</div>
  </div>`;
}

// ------------------------------------------------------------------ center: conversation, pages
const CODE_LOGIN = `<span class="kw">let</span> target = <span class="fn">safe_next</span>(&amp;req).<span class="fn">unwrap_or</span>(<span class="str">"/home"</span>);`;

function card(who, body, extra = '') {
  return `<div class="card ${who === 'You' ? 'you' : 'cl'}" ${extra}><div class="who"><span>${who}</span>
    <span class="ico"><span>🗑</span><span class="copyico"><i></i><i></i></span></span></div><div class="md">${body}</div></div>`;
}

function stream(text, t, t0, cps = 70) {
  // text as Markdown-ish HTML; reveals visible characters only
  const n = Math.max(0, Math.floor((t - t0) * cps));
  let out = '', count = 0, i = 0;
  while (i < text.length && count < n) {
    if (text[i] === '<') { const j = text.indexOf('>', i); out += text.slice(i, j + 1); i = j + 1; continue; }
    if (text[i] === '&') { const j = text.indexOf(';', i); out += text.slice(i, j + 1); i = j + 1; count++; continue; }
    out += text[i]; i++; count++;
  }
  // close any open tags by letting the browser fix it
  return out;
}
const visLen = s => s.replace(/<[^>]+>/g, '').replace(/&[a-z]+;/g, '_').length;

function workingLine(t, label) {
  return `<div class="working">${spinner(t, 15)}<span>${esc(label)}</span></div>`;
}

function convLogin(t) {
  let h = card('You', `<p>after login, users land on /home instead of the page they came from. can you fix it?</p>`);
  h += card('Claude', `<p>Found it: <code>login_handler</code> drops the <code>next</code> parameter when the session cookie is refreshed.</p>
    <div class="tool">⚙ Read · Grep · Read · Edit</div>
    <p>I now carry <code>next</code> through the refresh and check it against our own host, so it can't redirect off-site:</p>
    <pre style="font:13.5px var(--mono);background:var(--pre);padding:8px 10px;border-radius:4px;margin-bottom:7px">${CODE_LOGIN}</pre>
    <p>Running the auth tests now.</p>`);
  h += `<div class="tool" style="margin-left:14px">⚙ Bash</div>`;
  if (t < T.loginDone) h += workingLine(t, '⚙ Bash…');
  return h;
}

const C1A = `<p>I'll add a streaming CSV export that reuses the page's filters.</p>`;
const C1TOOLS = ['Read', 'Grep', 'Read', 'Edit', 'Write', 'Edit', 'Bash'];
const C1B = `<p><b>Done.</b> The Orders page has an <b>Export CSV</b> button:</p>`;
const C1T = [['Streaming writer', 'src/orders/export.rs'], ['GET /orders/export', 'src/orders/routes.rs'], ['Export button', 'web/OrdersPage.tsx']];
const C1C = `<p>Tests pass: <code>cargo test orders</code> (14 passed).</p>`;
const C2 = `<p>Done: the file now starts with <code># status=shipped · from 2026-10-01 · 1,284 rows</code>, so every export says what it contains.</p>`;

function convOrders(t) {
  let h = card('You', `<p>${esc(FIRST_PROMPT)}</p>`, `style="opacity:${E.out(P(t, T.ordersBorn, T.ordersBorn + 0.3))}"`);
  const s1 = T.ordersBorn + 0.5;
  if (t >= s1) {
    let b = stream(C1A, t, s1, 80);
    const toolsAt = s1 + 0.9;
    const nTools = Math.min(C1TOOLS.length, Math.floor((t - toolsAt) / 0.32) + 1);
    if (t >= toolsAt) b += `<div class="tool">⚙ ${C1TOOLS.slice(0, nTools).join(' · ')}</div>`;
    const bAt = toolsAt + C1TOOLS.length * 0.32 + 0.1;
    if (t >= bAt) b += stream(C1B, t, bAt, 90);
    const tblAt = bAt + 0.6;
    if (t >= tblAt) {
      b += `<table class="mdtable"><tr><th>Piece</th><th>Where</th></tr>`;
      C1T.forEach((r, i) => { if (t >= tblAt + i * 0.2) b += `<tr><td>${r[0]}</td><td><code>${r[1]}</code></td></tr>`; });
      b += `</table>`;
    }
    if (t >= tblAt + 0.8) b += stream(C1C, t, tblAt + 0.8, 90);
    h += card('Claude', b);
    if (t < T.ordersDone) {
      const tool = t < toolsAt ? 'Claude is working…' : t < bAt ? `⚙ ${C1TOOLS[Math.max(0, nTools - 1)]}…` : 'Writing…';
      h += workingLine(t, tool);
    }
  }
  if (t >= T.send2) {
    h += card('You', `<p>${esc(FOLLOW_UP)}</p>`);
    const s2 = T.send2 + 0.5;
    if (t >= s2) {
      let b = '';
      if (t >= s2 + 0.2) b += `<div class="tool">⚙ Read · Edit${t >= s2 + 0.8 ? ' · Bash' : ''}</div>`;
      if (t >= s2 + 1.2) b += stream(C2, t, s2 + 1.2, 75);
      h += card('Claude', b || '<p></p>');
      if (t < T.ordersDone2) h += workingLine(t, t < s2 + 1.2 ? '⚙ Edit…' : 'Writing…');
    }
  }
  if (t >= T.send3) {
    h += card('You', `<p>${esc(TEMPLATE_TEXT)}</p>`, `style="opacity:${E.out(P(t, T.send3, T.send3 + 0.3))}"`);
    if (t >= T.send3 + 0.5 && t < T.tmplDone) h += workingLine(t, '⚙ Read…');
    if (t >= T.tmplDone) h += card('Claude', `<div class="tool">⚙ Read · Write · Bash</div><p>Added 6 tests in <code>tests/orders_export.rs</code>; all pass.</p>`);
  }
  if (t >= T.thSend) {
    h += card('You', `<p>${esc(THEME_PROMPT)}</p>`, `style="opacity:${E.out(P(t, T.thSend, T.thSend + 0.3))}"`);
    let b = '';
    if (t >= T.thTool) b += `<div class="tool">⚙ Bash(scope style set background=#282c34 text=#abb2bf accent=#61afef …)</div>`;
    if (t >= T.thReply) b += `<p>Done: scope now wears <b>One Dark Pro</b>. Say the word for another theme.</p>`;
    if (b) h += card('Claude', b);
    else h += workingLine(t, 'Claude is working…');
  }
  if (t >= DIC.send) {
    const [head, ...rest] = REFINED.split('\n');
    const items = rest.filter(l => /^\d\./.test(l)).map(l => `<li>${esc(l.replace(/^\d\.\s*/, ''))}</li>`).join('');
    h += card('You', `<p>${esc(head)}</p><ol style="margin:0 0 7px 22px">${items}</ol><p>${esc(rest[rest.length - 1])}</p>`, `style="opacity:${E.out(P(t, DIC.send, DIC.send + 0.3))}"`);
    if (t >= DIC.send + 0.4) h += workingLine(t, '⚙ Read…');
  }
  CHIPS.forEach(c => {
    if (t < c.send) return;
    h += card('You', `<p>${esc(c.text)}</p>`);
    if (t >= c.send + 0.25) h += card('Claude', `<div class="tool">⚙ Bash(scope style set …)</div>${t >= c.send + 0.35 ? `<p>Done: <b>${THEME_NAMES[c.to]}</b>.</p>` : ''}`);
  });
  return h;
}

function newSession(t) {
  return `<div class="newsess" style="padding:10px 6px"><div class="h1">New session in ~/code/shop</div><div class="h2">on main</div>
    <div class="st">${t >= T.send1 ? 'Starting…' : 'Type your first prompt below.'}</div></div>`;
}

function orchPage(tab, t) {
  const o = ORCH[tab];
  const sc = SCOPES[tab].c;
  const ids = listFor(tab, t);
  const working = ids.filter(id => sessState(id, t).working).length;
  const since = t - (tab === 'WEB' ? T.web : t < T.web ? T.api : T.api2);
  const fade = E.out(clamp(since / 0.35));
  const fill = E.out(clamp((since - 0.2) / 0.9)) * o.pct;
  let tiles = '';
  ids.forEach((id, i) => {
    const st = sessState(id, t);
    const [now, done, of] = NOW[id];
    const u = E.out(clamp((since - 0.15 - i * 0.07) / 0.4));
    const mark = st.working ? '' : st.tag ? `<span>${tagHtml(st.tag)}</span>` : '<span>idle</span>';
    tiles += `<div class="tile" style="opacity:${u};transform:translateY(${(1 - u) * 14}px)"><div class="tt">${esc(st.name)}</div>
      <div class="nw">${esc(now)}</div>
      <div class="small" style="margin-top:6px">Goal ${done}/${of}</div><div class="obar" style="margin:3px 0 0"><div class="f" style="width:${done / of * 100}%;background:${rgb(sc)}"></div></div>
      <div class="ft">${mark}<span>${st.working ? `working ${spinner(t, 13)}` : esc(st.time)}</span></div></div>`;
  });
  return `<div style="padding:12px 16px;opacity:${fade}">
    <div style="display:flex;align-items:baseline;gap:12px"><span style="font-size:21.6px;color:var(--strong)">${tab}</span>
      <span class="small" style="font-size:13px">${ids.length} sessions · ${working} working · ${tab === 'WEB' ? 2 : 0} queued</span>
      <span style="flex:1"></span><span class="btn" style="font-size:12px">↻ Update</span><span class="btn" style="font-size:12px">Clear &amp; re-summarize</span></div>
    <div class="ocard"><div class="stripe" style="background:${rgb(sc)}"></div>
      <div class="sum">${esc(o.summary)}</div>
      <div class="small">Progress ${Math.round(fill)}%</div>
      <div class="obar"><div class="f" style="width:${fill}%;background:${rgb(sc)}"></div></div>
      ${o.attn.map(a => `<div class="small gold" style="font-size:12px;margin:2px 0">⚠ ${esc(a)}</div>`).join('')}
      <div style="font-size:12px;margin:6px 0 2px">Next: ${esc(o.next)}</div>
      <div class="small" style="margin-top:8px">In the bar's order · drag a tile before or after another to reorder · right-click for more</div>
      <div class="tiles">${tiles}</div></div></div>`;
}

const TMPL_BODY = `<p>Write focused tests for the code changed in this session.</p>
  <ul><li>One test per behavior, named after it</li><li>Edge cases: empty input, very large input, bad input</li>
  <li>Run them, and fix what fails before replying</li></ul>
  <p>Reply with what each test covers, in one line each.</p>`;

function templatePage(t) {
  const u = E.out(P(t, T.tmplClick, T.tmplClick + 0.35));
  return `<div class="tpage" style="opacity:${u};transform:translateY(${(1 - u) * 12}px)">
    <h2>Write tests</h2><div class="small" style="font-size:12px">Prompt templates · ~/code/shop/.claude/templates/write-tests.md</div>
    <div style="margin-top:8px">Cover the changed code with focused tests</div>
    <div class="bar"><span class="sw"><span class="on">Rendered</span><span>Source</span></span><span style="color:var(--border)">|</span>
      <span class="btn">Copy</span><span class="btn" data-k="use-in-prompt" ${t >= T.useClick ? 'style="background:var(--sel);color:var(--selText)"' : ''}>Use in prompt</span><span class="btn">✎ Change with Claude</span></div>
    <div class="hr"></div><div class="md" style="padding:4px 12px">${TMPL_BODY}</div></div>`;
}

function inputState(t) {
  const working = t < T.loginDone && t < T.newClick ? true
    : (t >= T.ordersBorn && t < T.ordersDone) || (t >= T.send2 && t < T.ordersDone2) || (t >= T.send3 && t < T.tmplDone) || themeWorking(t) || t >= DIC.send;
  let text = '', caret = false;
  const recording = t >= DIC.micOn && t < DIC.micOff, transcribing = t >= DIC.micOff && t < DIC.text[0];
  if (recording) return { text: '', caret: false, hint: 'listening… enter, cmd+d or ⏹ stops and transcribes', working, recording };
  if (transcribing) return { text: '', caret: false, hint: 'transcribing…', working, transcribing };
  if (t >= DIC.text[0] && t < DIC.send) {
    if (t < DIC.refined) {
      const words = MESSY.split(' ');
      text = words.slice(0, Math.ceil(words.length * P(t, ...DIC.text))).join(' ');
    } else text = REFINED;
    return { text, caret: true, hint: '', working };
  }
  if (t >= T.newClick && t < T.send1) { text = typed(FIRST_PROMPT, t, ...T.type1); caret = true; }
  else if (t >= T.type2[0] - 0.3 && t < T.send2) { text = typed(FOLLOW_UP, t, ...T.type2); caret = true; }
  else if (t >= T.useClick + 0.1 && t < T.send3) { text = TEMPLATE_TEXT; caret = true; }
  else if (t >= T.thType[0] - 0.2 && t < T.thSend) { text = typed(THEME_PROMPT, t, ...T.thType); caret = true; }
  else for (const c of CHIPS) if (t >= c.type[0] - 0.1 && t < c.send) { text = typed(c.text, t, ...c.type); caret = true; }
  const hint = working ? 'claude is working… enter queues a prompt · ctrl+c stops'
    : 'type a prompt · enter to send · cmd+enter refines · up/down for history';
  return { text, caret, hint, working };
}

function meter(t) {
  let h = '<span style="display:inline-flex;gap:2px;align-items:center;height:18px;margin-right:4px">';
  for (let i = 0; i < 14; i++) {
    const v = 0.2 + 0.8 * Math.abs(Math.sin(t * (6 + i * 0.7) + i * 1.3)) * (0.5 + 0.5 * rnd(i + Math.floor(t * 12)));
    h += `<span style="width:3px;height:${3 + v * 15}px;background:#ff5050;border-radius:1px"></span>`;
  }
  return h + '</span>';
}

function refinePanel(t) {
  if (t < DIC.refine || t >= DIC.send) return '';
  const box = inner => `<div style="margin:0 14px 6px;border:1px solid var(--border);border-radius:4.8px;padding:8px 12px;background:var(--faint)">${inner}</div>`;
  if (t < DIC.questions) return box(`<div class="working" style="font-family:var(--ui)">${spinner(t, 14)} ✨ Refining…</div>`);
  if (t < DIC.go + 0.05 || t < DIC.go) {
    const u = E.out(P(t, DIC.questions, DIC.questions + 0.35));
    let h = `<div style="opacity:${u};transform:translateY(${(1 - u) * 14}px)"><div style="color:var(--strong);-webkit-text-stroke:.35px var(--strong);margin-bottom:6px">A few questions first. Pick an answer, write your own, or skip.</div>`;
    QUESTIONS.forEach(([q, opts, pick], i) => {
      const chosen = t >= DIC.answers[i] ? pick : -1;
      h += `<div style="margin:6px 0 2px">${esc(q)}</div><div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap">`;
      opts.forEach((o, j) => {
        const on = chosen === j;
        const pop = on ? E.back(P(t, DIC.answers[i], DIC.answers[i] + 0.25)) : 1;
        h += `<span class="btn" data-k="ans-${i}-${j}" style="${on ? 'background:var(--sel);color:var(--selText);' : ''}transform:scale(${pop})">${esc(o)}</span>`;
      });
      h += `<span style="font-size:12px;color:var(--weak);border:1px solid var(--border);border-radius:2.4px;padding:2px 8px">Other…</span><span style="font-size:12px;color:var(--weak)">Skip</span></div>`;
    });
    h += `<div style="display:flex;gap:8px;justify-content:flex-end;margin-top:8px"><span class="btn" style="font-size:13px">Skip all</span>
      <span class="btn" data-k="refine-go" style="font-size:13px;${t >= DIC.go - 0.1 ? 'background:var(--sel);color:var(--selText)' : ''}">✨ Refine</span></div></div>`;
    return box(h);
  }
  if (t < DIC.refined) return box(`<div class="working" style="font-family:var(--ui)">${spinner(t, 14)} ✨ Refining with your answers…</div>`);
  const u = E.out(P(t, DIC.refined, DIC.refined + 0.4));
  return `<div style="margin:0 14px 4px;display:flex;gap:10px;align-items:center;font-size:12px;opacity:${u}"><span style="color:#5ae1ff">✨ Refined.</span>
    <span style="color:var(--weak)">Review it, then Enter to send.</span><span class="btn" style="font-size:12px">Undo refine</span></div>`;
}

const MD_SOURCE = `# Exporting orders

The Orders page downloads exactly what you see as CSV:
same filters, same order.

## How it works

- Filters become the query's \`WHERE\`
- Rows are streamed, never held in memory
- The first line records the filters used

| Column   | Example        |
|----------|----------------|
| id       | 10482          |
| status   | shipped        |
| total    | 129.90         |

    GET /orders/export?status=shipped`;

function mdPage(t) {
  const src = t >= T.srcClick && t < T.rendClick;
  const u = E.out(P(t, T.mdOpen, T.mdOpen + 0.35));
  const body = src
    ? `<pre style="font:15px/1.5 var(--mono);color:var(--text);padding:6px 12px">${MD_SOURCE.split('\n').map((l, i) => `<span style="color:var(--weak);display:inline-block;width:34px">${i + 1}</span>${esc(l)}`).join('\n')}</pre>`
    : `<div class="md" style="padding:4px 12px">
      <div style="font-size:30px;color:var(--strong);margin:4px 0 10px">Exporting orders</div>
      <p>The Orders page downloads exactly what you see as CSV: same filters, same order.</p>
      <div style="font-size:22px;color:var(--strong);margin:14px 0 8px">How it works</div>
      <ul><li>Filters become the query's <code>WHERE</code></li><li>Rows are streamed, never held in memory</li><li>The first line records the filters used</li></ul>
      <table class="mdtable"><tr><th>Column</th><th>Example</th></tr><tr><td>id</td><td>10482</td></tr><tr><td>status</td><td>shipped</td></tr><tr><td>total</td><td>129.90</td></tr></table>
      <pre style="font:13.5px var(--mono);background:var(--pre);padding:8px 10px;border-radius:4px">GET /orders/export?status=shipped</pre></div>`;
  return `<div class="tpage" style="opacity:${u};transform:translateY(${(1 - u) * 12}px)">
    <div class="small" style="font-size:12px">docs/orders-export.md · read-only</div>
    <div class="bar"><span class="sw"><span class="${src ? '' : 'on'}" data-k="md-rendered">Rendered</span><span class="${src ? 'on' : ''}" data-k="md-source">Source</span></span></div>
    <div class="hr"></div>${body}</div>`;
}

function centerPanel(t) {
  const build = P(t, 13.65, 14.6);
  let tabs = '', content;
  const tmplOpen = t >= T.tmplClick && t < T.useClick + 0.05;
  const mdOpen = t >= T.mdOpen && t < T.tmplClick;
  if (t >= T.mdOpen && t < T.all) {
    const main = !tmplOpen && !mdOpen;
    tabs = `<div class="ftabs"><span class="ftab ${main ? 'on' : ''}">Main</span><span class="ftab ${mdOpen ? 'on' : ''}">orders-export.md <span class="x">✕</span></span>${t >= T.tmplClick ? `<span class="ftab ${tmplOpen ? 'on' : ''}">📝 Write tests <span class="x">✕</span></span>` : ''}</div>`;
  }
  const sel = selectedAt(t);
  if (tmplOpen) content = templatePage(t);
  else if (mdOpen) content = mdPage(t);
  else if (sel === 'login') content = `<div class="conv"><div class="inner">${convLogin(t)}</div></div>`;
  else if (sel === 'orch') content = orchPage(tabAt(t), t);
  else if (sel === null) content = `<div class="conv">${newSession(t)}</div>`;
  else content = `<div class="conv"><div class="inner">${convOrders(t)}</div></div>`;
  if (!tmplOpen && sel !== 'orch' && !content.startsWith('<div class="conv">')) content = `<div class="conv">${content}</div>`;
  let input = '';
  if (sel !== 'orch' && !tmplOpen && !mdOpen) {
    const st = inputState(t);
    const txt = st.text ? `<span class="txt">${esc(st.text)}${st.caret ? '<span class="caret"></span>' : ''}</span>`
      : `<span class="txt"><span class="hint">${esc(st.hint)}</span></span>`;
    input = `<div class="inputwrap"><div class="handle"></div><div class="input"><span class="gt">&gt;</span>${txt}
      <span class="btns">${st.working ? `${spinner(t, 16)}<span>⏹</span>` : ''}${st.recording ? meter(t) + '<span data-k="mic" style="color:#ff5050">⏹</span>' : '<span data-k="mic">🎤</span>'}<span data-k="refine" ${t >= DIC.refine && t < DIC.refined ? 'style="color:#5ae1ff"' : ''}>✨</span></span></div></div>`;
    if (st.recording) input = input.replace('<div class="input">', `<div class="input" style="border-color:rgba(255,80,80,${0.55 + 0.45 * Math.sin(t * 7)});box-shadow:0 0 ${10 + 8 * Math.sin(t * 7)}px rgba(255,80,80,.35)">`);
    input = refinePanel(t) + input;
  }
  return `<div class="cp" style="opacity:${build}">${tabs}${sel === 'orch' && !tmplOpen ? `<div class="conv" style="padding:0">${content}</div>` : content}${input}</div>`;
}

// ------------------------------------------------------------------ right panel
const D = (cls, pf, code) => `<div class="dl ${cls}"><span class="pf ${cls}">${pf}</span>${code}</div>`;
const FILES_LOGIN = [
  { n: 'redirect.rs', dir: 'src/auth', a: 9, d: 3, lines: [
    D('c', '  ', `<span class="kw">pub fn</span> <span class="fn">login_handler</span>(req: &amp;<span class="ty">Request</span>) -&gt; <span class="ty">Response</span> {`),
    D('d', '- ', `    <span class="fn">redirect</span>(<span class="str">"/home"</span>)`),
    D('a', '+ ', `    <span class="kw">let</span> target = <span class="fn">safe_next</span>(req).<span class="fn">unwrap_or</span>(<span class="str">"/home"</span>);`),
    D('a', '+ ', `    <span class="fn">redirect</span>(target)`),
    D('c', '  ', `}`)] },
  { n: 'session.rs', dir: 'src/auth', a: 2, d: 1, lines: [
    D('d', '- ', `    cookie.<span class="fn">refresh</span>()`),
    D('a', '+ ', `    cookie.<span class="fn">refresh_keeping</span>(<span class="str">"next"</span>)`)] },
];
const FILES_ORDERS = [
  { n: 'export.rs', dir: 'src/orders', a: 48, d: 0, lines: [
    D('a', '+ ', `<span class="kw">pub fn</span> <span class="fn">export_csv</span>(q: &amp;<span class="ty">OrderQuery</span>, out: <span class="kw">impl</span> <span class="ty">Write</span>) {`),
    D('a', '+ ', `    <span class="kw">let mut</span> w = csv::<span class="ty">Writer</span>::<span class="fn">from_writer</span>(out);`),
    D('a', '+ ', `    w.<span class="fn">write_record</span>(<span class="ty">HEADER</span>)?;`),
    D('a', '+ ', `    <span class="kw">for</span> row <span class="kw">in</span> db::<span class="fn">orders</span>(q).<span class="fn">stream</span>() {`),
    D('a', '+ ', `        w.<span class="fn">serialize</span>(row?)?; <span class="cm">// one row at a time</span>`),
    D('c', '  ', `  ⋮`)] },
  { n: 'routes.rs', dir: 'src/orders', a: 6, d: 1, lines: [
    D('c', '  ', `    .<span class="fn">route</span>(<span class="str">"/orders"</span>, <span class="fn">get</span>(list))`),
    D('d', '- ', `    .<span class="fn">route</span>(<span class="str">"/orders/:id"</span>, <span class="fn">get</span>(show))`),
    D('a', '+ ', `    .<span class="fn">route</span>(<span class="str">"/orders/:id"</span>, <span class="fn">get</span>(show))`),
    D('a', '+ ', `    .<span class="fn">route</span>(<span class="str">"/orders/export"</span>, <span class="fn">get</span>(export))`)] },
  { n: 'orders-export.md', dir: 'docs', a: 22, d: 0, lines: [
    D('a', '+ ', `<span class="kw"># Exporting orders</span>`),
    D('a', '+ ', ``),
    D('a', '+ ', `The Orders page downloads exactly what you see as CSV.`),
    D('a', '+ ', ``),
    D('a', '+ ', `<span class="kw">## How it works</span>`),
    D('a', '+ ', `- Filters become the query's <span class="str">\`WHERE\`</span>`),
    D('a', '+ ', `- Rows are streamed, never held in memory`)] },
  { n: 'OrdersPage.tsx', dir: 'web/src/pages', a: 14, d: 2, lines: [
    D('d', '- ', `  &lt;<span class="ty">Toolbar</span> filters={filters} /&gt;`),
    D('a', '+ ', `  &lt;<span class="ty">Toolbar</span> filters={filters}&gt;`),
    D('a', '+ ', `    &lt;<span class="ty">Button</span> onClick={() =&gt; <span class="fn">exportCsv</span>(filters)}&gt;`),
    D('a', '+ ', `      Export CSV`),
    D('a', '+ ', `    &lt;/<span class="ty">Button</span>&gt;`),
    D('a', '+ ', `  &lt;/<span class="ty">Toolbar</span>&gt;`)] },
];

function changesTab(t) {
  const sel = selectedAt(t);
  const files = sel === 'login' || sel === 'orch' ? FILES_LOGIN : sel === null ? [] : FILES_ORDERS;
  if (!files.length) return `<div class="rscroll"><div class="small" style="font-size:13px;padding:6px">No code changes in this session</div></div>`;
  const filter = t >= T.mdClick && t < T.info2 ? 'orders-export.md' : null;
  const enter = t >= T.changes && t < T.changes + 1 ? T.changes : null;
  let cards = '';
  files.forEach((f, i) => {
    if (filter && f.n !== filter) return;
    const u = enter ? E.out(P(t, enter + 0.1 + i * 0.12, enter + 0.5 + i * 0.12)) : 1;
    cards += `<div class="fcard" style="opacity:${u};transform:translateX(${(1 - u) * 40}px)"><div class="hd"><span>${f.n}</span>
      <span style="color:var(--green)">+${f.a}</span><span style="color:var(--red)">-${f.d}</span></div><div class="dir">${f.dir}</div>${f.lines.join('')}</div>`;
  });
  let rows = '';
  files.forEach(f => {
    rows += `<div class="fr ${filter === f.n ? 'on' : ''}" data-k="file-${f.n}"><span class="n">${f.n}</span><span class="c">+${f.a} -${f.d}</span><div class="p">${f.dir}</div></div>`;
  });
  return `<div class="rscroll">${cards}</div>
    <div class="files"><div class="fh"><b>Files (${files.length})</b><span class="sw">${filter ? '<span class="btn" style="font-size:12px">Show all</span>' : ''}<span class="on">All</span><span>Uncommitted</span></span></div>${rows}</div>`;
}

const GOAL = 'Orders page exports the filtered view as a streamed CSV; tests and docs still open.';
const SUBGOALS = ['Streaming CSV writer', 'Export endpoint with the page filters', 'Export CSV button on the Orders page', 'Tests for very large exports', 'Document the export in the API guide'];

function askCards(t) {
  let h = '';
  const card = (q, sendAt, toolAt, toolTxt, ans, ansAt) => {
    const u = E.out(P(t, sendAt, sendAt + 0.3));
    let body;
    if (t < toolAt) body = `<div class="working" style="font-family:var(--ui);font-size:14px">${spinner(t, 14)} Thinking (not added to the session)…</div>`;
    else {
      body = `<div class="tool" style="margin-top:6px">⚙ ${toolTxt}</div>`;
      if (t < ansAt[0]) body += `<div class="working" style="font-family:var(--ui);font-size:14px">${spinner(t, 14)} Thinking (not added to the session)…</div>`;
      else body += `<div class="md">${stream(ans, t, ansAt[0], visLen(ans) / (ansAt[1] - ansAt[0]))}</div>`;
    }
    return `<div class="askq" style="opacity:${u};transform:translateY(${(1 - u) * 16}px)"><div class="q">${esc(q)}</div><span class="x">×</span>${body}</div>`;
  };
  if (t >= T.sendQ1) h += card(Q1, T.sendQ1, T.toolQ1, 'Grep · Read', `<p>Orders can reach 2M rows. <code>export_csv</code> streams them through <code>csv::Writer</code>, so memory stays flat (about 4 MB) instead of holding every row: see <code>src/orders/export.rs:48</code>.</p>`, T.ansQ1);
  if (t >= T.sendQ2) h += card(Q2, T.sendQ2, T.toolQ2, 'Read', `<p>Yes. The filters become the query's <code>WHERE</code>, so only matching rows are streamed and nothing more is held in memory.</p>`, T.ansQ2);
  return h;
}

function infoTab(t) {
  const sel = selectedAt(t);
  const ticked = t >= T.tick ? 4 : 3;
  const enter = t >= T.info2 && t < T.info2 + 1.2 ? T.info2 : t >= T.info1 && t < T.info1 + 1.2 ? T.info1 : null;
  const fillU = enter ? E.out(P(t, enter + 0.1, enter + 1.0)) : 1;
  let goal = '';
  if (sel === 'orders') {
    goal = `<div class="sec">Goal</div><div>${esc(GOAL)}</div>
      <div class="small" style="margin:4px 0">${ticked} of 5 done</div>
      ${SUBGOALS.map((g, i) => {
        const on = i < ticked;
        const pop = i === 3 && t >= T.tick ? E.back(P(t, T.tick, T.tick + 0.3)) : 1;
        return `<div class="chk" data-k="chk-${i}"><span class="box ${on ? 'on' : ''}" style="transform:scale(${pop})">${on ? check : ''}</span><span>${esc(g)}</span></div>`;
      }).join('')}
      <span class="btn" style="margin-top:6px">Summarize again</span>`;
  }
  const bar = (txt, pct) => `<div class="pbar"><div class="f" style="width:${pct * fillU}%"></div><div class="tx">${txt}</div></div>`;
  const askFocus = t >= T.askClick && t < T.changes;
  let field;
  if (t >= T.typeQ1[0] && t < T.sendQ1) field = `${esc(typed(Q1, t, ...T.typeQ1))}<span class="caret" style="height:16px;width:2px"></span>`;
  else if (t >= T.typeQ2[0] && t < T.sendQ2) field = `${esc(typed(Q2, t, ...T.typeQ2))}<span class="caret" style="height:16px;width:2px"></span>`;
  else field = `<span class="hint">Ask about this goal, Enter to send</span>`;
  return `<div class="rscroll">${goal}
    <div class="sec">Model</div><span class="combo">claude-opus-5-5</span> <span class="small">(Claude Code's default)</span>
    <div class="small" style="font-family:var(--mono);margin-top:3px">Latest reply: claude-opus-5-5</div>
    <div class="sec">Context</div>${bar('204k / 1.0M tokens (20%)', 20)}<div class="small">As of the latest reply</div>
    <div class="sec">Session limit</div>${bar('38% used · 62% left · resets in 2h 14m', 38)}
    <div class="sec">Usage limits</div><div class="small" style="font-size:12px">Weekly · all models</div>${bar('45% used · 55% left · resets in 3d 4h', 45)}
    <div class="small">Updated 12s ago</div></div>
    <div style="flex:none">${askCards(t)}<div class="askfield" data-k="askfield" ${askFocus ? 'style="border-color:var(--selText)"' : ''}>${field}</div></div>`;
}

function templatesTab(t) {
  const open = E.out(P(t, T.tmplOpen, T.tmplOpen + 0.35));
  const items = [['write-tests', 'Write tests', 'Cover the changed code with focused tests'], ['code-review', 'Code review', 'Review the diff for bugs and risky changes'], ['release-notes', 'Release notes', 'Summarize merged work for users']];
  const selT = t >= T.tmplClick ? 'write-tests' : null;
  return `<div class="rscroll"><div class="small">~/code/shop</div>
    <div class="createbtn">✨&nbsp; Create template</div>
    <div class="coll"><div class="ch"><span class="tri">${open > 0.5 ? '⏷' : '⏵'}</span>Prompt templates · 3</div>
      <div style="overflow:hidden;max-height:${open * 200}px">${items.map(([k, n, d]) => `<div class="tmpl ${selT === k ? 'on' : ''}" data-k="tmpl-${k}"><div class="n">${n}</div><div class="d">${d}</div></div>`).join('')}</div></div>
    <div class="coll"><div class="ch"><span class="tri">⏵</span>Skills · 2</div></div>
    <div class="coll"><div class="ch"><span class="tri">⏵</span>Commands · 4</div></div>
    <div class="coll"><div class="ch"><span class="tri">⏵</span>Your skills (~/.claude) · 6</div></div>
    <div class="coll"><div class="ch"><span class="tri">⏵</span>Your commands (~/.claude) · 3</div></div></div>`;
}

function rightTabAt(t) {
  if (t >= T.info1 && t < T.changes) return 'Info';
  if (t >= T.changes && t < T.info2) return 'Changes';
  if (t >= T.info2 && t < T.tmpl) return 'Info';
  if (t >= T.tmpl && t < 84.5) return 'Templates';
  return 'Changes';
}

function rightPanel(t) {
  const build = P(t, 13.75, 14.7);
  const tab = rightTabAt(t);
  const body = tab === 'Info' ? infoTab(t) : tab === 'Templates' ? templatesTab(t) : changesTab(t);
  return `<div class="rp" style="opacity:${build};transform:translateX(${(1 - E.out(build)) * 40}px)">
    <div class="rtabs">${['Changes', 'Info', 'Templates'].map(n => `<span class="rtab ${tab === n ? 'on' : ''}" data-k="rtab-${n}">${n}</span>`).join('')}</div>
    ${body}</div>`;
}

const THEMES = {
  classic: { bg: '#1b1b1b', text: '#b4b4b4', strong: '#ffffff', weak: '#7c7c7c', faint: '#202020', extreme: '#0a0a0a', hover: '#464646', border: '#3c3c3c',
    sel: '#005c80', selText: '#c0deff', selWeak: '#8fb6d6', user: '#78aaff', asst: '#e6965a', green: '#6ed278', red: '#f06e6e', gold: '#c8aa5a', lightred: '#ff8080',
    tabText: '#e8e8e8', ftab: '#1e1e1e', userTint: 'rgba(120,170,255,.08)', code: '#d0d0d0', codeBg: '#2a2a2a', rule: '#2c2c2c', promptBg: '#0e0e10', promptText: '#e6e6e6',
    hint: '#5d5d5d', addBg: '#1c3020', delBg: '#381c1e', track: '#2e2e2e', barText: '#e2e2e2', pre: '#141414',
    kw: '#c594c5', fn: '#6699cc', str: '#99c794', cm: '#65737e', ty: '#fac863', nu: '#f99157' },
  onedark: { bg: '#282c34', text: '#abb2bf', strong: '#e6e9ef', weak: '#5c6370', faint: '#2c313a', extreme: '#1e2227', hover: '#3e4451', border: '#3e4451',
    sel: '#3d5a8a', selText: '#e6efff', selWeak: '#a9bbd8', user: '#61afef', asst: '#d19a66', green: '#98c379', red: '#e06c75', gold: '#e5c07b', lightred: '#e06c75',
    tabText: '#e6e9ef', ftab: '#21252b', userTint: 'rgba(97,175,239,.10)', code: '#c8ccd4', codeBg: '#3a3f4b', rule: '#3a3f4b', promptBg: '#21252b', promptText: '#dcdfe4',
    hint: '#5c6370', addBg: '#2b3a2b', delBg: '#42292d', track: '#3a3f4b', barText: '#e6e9ef', pre: '#21252b',
    kw: '#c678dd', fn: '#61afef', str: '#98c379', cm: '#5c6370', ty: '#e5c07b', nu: '#d19a66' },
  dracula: { bg: '#282a36', text: '#e6e6e0', strong: '#ffffff', weak: '#6272a4', faint: '#2f3241', extreme: '#1e1f29', hover: '#44475a', border: '#44475a',
    sel: '#6c4fa8', selText: '#ffffff', selWeak: '#d6c8f5', user: '#8be9fd', asst: '#ffb86c', green: '#50fa7b', red: '#ff5555', gold: '#f1fa8c', lightred: '#ff5555',
    tabText: '#ffffff', ftab: '#21222c', userTint: 'rgba(139,233,253,.08)', code: '#f8f8f2', codeBg: '#44475a', rule: '#44475a', promptBg: '#21222c', promptText: '#f8f8f2',
    hint: '#6272a4', addBg: '#24402f', delBg: '#4a2630', track: '#44475a', barText: '#ffffff', pre: '#21222c',
    kw: '#ff79c6', fn: '#50fa7b', str: '#f1fa8c', cm: '#6272a4', ty: '#8be9fd', nu: '#bd93f9' },
  matrix: { bg: '#000000', text: '#22e04a', strong: '#8dff9e', weak: '#0f8a2a', faint: '#020a03', extreme: '#000000', hover: '#06300f', border: '#0d5a1c',
    sel: '#0b3d16', selText: '#b6ffc3', selWeak: '#3fd463', user: '#7dff8f', asst: '#00ff41', green: '#00ff41', red: '#00ff41', gold: '#9dff6a', lightred: '#9dff6a',
    tabText: '#8dff9e', ftab: '#000000', userTint: 'rgba(0,255,65,.06)', code: '#8dff9e', codeBg: '#062b0e', rule: '#0d5a1c', promptBg: '#000000', promptText: '#8dff9e',
    hint: '#0f8a2a', addBg: '#06300f', delBg: '#0a1f0d', track: '#06300f', barText: '#b6ffc3', pre: '#020a03',
    kw: '#00ff41', fn: '#8dff9e', str: '#3fd463', cm: '#0f8a2a', ty: '#b6ffc3', nu: '#00ff41',
    ui: "'Hack', 'NotoEmoji', monospace", glow: '0 0 6px rgba(0,255,65,.65)' },
  paper: { bg: '#ffffff', text: '#161616', strong: '#000000', weak: '#7a7a7a', faint: '#f7f7f7', extreme: '#ffffff', hover: '#ececec', border: '#d4d4d4',
    sel: '#0047ff', selText: '#ffffff', selWeak: '#d6e0ff', user: '#0047ff', asst: '#161616', green: '#0047ff', red: '#161616', gold: '#0047ff', lightred: '#161616',
    tabText: '#161616', ftab: '#ffffff', userTint: 'rgba(0,71,255,.06)', code: '#161616', codeBg: '#eef2ff', rule: '#e4e4e4', promptBg: '#ffffff', promptText: '#161616',
    hint: '#a0a0a0', addBg: '#e9efff', delBg: '#f2f2f2', track: '#eeeeee', barText: '#161616', pre: '#f5f7ff',
    kw: '#0047ff', fn: '#161616', str: '#0047ff', cm: '#9a9a9a', ty: '#161616', nu: '#0047ff' },
  github: { bg: '#0d1117', text: '#c9d1d9', strong: '#f0f6fc', weak: '#8b949e', faint: '#161b22', extreme: '#010409', hover: '#30363d', border: '#30363d',
    sel: '#1f6feb', selText: '#ffffff', selWeak: '#cfe0ff', user: '#58a6ff', asst: '#f0883e', green: '#3fb950', red: '#f85149', gold: '#d29922', lightred: '#f85149',
    tabText: '#f0f6fc', ftab: '#161b22', userTint: 'rgba(56,139,253,.10)', code: '#e6edf3', codeBg: '#262c36', rule: '#21262d', promptBg: '#010409', promptText: '#e6edf3',
    hint: '#6e7681', addBg: '#12301d', delBg: '#3c1618', track: '#21262d', barText: '#ffffff', pre: '#161b22',
    kw: '#ff7b72', fn: '#d2a8ff', str: '#a5d6ff', cm: '#8b949e', ty: '#ffa657', nu: '#79c0ff' },
};
const themeVars = name => Object.entries(THEMES[name]).map(([k, v]) => `--${k}:${v}`).join(';');
const WIPE = 0.75, WIPE_AT = [704, 900];
// The theme at t, and the wipe in progress (if any): { name, from, u }
function themeAt(t) {
  let name = 'classic', from = null, u = 1;
  for (const w of WIPES) {
    if (t < w.t) break;
    from = name; name = w.to; u = P(t, w.t, w.t + WIPE);
  }
  return { name, from: u < 1 ? from : null, u };
}
function themeWorking(t) {
  return (t >= T.thSend && t < T.thReply) || CHIPS.some(c => t >= c.send && t < c.send + 0.35);
}

function rain(t) {
  const glyphs = 'ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄ0123456789ﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎ';
  let h = '<div style="position:absolute;inset:0;pointer-events:none;opacity:.32;mix-blend-mode:screen;overflow:hidden;z-index:20">';
  for (let c = 0; c < 40; c++) {
    const speed = 260 + rnd(c * 3.1) * 420, len = 8 + Math.floor(rnd(c * 7.7) * 14);
    const head = ((t * speed + rnd(c) * 1400) % 1400) - 200;
    for (let k = 0; k < len; k++) {
      const y = head - k * 22;
      if (y < -22 || y > 960) continue;
      const g = glyphs[Math.floor(rnd(c * 31 + k + Math.floor(t * 8)) * glyphs.length)];
      h += `<span style="position:absolute;left:${c * 40 + 8}px;top:${y}px;font:18px var(--mono);color:${k === 0 ? '#eaffea' : '#00ff41'};opacity:${1 - k / len}">${g}</span>`;
    }
  }
  return h + '</div>';
}

function ideHtml(t, theme = 'classic') {
  return `<div class="win" style="${themeVars(theme)}"><div class="top"><div class="lights"><i></i><i></i><i></i></div><span class="gear">⚙</span></div>
    <div class="body">${leftPanel(t)}${centerPanel(t)}${rightPanel(t)}</div></div>`;
}

// ------------------------------------------------------------------ measuring (window coordinates)
function rectOf(sel) {
  const win = document.querySelector('.win');
  const el = document.querySelector(sel);
  if (!win || !el) return null;
  const w = win.getBoundingClientRect(), r = el.getBoundingClientRect();
  const s = w.width / 1600;
  return { x: (r.left - w.left) / s, y: (r.top - w.top) / s, w: r.width / s, h: r.height / s };
}

function afterLayout(t) {
  // keep conversations scrolled to their end, like a chat
  document.querySelectorAll('.conv').forEach(c => {
    const inner = c.querySelector('.inner');
    if (!inner) return;
    const over = inner.scrollHeight - c.clientHeight + 16;
    if (over > 0) inner.style.transform = `translateY(${-over}px)`;
  });
  const win = document.querySelector('.win');
  let extra = '';
  // sessions flying onto their scope tab
  SESS.forEach((s, i) => {
    const a = FLY0 + i * FLYSTEP;
    if (t < a || t > a + FLYDUR) return;
    const from = rectOf(`[data-k="row-${s.id}"]`), to = rectOf(`[data-k="tab-${s.scope}"]`);
    if (!from || !to) return;
    const u = E.io(P(t, a, a + FLYDUR));
    const x = lerp(from.x + 12, to.x + to.w / 2 - 60, u), y = lerp(from.y + 8, to.y + to.h / 2 - 14, u) - Math.sin(u * Math.PI) * 60;
    const c = SCOPES[s.scope].c;
    extra += `<div style="position:absolute;left:${x}px;top:${y}px;z-index:30;padding:5px 12px;border-radius:12px;white-space:nowrap;
      background:${rgb(c, 0.92)};color:#fff;font:15px var(--ui);transform:scale(${lerp(1, 0.45, u)});opacity:${1 - P(u, 0.85, 1)};
      box-shadow:0 8px 24px rgba(0,0,0,.5);transform-origin:60px 14px">${esc(s.name)}</div>`;
  });
  // highlights around the status marks
  MARKS.forEach(m => {
    if (t < m.t || t > m.t + 1.25) return;
    const r = rectOf(`[data-k="${m.k}"]`);
    if (!r) return;
    const u = E.back(P(t, m.t, m.t + 0.3)), o = 1 - P(t, m.t + 0.95, m.t + 1.25);
    const pad = 5;
    extra += `<div class="callout" style="left:${r.x - pad}px;top:${r.y - pad}px;width:${r.w + 2 * pad}px;height:${r.h + 2 * pad}px;opacity:${o};transform:scale(${lerp(1.6, 1, clamp(u))})"></div>`;
    extra += `<div style="position:absolute;z-index:41;left:${r.x + r.w / 2}px;top:${r.y - 30}px;transform:translateX(-50%) scale(${clamp(u)});opacity:${o};
      font:600 13px var(--brand);color:#06080b;background:#5ae1ff;border-radius:6px;padding:2px 8px;white-space:nowrap">${m.label}</div>`;
  });
  // the cursor
  if (cursorPos) {
    const c = cursorAt(t);
    if (c) {
      extra += `<svg class="cursor" style="transform:translate(${c.x - 3}px,${c.y - 2}px) scale(${c.press})" viewBox="0 0 26 34" width="26" height="34">
        <path d="M3 2 L3 26 L9 20.5 L13.2 30 L17.2 28.2 L13.2 19 L21 19 Z" fill="#000" stroke="#fff" stroke-width="1.8" stroke-linejoin="round"/></svg>`;
      if (c.ripple) extra += `<div class="ripple" style="left:${c.x - c.ripple.r}px;top:${c.y - c.ripple.r}px;width:${2 * c.ripple.r}px;height:${2 * c.ripple.r}px;opacity:${c.ripple.o}"></div>`;
    }
  }
  if (extra) win.insertAdjacentHTML('beforeend', extra);
}

const MARKS = [
  { t: 41.9, k: 'meta-orders', label: 'working' },
  { t: 42.65, k: 'meta-login', label: 'just finished' },
  { t: 43.4, k: 'meta-token', label: 'needs you' },
  { t: 44.1, k: 'meta-docs', label: 'blocked' },
  { t: 44.8, k: 'time-rate', label: 'last activity' },
];

function measure() {
  // where each cursor target is at its own time; the same in every render worker
  const stage = $('stage');
  cursorPos = CURSOR.map(k => {
    if (!k.k) return { x: k.x, y: k.y };
    stage.innerHTML = ideHtml(k.t - 0.05);
    stage.style.transform = 'none';
    const r = rectOf(`[data-k="${k.k}"]`);
    if (!r) { console.warn('cursor target missing', k.k, k.t); return { x: 800, y: 480 }; }
    // tabs and buttons: aim near the center; rows: a bit left of center
    return { x: r.x + Math.min(r.w / 2, 60), y: r.y + r.h / 2 };
  });
  stage.innerHTML = '';
}

function cursorAt(t) {
  if (t < CURSOR[0].t || t > CURSOR[CURSOR.length - 1].t + 0.4) return null;
  let i = 0;
  while (i + 1 < CURSOR.length && CURSOR[i + 1].t - 0.7 <= t) i++;
  const k = CURSOR[i], p = cursorPos[i];
  let x = p.x, y = p.y;
  const next = CURSOR[i + 1];
  if (next && t > next.t - 0.7) { /* unreachable: covered by the loop */ }
  if (i > 0) {
    const prev = cursorPos[i - 1];
    const d = Math.min(0.7, k.t - CURSOR[i - 1].t - 0.05);
    const u = E.io(P(t, k.t - d, k.t));
    x = lerp(prev.x, p.x, u) + Math.sin(u * Math.PI) * 18;
    y = lerp(prev.y, p.y, u) - Math.sin(u * Math.PI) * 26;
  }
  let press = 1, ripple = null;
  if (k.click && t >= k.t - 0.08 && t < k.t + 0.45) {
    press = t < k.t + 0.08 ? 0.82 : 1;
    const ru = P(t, k.t, k.t + 0.45);
    if (t >= k.t) ripple = { r: 6 + 22 * E.out(ru), o: 1 - ru };
  }
  const fadeIn = P(t, CURSOR[0].t, CURSOR[0].t + 0.3), fadeOut = 1 - P(t, CURSOR[CURSOR.length - 1].t, CURSOR[CURSOR.length - 1].t + 0.4);
  let vis = 1;
  if (k.hide && t >= k.t) vis = 1 - P(t, k.t, k.t + 0.3);
  else if (i > 0 && CURSOR[i - 1].hide) vis = P(t, k.t - 0.7, k.t - 0.45);
  if (fadeIn * fadeOut * vis <= 0.01) return null;
  return { x, y, press: press * fadeIn * fadeOut * vis, ripple };
}

// ------------------------------------------------------------------ notification
function renderOver(t) {
  let h = '';
  const a = T.loginDone + 0.1, b = a + 2.9;
  if (t >= a && t < b) {
    const u = E.out5(P(t, a, a + 0.5)), o = 1 - E.in(P(t, b - 0.4, b));
    h += `<div class="notif" style="right:40px;top:36px;transform:translateX(${(1 - u) * 520}px);opacity:${o}">
      <div class="ic">${iconSvg(46, 1, 1, 1, 'n')}</div>
      <div><div class="t1">Done · Fix login redirect</div><div class="t2">Scope: API</div><div class="t3">Claude finished its turn</div></div>
      <div class="now">now</div></div>`;
  }
  // the prompt to Claude, large, while the window is wide
  for (const c of CHIPS) {
    const a0 = c.type[0] - 0.25, a1 = c.send + 0.45;
    if (t < a0 || t > a1) continue;
    const u = E.out5(P(t, a0, a0 + 0.3)), o = 1 - E.in(P(t, a1 - 0.25, a1));
    const sent = t >= c.send;
    h += `<div style="position:absolute;left:50%;top:930px;transform:translateX(-50%) translateY(${(1 - u) * 30 - (sent ? 14 * P(t, c.send, a1) : 0)}px) scale(${sent ? 1 - 0.04 * P(t, c.send, c.send + 0.15) : 1});opacity:${u * o};
      font:30px var(--mono);color:#e6e6e6;background:rgba(14,14,16,.94);border:1px solid ${sent ? '#5ae1ff' : '#3c3c3c'};border-radius:16px;padding:18px 30px;white-space:nowrap;
      box-shadow:0 20px 60px rgba(0,0,0,.6)${sent ? ',0 0 30px rgba(90,225,255,.35)' : ''}"><span style="color:#6ed278">&gt;</span> ${esc(typed(c.text, t, ...c.type))}${sent ? '' : '<span class="caret" style="height:30px;width:14px;vertical-align:-5px"></span>'}</div>`;
  }
  $('over').innerHTML = h;
}

// ================================================================== frame
function renderBg(t) {
  const g1 = $('g1'), g2 = $('g2'), g3 = $('g3');
  const hot = P(t, 5, 9.6) * (1 - P(t, 9.8, 10.2));
  g1.style.cssText = `left:${-200 + 80 * Math.sin(t * 0.13)}px;top:${-260 + 60 * Math.cos(t * 0.11)}px;width:1500px;height:1300px;margin:-200px;background:radial-gradient(closest-side,${t < 10 ? `rgba(${120 + 100 * hot},60,${90 - 40 * hot},.35)` : 'rgba(40,110,255,.30)'},transparent)`;
  g2.style.cssText = `right:${-260 + 70 * Math.cos(t * 0.09)}px;bottom:${-300 + 60 * Math.sin(t * 0.12)}px;width:1600px;height:1400px;margin:-200px;background:radial-gradient(closest-side,rgba(230,150,90,${t < 10 ? 0.10 : 0.16}),transparent)`;
  g3.style.cssText = `left:${700 + 100 * Math.sin(t * 0.07)}px;top:${300}px;width:1000px;height:800px;margin:-150px;background:radial-gradient(closest-side,rgba(90,225,255,${0.06 + 0.1 * P(t, 9.9, 10.3) * (1 - P(t, 11, 14))}),transparent)`;
  $('grid').style.transform = `translate(${(t * 6) % 40}px, ${(t * 3) % 40}px)`;
}

function render(t) {
  const frame = Math.round(t * 60);
  renderBg(t);
  renderChaos(t, frame);
  renderLogo(t);
  // the IDE window
  const stage = $('stage');
  if (t >= 13.4 && t < 109.2) {
    const th = themeAt(t);
    if (th.from) {
      const r = E.io(th.u) * 2100;
      stage.innerHTML = ideHtml(t, th.from) + `<div style="position:absolute;left:0;top:0;width:1600px;height:960px;border-radius:12px;clip-path:circle(${r}px at ${WIPE_AT[0]}px ${WIPE_AT[1]}px)">${ideHtml(t, th.name)}</div>`
        + `<div style="position:absolute;left:0;top:0;width:1600px;height:960px;overflow:hidden;border-radius:12px;pointer-events:none">
          <div style="position:absolute;left:${WIPE_AT[0] - r}px;top:${WIPE_AT[1] - r}px;width:${2 * r}px;height:${2 * r}px;border-radius:50%;
          box-shadow:0 0 0 3px rgba(90,225,255,${0.9 * (1 - th.u)}), 0 0 60px 10px rgba(90,225,255,${0.5 * (1 - th.u)}), inset 0 0 60px 10px rgba(90,225,255,${0.35 * (1 - th.u)})"></div></div>`;
    } else stage.innerHTML = ideHtml(t, th.name);
    const cam = camera(t);
    const intro = E.out5(P(t, 13.4, 14.7));
    const outro = E.in(P(t, 107.9, 108.7));
    const s = cam.s * lerp(0.82, 1, intro) * lerp(1, 0.8, outro);
    const tx = cam.a[0] - cam.f[0] * s, ty = cam.a[1] - cam.f[1] * s + (1 - intro) * 80;
    stage.style.transform = `translate(${tx}px,${ty}px) scale(${s})`;
    stage.style.opacity = intro * (1 - outro);
    stage.style.filter = intro < 1 || outro > 0 ? `blur(${(1 - intro) * 12 + outro * 10}px)` : 'none';
    afterLayout(t);
  } else {
    stage.innerHTML = '';
  }
  renderOver(t);
  const { leftOn, topOn } = renderCaps(t);
  $('scrim').style.opacity = leftOn;
  $('topscrim').style.opacity = topOn;
  // flashes on the two hits
  const f = Math.max(0.85 * (1 - E.out(P(t, 10.0, 10.6))) * (t >= 10 ? 1 : 0), 0.35 * (1 - E.out(P(t, 108.0, 108.6))) * (t >= 108 ? 1 : 0));
  $('flash').style.opacity = f;
  $('vignette').style.opacity = 1;
}

buildCues();
window.PROMO = { render, measure, DUR, MARKERS, CUES };
