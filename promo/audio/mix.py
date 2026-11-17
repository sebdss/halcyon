#!/usr/bin/env python3
"""Mixage : musique + sound design → build/mix.wav ; sound design seul → build/nomusic.wav.
La sonie finale (−14 LUFS, crête vraie −1 dBTP) est appliquée par build.sh avec ffmpeg."""
import os
import subprocess
import json
import numpy as np
from synth import HERE, load_wav, write_wav

B = os.path.join(HERE, "build")


def lufs(path):
    r = subprocess.run(["ffmpeg", "-hide_banner", "-i", path, "-af", "loudnorm=print_format=json", "-f", "null", "-"],
                       capture_output=True, text=True).stderr
    return float(json.loads(r[r.rindex("{"):])["input_i"])


music = load_wav(os.path.join(B, "music.wav"))
sfx = load_wav(os.path.join(B, "sfx.wav"))
lm, ls = lufs(os.path.join(B, "music.wav")), lufs(os.path.join(B, "sfx.wav"))
# le sound design se place 5 dB sous la musique : il ponctue sans la masquer
g = 10 ** ((lm - 5.0 - ls) / 20)
n = min(len(music), len(sfx))
mix = music[:n] + g * sfx[:n]
peak = np.max(np.abs(mix))
write_wav(os.path.join(B, "mix-raw.wav"), mix / (peak * 1.05))
write_wav(os.path.join(B, "nomusic-raw.wav"), sfx[:n] / (np.max(np.abs(sfx)) * 1.05))
print(f"musique {lm:.1f} LUFS, sound design {ls:.1f} LUFS, gain sound design {20*np.log10(g):+.1f} dB")
