#!/bin/sh
# Tiles PNG stills 2x2 (each 960x540) into contact sheets for review: sheet.sh out.png a.png b.png c.png d.png
o="$1"; shift
ffmpeg -y -loglevel error $(for f in "$@"; do printf -- "-i %s " "$f"; done) -filter_complex "$(i=0; for f in "$@"; do printf "[%d]scale=960:540[s%d];" $i $i; i=$((i+1)); done)$(i=0; for f in "$@"; do printf "[s%d]" $i; i=$((i+1)); done)xstack=inputs=$#:layout=0_0|w0_0|0_h0|w0_h0" "$o"
