#!/usr/bin/env python3
"""Bibliothèque de démonstration avec de vrais jeux Steam, pour les captures du film et du site.

Télécharge les visuels de bibliothèque officiels de chaque jeu (jaquette 600 × 900, bandeau
1920 × 620, logo) depuis le CDN de Steam, puis crée une fausse installation Steam aux formats
réels (manifestes, librarycache, temps de jeu) que Halcyon détecte comme une vraie.
Les visuels appartiennent à leurs éditeurs : ils ne sont pas versionnés, seulement téléchargés.

Usage : python3 tools/real-games.py <dossier_cible>
Puis :  HALCYON_DATA_DIR=<dossier_cible>/data halcyon
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/halcyon-real").resolve()
CACHE = Path(os.environ.get("HALCYON_ART_CACHE", ROOT.parent / "steam-art"))
CDN = ["https://cdn.cloudflare.steamstatic.com/steam/apps", "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps"]

# (appid, nom, heures de jeu, jours depuis la dernière partie, taille en Go, mise à jour en attente)
GAMES = [
    (730, "Counter-Strike 2", 1284, 0, 38, False),
    (1245620, "Elden Ring", 212, 2, 60, False),
    (1091500, "Cyberpunk 2077", 146, 4, 72, True),
    (1086940, "Baldur's Gate 3", 188, 7, 122, False),
    (2767030, "Marvel Rivals", 97, 9, 70, True),
    (553850, "Helldivers 2", 64, 12, 80, False),
    (570, "Dota 2", 920, 15, 45, False),
    (1938090, "Call of Duty", 77, 21, 140, True),
    (3240220, "Grand Theft Auto V Enhanced", 301, 26, 110, False),
    (252950, "Rocket League", 455, 33, 25, False),
    (1172470, "Apex Legends", 133, 41, 75, False),
    (1174180, "Red Dead Redemption 2", 118, 58, 121, False),
    (2358720, "Black Myth: Wukong", 54, 75, 130, False),
    (292030, "The Witcher 3: Wild Hunt", 160, 120, 50, False),
    (1030300, "Hollow Knight: Silksong", 0, 0, 8, False),
]
FILES = {"library_600x900.jpg": "library_600x900_2x.jpg", "library_hero.jpg": "library_hero.jpg", "logo.png": "logo.png"}


def fetch(appid):
    d = CACHE / str(appid)
    d.mkdir(parents=True, exist_ok=True)
    for name in set(FILES.values()):
        if (d / name).exists():
            continue
        for base in CDN:
            if subprocess.run(["curl", "-sf", "--max-time", "30", "-o", str(d / name), f"{base}/{appid}/{name}"]).returncode == 0:
                break
        else:
            sys.exit(f"visuel introuvable : {appid}/{name}")
    return d


def main():
    steam = ROOT / "Steam"
    for p in [steam / "steamapps/common", steam / "appcache/librarycache", steam / "config", steam / "userdata/22202/config"]:
        p.mkdir(parents=True, exist_ok=True)
    (steam / "steamapps/libraryfolders.vdf").write_text(
        '"libraryfolders"\n{\n' f'\t"0"\n\t{{\n\t\t"path"\t\t"{steam}"\n\t}}\n}}\n')
    (steam / "config/loginusers.vdf").write_text(
        '"users"\n{\n\t"76561197960287930"\n\t{\n\t\t"AccountName"\t\t"demo"\n\t\t"MostRecent"\t\t"1"\n\t}\n}\n')
    now = int(time.time())
    played = []
    for i, (appid, name, hours, days, size, update) in enumerate(GAMES):
        installdir = "".join(c for c in name if c.isalnum())
        (steam / "steamapps/common" / installdir).mkdir(exist_ok=True)
        flags = 6 if update else 4
        extra = ""
        if appid == 1091500:  # une mise à jour en cours de téléchargement
            flags = 4 | 2 | 1024 | 1048576
            extra = '\t"BytesToDownload"\t\t"8400000000"\n\t"BytesDownloaded"\t\t"3100000000"\n'
        build = 18_000_000 + i * 4111
        target = build + 37 if flags & 2 else build
        (steam / f"steamapps/appmanifest_{appid}.acf").write_text(
            f'"AppState"\n{{\n\t"appid"\t\t"{appid}"\n\t"name"\t\t"{name}"\n\t"StateFlags"\t\t"{flags}"\n'
            f'\t"installdir"\t\t"{installdir}"\n\t"SizeOnDisk"\t\t"{size * 1_000_000_000}"\n'
            f'\t"buildid"\t\t"{build}"\n\t"TargetBuildID"\t\t"{target}"\n{extra}}}\n')
        src = fetch(appid)
        cache = steam / "appcache/librarycache" / str(appid)
        cache.mkdir(exist_ok=True)
        for dst, name_ in FILES.items():
            (cache / dst).write_bytes((src / name_).read_bytes())
        if hours:
            last = now - days * 86400 - 3600 * (i % 5)
            played.append(f'\t\t\t\t\t"{appid}"\n\t\t\t\t\t{{\n\t\t\t\t\t\t"LastPlayed"\t\t"{last}"\n'
                          f'\t\t\t\t\t\t"Playtime"\t\t"{hours * 60 + (i * 7) % 60}"\n\t\t\t\t\t}}\n')
    (steam / "userdata/22202/config/localconfig.vdf").write_text(
        '"UserLocalConfigStore"\n{\n\t"Software"\n\t{\n\t\t"Valve"\n\t\t{\n\t\t\t"Steam"\n\t\t\t{\n\t\t\t\t"apps"\n\t\t\t\t{\n'
        + "".join(played) + "\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n")
    data = ROOT / "data"
    data.mkdir(exist_ok=True)
    (data / "settings.json").write_text(json.dumps({
        "steam_path": str(steam), "online_artwork": False, "online_metadata": False,
        "sync_achievements_on_startup": False, "check_updates": False, "minimize_on_launch": False}, indent=2))
    print(f"Bibliothèque prête dans {ROOT} : {len(GAMES)} jeux Steam")


if __name__ == "__main__":
    main()
