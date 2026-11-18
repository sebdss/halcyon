"""Briques de synthèse du film Halcyon (numpy + scipy, aucun échantillon externe).

Même direction que l'identité sonore du launcher (tools/sound/make_sounds.py) :
timbres de verre et de cloche, tonalité de ré, attaques arrondies, espaces réverbérants.
Tout est stéréo 48 kHz, en flottant ; graine fixe pour un rendu identique à chaque fois.
"""
import json
import os
import wave

import numpy as np
from scipy import signal

SR = 48000
RNG = np.random.default_rng(0x4A1C0)
HERE = os.path.dirname(os.path.abspath(__file__))
TIMELINE = json.load(open(os.path.join(HERE, "..", "src", "timeline.json")))
BPM = TIMELINE["bpm"]
BEAT = 60.0 / BPM
BAR = 4 * BEAT
DUR = TIMELINE["durationSeconds"]

NOTE = {n: i for i, n in enumerate(["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"])}


def hz(name):
    """« D4 » → fréquence (la4 = 440 Hz)."""
    pitch, octave = name[:-1], int(name[-1])
    return 440.0 * 2 ** ((NOTE[pitch] + 12 * (octave + 1) - 69) / 12)


def n(seconds):
    return int(round(seconds * SR))


def tt(dur):
    return np.arange(n(dur)) / SR


def track(dur=DUR + 2):
    return np.zeros((n(dur), 2))


def add(dst, t0, sig, gain=1.0, pan=0.0):
    """Ajoute un signal mono ou stéréo à `dst` au temps t0 (secondes), avec panoramique."""
    if sig.ndim == 1:
        left = np.cos((pan + 1) * np.pi / 4)
        right = np.sin((pan + 1) * np.pi / 4)
        sig = np.stack([sig * left, sig * right], 1) * np.sqrt(2)
    i = n(t0)
    if i >= len(dst):
        return
    j = min(len(dst), i + len(sig))
    if i < 0:
        sig = sig[-i:]
        i = 0
    dst[i:j] += gain * sig[: j - i]


def adsr(length, a=0.005, d=0.1, s=0.0, r=0.05, curve=2.0):
    """Enveloppe : attaque en demi-cosinus, décroissance exponentielle vers s, relâche finale."""
    x = np.arange(length) / SR
    att = np.clip(x / max(a, 1e-4), 0, 1)
    att = 0.5 - 0.5 * np.cos(np.pi * att)
    dec = s + (1 - s) * np.exp(-np.maximum(x - a, 0) * curve / max(d, 1e-4))
    env = att * dec
    rl = min(n(r), length)
    if rl > 1:
        env[-rl:] *= np.linspace(1, 0, rl) ** 1.5
    return env


def lp(x, fc, order=2):
    b, a = signal.butter(order, min(fc, SR * 0.45) / (SR / 2), "low")
    return signal.lfilter(b, a, x, axis=0)


def hp(x, fc, order=2):
    b, a = signal.butter(order, fc / (SR / 2), "high")
    return signal.lfilter(b, a, x, axis=0)


def bp(x, f1, f2, order=2):
    b, a = signal.butter(order, [f1 / (SR / 2), min(f2, SR * 0.45) / (SR / 2)], "band")
    return signal.lfilter(b, a, x, axis=0)


def sweep_filter(x, f_start, f_end, kind="low", q=0.9, block=256, curve="exp"):
    """Filtre d'état (SVF) dont la fréquence balaie, traité par blocs (rapide, sans clics)."""
    out = np.zeros_like(x)
    nb = int(np.ceil(len(x) / block))
    zi = np.zeros(2)
    for k in range(nb):
        u = k / max(nb - 1, 1)
        fc = f_start * (f_end / f_start) ** u if curve == "exp" else f_start + (f_end - f_start) * u
        w0 = 2 * np.pi * min(fc, SR * 0.45) / SR
        alpha = np.sin(w0) / (2 * q)
        cw = np.cos(w0)
        if kind == "low":
            b = [(1 - cw) / 2, 1 - cw, (1 - cw) / 2]
        elif kind == "high":
            b = [(1 + cw) / 2, -(1 + cw), (1 + cw) / 2]
        else:
            b = [alpha, 0, -alpha]
        a = [1 + alpha, -2 * cw, 1 - alpha]
        b = np.array(b) / a[0]
        a = np.array(a) / a[0]
        seg = x[k * block:(k + 1) * block]
        y, zi = signal.lfilter(b, a, seg, zi=zi)
        out[k * block:(k + 1) * block] = y
    return out


def noise(dur):
    return RNG.standard_normal(n(dur))


# ---------------------------------------------------------------- réverbérations
def _ir(seconds, damp_start=9000, damp_end=1800, predelay=0.012, seed=1):
    r = np.random.default_rng(seed)
    length = n(seconds)
    x = np.arange(length) / SR
    decay = np.exp(-6.9 * x / seconds)
    ir = np.zeros((length, 2))
    for ch in range(2):
        nz = r.standard_normal(length) * decay
        # assombrissement progressif de la queue
        a = sweep_filter(nz, damp_start, damp_end, "low", 0.7, 1024)
        ir[:, ch] = a
    pd = n(predelay)
    ir = np.concatenate([np.zeros((pd, 2)), ir])
    return ir / np.sqrt(np.sum(ir ** 2) / 2)


IR_ROOM = _ir(1.1, 7000, 2000, 0.008, 3)
IR_HALL = _ir(3.6, 8000, 1200, 0.025, 7)


def reverb(x, ir=IR_HALL, wet=0.3):
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    y = np.stack([signal.fftconvolve(x[:, c], ir[:, c])[: len(x)] for c in range(2)], 1)
    return x * (1 - wet * 0.5) + wet * y


def delay(x, seconds, fb=0.35, mix=0.3, pingpong=True, damp=4500):
    """Écho (ping-pong) assombri à chaque répétition."""
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    d = n(seconds)
    out = x.copy()
    tap = x.copy()
    for k in range(1, 7):
        tap = lp(tap, damp, 1) * fb
        if pingpong:
            tap = tap[:, ::-1]
        shifted = np.zeros_like(x)
        shifted[d * k:] = tap[: len(x) - d * k]
        out += mix * shifted
    return out


# ---------------------------------------------------------------- instruments
def bell(f, dur, decay=0.6, bright=1.0, attack=0.004):
    """Cloche de verre, mêmes rapports de partiels que l'identité sonore du launcher."""
    x = tt(dur)
    out = np.zeros_like(x)
    for ratio, gain, dk in ((1.0, 1.0, 1.0), (2.0, 0.32 * bright, 0.55), (2.76, 0.18 * bright, 0.4),
                            (5.40, 0.07 * bright, 0.22), (0.5, 0.12, 1.3)):
        out += gain * np.sin(2 * np.pi * f * ratio * x + ratio) * adsr(len(x), attack, decay * dk, 0, 0.02, 1)
    return out


def pluck(f, dur, decay=0.35, bright=0.8):
    """Pincé doux : harmoniques qui s'éteignent d'autant plus vite qu'elles sont aiguës."""
    x = tt(dur)
    out = np.zeros_like(x)
    for h in range(1, 9):
        g = (bright ** (h - 1)) / h
        out += g * np.sin(2 * np.pi * f * h * x + 0.3 * h) * np.exp(-x * (1 + h * 0.9) / decay)
    return out * adsr(len(x), 0.002, 10, 1, 0.03)


def pad(freqs, dur, attack=1.2, release=1.5, bright=2400, detune=0.12):
    """Nappe : scies douces désaccordées (synthèse additive), filtrées, stéréo large."""
    x = tt(dur)
    L = np.zeros_like(x)
    R = np.zeros_like(x)
    for f in freqs:
        for k, det in enumerate((-detune, 0.0, detune)):
            ff = f * 2 ** (det / 12)
            ph = RNG.uniform(0, 2 * np.pi)
            s = np.zeros_like(x)
            for h in range(1, 12):
                if ff * h > bright * 2.5:
                    break
                s += np.sin(2 * np.pi * ff * h * x + ph * h) / h * np.exp(-((ff * h) / bright) ** 2)
            if k == 0:
                L += s
            elif k == 2:
                R += s
            else:
                L += 0.7 * s
                R += 0.7 * s
    env = adsr(len(x), attack, 10, 1, release)
    out = np.stack([L, R], 1) * env[:, None] / max(len(freqs), 1)
    return out


def sub(f, dur, attack=0.004, release=0.06, drive=1.6):
    x = tt(dur)
    s = np.sin(2 * np.pi * f * x) + 0.18 * np.sin(4 * np.pi * f * x)
    s = np.tanh(drive * s) / np.tanh(drive)
    return s * adsr(len(x), attack, 10, 1, release)


def kick(dur=0.45, f0=140, f1=44, punch=0.9):
    x = tt(dur)
    f = f1 + (f0 - f1) * np.exp(-x * 28)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-x * 6.5)
    click = hp(noise(dur), 2500) * np.exp(-x * 220) * 0.25
    return np.tanh((body + click) * (1 + punch)) * 0.9


def clap(dur=0.35):
    x = tt(dur)
    nz = bp(noise(dur), 900, 6000)
    env = np.zeros_like(x)
    for k, o in enumerate((0.0, 0.011, 0.022, 0.034)):
        env += (x >= o) * np.exp(-np.maximum(x - o, 0) * (90 if k < 3 else 16))
    return nz * env * 0.6


def hat(dur=0.08, open_=False):
    x = tt(dur)
    nz = hp(noise(dur), 7500, 2)
    return nz * np.exp(-x * (18 if open_ else 70)) * 0.5


def boom(dur=4.0, f0=70, f1=28):
    """Impact grave : chute de hauteur longue, souffle et grain."""
    x = tt(dur)
    f = f1 + (f0 - f1) * np.exp(-x * 3)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.tanh(1.8 * np.sin(ph)) * np.exp(-x * 1.4)
    air = lp(noise(dur), 1800) * np.exp(-x * 4) * 0.35
    crack = hp(noise(dur), 3000) * np.exp(-x * 40) * 0.3
    return body + air + crack


def riser(dur, f0=300, f1=9000):
    nz = noise(dur)
    s = sweep_filter(nz, f0, f1, "band", 1.2, 256)
    x = tt(dur)
    env = (x / dur) ** 2.2
    tone = np.sin(2 * np.pi * np.cumsum(220 * 2 ** (2 * x / dur)) / SR) * 0.15 * env
    return (s * 1.6 + tone) * env


def whoosh(dur=0.8, f0=500, f1=5000, peak=0.6):
    """Souffle de passage : bande filtrée qui balaie, enveloppe en cloche décentrée."""
    nz = noise(dur)
    s = sweep_filter(nz, f0, f1, "band", 0.8, 256)
    x = tt(dur) / dur
    env = np.where(x < peak, (x / peak) ** 2, np.exp(-(x - peak) * 7))
    return s * env


def reverse_swell(dur=0.65):
    x = tt(dur)
    s = hp(noise(dur), 2000) * np.exp(-x[::-1] * 5)
    s = reverb(s, IR_ROOM, 0.6)
    return s * ((x / dur) ** 2)[:, None]


# ---------------------------------------------------------------- sorties
def load_wav(path):
    with wave.open(path) as w:
        sr = w.getframerate()
        ch = w.getnchannels()
        data = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64) / 32768
    data = data.reshape(-1, ch)
    if sr != SR:
        data = signal.resample_poly(data, SR, sr, axis=0)
    return data[:, 0] if ch == 1 else data


def write_wav(path, x):
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    data = np.clip(np.round(x * 32767), -32768, 32767).astype("<i2")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with wave.open(path, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())


def shelf(x, fc, gain_db, kind="high", s=0.8):
    """Correcteur en plateau (RBJ)."""
    A = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * fc / SR
    cw, sw = np.cos(w0), np.sin(w0)
    alpha = sw / 2 * np.sqrt((A + 1 / A) * (1 / s - 1) + 2)
    sq = 2 * np.sqrt(A) * alpha
    if kind == "high":
        b = [A * ((A + 1) + (A - 1) * cw + sq), -2 * A * ((A - 1) + (A + 1) * cw), A * ((A + 1) + (A - 1) * cw - sq)]
        a = [(A + 1) - (A - 1) * cw + sq, 2 * ((A - 1) - (A + 1) * cw), (A + 1) - (A - 1) * cw - sq]
    else:
        b = [A * ((A + 1) - (A - 1) * cw + sq), 2 * A * ((A - 1) - (A + 1) * cw), A * ((A + 1) - (A - 1) * cw - sq)]
        a = [(A + 1) + (A - 1) * cw + sq, -2 * ((A - 1) + (A + 1) * cw), (A + 1) + (A - 1) * cw - sq]
    b = np.array(b) / a[0]
    a = np.array(a) / a[0]
    return signal.lfilter(b, a, x, axis=0)
