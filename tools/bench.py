#!/usr/bin/env python3
"""Mesures de performance reproductibles (Linux, X11 / Xvfb).

Usage : python3 tools/bench.py <binaire> <dossier_bibliotheque> [--renderer software|femtovg]
Le dossier est créé par tools/make_big_library.py. Les données sont copiées dans un
dossier temporaire : chaque mesure part du même état (1er lancement puis 2e lancement).
Mesures : démarrage jusqu'à l'interface, analyse, RAM (accueil, après défilement de la
bibliothèque), CPU au repos, taille de la base.
"""
import json, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

binary, lib = sys.argv[1], Path(sys.argv[2])
renderer = "software"
if "--renderer" in sys.argv:
    renderer = sys.argv[sys.argv.index("--renderer") + 1]
env = dict(os.environ, DISPLAY=os.environ.get("DISPLAY", ":99"))
env["SLINT_BACKEND"] = "winit-software" if renderer == "software" else "winit-femtovg"

def rss(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS"):
            return int(line.split()[1]) // 1024
    return 0

def ticks(pid):
    total = 0
    for t in os.listdir(f"/proc/{pid}/task"):
        f = open(f"/proc/{pid}/task/{t}/stat").read().rsplit(")", 1)[1].split()
        total += int(f[11]) + int(f[12])
    return total

def xdo(*a):
    subprocess.run(["xdotool", *a], env=env, capture_output=True)

def run(data, label, scroll=True):
    logdir = data / "logs"
    shutil.rmtree(logdir, ignore_errors=True)
    t0 = time.time()
    p = subprocess.Popen([binary], env=dict(env, HALCYON_DATA_DIR=str(data)), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ready = scan = None
    deadline = time.time() + 60
    while time.time() < deadline and (ready is None or scan is None):
        time.sleep(0.1)
        for f in logdir.glob("*.log") if logdir.exists() else []:
            text = f.read_text(errors="ignore")
            m = re.search(r"interface prête en (\d+) ms", text)
            if m:
                ready = int(m.group(1))
            if "Émulation :" in text or "Steam : +" in text and "Standalone" in text:
                scan = time.time() - t0
    time.sleep(6)
    out = {"run": label, "ready_ms": ready, "scan_done_s": round(scan, 2) if scan else None, "rss_home_mb": rss(p.pid)}
    t1 = ticks(p.pid); time.sleep(10); out["idle_cpu_pct"] = round((ticks(p.pid) - t1) / 10 / 100 * 100, 2)
    if scroll:
        w = subprocess.run(["xdotool", "search", "--name", "^Halcyon$"], env=env, capture_output=True, text=True).stdout.split()
        if w:
            xdo("windowsize", w[0], "1600", "1000"); xdo("windowmove", w[0], "0", "0"); xdo("windowfocus", "--sync", w[0])
            time.sleep(0.5)
            xdo("key", "ctrl+2"); time.sleep(1.5)
            xdo("mousemove", "900", "600")
            t = time.time()
            for _ in range(150):
                xdo("click", "5")
            out["scroll_150_s"] = round(time.time() - t, 2)
            time.sleep(4)
            out["rss_after_scroll_mb"] = rss(p.pid)
    p.terminate(); p.wait(10)
    out["db_mb"] = round(sum(f.stat().st_size for f in data.glob("library.db*")) / 1e6, 2)
    return out

work = Path(tempfile.mkdtemp(prefix="halcyon-bench-"))
shutil.copytree(lib / "data", work / "data")
s = json.loads((work / "data/settings.json").read_text())
s["steam_path"] = str(lib / "Steam")
(work / "data/settings.json").write_text(json.dumps(s))
results = [run(work / "data", "1er lancement (base vide, analyse)"), run(work / "data", "2e lancement (base remplie, miniatures en cache)")]
print(json.dumps({"binary": binary, "renderer": renderer, "results": results}, ensure_ascii=False, indent=2))
shutil.rmtree(work, ignore_errors=True)
