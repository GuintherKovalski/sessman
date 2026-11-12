# scope promo video

A ~95 s launch video for scope, made entirely from code: no screen recordings, no stock
footage, no licensed music.

- `index.html` + `main.js`: every frame is a pure function of time (`PROMO.render(t)`). The
  IDE is redrawn in HTML from scope's own look (egui's Ubuntu-Light and Hack fonts, the
  classic dark palette, sizes at the 1.2 zoom), with a camera, cursor, captions and the
  chaos/logo/end-card scenes. Timings, captions, camera keys and sound cues all live in `main.js`.
- `render.mjs`: steps through the frames in parallel headless Chrome workers and encodes them
  with ffmpeg; it also writes `out/cues.json` (the sound effects and music markers).
- `audio.py`: synthesizes the soundtrack (120 BPM, Am-F-C-G) and every sound effect with numpy,
  so the audio is original and royalty-free by construction.
- `build.sh`: all of it -> `out/scope-promo.mp4` (1080p60, -14 LUFS). `FPS=30` for a quicker pass.
- `gif.sh`: the short silent loop for the README -> `out/scope.gif`.
- `node render.mjs --stills 12,40.5` renders single frames to `out/stills/` for checking.

Setup once: `npm install` here (puppeteer-core; it drives the installed Google Chrome).
Fonts in `fonts/` are egui's defaults (Ubuntu Font Licence, Hack/MIT-Bitstream, OFL).
