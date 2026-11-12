#!/bin/sh
# The short, silent loop for the README, cut from the full video:
#   promo/gif.sh [out/scope-promo.mp4]  ->  out/scope.gif (640 px wide, 10 fps)
# Cuts: the punchline and logo, scopes being sorted, the theme switches, dictate and refine.
set -eu
cd "$(dirname "$0")"
in="${1:-out/scope-promo.mp4}"
cuts="6.9:12.4 22.6:27.2 90.0:94.2 97.0:103.4"
f=""; n=0
for c in $cuts; do
  a=${c%:*}; b=${c#*:}
  f="$f[0:v]trim=$a:$b,setpts=PTS-STARTPTS,fps=10,scale=640:-1:flags=lanczos[v$n];"
  n=$((n+1))
done
cat=""; i=0; while [ $i -lt $n ]; do cat="$cat[v$i]"; i=$((i+1)); done
ffmpeg -y -loglevel error -i "$in" -filter_complex \
  "${f}${cat}concat=n=$n:v=1:a=0,split[x][y];[x]palettegen=max_colors=128:stats_mode=diff[p];[y][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" \
  out/scope.gif
ls -la out/scope.gif
