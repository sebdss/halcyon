# Halcyon — film promotionnel

Film de lancement de 57 s : 16:9, 1920 × 1080, 60 i/s, H.264, son AAC 48 kHz (−14 LUFS).
Tout est reproductible depuis ce dossier : l'image avec [Remotion](https://www.remotion.dev)
(React), le son avec Python (numpy, scipy), l'assemblage avec FFmpeg.

```sh
npm ci                  # Remotion, React, TypeScript
sh render.sh            # bande-son + image + assemblage → out/
sh render.sh vertical   # déclinaison 9:16 (voir plus bas)
npm run studio          # prévisualisation et montage interactifs (Remotion Studio)
```

Prérequis : Node 18+, Python 3 avec `numpy` et `scipy`, FFmpeg, Chrome ou Chromium
(`REMOTION_CHROME=/chemin/vers/chrome` si besoin). Rendu complet : environ 10 minutes sur
deux cœurs.

## Sorties

| Fichier | Contenu |
|---|---|
| `out/halcyon-promo.mp4` | film final, musique et sound design |
| `out/halcyon-promo-sans-musique.mp4` | même image, sound design seul (ambiance, souffles, impacts, sons du launcher) |
| `out/video-sans-son.mp4` | image seule |
| `audio/out/*.wav` | bande-son mixée, version sans musique, musique seule |
| `audio/build/stems/*.wav` | pistes séparées de la musique (batterie, basse, harmonie, cloches) |

## Organisation

```
src/timeline.json      découpage unique : scènes, tempo (120 BPM), repères sonores
src/Root.tsx           compositions « Promo » (16:9) et « PromoVertical » (9:16)
src/Promo.tsx          enchaînement des scènes + piste audio
src/scenes/            une scène par séquence (Identity, Discover, Sources, Views, Detail,
                       Console, Tools, Emulation, Immersion, Play, Signature)
src/components/        Logo animé, caméra et calques sur captures (Layer), titres (Type),
                       bandeaux et repères (Lower), bouton Jouer (PlayButton), polices
src/lib/               jetons du design system Berge, temps et courbes, mise en page
src/logo.json          logo vectorisé (tuile, oiseau, pattes, œil)
public/captures/       captures réelles de Halcyon 1.0.0-alpha.4 (2880 × 1800)
public/brand/          logo vectoriel (SVG)
public/fonts/          Archivo, fichiers du launcher (SIL OFL)
audio/synth.py         synthèse : cloches, pincés, nappes, batterie, réverbérations…
audio/music.py         musique originale « Berge »
audio/sfx.py           sound design, calé sur src/timeline.json
audio/mix.py, build.sh mixage, sonie et versions
tools/stills.mjs       rendu d'images clés pour vérifier une scène sans tout rendre
render.sh              chaîne complète
```

Le tempo pilote tout : une mesure = 2 s = 120 images. Chaque scène commence sur un premier
temps, les coupes et les mouvements tombent sur les temps (`BEAT` = 30 images dans
`src/lib/time.ts`). Pour déplacer une scène ou un son, modifier `src/timeline.json` puis
relancer `render.sh`.

## Découpage

| Temps | Scène | Contenu |
|---|---|---|
| 0–10 s | Identité | ligne d'eau, perche, « Immobile au-dessus de l'eau, puis un geste net. », plongeon orange, naissance du logo, nom |
| 10–18 s | Découverte | titre sur les temps, l'accueil entre en 3D, les panneaux se détachent, approche de « Reprendre » et de Jouer |
| 18–22 s | Sources | bobine Steam → Émulateurs, la bibliothèque se remplit jaquette par jaquette |
| 22–26 s | Vues | grille, liste, carrousel, immersive, volets à perche |
| 26–30 s | Fiche | plan serré puis recul, deux repères qui suivent l'interface |
| 30–34 s | Mode console | carton, puis sept captures successives de la navigation à la manette |
| 34–38 s | Outils | travelling sur Mises à jour, Que jouer ?, Statistiques, Apparence |
| 38–40 s | Émulation | dix émulateurs reconnus, un par double-croche |
| 40–44 s | Immersion | mur des vrais écrans en plongée, « Natif. Léger. Hors ligne. » |
| 44–48 s | Élan | tout s'éteint sauf Jouer, focus, pression, l'orange envahit l'image |
| 48–57 s | Signature | impact, logo, nom, phrase, Windows · Linux, invitation |

## Fidélité au produit

- Chaque écran vient du logiciel réel : Halcyon 1.0.0-alpha.4 (binaire Linux publié sur
  GitHub), lancé sous Xvfb avec un facteur d'échelle 2. Aucun écran n'est redessiné ; les
  mouvements, découpes et profondeurs sont appliqués aux captures elles-mêmes.
- La bibliothèque montrée est celle de démonstration du dépôt (`tools/make_demo_fixture.py`) :
  jeux et jaquettes fictifs. Pour ce film, les jaquettes de démonstration ont été générées en
  aplats graphiques (variante du script, dans `tools/demo-covers.py`).
- Seuls deux éléments sont dessinés : le logo (vectorisé fidèlement depuis
  `crates/halcyon-app/ui/icons/logo.png`) et le bouton orange, reproduit d'après `PlayButton`
  (couleurs, rayon, police, icône).
- Les textes ne citent que des fonctions présentes dans la 1.0 (README, GUIDE, notes de version).
  La disponibilité « Windows · Linux » correspond aux fichiers publiés ; la 1.0 est annoncée
  comme préversion.

## Déclinaison 9:16

`PromoVertical` (1080 × 1920) réutilise le même montage, le même son et les mêmes scènes.
Les positions clés passent par `useLayout()` (`src/lib/layout.ts`) : `u` est l'unité
(1 = 1 px d'une sortie 1080p), `v` vaut vrai en vertical. Les scènes prévoient déjà des
valeurs verticales pour les titres, la caméra et les tailles ; avant diffusion, vérifier les
cadrages avec `node tools/stills.mjs PromoVertical /tmp <images…>` et ajuster les scènes
qui en ont besoin.

## Licences

Voir `LICENSES.md`.
