#!/usr/bin/env python3
"""Identité sonore de Halcyon : synthèse reproductible de tous les effets.

Usage : python3 tools/sound/make_sounds.py [dossier_de_sortie]
        (par défaut : crates/halcyon-audio/sounds)

Direction : timbres de verre et de cloche douce (partiels légèrement inharmoniques),
attaques arrondies, queues courtes dans un petit espace réverbérant. Motif signature
« Halcyon » : ré – la – mi (quinte puis neuvième), utilisé pour le lancement d'un jeu et
les grandes réussites. Tout est calculé ici à partir de sinusoïdes et de bruit filtré :
aucun échantillon externe, aucun droit tiers. Sons placés sous la licence du projet (MIT).

Sortie : WAV PCM 16 bits mono 44 100 Hz, crête ≤ −3 dBFS, niveau homogène par catégorie.
"""
import os
import sys
import wave

import numpy as np

SR = 44100
RNG = np.random.default_rng(0x4A1C)  # graine fixe : fichiers identiques à chaque génération

# Notes (Hz). Tonalité de ré : la palette reste consonante quels que soient les enchaînements.
D5, E5, FS5, A5, B5 = 587.33, 659.26, 739.99, 880.00, 987.77
D6, E6, FS6, A6 = 1174.66, 1318.51, 1479.98, 1760.00
A4, D4, FS4 = 440.00, 293.66, 369.99


def t(dur):
    return np.arange(int(SR * dur)) / SR


def env(n, attack=0.004, decay=0.25, curve=1.0):
    """Attaque arrondie (demi-cosinus) puis décroissance exponentielle."""
    x = np.arange(n) / SR
    a = np.clip(x / max(attack, 1e-4), 0, 1)
    a = 0.5 - 0.5 * np.cos(np.pi * a)
    d = np.exp(-np.maximum(x - attack, 0) / max(decay, 1e-4)) ** curve
    return a * d


def bell(f, dur, decay=0.25, bright=1.0, attack=0.004):
    """Cloche de verre : fondamentale + partiels inharmoniques qui s'éteignent plus vite."""
    x = t(dur)
    out = np.zeros_like(x)
    for ratio, gain, dk in ((1.0, 1.0, 1.0), (2.0, 0.32 * bright, 0.55), (2.76, 0.18 * bright, 0.4),
                            (5.40, 0.07 * bright, 0.22), (0.5, 0.12, 1.3)):
        out += gain * np.sin(2 * np.pi * f * ratio * x + ratio) * env(len(x), attack, decay * dk)
    return out


def glide(f0, f1, dur, decay=0.2, attack=0.006):
    """Sinusoïde dont la hauteur glisse (courbe exponentielle), phase continue."""
    x = t(dur)
    f = f0 * (f1 / f0) ** (x / dur)
    phase = 2 * np.pi * np.cumsum(f) / SR
    return (np.sin(phase) + 0.15 * np.sin(2 * phase)) * env(len(x), attack, decay)


def svf_noise(dur, f_start, f_end, q=1.2, attack=0.02, decay=0.12):
    """Souffle : bruit filtré par un filtre d'état passe-bande dont la fréquence balaie."""
    n = int(SR * dur)
    noise = RNG.standard_normal(n)
    fc = f_start * (f_end / f_start) ** (np.arange(n) / n)
    low = band = 0.0
    out = np.empty(n)
    damp = 1.0 / q
    for i in range(n):
        g = 2 * np.sin(np.pi * min(fc[i], SR / 6) / SR)
        high = noise[i] - low - damp * band
        band += g * high
        low += g * band
        out[i] = band
    return out * env(n, attack, decay)


def tick(f=3200, dur=0.03, decay=0.008, body=0.0, body_f=900):
    """Clic feutré : impulsion tonale brève, éventuellement avec un corps grave."""
    x = t(dur)
    s = np.sin(2 * np.pi * f * x) * env(len(x), 0.0008, decay)
    s += 0.25 * RNG.standard_normal(len(x)) * env(len(x), 0.0003, decay * 0.35)
    if body:
        s += body * np.sin(2 * np.pi * body_f * x) * env(len(x), 0.002, decay * 2.5)
    return s


def place(total, *parts):
    """Assemble des fragments (décalage en secondes, signal, gain)."""
    out = np.zeros(int(SR * total))
    for offset, sig, gain in parts:
        i = int(SR * offset)
        j = min(len(out), i + len(sig))
        out[i:j] += gain * sig[: j - i]
    return out


def room(sig, wet=0.16, size=1.0):
    """Petit espace : quatre filtres en peigne amortis en parallèle + deux passe-tout."""
    tail = int(SR * 0.35 * size)
    x = np.concatenate([sig, np.zeros(tail)])
    acc = np.zeros_like(x)
    for delay_ms, fb in ((29.7, 0.74), (37.1, 0.72), (41.1, 0.70), (43.7, 0.68)):
        d = int(SR * delay_ms / 1000 * size)
        y = np.zeros_like(x)
        lp = 0.0
        for i in range(len(x)):
            prev = y[i - d] if i >= d else 0.0
            lp = 0.55 * prev + 0.45 * lp  # amortissement des aigus dans la queue
            y[i] = x[i] + fb * lp
        acc += y - x
    for delay_ms, g in ((5.0, 0.6), (1.7, 0.6)):
        d = int(SR * delay_ms / 1000)
        y = np.zeros_like(acc)
        for i in range(len(acc)):
            xd = acc[i - d] if i >= d else 0.0
            yd = y[i - d] if i >= d else 0.0
            y[i] = -g * acc[i] + xd + g * yd
        acc = y
    out = x + wet * acc / 4
    # coupe le silence final
    thresh = np.max(np.abs(out)) * 10 ** (-46 / 20)
    last = np.nonzero(np.abs(out) > thresh)[0]
    return out[: last[-1] + 1] if len(last) else out


def finish(sig, target_rms_db, peak_db=-3.0):
    """Fondu de sortie, sonie à court terme cible (perçue homogène), crête bornée."""
    sig = sig - np.mean(sig)
    fade = min(len(sig) // 4, int(SR * 0.012))
    if fade:
        sig[-fade:] *= np.linspace(1, 0, fade)
    # Sonie à court terme (fenêtre de 30 ms la plus forte) : c'est l'attaque qu'on entend.
    w = int(SR * 0.03)
    pw = np.convolve(sig ** 2, np.ones(w) / w, mode="valid") if len(sig) > w else np.array([np.mean(sig ** 2)])
    loud = np.sqrt(np.max(pw)) + 1e-12
    sig = sig * (10 ** (target_rms_db / 20) / loud)
    peak = np.max(np.abs(sig))
    limit = 10 ** (peak_db / 20)
    if peak > limit:
        sig *= limit / peak
    return sig


# Sonie à court terme cible par catégorie (dBFS RMS sur 30 ms) : la navigation reste en retrait.
NAV, INTER, NOTIF, IMPORTANT = -24.0, -19.0, -16.0, -14.0

SOUNDS = {
    # --- navigation
    "ui/select": (lambda: tick(3400, 0.035, 0.007, body=0.35, body_f=1180), NAV),
    "ui/hover": (lambda: tick(4200, 0.02, 0.004), NAV - 6),
    "ui/section": (lambda: room(place(0.16, (0, bell(A5, 0.12, 0.05, 0.6), 0.8), (0.045, bell(D6, 0.11, 0.05, 0.6), 0.6)), 0.12), NAV + 2),
    "ui/panel-open": (lambda: room(place(0.24, (0, svf_noise(0.22, 700, 4200, 1.5, 0.05, 0.06), 0.5),
                                         (0.05, glide(D5, A5, 0.18, 0.07), 0.35)), 0.14), NAV + 1),
    "ui/panel-close": (lambda: room(place(0.2, (0, svf_noise(0.18, 3800, 800, 1.5, 0.02, 0.05), 0.5),
                                          (0.02, glide(A5, D5, 0.16, 0.06), 0.3)), 0.12), NAV),
    "ui/menu-open": (lambda: room(place(0.14, (0, tick(2600, 0.03, 0.006), 0.5), (0.012, bell(FS6, 0.1, 0.035, 0.4), 0.45)), 0.1), NAV),
    "ui/menu-close": (lambda: place(0.09, (0, tick(2000, 0.03, 0.006), 0.5), (0.008, bell(D6, 0.07, 0.025, 0.3), 0.3)), NAV - 1),
    "ui/confirm": (lambda: room(place(0.3, (0, bell(D6, 0.2, 0.08), 0.7), (0.06, bell(A6, 0.24, 0.1), 0.6)), 0.16), INTER),
    "ui/back": (lambda: room(place(0.16, (0, glide(E6, B5, 0.12, 0.05), 0.6), (0, tick(1800, 0.02, 0.005), 0.4)), 0.1), NAV),
    # --- bibliothèque
    "game/select": (lambda: tick(2900, 0.05, 0.009, body=0.6, body_f=740), NAV + 1),
    "game/open": (lambda: room(place(0.42, (0, svf_noise(0.3, 400, 2600, 1.1, 0.06, 0.09), 0.45),
                                     (0.08, bell(D5, 0.33, 0.14, 0.7), 0.5), (0.08, bell(A5, 0.33, 0.14, 0.5), 0.35)), 0.2), INTER),
    "game/favorite": (lambda: room(place(0.42, (0, bell(A5, 0.2, 0.07), 0.55), (0.06, bell(D6, 0.2, 0.07), 0.55),
                                         (0.12, bell(FS6, 0.28, 0.1), 0.6)), 0.2), INTER),
    "game/collection": (lambda: room(place(0.32, (0, bell(FS5, 0.2, 0.08), 0.6), (0.07, bell(B5, 0.24, 0.1), 0.6)), 0.16), INTER),
    "game/launch": (lambda: room(place(1.05, (0, svf_noise(0.9, 180, 5200, 0.9, 0.55, 0.22), 0.5),
                                       (0.0, bell(D4, 0.9, 0.35, 0.4, 0.04), 0.45),
                                       (0.12, bell(D5, 0.6, 0.22), 0.55), (0.26, bell(A5, 0.6, 0.22), 0.55),
                                       (0.40, bell(E6, 0.65, 0.28), 0.6)), 0.26, 1.3), IMPORTANT),
    "game/launched": (lambda: room(place(0.7, (0, bell(D5, 0.6, 0.3, 0.6, 0.02), 0.45), (0, bell(FS5, 0.6, 0.3, 0.5, 0.02), 0.35),
                                         (0, bell(A5, 0.6, 0.3, 0.5, 0.02), 0.35), (0.04, bell(E6, 0.5, 0.2, 0.4), 0.3)), 0.24, 1.2), INTER),
    # --- notifications
    "notify/success": (lambda: room(place(0.4, (0, bell(A5, 0.25, 0.1), 0.6), (0.08, bell(E6, 0.3, 0.13), 0.6)), 0.18), NOTIF),
    "notify/warning": (lambda: room(place(0.42, (0, bell(B5, 0.25, 0.11, 0.5), 0.6), (0.11, bell(FS5, 0.3, 0.14, 0.5), 0.6)), 0.16), NOTIF),
    "notify/error": (lambda: room(place(0.4, (0, bell(D4 * 2, 0.18, 0.07, 0.3), 0.6), (0.13, bell(D4 * 2 * 0.944, 0.22, 0.09, 0.3), 0.6)), 0.14), NOTIF),
    "notify/download": (lambda: room(place(0.48, (0, glide(D5, A5, 0.14, 0.05), 0.3), (0.1, bell(A5, 0.3, 0.12), 0.55), (0.16, bell(D6, 0.32, 0.14), 0.5)), 0.2), NOTIF),
    "notify/install": (lambda: room(place(0.55, (0, bell(D5, 0.3, 0.12), 0.5), (0.08, bell(FS5, 0.3, 0.12), 0.5), (0.16, bell(A5, 0.4, 0.16), 0.55)), 0.2), NOTIF),
    "notify/update": (lambda: room(place(0.55, (0, bell(E5, 0.3, 0.12), 0.5), (0.09, bell(A5, 0.3, 0.12), 0.5), (0.18, bell(E6, 0.4, 0.16), 0.55)), 0.2), NOTIF),
    "notify/major": (lambda: room(place(0.9, (0, bell(D5, 0.5, 0.2), 0.55), (0.12, bell(A5, 0.5, 0.2), 0.55),
                                        (0.24, bell(E6, 0.6, 0.26), 0.6), (0.24, bell(A4, 0.6, 0.3, 0.3, 0.02), 0.3)), 0.24, 1.2), IMPORTANT),
    # --- interface générale
    "ui/command-center": (lambda: room(place(0.3, (0, svf_noise(0.24, 900, 3600, 1.6, 0.04, 0.07), 0.4), (0.04, bell(B5, 0.22, 0.08, 0.6), 0.45)), 0.16), INTER - 2),
    "ui/mode": (lambda: room(place(0.45, (0, glide(A4, D5, 0.25, 0.1), 0.45), (0.12, bell(A5, 0.3, 0.12, 0.6), 0.5)), 0.2), INTER),
    "ui/toggle-on": (lambda: place(0.12, (0, tick(2400, 0.03, 0.006), 0.45), (0.01, bell(A6, 0.1, 0.03, 0.3), 0.4)), INTER - 3),
    "ui/toggle-off": (lambda: place(0.12, (0, tick(1800, 0.03, 0.006), 0.45), (0.01, bell(D6, 0.1, 0.03, 0.3), 0.35)), INTER - 4),
    "ui/window": (lambda: room(place(0.35, (0, svf_noise(0.28, 500, 3000, 1.3, 0.06, 0.08), 0.4), (0.06, bell(D6, 0.25, 0.1, 0.5), 0.4)), 0.18), INTER - 1),
    "ui/setting": (lambda: place(0.1, (0, tick(3000, 0.03, 0.006, body=0.3, body_f=1480), 0.6)), INTER - 4),
}


def write_wav(path, sig):
    data = np.clip(np.round(sig * 32767), -32768, 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "..", "crates", "halcyon-audio", "sounds")
    total = 0
    for name, (make, level) in SOUNDS.items():
        sig = finish(make(), level)
        path = os.path.join(out_dir, name + ".wav")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        write_wav(path, sig)
        size = os.path.getsize(path)
        total += size
        peak = 20 * np.log10(np.max(np.abs(sig)) + 1e-12)
        print(f"{name:20s} {len(sig) / SR * 1000:6.0f} ms  crête {peak:5.1f} dBFS  {size / 1024:5.1f} Kio")
    print(f"{len(SOUNDS)} sons, {total / 1024:.0f} Kio")


if __name__ == "__main__":
    main()
