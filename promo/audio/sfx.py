#!/usr/bin/env python3
"""Sound design du film Halcyon : ambiance, souffles, impacts et sons d'interface.

Les repères viennent de src/timeline.json (champ « cues »). Les sons d'interface sont ceux
du launcher lui-même, générés par tools/sound/make_sounds.py dans audio/build/ui.
Tout est synthétisé : aucun échantillon tiers. Écrit audio/build/sfx.wav.
"""
import os

import numpy as np

from synth import (DUR, HERE, IR_HALL, IR_ROOM, SR, TIMELINE, add, bell, boom, hp, hz, load_wav, lp, noise,
                   reverb, reverse_swell, riser, sweep_filter, track, tt, whoosh, write_wav, bp)

UI_DIR = os.path.join(HERE, "build", "ui")


def ambience():
    """Air de bord de l'eau : souffle très bas, lentement modulé, et grave lointain."""
    d = DUR + 1
    x = tt(d)
    air = sweep_filter(noise(d), 600, 600, "band", 0.5, 2048)
    air2 = sweep_filter(noise(d), 2200, 2200, "band", 0.6, 2048)
    mod = 0.6 + 0.4 * np.sin(2 * np.pi * x / 7.3) * np.sin(2 * np.pi * x / 3.1 + 1)
    low = lp(noise(d), 120) * 2.5
    sig = np.stack([air * mod + 0.35 * air2, np.roll(air, 4800) * mod + 0.35 * np.roll(air2, 9000)], 1)
    sig = sig * 0.25 + np.stack([low, np.roll(low, 2400)], 1) * 0.18
    # présente surtout dans l'ouverture et la signature
    env = np.interp(x, [0, 1.5, 9, 11, 46, 48.5, DUR - 1.5, DUR, d], [0, 1, 1, 0.35, 0.35, 0.9, 0.9, 0, 0])
    return sig * env[:, None]


def line_swell():
    s = sweep_filter(noise(2.0), 300, 2400, "band", 2.5, 256)
    x = tt(2.0)
    return reverb(s * np.sin(np.pi * x / 2.0) ** 2 * 0.5, IR_HALL, 0.5)


def dive():
    """Le plongeon : souffle qui tombe, contact avec l'eau, grave qui s'étend."""
    fall = whoosh(0.5, 6000, 500, 0.8) * 1.2
    splash = bp(noise(0.9), 400, 5000) * np.exp(-tt(0.9) * 9) * 0.8
    out = np.zeros(int(SR * 3.5))
    out[: len(fall)] += fall
    i = int(SR * 0.45)
    out[i:i + len(splash)] += splash
    b = boom(3.0, 90, 34) * 0.9
    out[i:i + len(b)] += b[: len(out) - i]
    return reverb(out, IR_HALL, 0.35)


def tick_():
    x = tt(0.06)
    s = np.sin(2 * np.pi * 3100 * x) * np.exp(-x * 160) + 0.3 * hp(noise(0.06), 3000) * np.exp(-x * 400)
    return s * 0.35


def soft_hit():
    b = boom(1.4, 85, 45) * 0.45
    return reverb(b + 0.4 * bell(hz("A4"), 1.4, 0.5, 0.3), IR_HALL, 0.3)


def impact():
    """Le geste final : impact grave, éclat de verre, longue queue."""
    b = boom(5.0, 75, 26) * 1.1
    glass = sum(bell(hz(n_), 4.0, 1.4, 1.0) for n_ in ("D6", "A6", "E7")) * 0.12
    shimmer = hp(noise(3.0), 6000) * np.exp(-tt(3.0) * 2.2) * 0.25
    out = np.zeros(int(SR * 5.0))
    out += b
    out[: len(glass)] += glass
    out[: len(shimmer)] += shimmer
    return reverb(out, IR_HALL, 0.5)


def build():
    sfx = track()
    add(sfx, 0.0, ambience(), 1.0)
    for cue in TIMELINE["cues"]:
        t = cue["t"]
        kind = cue["sfx"]
        if kind == "ambience":
            continue
        elif kind == "line-swell":
            add(sfx, t, line_swell(), 0.8)
        elif kind == "bell-soft":
            add(sfx, t, reverb(bell(hz(cue["note"]), 3.0, 1.2, 0.5, 0.02), IR_HALL, 0.55), 0.22)
        elif kind == "reverse-swell":
            add(sfx, t, reverse_swell(0.65), 0.55)
        elif kind == "dive":
            add(sfx, t - 0.45, dive(), 0.9)
        elif kind == "whoosh":
            add(sfx, t - 0.25, reverb(whoosh(0.7, 400, 5000, 0.55), IR_ROOM, 0.3), 0.55, pan=0.2)
        elif kind == "whoosh-soft":
            add(sfx, t - 0.1, reverb(whoosh(1.2, 300, 2400, 0.5), IR_HALL, 0.4), 0.35, pan=-0.2)
        elif kind == "air-whoosh":
            add(sfx, t - 0.4, reverb(whoosh(1.8, 200, 3500, 0.45), IR_HALL, 0.4), 0.6)
        elif kind == "zoom-whoosh":
            add(sfx, t - 0.35, reverb(whoosh(0.9, 300, 8000, 0.75), IR_ROOM, 0.25), 0.6)
        elif kind == "deep-whoosh":
            add(sfx, t - 0.6, reverb(whoosh(2.2, 120, 1800, 0.35), IR_HALL, 0.45), 0.75)
        elif kind == "swish":
            add(sfx, t - 0.12, whoosh(0.35, 1500, 9000, 0.6), 0.25, pan=0.35)
        elif kind == "tick":
            add(sfx, t, tick_(), 0.9)
        elif kind == "ticks-fast":
            for k in range(cue["count"]):
                add(sfx, t + k * cue["step"], tick_(), 0.6 + 0.04 * k, pan=0.3 * (-1) ** k)
        elif kind == "soft-hit":
            add(sfx, t, soft_hit(), 0.55)
        elif kind == "riser":
            add(sfx, t, reverb(riser(cue["length"]), IR_HALL, 0.3), 0.45)
        elif kind == "impact":
            add(sfx, t, impact(), 1.0)
        elif kind == "ui":
            s = load_wav(os.path.join(UI_DIR, cue["file"] + ".wav"))
            add(sfx, t, reverb(s, IR_ROOM, 0.2), 0.9 * 10 ** (cue.get("gain", 0) / 20))
        else:
            raise SystemExit(f"repère inconnu : {kind}")
    end = int(DUR * SR)
    sfx[end:] = 0
    return sfx


if __name__ == "__main__":
    s = build()
    write_wav(os.path.join(HERE, "build", "sfx.wav"), s / (np.max(np.abs(s)) * 1.12))
    print("sound design :", round(len(s) / SR, 2), "s")
