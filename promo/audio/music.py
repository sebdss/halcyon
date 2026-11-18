#!/usr/bin/env python3
"""Musique originale du film Halcyon — « Berge », ré majeur, 120 BPM, 57 s.

Composée et synthétisée ici (aucun échantillon, aucune boucle tierce) : propriété du projet,
même licence que le code (MIT). Écrit audio/build/music.wav et ses pistes séparées.

Structure (mesures de 2 s, voir src/timeline.json) :
  0–10 s   Identité     nappe de quinte, souffle ; la nappe s'ouvre sur ré maj9 au plongeon (6 s)
  10–18 s  Découverte   arpège filtré qui s'ouvre, basse et grosse caisse légère, montée
  18–40 s  Fonctions    groove complet : ré – si m – sol – la – ré – sol
  40–44 s  Immersion    respiration : nappes, plus de batterie
  44–48 s  Élan         montée sur la, roulement, silence d'un quart de temps
  48–57 s  Signature    résolution sur ré, motif ré–la–mi en cloches, queue de réverbération
"""
import numpy as np

from synth import (BAR, BEAT, DUR, IR_HALL, IR_ROOM, add, adsr, bell, clap, delay, hat, hz, kick, lp, pad,
                   pluck, reverb, shelf, sub, sweep_filter, track, write_wav, HERE)
import os

S16 = BEAT / 4
CHORDS = {
    "D":  {"pad": ["D3", "A3", "E4", "F#4", "C#5"], "arp": ["D4", "F#4", "A4", "C#5", "E5", "A5"], "bass": "D2"},
    "Bm": {"pad": ["B2", "F#3", "C#4", "D4", "A4"], "arp": ["B3", "D4", "F#4", "A4", "C#5", "F#5"], "bass": "B1"},
    "G":  {"pad": ["G2", "D3", "B3", "F#4", "A4"],  "arp": ["G3", "B3", "D4", "F#4", "A4", "D5"], "bass": "G1"},
    "A":  {"pad": ["A2", "E3", "D4", "E4", "B4"],   "arp": ["A3", "D4", "E4", "B4", "D5", "E5"], "bass": "A1"},
}
# (début, fin, accord) en secondes
HARMONY = [
    (6, 10, "D"), (10, 14, "D"), (14, 18, "Bm"),
    (18, 22, "D"), (22, 26, "Bm"), (26, 30, "G"), (30, 34, "A"), (34, 38, "D"), (38, 40, "G"),
    (40, 42, "G"), (42, 44, "Bm"), (44, 48, "A"),
    (48, 52, "D"), (52, 54, "G"), (54, 57, "D"),
]
ARP_ORDER = [0, 2, 1, 3, 2, 4, 3, 5, 4, 3, 2, 4, 1, 3, 2, 0]
ARP_GATE = [1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1]


def chord_at(t):
    for a, b, c in HARMONY:
        if a <= t < b:
            return c
    return None


def kicks():
    times = []
    t = 12.0
    while t < 18.0:  # découverte : temps 1 et 3
        times.append(t)
        t += 2 * BEAT
    t = 18.0
    while t < 40.0:  # groove : quatre temps
        times.append(t)
        t += BEAT
    t = 44.0
    while t < 46.0:
        times.append(t)
        t += BEAT
    t = 46.0
    while t < 47.75:
        times.append(t)
        t += BEAT / 2
    times += [48.0, 52.0]
    return times


def sidechain(length, times, depth=0.55, rel=0.14):
    g = np.ones(length)
    x = np.arange(length) / 48000
    for tk in times:
        i = int(tk * 48000)
        seg = x[i:i + int(0.6 * 48000)] - tk
        g[i:i + len(seg)] = np.minimum(g[i:i + len(seg)], 1 - depth * np.exp(-seg / rel))
    return g[:, None]


def main():
    out = os.path.join(HERE, "build")
    drums, bass, harm, lead = track(), track(), track(), track()
    K = kicks()

    # ---------------- nappes
    add(harm, 0.0, pad([hz("D2"), hz("A2"), hz("D3")], 6.6, 3.0, 1.2, 900, 0.08), 0.55)
    for a, b, c in HARMONY:
        bright = 1600 if a < 18 else 2600
        if 40 <= a < 44:
            bright = 3200
        g = 0.42 if a >= 18 else 0.5
        if a >= 48:
            bright, g = 3000, 0.6
        add(harm, a, pad([hz(x) for x in CHORDS[c]["pad"]], b - a + 1.2, 0.5 if a in (6, 48) else 0.9, 1.4, bright), g)

    # ---------------- arpège (pincé, écho ping-pong de 3 doubles-croches)
    arp = track()
    t = 8.0
    step = 0
    while t < 56.0:
        c = chord_at(t + 1e-6)
        in_break = 40 <= t < 44
        in_sig = t >= 48
        play = ARP_GATE[step % 16]
        if in_sig:
            play = step % 4 == 0  # signature : noires seulement
        if t < 10:
            play = step % 2 == 0
        if c and play:
            notes = CHORDS[c]["arp"]
            f = hz(notes[ARP_ORDER[step % 16] % len(notes)])
            vel = 0.55 + 0.25 * (step % 4 == 0) + 0.1 * RNG_jitter(step)
            add(arp, t, pluck(f, 0.6, 0.22 if not in_break else 0.4, 0.86), vel * (0.8 if in_break else 1.0),
                pan=0.35 * np.sin(step * 0.9))
        t += S16
        step += 1
    # filtre : fermé au début, s'ouvre pendant la découverte, se referme dans la respiration
    arp = shape_filter(arp, [(8, 700), (10, 900), (17.5, 4200), (18, 5200), (39.5, 6500), (40, 1400),
                             (44, 1200), (47.8, 7000), (48, 3500), (57, 2500)])
    arp = delay(arp, 3 * S16, 0.38, 0.32)
    add(harm, 0, arp, 0.58)

    # ---------------- basse
    t = 12.0
    while t < 48.0:
        c = chord_at(t + 1e-6)
        if c and not (40 <= t < 44):
            f = hz(CHORDS[c]["bass"])
            pos = int(round((t % BAR) / (BEAT / 2)))
            # croches ; octave sur les contretemps pendant le groove
            if t >= 18 and pos % 2 == 1:
                f *= 2
            vel = 0.9 if pos % 2 == 0 else 0.65
            if t < 18 and pos % 2 == 1:
                vel = 0.5
            add(bass, t, sub(f, BEAT / 2 * 0.92, 0.003, 0.03), vel)
        t += BEAT / 2
    add(bass, 48.0, sub(hz("D1"), 3.8, 0.002, 2.5, 2.2), 1.1)
    add(bass, 52.0, sub(hz("G1"), 1.9, 0.01, 1.2), 0.6)
    add(bass, 54.0, sub(hz("D1"), 3.0, 0.05, 2.5), 0.7)
    bass = lp(bass, 900)

    # ---------------- batterie
    for tk in K:
        g = 0.75 if tk < 18 else 1.0
        if tk >= 48:
            g = 1.15
        add(drums, tk, kick(0.5 if tk < 48 else 1.2, 150, 42), g)
    t = 18.0
    while t < 40.0:
        b = int(round((t - 18.0) / BEAT))
        if b % 2 == 1:
            add(drums, t, clap(), 0.55)
        t += BEAT
    t = 14.0
    k = 0
    while t < 48.0:
        if not (40 <= t < 44) and not (47.75 <= t):
            dense = t >= 18
            if dense or k % 2 == 0:
                acc = 1.0 if k % 4 == 2 else (0.55 if k % 2 == 0 else 0.38)
                add(drums, t, hat(0.07), acc * (0.45 if t < 18 else 0.6), pan=0.25)
            if dense and k % 4 == 2 and k % 16 in (6, 14):
                add(drums, t, hat(0.3, True), 0.3, pan=-0.2)
        t += S16
        k += 1
    # roulements : fin de la découverte, transitions, élan final
    for start, end in ((17.0, 18.0), (33.5, 34.0), (37.5, 38.0), (46.0, 47.75)):
        t = start
        i = 0
        steps = int(round((end - start) / S16))
        while t < end - 1e-6:
            add(drums, t, clap(0.2), 0.12 + 0.45 * (i / max(steps - 1, 1)) ** 1.5, pan=0.15 * (-1) ** i)
            t += S16
            i += 1
    drums = reverb(drums, IR_ROOM, 0.12)

    # ---------------- cloches (le motif ré – la – mi du launcher)
    motif = [(0, "D5"), (BEAT / 2, "A5"), (BEAT, "E6")]
    for t0 in (18.0, 26.0, 34.0):
        for dt, note in motif:
            add(lead, t0 + dt, bell(hz(note), 1.6, 0.7, 0.8), 0.32, pan=0.2)
    for t0, notes in ((22.0, ("F#5", "A5", "C#6")), (30.0, ("E5", "A5", "B5"))):
        for k2, note in enumerate(notes):
            add(lead, t0 + k2 * BEAT / 2, bell(hz(note), 1.4, 0.6, 0.7), 0.26, pan=-0.2)
    for note in ("D5", "F#5", "A5", "E6"):  # accord de résolution, à 48 s
        add(lead, 48.0, bell(hz(note), 3.5, 1.6, 0.6), 0.22)
    for t0, notes in ((40.0, ("B5", "D6")), (42.0, ("A5", "F#5"))):
        for k2, note in enumerate(notes):
            add(lead, t0 + k2 * BEAT, bell(hz(note), 2.0, 1.0, 0.5), 0.18, pan=0.3 * (-1) ** k2)
    lead = delay(lead, BEAT * 0.75, 0.3, 0.22)
    lead = reverb(lead, IR_HALL, 0.45)

    # ---------------- assemblage et compression latérale
    L = len(harm)
    sc = sidechain(L, [k for k in K if k < 48])
    harm = reverb(harm, IR_HALL, 0.32) * sc
    bass = bass * sidechain(L, [k for k in K if k < 48], 0.8, 0.1)
    harm = shelf(harm, 2500, 5.0, "high")
    drums = shelf(drums, 4000, 3.0, "high")
    stems = {"drums": drums * 0.78, "bass": bass * 0.42, "harmony": harm * 0.9, "bells": lead * 1.15}
    music = sum(stems.values())
    music = shelf(shelf(music, 110, -2.0, "low"), 3500, 2.0, "high")
    # coupure d'un quart de temps avant l'impact de 48 s
    i0, i1 = int(47.75 * 48000), int(48.0 * 48000)
    fade = np.linspace(1, 0, 1200)[:, None]
    music[i0:i0 + 1200] *= fade
    music[i0 + 1200:i1] *= 0.0
    for v in stems.values():
        v[i0:i0 + 1200] *= fade
        v[i0 + 1200:i1] *= 0.0
    # fin
    end = int(DUR * 48000)
    music[end:] = 0
    tail = np.linspace(1, 0, int(2.0 * 48000))[:, None] ** 2
    music[end - len(tail):end] *= tail
    for name, v in stems.items():
        v[end:] = 0
        v[end - len(tail):end] *= tail
        write_wav(os.path.join(out, "stems", f"music-{name}.wav"), v / max(1.0, np.max(np.abs(v)) * 1.12))
    write_wav(os.path.join(out, "music.wav"), music / (np.max(np.abs(music)) * 1.12))
    print("musique :", round(len(music) / 48000, 2), "s")


def shape_filter(x, points):
    """Passe-bas dont la fréquence suit une courbe (temps, Hz), interpolée en log."""
    out = np.zeros_like(x)
    block = 512
    ts = np.array([p[0] for p in points])
    fs = np.log(np.array([p[1] for p in points], dtype=float))
    from scipy import signal
    zi = [np.zeros(2), np.zeros(2)]
    for k in range(0, len(x), block):
        t = k / 48000
        fc = float(np.exp(np.interp(t, ts, fs)))
        b, a = signal.butter(2, min(fc, 20000) / 24000, "low")
        for ch in range(2):
            out[k:k + block, ch], zi[ch] = signal.lfilter(b, a, x[k:k + block, ch], zi=zi[ch] if zi[ch].any() else
                                                          signal.lfilter_zi(b, a) * x[k, ch])
    return out


def RNG_jitter(step):
    return ((step * 7919) % 13) / 13.0 - 0.5


if __name__ == "__main__":
    main()
