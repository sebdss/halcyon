# Design system « Nocturne » — Halcyon 0.4

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

## Typographie (Inter)

| Jeton | Taille de base | Usage |
|---|---|---|
| `t-display` | 44 px | Nom du jeu sur la fiche, titres Big Picture |
| `t-title` | 30 px | Titre de page |
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
