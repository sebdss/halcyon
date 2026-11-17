#!/bin/sh
# Régénère le film complet : bande-son, image (Remotion), assemblage des deux versions.
#   sh render.sh            → 16:9, 1920 × 1080, 60 i/s
#   sh render.sh vertical   → déclinaison 9:16 (1080 × 1920), même montage
set -e
cd "$(dirname "$0")"
COMP=Promo; SUFFIX=""
[ "$1" = "vertical" ] && COMP=PromoVertical && SUFFIX="-vertical"
CHROME=${REMOTION_CHROME:-$(command -v google-chrome || command -v chromium || true)}
[ -d node_modules ] || npm ci

sh audio/build.sh
mkdir -p public/audio out
cp audio/out/halcyon-promo-audio.wav audio/out/halcyon-promo-audio-sans-musique.wav public/audio/

# Image seule (le son est assemblé ensuite, sans réencoder l'image).
npx remotion render $COMP out/video$SUFFIX.tmp.mp4 --muted ${CHROME:+--browser-executable="$CHROME"}
# Plage vidéo standard (tv) et repères BT.709, pour tous les lecteurs et plateformes.
ffmpeg -hide_banner -loglevel error -y -i out/video$SUFFIX.tmp.mp4 \
  -vf "scale=in_range=full:out_range=tv,format=yuv420p" -c:v libx264 -preset medium -crf 15 \
  -profile:v high -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv \
  out/video$SUFFIX.tv.mp4
rm out/video$SUFFIX.tmp.mp4
mv out/video$SUFFIX.tv.mp4 out/video$SUFFIX-sans-son.mp4

mux() {
  ffmpeg -hide_banner -loglevel error -y -i out/video$SUFFIX-sans-son.mp4 -i "$1" -map 0:v -map 1:a \
    -c:v copy -c:a aac -b:a 320k -ar 48000 -shortest -movflags +faststart "$2.tmp.mp4"
  mv "$2.tmp.mp4" "$2"
}
mux audio/out/halcyon-promo-audio.wav out/halcyon-promo$SUFFIX.mp4
mux audio/out/halcyon-promo-audio-sans-musique.wav out/halcyon-promo$SUFFIX-sans-musique.mp4
echo "Films prêts : out/halcyon-promo$SUFFIX.mp4 et out/halcyon-promo$SUFFIX-sans-musique.mp4"
