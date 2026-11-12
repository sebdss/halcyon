#!/usr/bin/env python3
"""Génère une fausse installation Steam + Epic + dossier standalone pour tester
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
FONT = Path(__file__).resolve().parent.parent / "crates/halcyon-app/ui/fonts/Inter-Bold.ttf"

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


def art(path: Path, size, title, seed, logo=False):
    r = random.Random(seed)
    w, h = size
    if logo:
        img = Image.new("RGBA", size, (0, 0, 0, 0))
    else:
        h1, h2 = r.random(), r.random()
        img = Image.new("RGB", size)
        import colorsys
        c1 = tuple(int(v * 255) for v in colorsys.hsv_to_rgb(h1, 0.65, 0.55))
        c2 = tuple(int(v * 255) for v in colorsys.hsv_to_rgb(h2, 0.75, 0.18))
        px = img.load()
        for y in range(h):
            t = y / h
            row = tuple(int(c1[i] * (1 - t) + c2[i] * t) for i in range(3))
            for x in range(w):
                px[x, y] = row
        d = ImageDraw.Draw(img, "RGBA")
        for _ in range(7):
            cx, cy, rad = r.randint(0, w), r.randint(0, h), r.randint(w // 8, w // 2)
            col = tuple(int(v * 255) for v in colorsys.hsv_to_rgb(r.random(), 0.5, 0.9)) + (r.randint(25, 70),)
            d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], fill=col)
        img = img.filter(ImageFilter.GaussianBlur(radius=max(2, w // 60)))
        d = ImageDraw.Draw(img, "RGBA")
        for _ in range(3):
            y = r.randint(h // 3, h)
            d.polygon([(0, h), (r.randint(0, w), y), (w, h)], fill=(0, 0, 0, 80))
    d = ImageDraw.Draw(img, "RGBA")
    if logo:
        # Logo : titre sur une ou deux lignes, aligné à gauche, ajusté au cadre.
        words = title.upper().split(" ")
        lines = [" ".join(words[:2]), " ".join(words[2:])] if len(words) > 2 else [" ".join(words)]
        fs = int(h / (len(lines) * 1.25))
        font = ImageFont.truetype(str(FONT), fs)
        while max(d.textlength(l, font=font) for l in lines) > w * 0.96 and fs > 12:
            fs -= 2
            font = ImageFont.truetype(str(FONT), fs)
        y = (h - len(lines) * fs * 1.15) / 2
        for line in lines:
            d.text((4, y), line, font=font, fill=(255, 255, 255, 245))
            y += fs * 1.15
    elif size[0] < size[1]:
        fs = int(w * 0.13)
        font = ImageFont.truetype(str(FONT), fs)
        y = h * 0.62
        for line in title.upper().split(" "):
            tw = d.textlength(line, font=font)
            d.text(((w - tw) / 2, y), line, font=font, fill=(255, 255, 255, 240))
            y += fs * 1.1
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
