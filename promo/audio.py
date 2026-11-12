"""The promo's soundtrack and sound effects, synthesized from scratch (numpy only), so the
music is original and royalty-free by construction.

    python3 audio.py out/cues.json out/audio.wav

cues.json comes from the animation (render.mjs writes it): the video's length, the section
markers the music follows, and every sound effect with its time.
"""
import json
import sys
import wave

import numpy as np

SR = 48000
BPM = 120
BEAT = 60 / BPM
rng = np.random.default_rng(7)


def midi(m):
    return 440.0 * 2 ** ((m - 69) / 12)


# ---------------------------------------------------------------- filters (FFT, zero phase)

def _resp(n, fc, kind, order, q=1.0):
    f = np.fft.rfftfreq(n, 1 / SR)
    f[0] = 1e-6
    if kind == "lp":
        return 1 / np.sqrt(1 + (f / fc) ** (2 * order))
    if kind == "hp":
        return 1 / np.sqrt(1 + (fc / f) ** (2 * order))
    if kind == "bp":  # gaussian bump on a log axis, q = width in octaves
        return np.exp(-0.5 * (np.log2(f / fc) / q) ** 2)
    raise ValueError(kind)


def filt(x, fc, kind="lp", order=2, q=1.0):
    if x.ndim == 2:
        return np.stack([filt(x[:, c], fc, kind, order, q) for c in range(x.shape[1])], 1)
    n = len(x)
    m = 1 << int(np.ceil(np.log2(max(n, 2))))
    X = np.fft.rfft(x, m)
    return np.fft.irfft(X * _resp(m, fc, kind, order, q), m)[:n]


def sweep(x, fcs, kind="bp", q=0.6, block=2048):
    """Time-varying filter: overlap-add of Hann blocks, each filtered at its own cutoff.
    fcs(t in 0..1) gives the cutoff along the buffer."""
    n = len(x)
    hop = block // 2
    win = np.hanning(block)
    out = np.zeros(n + block)
    xp = np.concatenate([x, np.zeros(block)])
    for i in range(0, n, hop):
        seg = xp[i:i + block] * win
        fc = fcs(min(i / max(n, 1), 1.0))
        X = np.fft.rfft(seg)
        out[i:i + block] += np.fft.irfft(X * _resp(block, fc, kind, 2, q), block)
    return out[:n]


# ---------------------------------------------------------------- oscillators / voices

def t_axis(dur):
    return np.arange(int(dur * SR)) / SR


def saw(freq, dur, phase=0.0):
    t = t_axis(dur)
    return 2 * ((freq * t + phase) % 1.0) - 1


def env_adsr(n, a=0.005, d=0.1, s=0.7, r=0.1, hold=None):
    """hold = seconds before release starts (default: until the end minus r)."""
    t = np.arange(n) / SR
    dur = n / SR
    hold = dur - r if hold is None else hold
    e = np.where(t < a, t / max(a, 1e-6), s + (1 - s) * np.exp(-(t - a) / max(d, 1e-6)))
    rel = np.clip((t - hold) / max(r, 1e-6), 0, 1)
    return e * (1 - rel)


def pan(x, p):
    """p in -1..1 (equal power)."""
    a = (p + 1) * np.pi / 4
    return np.stack([x * np.cos(a), x * np.sin(a)], 1)


class Bus:
    def __init__(self, n):
        self.x = np.zeros((n, 2))

    def add(self, t0, sig, gain=1.0, p=0.0):
        if sig.ndim == 1:
            sig = pan(sig, p)
        i = int(round(t0 * SR))
        if i < 0:
            sig = sig[-i:]
            i = 0
        j = min(i + len(sig), len(self.x))
        if j > i:
            self.x[i:j] += sig[: j - i] * gain


# ---------------------------------------------------------------- drums

def kick(punch=1.0):
    n = int(0.45 * SR)
    t = np.arange(n) / SR
    f = 41 + 105 * np.exp(-t / 0.045)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-t / 0.3)
    click = filt(rng.standard_normal(n) * np.exp(-t / 0.003), 3000, "bp", q=1.0) * 0.6
    return np.tanh((body + click * punch) * 1.6) * 0.9


def clap():
    n = int(0.35 * SR)
    t = np.arange(n) / SR
    noise = rng.standard_normal(n)
    e = np.zeros(n)
    for k, off in enumerate([0, 0.011, 0.022]):
        e += (t >= off) * np.exp(-np.clip(t - off, 0, None) / 0.006) * (0.8 if k < 2 else 1)
    e += (t >= 0.03) * np.exp(-np.clip(t - 0.03, 0, None) / 0.09) * 0.7
    return filt(noise * e, 1400, "bp", q=0.9) * 1.3


def hat(open_=False):
    n = int((0.22 if open_ else 0.06) * SR)
    t = np.arange(n) / SR
    x = filt(rng.standard_normal(n), 7000, "hp", order=3)
    return x * np.exp(-t / (0.07 if open_ else 0.014)) * 0.5


def snare():
    n = int(0.25 * SR)
    t = np.arange(n) / SR
    tone = np.sin(2 * np.pi * 190 * t) * np.exp(-t / 0.05)
    nz = filt(rng.standard_normal(n), 3500, "bp", q=1.2) * np.exp(-t / 0.08)
    return (tone * 0.5 + nz) * 0.7


def tick(freq):
    n = int(0.03 * SR)
    t = np.arange(n) / SR
    return np.sin(2 * np.pi * freq * t) * np.exp(-t / 0.006)


# ---------------------------------------------------------------- tonal voices

def supersaw(notes, dur, cutoff, voices=5, detune=0.12):
    n = int(dur * SR)
    L = np.zeros(n)
    R = np.zeros(n)
    for m in notes:
        for v in range(voices):
            d = (v - (voices - 1) / 2) / ((voices - 1) / 2) * detune
            f = midi(m + d)
            s = saw(f, dur, rng.random())
            w = (v / (voices - 1))
            L += s * (1 - w)
            R += s * w
    x = np.stack([L, R], 1) / (len(notes) * voices) * 2
    return filt(x, cutoff, "lp", order=2)


def bass_note(m, dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(m)
    x = 0.6 * np.sin(2 * np.pi * f * t) + 0.5 * saw(f, dur)[:n]
    x = filt(x, 420, "lp", order=3)
    e = env_adsr(n, a=0.004, d=0.12, s=0.75, r=0.03)
    return np.tanh(x * e * 1.8) * 0.8


def sub(m, dur):
    """A pure sine sub-bass, a soft attack and release."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    return np.sin(2 * np.pi * midi(m) * t) * env_adsr(n, a=0.01, d=0.3, s=0.9, r=0.06)


def reese(m, dur, cutoff=420):
    """Two detuned saws, low-passed: a growling synth bass."""
    n = int(dur * SR)
    f = midi(m)
    x = saw(f * 1.004, dur, rng.random())[:n] + saw(f * 0.996, dur, rng.random())[:n] + 0.5 * saw(f / 2, dur)[:n]
    bright = filt(x, cutoff * 2.2, "lp", order=3)
    dark = filt(x, cutoff, "lp", order=3)
    t = np.arange(n) / SR
    k = np.exp(-t / 0.07)  # filter envelope: opens on the attack, then closes
    y = bright * k + dark * (1 - k)
    return np.tanh(y * env_adsr(n, a=0.004, d=0.15, s=0.8, r=0.04) * 1.2) * 0.7


def synth(m, dur=0.3, cutoff=1300, decay=0.16):
    """A saw + square synth pluck with a closing filter, warm rather than bright."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(m)
    sq = np.sign(np.sin(2 * np.pi * f * 1.003 * t))
    x = saw(f, dur, rng.random())[:n] + 0.45 * sq
    k = np.exp(-t / 0.05)
    y = filt(x, cutoff * 2.5, "lp", order=2) * k + filt(x, cutoff, "lp", order=2) * (1 - k)
    return y * np.exp(-t / decay) * np.minimum(1, t / 0.003) * 0.6


def lead(m, dur, cutoff=1600):
    """A detuned saw lead with a little vibrato, for the long notes."""
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(m) * (1 + 0.004 * np.sin(2 * np.pi * 5.5 * t) * np.minimum(1, t / 0.3))
    ph = np.cumsum(f) / SR
    x = (2 * ((ph * 1.003) % 1) - 1) + (2 * ((ph * 0.997) % 1) - 1)
    return filt(x, cutoff, "lp", order=2) * env_adsr(n, a=0.03, d=0.3, s=0.75, r=0.2) * 0.5


def pluck(m, dur=0.35, bright=3200):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(m)
    x = np.sin(2 * np.pi * f * t) + 0.35 * saw(f, dur)[:n] + 0.2 * np.sin(4 * np.pi * f * t)
    x = filt(x, bright, "lp", order=2)
    return x * np.exp(-t / 0.11) * np.minimum(1, t / 0.002)


def bell(m, dur=1.2):
    n = int(dur * SR)
    t = np.arange(n) / SR
    f = midi(m)
    x = (np.sin(2 * np.pi * f * t) + 0.45 * np.sin(2 * np.pi * f * 2.76 * t) * np.exp(-t / 0.15)
         + 0.25 * np.sin(2 * np.pi * f * 5.4 * t) * np.exp(-t / 0.06))
    return x * np.exp(-t / 0.45) * np.minimum(1, t / 0.002)


# ---------------------------------------------------------------- sound effects

def noise_sweep(dur, f0, f1, q=0.5):
    n = int(dur * SR)
    x = rng.standard_normal(n)
    y = sweep(x, lambda u: f0 * (f1 / f0) ** u, "bp", q)
    t = np.arange(n) / SR
    return y, t


def sfx_whoosh(dur=0.55, up=True):
    y, t = noise_sweep(dur, 500 if up else 5000, 5000 if up else 400, 0.55)
    peak = 0.6 if up else 0.25
    e = np.where(t / dur < peak, (t / dur / peak) ** 2, np.exp(-(t / dur - peak) / 0.12))
    return y * e * 0.9


def sfx_riser(dur):
    n = int(dur * SR)
    t = np.arange(n) / SR
    y, _ = noise_sweep(dur, 300, 9000, 0.45)
    f = 180 * (8 ** (t / dur)) * (1 + 0.01 * np.sin(2 * np.pi * 6 * t))
    tone = np.sin(2 * np.pi * np.cumsum(f) / SR) * 0.25
    e = (t / dur) ** 2.2
    return (y * 0.8 + tone) * e


def sfx_swell(dur):  # reverse cymbal
    n = int(dur * SR)
    t = np.arange(n) / SR
    x = filt(rng.standard_normal(n), 5000, "hp", order=2)
    return x * (t / dur) ** 3 * 0.6


def sfx_impact():
    n = int(2.6 * SR)
    t = np.arange(n) / SR
    f = 38 + 70 * np.exp(-t / 0.08)
    sub = np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / 0.9)
    nz = filt(rng.standard_normal(n), 900, "lp", order=2) * np.exp(-t / 0.25)
    crack = filt(rng.standard_normal(n), 3000, "hp", order=2) * np.exp(-t / 0.03)
    return np.tanh((sub * 1.2 + nz * 0.8 + crack * 0.5) * 1.3) * 0.95


def sfx_pop(freq=900):
    n = int(0.09 * SR)
    t = np.arange(n) / SR
    f = freq * (0.45 + 0.55 * np.exp(-t / 0.02))
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / 0.03) * 0.7


def sfx_click():
    n = int(0.03 * SR)
    t = np.arange(n) / SR
    x = filt(rng.standard_normal(n), 2500, "bp", q=0.7) * np.exp(-t / 0.002)
    return x * 0.8 + np.sin(2 * np.pi * 1800 * t) * np.exp(-t / 0.004) * 0.3


def sfx_key():
    n = int(0.025 * SR)
    t = np.arange(n) / SR
    x = filt(rng.standard_normal(n), 3000 + rng.random() * 2500, "bp", q=0.5)
    return x * np.exp(-t / 0.0025) * (0.3 + 0.15 * rng.random())


def sfx_ding():
    return (bell(96, 1.0) * 0.5 + bell(103, 1.0) * 0.3) * 0.6


def sfx_success():
    a = bell(84, 0.8) * 0.4
    b = bell(91, 1.0) * 0.4
    out = np.zeros(int(1.1 * SR))
    out[: len(a)] += a
    k = int(0.09 * SR)
    out[k:k + len(b)] += b[: len(out) - k]
    return out


def sfx_alert():  # the "?" bubbles in the chaos: a soft, slightly sour blip
    n = int(0.22 * SR)
    t = np.arange(n) / SR
    x = np.sin(2 * np.pi * 523 * t) + 0.7 * np.sin(2 * np.pi * 554 * t)
    return x * np.exp(-t / 0.06) * 0.35


def sfx_glitch(dur=0.35):
    n = int(dur * SR)
    x = np.zeros(n)
    seg = int(0.018 * SR)
    for i in range(0, n, seg):
        kind = rng.integers(3)
        t = np.arange(min(seg, n - i)) / SR
        if kind == 0:
            x[i:i + len(t)] = np.sign(np.sin(2 * np.pi * (200 + rng.random() * 1800) * t)) * 0.4
        elif kind == 1:
            x[i:i + len(t)] = rng.standard_normal(len(t)) * 0.5
    x = np.round(x * 6) / 6  # crush
    return filt(x, 6000, "lp", order=1) * 0.55


def sfx_zoom():
    w = sfx_whoosh(0.45, True)
    n = int(0.3 * SR)
    t = np.arange(n) / SR
    thump = np.sin(2 * np.pi * (60 + 60 * np.exp(-t / 0.03)) * t) * np.exp(-t / 0.1) * 0.6
    out = np.zeros(max(len(w), int(0.35 * SR) + n))
    out[: len(w)] += w * 0.7
    k = int(0.33 * SR)
    out[k:k + n] += thump[: len(out) - k]
    return out


def sfx_shimmer():
    out = np.zeros(int(2.2 * SR))
    for k, m in enumerate([88, 93, 95, 100, 105, 107]):
        b = bell(m, 1.5) * 0.18
        i = int(k * 0.06 * SR)
        out[i:i + len(b)] += b[: len(out) - i]
    return out


# ---------------------------------------------------------------- reverb / delay

def reverb(x, secs=2.2, wet=0.25, damp=5000):
    n = int(secs * SR)
    t = np.arange(n) / SR
    ir = rng.standard_normal((n, 2)) * np.exp(-t / (secs / 6.9))[:, None]
    ir = filt(ir, damp, "lp", order=1)
    ir[: int(0.012 * SR)] = 0  # pre-delay
    ir /= np.sqrt((ir ** 2).sum(0))[None, :]
    out = np.zeros_like(x)
    m = 1 << int(np.ceil(np.log2(len(x) + n)))
    for c in range(2):
        y = np.fft.irfft(np.fft.rfft(x[:, c], m) * np.fft.rfft(ir[:, c], m), m)[: len(x)]
        out[:, c] = y
    return x + out * wet


def pingpong(x, delay, fb=0.35, taps=5):
    out = x.copy()
    d = int(delay * SR)
    g = 1.0
    for k in range(1, taps + 1):
        g *= fb
        src = x[:, 1] if k % 2 else x[:, 0]
        ch = 0 if k % 2 else 1
        out[k * d:, ch] += src[: len(x) - k * d] * g
    return out


# ---------------------------------------------------------------- the track

CHORDS = [  # vi-IV-I-V in C: Am(add9) F(maj7) C G, one bar each
    (45, [57, 60, 64, 71]),
    (41, [53, 57, 60, 64]),
    (48, [55, 60, 64, 67]),
    (43, [55, 59, 62, 67]),
]


def music(D, M):
    n = int(D * SR)
    drums, bass, pad, arp, fx, low = (Bus(n) for _ in range(6))
    chaos_end = M["chaos_end"]
    logo_end = M["logo_end"]
    full_from = M["full_from"]
    outro = M["outro"]
    final = M["final"]
    bar = 4 * BEAT
    K = kick()

    # --- chaos: drone, clock ticks speeding up, a heartbeat kick, the riser
    dur = chaos_end
    t = t_axis(dur)
    drone = (saw(midi(33), dur) + 0.6 * saw(midi(39) * 1.003, dur)) * 0.5
    drone = filt(drone, 260, "lp", 3) * np.minimum(1, t / 3) * (0.55 + 0.45 * t / dur)
    drone *= 0.8 + 0.2 * np.sin(2 * np.pi * 4 * t)
    pad.add(0, pan(drone, 0) * 0.35)
    tt = 0.0
    k = 0
    while tt < chaos_end - 0.05:
        step = BEAT / 2 if tt < chaos_end - 4 else BEAT / 4
        fx.add(tt, tick(1000 if k % 2 == 0 else 780), 0.17 + 0.08 * tt / chaos_end, p=0.3 if k % 2 else -0.3)
        tt += step
        k += 1
    for b in np.arange(2.0, chaos_end - 0.01, BEAT):
        drums.add(b, filt(K, 900, "lp", 2), 0.35 + 0.35 * b / chaos_end)
    fx.add(chaos_end - 4.0, sfx_riser(4.0), 0.55)
    fx.add(chaos_end - 1.5, sfx_swell(1.5), 0.7)

    # --- logo: impact, a wide chord, shimmer, a swell into the drop
    fx.add(chaos_end, sfx_impact(), 0.95)
    root, notes = CHORDS[0]
    p = supersaw(notes + [76], logo_end - chaos_end + 1.2, 2600)
    e = env_adsr(len(p), a=0.05, d=1.5, s=0.6, r=0.9)
    pad.add(chaos_end, p * e[:, None], 0.9)
    bass.add(chaos_end, bass_note(root - 12, logo_end - chaos_end - 0.3), 0.6)
    low.add(chaos_end, sub(root - 12, logo_end - chaos_end - 0.2), 0.9)
    pad.add(chaos_end, supersaw([n - 12 for n in notes[:3]], logo_end - chaos_end, 700) * env_adsr(int((logo_end - chaos_end) * SR), 0.3, 1.0, 0.8, 0.8)[:, None], 0.7)
    fx.add(chaos_end + 0.05, sfx_shimmer(), 0.7)
    fx.add(logo_end - 1.0, sfx_swell(1.0), 0.6)

    # --- groove: bars from logo_end to outro
    nbars = int(round((outro - logo_end) / bar))
    for bi in range(nbars):
        b0 = logo_end + bi * bar
        full = b0 >= full_from - 1e-6
        root, notes = CHORDS[bi % 4]
        phrase_end = (bi % 8) == 7
        # drums: deep kick, clap, quiet closed hats
        for q in range(4):
            bt = b0 + q * BEAT
            if not (phrase_end and q == 3 and bi % 16 == 15):
                drums.add(bt, K, 1.1)
            drums.add(bt + BEAT / 2, hat(), 0.45 if full else 0.35, p=0.2)
            if full and q in (1, 3):
                drums.add(bt, filt(clap(), 1100, "lp", 2), 0.85, p=0.05)
        if phrase_end:
            for s in range(8):
                drums.add(b0 + 2 * BEAT + s * BEAT / 4, filt(snare(), 2500, "lp", 2), 0.25 + 0.06 * s, p=-0.1 + 0.03 * s)
        # bass: a sub on the root all bar, a reese synth bass rolling on the off-beats
        low.add(b0, sub(root - 12, bar), 0.7)
        sweep_c = 320 + 380 * ((bi % 8) / 7)  # the bass opens up across each 8-bar phrase
        for s in range(8):
            if s % 2 == 1 or s == 6:
                m = root - 12 + (12 if s == 7 else 0)
                low.add(b0 + s * BEAT / 2, reese(m, BEAT / 2 * 0.95, sweep_c), 0.9)
        # pads: a warm low voicing under the chord
        pd = supersaw([x - 12 for x in notes], bar + 0.25, 900 if full else 700)
        pe = env_adsr(len(pd), a=0.03, d=0.4, s=0.8, r=0.25)
        pad.add(b0, pd * pe[:, None], 0.6 if full else 0.5)
        if full:
            pd = supersaw(notes, bar + 0.25, 1100)
            pad.add(b0, pd * pe[:, None], 0.3)
        # synth arp in the middle register (no high plucks)
        if full:
            tones = [x - 12 for x in notes] + notes
            order = [0, 2, 4, 1, 3, 5, 2, 6, 4, 7, 5, 3, 6, 4, 2, 1]
            cut = 900 + 700 * ((bi % 8) / 7)
            for s in range(16):
                arp.add(b0 + s * BEAT / 4, synth(tones[order[s] % len(tones)], 0.3, cut), 0.42, p=0.3 if s % 2 else -0.3)
        # a synth lead on every other phrase: one long note per bar, A minor
        if full and (bi // 8) % 2 == 1:
            melody = [64, 65, 67, 62]
            arp.add(b0, lead(melody[bi % 4], bar * 0.95), 0.32, p=0.0)
        if phrase_end:
            fx.add(b0 + 2 * BEAT, sfx_riser(2 * BEAT), 0.25)

    # --- outro: pad and arp alone, a riser, the final chord
    root, notes = CHORDS[1]
    p = supersaw([x - 12 for x in notes] + notes, final - outro + 0.3, 1000)
    pad.add(outro, p * env_adsr(len(p), 0.02, 0.6, 0.85, 0.3)[:, None], 0.6)
    low.add(outro, sub(root - 12, final - outro), 0.8)
    bass.add(outro, bass_note(root - 12, 2 * BEAT), 0.6)
    root2, notes2 = CHORDS[3]
    for s in range(int((final - outro) / (BEAT / 4))):
        tones = [x - 12 for x in (notes if s < 8 else notes2)] + notes
        arp.add(outro + s * BEAT / 4, synth(tones[(s * 3) % len(tones)], 0.3, 1100), 0.26, p=0.3 if s % 2 else -0.3)
    fx.add(final - 2.0, sfx_riser(2.0), 0.4)
    fx.add(final - 1.2, sfx_swell(1.2), 0.6)
    fx.add(final, sfx_impact(), 0.8)
    endc = [55, 60, 64, 71, 74]
    tail = D - final
    p = supersaw(endc, tail, 2400)
    pad.add(final, p * env_adsr(len(p), 0.02, 2.0, 0.5, tail * 0.7)[:, None], 0.85)
    bass.add(final, bass_note(36, min(3.0, tail)) * np.exp(-t_axis(min(3.0, tail)) / 1.2)[: int(min(3.0, tail) * SR)], 0.7)
    fx.add(final + 0.05, sfx_shimmer(), 0.6)

    # --- sidechain: duck bass and pad under each kick in the groove
    sc = np.ones(n)
    tk = np.arange(n) / SR
    for bt in np.arange(logo_end, outro - 1e-6, BEAT):
        i = int(bt * SR)
        j = min(n, i + int(0.3 * SR))
        u = tk[i:j] - bt
        sc[i:j] = np.minimum(sc[i:j], 1 - 0.65 * np.exp(-u / 0.09) * np.minimum(1, u / 0.004 + 0.4))
    bass.x *= sc[:, None]
    low.x *= sc[:, None]
    pad.x *= sc[:, None]

    arpx = pingpong(arp.x, 3 * BEAT / 4, 0.3, 4)
    mix = drums.x * 0.8 + bass.x * 0.75 + pad.x * 0.5 + arpx * 0.55 + fx.x * 0.7
    mix = reverb(mix, 2.4, 0.22)
    mix += low.x * 0.6  # the low end stays dry and centered
    return mix


def sfx_track(D, cues):
    n = int(D * SR)
    bus = Bus(n)
    for c in cues:
        k = c["kind"]
        t = c["t"]
        g = c.get("gain", 1.0)
        p = c.get("pan", 0.0)
        if k == "whoosh":
            bus.add(t, sfx_whoosh(c.get("dur", 0.55), True), 0.55 * g, p)
        elif k == "whooshDown":
            bus.add(t, sfx_whoosh(c.get("dur", 0.6), False), 0.5 * g, p)
        elif k == "zoom":
            bus.add(t, sfx_zoom(), 0.6 * g, p)
        elif k == "pop":
            bus.add(t, sfx_pop(c.get("freq", 900)), 0.5 * g, p)
        elif k == "click":
            bus.add(t, sfx_click(), 0.6 * g, p)
        elif k == "type":
            tt = t
            while tt < c["end"]:
                bus.add(tt, sfx_key(), 0.9 * g, p + rng.uniform(-0.15, 0.15))
                tt += c.get("every", 0.055) * rng.uniform(0.6, 1.4)
        elif k == "ding":
            bus.add(t, sfx_ding(), 0.5 * g, p)
        elif k == "success":
            bus.add(t, sfx_success(), 0.5 * g, p)
        elif k == "alert":
            bus.add(t, sfx_alert(), 0.6 * g, p)
        elif k == "glitch":
            bus.add(t, sfx_glitch(c.get("dur", 0.35)), 0.6 * g, p)
        elif k == "shimmer":
            bus.add(t, sfx_shimmer(), 0.4 * g, p)
        else:
            raise ValueError(k)
    return reverb(bus.x, 1.2, 0.12, 7000)


def main():
    cues = json.load(open(sys.argv[1]))
    out = sys.argv[2]
    D = cues["duration"]
    m = music(D, cues["markers"]) * 0.5
    s = sfx_track(D, cues["sfx"]) * 2.4
    # duck the music a little under the effects, so clicks and pops read clearly
    env = np.abs(s).max(1)
    k = int(0.06 * SR)
    env = np.convolve(env, np.ones(k) / k, mode="same")
    duck = 1 - 0.35 * np.clip(env / 0.08, 0, 1)
    mix = m * duck[:, None] + s
    # master: gentle glue, soft clip, fade edges, peak at -1 dBFS (loudness is set at the mux)
    mix = np.tanh(mix * 1.1) / np.tanh(1.1)
    n = len(mix)
    fade = int(0.01 * SR)
    mix[:fade] *= np.linspace(0, 1, fade)[:, None]
    end = int(1.5 * SR)
    mix[-end:] *= np.linspace(1, 0, end)[:, None] ** 2
    mix /= np.abs(mix).max() / 0.89
    pcm = (mix * 32767).astype("<i2")
    with wave.open(out, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    print(f"wrote {out}: {n / SR:.1f}s")


if __name__ == "__main__":
    main()
