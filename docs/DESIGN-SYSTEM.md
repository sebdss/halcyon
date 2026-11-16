# Design system « Alcyon » — Halcyon 1.0

> Successeur de « Nocturne » (0.4–0.5), toujours disponible comme thème « Nocturne (0.5) ».

## Identité 1.0

Halcyon est le **martin-pêcheur** ; les « jours alcyoniens » désignent la mer calme d'hiver.
L'interface en tire tout son vocabulaire :

| Jeton | Valeur | Origine | Usage |
|---|---|---|---|
| `bg` | `#0c1726` | eau profonde | fond des pages |
| `sidebar` | `#091220` | rive | barre latérale |
| `surface` / `-hover` / `-active` | `#13223a` / `#1a2d4a` / `#22395c` | couches d'eau | cartes, survol, sélection |
| `accent` | `#32b4ff` | plumes cobalt | actions, sélection, focus |
| `warm` | `#ff8b3d` | poitrail orangé | **réservé à ce qui est vivant** : en jeu, hors ligne, alerte, mise à jour |
| `text` / `text-muted` / `text-dim` | `#e9f0f8` / `#9fb3cc` / `#7b93b3` | — | texte |
| `horizon` | dégradé `accent → warm` | ligne d'horizon | **signature unique** : page active du menu, onglet actif, barres de progression, marque |

Contrastes mesurés (WCAG) sur `surface` : texte 13,9:1, texte secondaire 7,4:1, texte discret
5,1:1, accent 6,9:1, orange 6,8:1 ; texte sur bouton accent 7,2:1.

Règles 1.0 :

1. **Une seule signature** : la ligne d'horizon, et nulle part ailleurs. Pas de dégradés décoratifs.
2. **Pas de capitales espacées** : étiquettes, badges et titres de groupes en casse normale
   (`Overline` n'impose plus de majuscules). Les sigles (GOG, GPU, RAM…) restent en capitales.
3. **La profondeur par la teinte** : bordures très légères teintées de bleu (`#9cc3ff` à 7 %),
   hiérarchie portée par les couches de surface.
4. **Hiérarchie des formes** : contrôles en pilule ; panneaux `radius-lg` 18 px ; lignes et
   vignettes plus serrées.
5. **Titres de page** : 34 px, graisse 800, approche −0,9 px ; sous-titres en `text-muted`.
6. **Coût nul** : la refonte n'ajoute ni ombre large, ni flou, ni fond animé ; le code d'interface
   généré a diminué (35,60 → 35,55 Mo). Un dégradé de fond « profondeur » a été essayé puis retiré :
   avec le rendu logiciel, les zones redessinées ne raccordaient pas.

---

## Expérience 1.0 : navigation et écrans

**Rail de six espaces** (88 px, 68 px en icônes seules, masqué en mode cinéma) : Accueil,
Bibliothèque, Minecraft, Activité, Outils, Système ; Console et Analyser en bas. L'espace actif
porte la ligne d'horizon au bord du rail. Les pastilles ne montrent que des états réels :
téléchargements actifs (accent), mises à jour en attente et nouvelle version (orange), partie
Minecraft en cours (orange).

**Deux niveaux d'onglets, deux formes** :
- *Espace* (Activité, Outils, Système) : barre d'espace sous la barre supérieure, titre Sora 26 px,
  onglets en pilule avec compteur. Les pages de ces espaces n'affichent plus leur propre titre
  (`Layout.tabbed`, lu par `PageHeader`) : leur sous-titre devient la première ligne.
- *Dans une page* (fiche de jeu, Minecraft) : onglets soulignés par la ligne d'horizon.

**Typographie d'affichage : Sora** (SIL OFL, `Theme.display`) pour les titres de page et de
section (`SectionTitle`), les grands chiffres (`BigStat`, mesures du Performance Center) et le
mode console ; **Inter** pour tout le texte courant.

**Images plein cadre** (accueil, fiche, bibliothèque immersive, console) : illustration puis
voiles `Art.fade-left`, `Art.fade-bottom`, `Art.fade-top` teintés par `colorize` à la couleur du
fond — aucun dégradé superposé, aucune ligne de raccord en rendu logiciel.

**Panneau ou pas** : les jeux reposent sur le fond (étagères de l'accueil, grilles) ; seules les
données et les réglages sont posés sur une surface. Les paramètres passent en deux colonnes
(catégories à gauche, une catégorie à la fois à droite).

**Mode console** : mise en page à l'échelle de la hauteur d'écran (`u = height / 1080`),
sections Jouer / Bibliothèque / Recherche, cadre de focus intérieur, barre d'aide contextuelle,
centre de contrôle en bas de l'écran. Détails de navigation : [`GUIDE.md`](GUIDE.md), section 6.

## Historique : « Nocturne » (0.4)

Toutes les valeurs visuelles sont centralisées dans `crates/halcyon-app/ui/theme.slint`
(global `Theme`) et les composants réutilisables dans `ui/components.slint` et `ui/shell.slint`.
Aucune page ne doit coder en dur une couleur, une taille de texte ou une durée d'animation.

## Principes

1. **L'illustration d'abord.** Les jaquettes et bannières occupent la surface ; l'interface se fait discrète
   (verre sombre, voiles, texte clair).
2. **Une action principale visible.** Sur chaque écran, un seul bouton accentué (`PrimaryButton`).
3. **Le focus est toujours visible.** Clavier et manette affichent un anneau `Theme.focus` de
   `Theme.focus-width` ; la sélection n'est jamais signalée par la seule couleur.
4. **Le mouvement informe, il ne décore pas.** Durées courtes, aucune animation bloquante,
   trois niveaux réglables (complet / réduit / aucun).
5. **Rien de fictif.** Une section sans données réelles est masquée ou affiche un état vide explicatif,
   jamais des valeurs inventées.

## Couleurs

| Jeton | Rôle |
|---|---|
| `bg` < `sidebar` < `surface` < `surface-hover` < `surface-active` | Couches de profondeur |
| `border`, `border-strong` | Séparateurs, contours de cartes |
| `glass`, `glass-strong` | Panneaux posés sur une image (opacité 72 / 88 %, sans flou coûteux) |
| `scrim` | Voile sous le texte affiché sur une image |
| `text`, `text-muted`, `text-dim` | Texte principal / secondaire / désactivé (contraste ≥ 4,5:1 pour les deux premiers sur `surface`) |
| `accent` (+ `-hover`, `-pressed`, `-soft`, `-glow`) | Couleur d'accent personnalisable (thème ou réglage) |
| `on-accent` | Texte sur fond accent |
| `success`, `warm`, `danger` | États : réussi, avertissement, erreur/destructif |
| `focus` | Anneau de focus |

Les thèmes 0.3 (fichiers `.halcyon-theme`) restent compatibles : les noms d'origine sont conservés,
les nouveaux jetons sont dérivés (`glass` dérive de `bg`, `accent-glow` de `accent`…).

## Typographie (Inter, et Sora pour l'affichage depuis la 1.0)

| Jeton | Taille de base | Usage |
|---|---|---|
| `t-display` | 48 px (44 en 0.4) | Nom du jeu sur la fiche, titres Big Picture |
| `t-title` | 34 px (30 en 0.4) | Titre de page |
| `t-heading` | 20 px | Titre de section |
| `t-subheading` | 16 px | Titre de carte |
| `t-body` | 14 px | Texte courant |
| `t-small` | 13 px | Métadonnées |
| `t-caption` | 12 px | Légendes, infobulles |
| `t-overline` | 11 px | Sur-titres en capitales |

Toutes sont multipliées par `text-scale` (réglage d'accessibilité 90 % à 130 %).

## Espacements, formes

- Grille de 4 px : `s1` 4 · `s2` 8 · `s3` 12 · `s4` 16 · `s5` 24 · `s6` 32 · `s7` 48 (réduits en densité compacte).
- `page-padding`, `control-height` (40 px, 34 px compact), `nav-height`.
- Rayons : `radius-sm` 6 · `radius` 10 · `radius-lg` 14 · `radius-xl` 22.

## Mouvement

| Jeton | Complet | Réduit | Aucun |
|---|---|---|---|
| `fast` (survol, focus, pression) | 120 ms | 70 ms | 0 |
| `normal` (panneaux, onglets, menus) | 200 ms | 100 ms | 0 |
| `slow` (transitions de page, apparition) | 320 ms | 140 ms | 0 |
| `motion-distance` (glissement) | 12 px | 0 | 0 |

Le mode Performance force « aucun », coupe `effects` (ombres larges, halo) et le fond animé.
Le mode minimal plafonne au niveau « réduit ».

Règle d'implémentation (performance de compilation et d'exécution) : un composant réutilisé garde une
racine « propre » (pas d'`animate`, d'`opacity`, d'ombre ni de `visible` sur la racine) ; les effets vont
sur un enfant `bg := Rectangle`. Sinon le compilateur Slint duplique le composant à chaque usage.

## Iconographie

Icônes SVG au trait 2 px, grille 24 × 24, `ui/icons/*.svg`, exposées par le global `Icons`
et colorées via `colorize`. Une icône seule doit toujours avoir une infobulle (`IconButton.tooltip`).

## Composants

| Composant | Rôle / particularités |
|---|---|
| `Focusable`, `FocusRing` | Base clavier/manette : Entrée/Espace → `activated` |
| `PrimaryButton`, `SecondaryButton`, `IconButton` | `enabled`, `badge`, infobulle ; un bouton désactivé explique pourquoi via son infobulle |
| `Chip`, `Segmented`, `Tabs`, `Dropdown`, `Toggle` | Navigables aux flèches ; `Tabs.hidden` masque les onglets sans données |
| `TextField`, `TextArea` | Champs de saisie |
| `GameCover`, `GameTile` | Jaquette avec chargement progressif ; tuile avec état sélectionné, menu contextuel (clic droit / touche Menu / bouton ☰ manette) |
| `Skeleton`, `Spinner`, `ProgressBar` | États de chargement ; `ProgressBar.value < 0` = indéterminé |
| `Badge`, `KeyHint`, `Notice`, `EmptyState` | Statuts, raccourcis, messages, états vides |
| `Panel`, `SectionTitle`, `Overline`, `InfoRow`, `BigStat`, `BarChart`, `MeterRow`, `PageHeader` | Mise en page de contenu |
| `ContextMenu` + `MenuEntry` | Menu contextuel clavier/manette, entrées désactivables et destructives (rouge) |
| `Modal`, `ModalPanel` | Dialogues ; toute action destructive passe par une confirmation |
| `Sidebar`, `TopBar`, `CommandCenter` (`shell.slint`) | Coquille : navigation repliable, historique, recherche, centre de commandes |

## Modes d'interface

| Mode | Effet |
|---|---|
| Standard | Barre latérale complète |
| Minimal | Barre repliée, accueil réduit (favoris, récents, recherche), animations réduites |
| Cinéma | Barre latérale masquée, illustrations plein cadre |
| Performance | Indépendant du mode : animations et effets coupés, tâches secondaires suspendues ; activable à la main ou automatiquement pendant une session de jeu, puis restauré |

## Responsive

Fenêtre minimale 800 × 560. Sous 1100 px de large, la barre latérale se replie automatiquement (icônes
+ infobulles). Les grilles calculent leur nombre de colonnes à partir de la largeur disponible ; les textes
longs sont élidés (`overflow: elide`) avec le texte complet en infobulle quand c'est utile.
