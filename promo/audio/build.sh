#!/bin/sh
# Régénère toute la bande-son du film : sons du launcher, musique, sound design, mixage, sonie.
set -e
cd "$(dirname "$0")"
python3 ../../tools/sound/make_sounds.py build/ui >/dev/null
python3 music.py
python3 sfx.py
python3 mix.py
norm() {  # sonie intégrée −14 LUFS, crête vraie −1 dBTP (deux passes)
  m=$(ffmpeg -hide_banner -i "$1" -af loudnorm=I=-14:TP=-1:LRA=11:print_format=json -f null - 2>&1 | sed -n '/{/,/}/p')
  get() { echo "$m" | sed -n "s/.*\"$1\" : \"\([^\"]*\)\".*/\1/p"; }
  ffmpeg -hide_banner -loglevel error -y -i "$1" -af "loudnorm=I=-14:TP=-1:LRA=11:measured_I=$(get input_i):measured_TP=$(get input_tp):measured_LRA=$(get input_lra):measured_thresh=$(get input_thresh):offset=$(get target_offset):linear=true,aresample=48000" -c:a pcm_s16le "$2"
}
mkdir -p out
norm build/mix-raw.wav out/halcyon-promo-audio.wav
norm build/nomusic-raw.wav out/halcyon-promo-audio-sans-musique.wav
cp build/music.wav out/halcyon-promo-musique-seule.wav
echo "bande-son prête dans audio/out"
