# Licences des ressources du film

## Son — aucune ressource tierce

| Élément | Origine | Licence |
|---|---|---|
| Musique « Berge » (`audio/music.py`) | composée et synthétisée pour ce film : sinusoïdes, bruit filtré, réverbérations calculées. Aucun échantillon, aucune boucle, aucune banque de sons | propriété du projet Halcyon, MIT |
| Sound design (`audio/sfx.py`) : ambiance, souffles, plongeon, impacts, montée, cliquetis | synthèse, même procédé | MIT |
| Sons d'interface (`ui/select`, `ui/section`, `ui/panel-open`, `ui/menu-open`, `ui/confirm`, `game/open`, `game/select`, `game/launch`, `notify/major`) | identité sonore du launcher, générée par `tools/sound/make_sounds.py` du dépôt | MIT (licence du projet) |

Le motif de cloches ré – la – mi est celui de l'identité sonore du launcher (lancement d'un
jeu, grandes réussites). Aucune musique commerciale n'est utilisée.

## Image

| Élément | Origine | Licence |
|---|---|---|
| Captures de l'interface (`public/captures/`) | Halcyon 1.0.0-alpha.4, capturé pour ce film | MIT (projet Halcyon) |
| Jeux, jaquettes et illustrations visibles dans les captures | fictifs, générés par `tools/demo-covers.py` | MIT |
| Logo (`public/brand/halcyon-logo.svg`, `src/logo.json`) | vectorisation du logo de Halcyon (`crates/halcyon-app/ui/icons/logo.png`) | appartient à l'auteur de Halcyon |
| Police Archivo (`public/fonts/`) | Omnibus-Type, fichiers du launcher | SIL Open Font License 1.1 (`public/fonts/LICENSE-Archivo.txt`) |
| Animations, compositions, code (`src/`, `tools/`, `render.sh`) | écrits pour ce film | MIT |

## Outils

| Outil | Licence |
|---|---|
| [Remotion](https://www.remotion.dev) 4 | Remotion License : gratuite pour les particuliers et les structures de 3 personnes au plus ; au-delà, licence entreprise payante (voir remotion.dev/license) |
| React, TypeScript | MIT, Apache 2.0 |
| numpy, scipy | BSD |
| FFmpeg (libx264) | LGPL / GPL |

Les noms Steam, Epic Games, GOG, Heroic, Legendary, RetroArch, Dolphin, PCSX2, RPCS3,
DuckStation, PPSSPP, Cemu, Ryujinx, melonDS et mGBA sont cités comme sources ou émulateurs
pris en charge ; ils appartiennent à leurs propriétaires. Aucun de leurs logos n'apparaît.
