#!/usr/bin/env python3
"""Bibliothèque de démonstration pour les collections, l'étagère et la désinstallation.

Reprend `make_demo_fixture.py` (faux Steam, Epic, dossier de jeux autonomes, ROM) avec des
séries reconnaissables (Resident Evil, Sonic) pour tester les suggestions de séries, et un
nombre de jeux réglable pour tester l'étagère sur une grande bibliothèque.

Usage : python3 tools/make_shelf_fixture.py <dossier_cible> [nombre_de_jeux_steam]
Puis :  HALCYON_DATA_DIR=<dossier_cible>/data cargo run -p halcyon-app
"""
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
target = sys.argv[1] if len(sys.argv) > 1 else "/tmp/halcyon-shelf"
count = int(sys.argv[2]) if len(sys.argv) > 2 else 40
sys.argv = [sys.argv[0], target, str(count)]

import make_demo_fixture as demo  # noqa: E402  (lit sys.argv à l'import)

# La police Inter a été remplacée par Archivo (1.0) : visuels générés avec Archivo.
demo.FONT = HERE.parent / "crates/halcyon-app/ui/fonts/Archivo-Bold.ttf"

SERIES = [
    "Resident Evil 2", "Resident Evil 3", "Resident Evil 4", "Resident Evil Village",
    "Sonic Mania", "Sonic Frontiers", "Sonic Generations", "Sonic Colors Ultimate",
    "Celeste", "Hades",
]


def names(n):
    out = list(SERIES[: min(n, len(SERIES))])
    rng = random.Random(7)
    seen = set(out)
    words_a, words_b = demo.WORDS_A, demo.WORDS_B
    i = 0
    while len(out) < n:
        name = f"{rng.choice(words_a)} {rng.choice(words_b)}"
        if name in seen:
            i += 1
            name = f"{name} {i}"
        seen.add(name)
        out.append(name)
    return sorted(out)


demo.names = names

if __name__ == "__main__":
    demo.main()
