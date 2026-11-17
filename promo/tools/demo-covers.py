#!/usr/bin/env python3
"""Variante de tools/make_demo_fixture.py pour le film : mêmes jeux fictifs, jaquettes en aplats
graphiques (palettes et compositions fixes). Génère une fausse installation Steam + Epic + dossier standalone pour tester
Halcyon de bout en bout sans vrais jeux (formats de fichiers identiques aux vrais).

Usage : python3 tools/make_demo_fixture.py <dossier_cible> [nombre_de_jeux_steam]
Puis :  HALCYON_DATA_DIR=<dossier_cible>/data cargo run -p halcyon-app
"""
import base64
import json
import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/halcyon-demo").resolve()
COUNT = int(sys.argv[2]) if len(sys.argv) > 2 else 36
FONT = Path(__file__).resolve().parent.parent.parent / "crates/halcyon-app/ui/fonts/Archivo-Bold.ttf"

WORDS_A = ["Ember", "Salt", "Neon", "Hollow", "Iron", "Velvet", "Glass", "Solar", "Rust", "Frost", "Echo",
           "Paper", "Cinder", "Quiet", "Lunar", "Wild", "Static", "Amber", "Tidal", "Copper"]
WORDS_B = ["Protocol", "Wind", "Reverie", "Crown", "Harbor", "Drift", "Garden", "Orbit", "Signal", "Hearts",
           "Legacy", "Frontier", "Lantern", "Tactics", "Odyssey", "Run", "Kingdom", "Atlas", "Requiem", "Shift"]

rng = random.Random(42)


def names(n):
    out = set()
    while len(out) < n:
        name = f"{rng.choice(WORDS_A)} {rng.choice(WORDS_B)}"
        if rng.random() < 0.25:
            name += f" {rng.choice(['II', '3', 'Remastered', 'Zero', '2077'])}"
        out.add(name)
    return sorted(out)


PALETTES = [
    ((22, 52, 58), (38, 92, 96), (231, 196, 140), (246, 236, 214)),
    ((74, 24, 30), (122, 40, 44), (232, 182, 120), (250, 236, 220)),
    ((28, 44, 32), (52, 86, 58), (214, 168, 64), (240, 232, 206)),
    ((30, 34, 46), (58, 66, 88), (232, 110, 84), (238, 232, 226)),
    ((18, 30, 64), (36, 62, 112), (238, 196, 72), (236, 236, 244)),
    ((54, 30, 62), (98, 58, 104), (244, 160, 128), (248, 232, 230)),
    ((58, 56, 36), (96, 94, 62), (222, 210, 170), (246, 242, 226)),
    ((24, 24, 26), (48, 48, 52), (226, 72, 52), (240, 238, 234)),
    ((14, 58, 78), (24, 98, 120), (250, 214, 160), (232, 244, 246)),
    ((86, 48, 20), (140, 82, 36), (244, 214, 150), (250, 242, 228)),
]
DISPLAY = Path(__file__).resolve().parent.parent.parent / "crates/halcyon-app/ui/fonts/ArchivoExpanded-ExtraBold.ttf"


def _scene(d, w, h, r, pal, kind):
    bg, mid, acc, light = pal
    if kind == 0:  # soleil et bandes
        cx, cy, rad = int(w * r.uniform(.3, .7)), int(h * .42), int(min(w, h) * r.uniform(.22, .32))
        d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], fill=acc)
        y = cy
        step = max(6, h // 40)
        k = 1
        while y < cy + rad:
            d.rectangle([0, y, w, y + k], fill=bg)
            y += step; k += max(1, h // 300)
        d.rectangle([0, int(h * .62), w, h], fill=mid)
        for i in range(6):
            yy = int(h * .62) + i * i * max(3, h // 120)
            d.line([0, yy, w, yy], fill=bg, width=max(1, h // 400))
    elif kind == 1:  # crêtes
        cx, cy, rad = int(w * r.uniform(.6, .85)), int(h * .22), int(min(w, h) * .07)
        d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], fill=light)
        for layer, col in enumerate([mid, acc, bg]):
            base = h * (.45 + layer * .14)
            pts = [(0, h)]
            x = 0
            while x <= w:
                pts.append((x, base - r.uniform(0, h * .16)))
                x += w / r.randint(3, 6)
            pts += [(w, base), (w, h)]
            d.polygon(pts, fill=col)
    elif kind == 2:  # tours
        d.rectangle([0, int(h * .78), w, h], fill=mid)
        x = -r.randint(0, w // 10)
        while x < w:
            bw = r.randint(w // 10, w // 5)
            top = int(h * r.uniform(.18, .62))
            d.rectangle([x, top, x + bw - max(2, w // 120), int(h * .78)], fill=mid if r.random() < .5 else (bg[0] + 10, bg[1] + 10, bg[2] + 10))
            for wy in range(top + h // 40, int(h * .76), max(8, h // 30)):
                for wx in range(x + bw // 8, x + bw - bw // 8, max(6, bw // 5)):
                    if r.random() < .22:
                        d.rectangle([wx, wy, wx + max(2, bw // 14), wy + max(2, h // 90)], fill=acc)
            x += bw
    elif kind == 3:  # vagues
        for i in range(9):
            col = [mid, acc, light, bg][i % 4] if i % 2 else mid
            base = h * (.3 + i * .08)
            pts = [(0, h)]
            ph = r.uniform(0, 6.28)
            amp = h * r.uniform(.015, .04)
            import math
            for s in range(0, 41):
                xx = w * s / 40
                pts.append((xx, base + math.sin(ph + s / 3.2) * amp))
            pts += [(w, h)]
            d.polygon(pts, fill=col if i % 3 else bg)
    else:  # diagonale
        d.polygon([(0, h * .35), (w, h * .05), (w, h), (0, h)], fill=mid)
        rad = int(min(w, h) * .2)
        cx, cy = int(w * .3), int(h * .38)
        d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], outline=acc, width=max(4, rad // 6))
        d.rectangle([int(w * .62), int(h * .3), int(w * .62) + max(6, w // 40), h], fill=acc)


def art(path: Path, size, title, seed, logo=False):
    r = random.Random(seed)
    w, h = size
    import zlib
    hsh = zlib.crc32((title + "670").encode())
    pal = PALETTES[hsh // 5 % len(PALETTES)]
    if logo:
        img = Image.new("RGBA", size, (0, 0, 0, 0))
        d = ImageDraw.Draw(img, "RGBA")
        words = title.split(" ")
        lines = [" ".join(words[:2]), " ".join(words[2:])] if len(words) > 2 else [" ".join(words)]
        fs = int(h / (len(lines) * 1.2))
        font = ImageFont.truetype(str(DISPLAY), fs)
        while max(d.textlength(l, font=font) for l in lines) > w * 0.96 and fs > 12:
            fs -= 2
            font = ImageFont.truetype(str(DISPLAY), fs)
        y = (h - len(lines) * fs * 1.1) / 2
        for line in lines:
            d.text((4, y), line, font=font, fill=pal[3] + (255,))
            y += fs * 1.1
        img.save(path)
        return
    img = Image.new("RGB", size, pal[0])
    d = ImageDraw.Draw(img)
    _scene(d, w, h, random.Random(title), pal, hsh % 5)
    noise = Image.effect_noise(size, 28).convert("RGB")
    img = Image.blend(img, noise, 0.06)
    d = ImageDraw.Draw(img)
    if size[0] < size[1]:
        fs = int(w * 0.105)
        font = ImageFont.truetype(str(DISPLAY), fs)
        words = title.split(" ")
        lines, cur = [], ""
        for wd in words:
            t = (cur + " " + wd).strip()
            if d.textlength(t, font=font) > w * .84 and cur:
                lines.append(cur); cur = wd
            else:
                cur = t
        lines.append(cur)
        y = h * .9 - len(lines) * fs * 1.08
        d.rectangle([0, y - fs * .55, w, h], fill=pal[0])
        d.rectangle([int(w * .08), y - fs * .55, int(w * .08) + int(w * .1), y - fs * .55 + max(4, h // 160)], fill=pal[2])
        for line in lines:
            d.text((w * .08, y), line, font=font, fill=pal[3])
            y += fs * 1.08
    img.save(path, quality=90) if path.suffix == ".jpg" else img.save(path)


def main():
    steam = ROOT / "Steam"
    lib2 = ROOT / "SteamLibrary"
    for p in [steam / "steamapps/common", lib2 / "steamapps/common", steam / "appcache/librarycache",
              steam / "config", steam / "userdata/22202/config"]:
        p.mkdir(parents=True, exist_ok=True)
    (steam / "steamapps/libraryfolders.vdf").write_text(
        '"libraryfolders"\n{\n'
        f'\t"0"\n\t{{\n\t\t"path"\t\t"{steam}"\n\t}}\n'
        f'\t"1"\n\t{{\n\t\t"path"\t\t"{lib2}"\n\t}}\n}}\n')
    (steam / "config/loginusers.vdf").write_text(
        '"users"\n{\n\t"76561197960287930"\n\t{\n\t\t"AccountName"\t\t"demo"\n\t\t"MostRecent"\t\t"1"\n\t}\n}\n')

    apps = []
    playtimes = []
    for i, name in enumerate(names(COUNT)):
        appid = 100000 + i * 10
        lib = steam if i % 3 else lib2
        installdir = name.replace(" ", "")
        (lib / "steamapps/common" / installdir).mkdir(exist_ok=True)
        # Quelques jeux « mise à jour requise », un en cours de téléchargement.
        flags = 4 if i % 11 else 6
        extra = ""
        if i == 5:
            flags = 4 | 2 | 1024 | 1048576
            extra = '\t"BytesToDownload"\t\t"2400000000"\n\t"BytesDownloaded"\t\t"900000000"\n'
        build = 9_000_000 + i * 137
        target = build + 12 if flags & 2 else build
        (lib / f"steamapps/appmanifest_{appid}.acf").write_text(
            f'"AppState"\n{{\n\t"appid"\t\t"{appid}"\n\t"name"\t\t"{name}"\n\t"StateFlags"\t\t"{flags}"\n'
            f'\t"installdir"\t\t"{installdir}"\n\t"SizeOnDisk"\t\t"{rng.randint(1, 120) * 1_000_000_000}"\n'
            f'\t"buildid"\t\t"{build}"\n\t"TargetBuildID"\t\t"{target}"\n{extra}}}\n')
        gdir = lib / "steamapps/common" / installdir
        if i % 5 == 0:  # moteurs reconnus et prérequis livrés avec le jeu
            (gdir / "UnityPlayer.dll").write_bytes(b"")
            (gdir / "_CommonRedist/DirectX").mkdir(parents=True, exist_ok=True)
            (gdir / "_CommonRedist/vcredist").mkdir(parents=True, exist_ok=True)
        elif i % 5 == 1:
            (gdir / "Engine/Binaries").mkdir(parents=True, exist_ok=True)
        cache = steam / "appcache/librarycache" / str(appid)
        if i % 7 != 3:  # quelques jeux sans jaquette : jaquette générée par Halcyon
            cache.mkdir(exist_ok=True)
            art(cache / "library_600x900.jpg", (600, 900), name, appid)
            if i % 2 == 0:
                art(cache / "library_hero.jpg", (1920, 620), name, appid + 1)
                art(cache / "logo.png", (640, 240), name, appid + 2, logo=True)
        if i % 4 == 0:
            playtimes.append(f'\t\t\t\t\t"{appid}"\n\t\t\t\t\t{{\n\t\t\t\t\t\t"LastPlayed"\t\t"{1793600000 - i * 86400 * 2}"\n'
                             f'\t\t\t\t\t\t"Playtime"\t\t"{rng.randint(30, 9000)}"\n\t\t\t\t\t}}\n')
        apps.append(name)
    (steam / "userdata/22202/config/localconfig.vdf").write_text(
        '"UserLocalConfigStore"\n{\n\t"Software"\n\t{\n\t\t"Valve"\n\t\t{\n\t\t\t"Steam"\n\t\t\t{\n\t\t\t\t"apps"\n\t\t\t\t{\n'
        + "".join(playtimes) + "\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n")

    # Jeu Linux natif de test : enregistre ses arguments et son environnement, puis dure 5 s.
    owl = ROOT / "Jeux/Night Owl"
    owl.mkdir(parents=True, exist_ok=True)
    script = owl / "nightowl.sh"
    script.write_text('#!/bin/sh\necho "$@" > "$(dirname "$0")/args.txt"\n'
                      'env | grep "^HALCYON_" > "$(dirname "$0")/env.txt"\npwd > "$(dirname "$0")/pwd.txt"\nsleep 5\n')
    script.chmod(0o755)

    # Epic Games
    epic = ROOT / "Epic/Data"
    (epic / "Manifests").mkdir(parents=True, exist_ok=True)
    (epic / "Catalog").mkdir(parents=True, exist_ok=True)
    catalog = []
    for i, name in enumerate(["Starfall Atelier", "Mosaic Tide", "Gravel Saints", "Nightjar"]):
        loc = ROOT / "EpicGames" / name.replace(" ", "")
        loc.mkdir(parents=True, exist_ok=True)
        app, item = f"App{i}", f"item{i}"
        (epic / "Manifests" / f"{i:02X}.item").write_text(json.dumps({
            "FormatVersion": 0, "DisplayName": name, "AppName": app, "MainGameAppName": app,
            "CatalogNamespace": f"ns{i}", "CatalogItemId": item, "InstallLocation": str(loc),
            "LaunchExecutable": f"{name.replace(' ', '')}.exe", "AppCategories": ["public", "games"],
            "bIsIncompleteInstall": False, "InstallSize": 9_000_000_000}))
        catalog.append({"id": item, "title": name, "developer": f"Studio {name.split()[0]}",
                        "description": f"{name} est un jeu de démonstration généré pour tester Halcyon : "
                                       "description lue depuis le cache du catalogue Epic, hors ligne.",
                        "keyImages": [{"type": "DieselGameBoxTall", "url": f"https://example.invalid/{item}.jpg"}]})
    (epic / "Catalog/catcache.bin").write_bytes(base64.b64encode(json.dumps(catalog).encode()))

    # Dossier standalone
    games = ROOT / "Jeux"
    for folder, exe in [("Pixel_Harvest-v1.4 [DRM-Free]", "PixelHarvest.exe"), ("Quiet Lantern", "QuietLantern.exe"),
                        ("Tidal Kingdom (2024)", "TidalKingdom.exe")]:
        d = games / folder
        d.mkdir(parents=True, exist_ok=True)
        (d / exe).write_bytes(b"MZ" + b"\0" * 200_000)
        (d / "unins000.exe").write_bytes(b"MZ" + b"\0" * 900_000)
        (d / "UnityCrashHandler64.exe").write_bytes(b"MZ" + b"\0" * 1_200_000)

    # Émulation : ROM factices (octets aléatoires) et faux émulateur (script).
    roms = ROOT / "ROMs"
    for platform, names_ext in {
        "gba": [("Lunar Tactics Advance (Europe)", "gba"), ("Pocket Odyssey (USA)", "gba")],
        "snes": [("Solar Drift (Japan)", "sfc"), ("Copper Kingdom (USA) [!]", "smc")],
        "ps1": [("Static Requiem (Disc 1)", "cue")],
    }.items():
        (roms / platform).mkdir(parents=True, exist_ok=True)
        for name, ext in names_ext:
            (roms / platform / f"{name}.{ext}").write_bytes(bytes(rng.randrange(256) for _ in range(4096)))
    emu = ROOT / "emu"
    emu.mkdir(exist_ok=True)
    fake = emu / "mgba-qt"
    fake.write_text("#!/bin/sh\necho \"faux émulateur : $@\" >> \"$(dirname \"$0\")/launches.log\"\nsleep 2\n")
    fake.chmod(0o755)

    data = ROOT / "data"
    data.mkdir(exist_ok=True)
    (data / "settings.json").write_text(json.dumps({
        "steam_path": str(steam), "epic_manifests_path": str(epic / "Manifests"),
        "scan_folders": [str(games)], "online_artwork": False, "online_metadata": False,
        "sync_achievements_on_startup": False, "check_updates": False, "minimize_on_launch": False,
        "emulators": [{"id": "mgba", "path": str(fake), "extra_args": "", "fullscreen": True, "enabled": True}],
        "rom_folders": [{"platform": p, "path": str(roms / p)} for p in ["gba", "snes", "ps1"]]}, indent=2))
    print(f"Démo prête dans {ROOT} : {len(apps)} jeux Steam, 4 Epic, 3 standalone, 5 ROM")


if __name__ == "__main__":
    main()
