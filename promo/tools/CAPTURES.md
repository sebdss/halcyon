# Refaire les captures

Les captures de `public/captures/` viennent de Halcyon 1.0.0-alpha.4 (archive Linux publiée
sur GitHub), avec la bibliothèque de démonstration de `tools/demo-covers.py`.

```sh
python3 tools/demo-covers.py /tmp/hdemo
Xvfb :94 -screen 0 2880x1800x24 &
DISPLAY=:94 HALCYON_DATA_DIR=/tmp/hdemo/data SLINT_SCALE_FACTOR=2 ./halcyon &
# navigation : xdotool (coordonnées logiques × 2), capture : import -window root <fichier>.png
```

| Fichier | Écran |
|---|---|
| `accueil` | Accueil (bloc « Reprendre ») |
| `bibliotheque`, `grille-m` | Bibliothèque en grille, jaquettes S et M |
| `liste`, `carrousel`, `immersive` | Bibliothèque en liste, carrousel, immersive |
| `fiche` | Fiche de Copper Odyssey |
| `console-0` … `console-6` | Mode console, sélection avancée d'un jeu à chaque image (flèche droite) |
| `mises-a-jour`, `emulateurs` | Outils › Mises à jour, Émulateurs |
| `que-jouer`, `statistiques` | Activité › Que jouer ?, Statistiques |
| `apparence` | Système › Apparence |

Conversion : JPEG qualité 92, sans sous-échantillonnage des couleurs.
