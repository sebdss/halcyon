# Refaire les captures

Les captures de `public/captures/` viennent de Halcyon 1.0.0-alpha.4 (archive Linux publiée
sur GitHub), avec la bibliothèque Steam d'exemple de `tools/real-games.py`.

```sh
python3 tools/real-games.py /home/sebi        # chemin visible dans la fiche du jeu
Xvfb :94 -screen 0 2880x1800x24 &
# sans proxy ni réseau : la fiche n'affiche pas d'actualités dans une langue imprévue
env -u HTTPS_PROXY DISPLAY=:94 HALCYON_DATA_DIR=/home/sebi/data SLINT_SCALE_FACTOR=2 ./halcyon &
# navigation : xdotool (coordonnées logiques × 2), capture : import -window root <fichier>.png
```

| Fichier | Écran |
|---|---|
| `accueil` | Accueil (bloc « Reprendre ») |
| `bibliotheque`, `grille-m` | Bibliothèque en grille, jaquettes S et M |
| `liste`, `carrousel`, `immersive` | Bibliothèque en liste, carrousel, immersive |
| `fiche` | Fiche de Cyberpunk 2077 |
| `console-0` … `console-6` | Mode console, sélection avancée d'un jeu à chaque image (flèche droite) |
| `mises-a-jour`, `emulateurs` | Outils › Mises à jour, Émulateurs |
| `que-jouer`, `statistiques` | Activité › Que jouer ?, Statistiques |
| `apparence` | Système › Apparence |

Conversion : JPEG qualité 92, sans sous-échantillonnage des couleurs.

## Version anglaise (`public/captures-en/`)

Mêmes écrans, mêmes cadrages et mêmes noms de fichiers, pris avec la version traduite du
launcher (branche `i18n`, compilée depuis les sources) et l'interface en anglais :

```sh
python3 tools/real-games.py /home/sebi
# langue forcée dans les réglages : "language": "en-US" dans /home/sebi/data/settings.json
env -u HTTPS_PROXY DISPLAY=:94 HALCYON_DATA_DIR=/home/sebi/data SLINT_SCALE_FACTOR=2 \
  SLINT_BACKEND=winit-software HOME=/home/sebi ./target/debug/halcyon &
```

Avant chaque capture, la souris est placée sur une zone neutre de la barre du haut pour
qu'aucune info-bulle n'apparaisse. Les mêmes fichiers alimentent les captures anglaises du
site (`site/assets/img/screens/en/`).
