#!/usr/bin/env python3
"""Grande bibliothèque Steam factice pour les mesures de performance.

Usage : python3 tools/make_big_library.py <dossier> [nombre=5000]
Crée <dossier>/Steam (manifestes + jaquettes pour 80 % des jeux) et <dossier>/data/settings.json.
"""
import json, random, sys
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(sys.argv[1]).resolve()
count = int(sys.argv[2]) if len(sys.argv) > 2 else 5000
rng = random.Random(7)
steam = root / "Steam"
apps = steam / "steamapps"
cache = steam / "appcache" / "librarycache"
apps.mkdir(parents=True, exist_ok=True)
(steam / "steamapps" / "common").mkdir(exist_ok=True)
(apps / "libraryfolders.vdf").write_text(f'"libraryfolders"\n{{\n\t"0"\n\t{{\n\t\t"path"\t\t"{steam}"\n\t}}\n}}\n')
syll = ["ka", "ro", "mi", "ten", "vor", "lux", "pha", "drim", "sol", "nex", "qua", "bel", "tor", "ix", "una", "zer"]
for i in range(count):
    appid = 200000 + i
    name = " ".join("".join(rng.choice(syll) for _ in range(rng.randint(2, 3))).capitalize() for _ in range(rng.randint(1, 3)))
    name = f"{name} {i}"
    inst = f"Game{i}"
    (apps / f"appmanifest_{appid}.acf").write_text(
        f'"AppState"\n{{\n\t"appid"\t\t"{appid}"\n\t"name"\t\t"{name}"\n\t"StateFlags"\t\t"4"\n'
        f'\t"installdir"\t\t"{inst}"\n\t"SizeOnDisk"\t\t"{rng.randint(10**8, 10**11)}"\n\t"buildid"\t\t"{rng.randint(1000, 99999)}"\n}}\n')
    if rng.random() < 0.8:
        d = cache / str(appid)
        d.mkdir(parents=True, exist_ok=True)
        c1 = tuple(rng.randint(20, 200) for _ in range(3))
        c2 = tuple(rng.randint(20, 200) for _ in range(3))
        img = Image.new("RGB", (300, 450), c1)
        dr = ImageDraw.Draw(img)
        dr.rectangle([0, 225, 300, 450], fill=c2)
        dr.ellipse([60, 120, 240, 300], fill=tuple((a + b) // 2 for a, b in zip(c1, c2)))
        img.save(d / "library_600x900.jpg", quality=80)
data = root / "data"
data.mkdir(exist_ok=True)
(data / "settings.json").write_text(json.dumps({
    "steam_path": str(steam), "epic_enabled": False, "online_artwork": False, "online_metadata": False,
    "check_updates": False, "sync_achievements_on_startup": False, "offline_forced": True,
}, indent=2))
print(f"{count} jeux dans {root}")
