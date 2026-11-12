#!/bin/sh
# Builds the promo end to end: frames (headless Chrome) -> video, cues -> synthesized audio, mux.
#   promo/build.sh            out/scope-promo.mp4 (1080p60, about 20 min)
#   FPS=30 promo/build.sh     a quicker preview
# Needs node (npm install once), ffmpeg, python3 with numpy, Google Chrome.
set -eu
cd "$(dirname "$0")"
[ -d node_modules ] || npm install
FPS="${FPS:-60}"
node render.mjs --fps "$FPS" --out out/video.mp4
"${PYTHON:-/opt/homebrew/bin/python3}" -I audio.py out/cues.json out/audio.wav
ffmpeg -y -loglevel error -i out/video.mp4 -i out/audio.wav -c:v copy -c:a aac -b:a 256k -ar 48000 \
  -af loudnorm=I=-14:TP=-1.5:LRA=11 -movflags +faststart -shortest out/scope-promo.mp4
echo "out/scope-promo.mp4"
