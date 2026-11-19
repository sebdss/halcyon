# Tests de la 1.0.0-alpha.6

Machine : conteneur Linux x86_64 (Ubuntu 24.04), 2 cœurs, 7,8 Go + 10 Go d'échange, sans
GPU ni manette ; Rust 1.97, Slint 1.18 ; affichage Xvfb (X11) piloté par `xdotool`, captures
`import`. Bibliothèques : `tools/make_shelf_fixture.py` (53 jeux : Steam, Epic, jeux
autonomes, ROM SNES/GBA/PS1, séries Resident Evil et Sonic) et la même avec 513 jeux.

## Tests automatiques ajoutés

| Module | Ce qui est vérifié |
|---|---|
| `halcyon-core::collections` (10) | séries numérotées et sous-titrées, séries d'un mot, marques ™, mots génériques et éditions écartés, plus long début commun, sous-séries rattachées, refus et collections existantes, jeux déjà rangés, ordre manuel puis alphabétique, déplacement borné, couleurs |
| `halcyon-core::uninstall` (4) | voie de chaque source (Steam, Legendary, Heroic, GOG Galaxy, Ubisoft sous Windows / Linux, jeu autonome), ROM et jeux manuels jamais supprimés, noms Legendary ou appid suspects refusés, **garde-fous de chemin** (racine, premier niveau, dossier enregistré lui-même, trop profond, hors dossiers, `..`, relatif, dossier contenant des ROM ou un autre jeu, casse) |
| `halcyon-storage` (1) | base v6 avec collections → v7 sans perte, ordre manuel, ajout en fin, style, renommage (nom pris refusé), ordre des collections, « non installé » qui garde les collections |
| `halcyon-integrations::trash` (1) | corbeille freedesktop : déplacement, `.trashinfo` encodé, noms uniques, dossier absent |
| `halcyon-app` (3) | fenêtre glissante de l'étagère (bornes, un emplacement par indice), nom proposé par la recherche, bouton de manette dédié sans conflit avec les autres |

`cargo test --workspace -j 1` : 283 tests, tous réussis. `cargo clippy --workspace --all-targets -- -D warnings` : aucun avertissement. `i18n.py lint` : 0 texte hors
traduction ; `cargo test -p halcyon-i18n` (catalogues) : réussi, 6 langues complètes.

## Scénarios à l'écran (Linux)

| Scénario | Résultat |
|---|---|
| `Ctrl+Espace`, flèches, `Maj+Espace` (plage), `Ctrl`-clic sur une jaquette | 8 jeux cochés (Steam, SNES, Epic), barre d'actions affichée |
| « Ajouter à une collection… » → nouvelle collection | collection mixte de 8 jeux créée, cases décochées |
| Création guidée « sonic », un jeu décoché | collection de 3 jeux, nom proposé |
| Page Collections : suggestion « Resident Evil » acceptée, `Ctrl+↑` ×2, couleur | ordre et couleur enregistrés, retrouvés après changement de collection |
| Mode console, `V` : étagère | Récents, collection, Resident Evil, sonic, Tous ; plaques colorées ; tranches avec titres tournés |
| Étagère d'une collection de 3–4 jeux, bibliothèque de 513 jeux (300 crans) | fluide, mémoire bornée (voir PERFORMANCES.md) |
| Étagère vide (bibliothèque sans jeux) | message « L'étagère est vide… », plaque et compteur masqués (corrigé : la plaque affichait « 0 » et « 1 / 0 ») |
| Jeux sans jaquette | jaquette générée (initiales, couleurs) sur la face et l'aperçu |
| Mode Performance | jaquettes à plat, sans tranche ni ombre |
| Texte à 130 % | plaque, titres et tranches agrandis (marque de la source élidée) |
| 800 × 560, 3840 × 2160 | même mise en page à l'échelle ; jaquette du jeu choisi nette en 4K |
| Rendu logiciel (`SLINT_BACKEND=winit-software`) | initiales empilées sur les tranches |
| Désinstaller un jeu autonome (menu contextuel, clavier) | focus sur Annuler, confirmation, dossier dans `~/.local/share/Trash` avec `.trashinfo`, jeu « Non installé », `game.uninstall` dans `security.log`, entrée du journal |
| Désinstaller un jeu Steam en mode console (Select › Désinstaller…) | dialogue à l'échelle console, `steam://uninstall/…` transmis au système, « confiée à Steam » |
| Outils › Stockage : « Désinstaller… » puis Échap | dialogue, puis « annulée » au journal |
| Interface en anglais | textes nouveaux traduits (étagère, collections, désinstallation) |

## Non vérifié (Windows, matériel)

- Corbeille Windows (PowerShell, `Microsoft.VisualBasic.FileIO`), *Applications* de Windows,
  ouverture de Steam / launcher Epic / GOG Galaxy / Battle.net sur leur page.
- `legendary uninstall` avec un vrai jeu (le flux est testé jusqu'au lancement de la
  commande ; la réponse `y` est envoyée sur l'entrée standard).
- « Sauvegarder avant » avec un jeu dont les emplacements de sauvegarde sont associés (le
  code réutilise la sauvegarde existante ; non rejoué ici faute d'emplacement associé).
- Manette réelle : R3 et la navigation dans le dialogue passent par les mêmes actions que le
  clavier, essayées ici au clavier.
- Fluidité sur une vraie carte graphique (mesures faites sans GPU).
