# Système de design « Berge » — Halcyon 1.0

> Successeur d'« Alcyon » (préversions 1.0) et de « Nocturne » (0.4–0.5), tous deux conservés
> comme thèmes : « Alcyon (1.0 préversion) » et « Nocturne (0.5) » dans *Système › Apparence*.

## Pourquoi une nouvelle direction

Les préversions avaient les traits des interfaces assemblées automatiquement : cartes arrondies
partout, pilules, dégradé décoratif (la « ligne d'horizon »), halos lumineux, fond bleu saturé,
grands titres resserrés, chaque bloc d'information posé dans sa propre carte. « Berge » repart
du sujet plutôt que d'une tendance.

## Le sujet

Halcyon est le martin-pêcheur. Il se tient **immobile sur une branche au-dessus de l'eau**, puis
plonge d'un **geste net**. Son logo réunit deux couleurs : le **bleu du plumage** et l'**orange
des pattes**. L'interface en tire quatre principes :

1. **Calme et mat.** Ardoise neutre, aucun verre, aucun dégradé décoratif, aucune lueur. Les
   ombres sont réservées à ce qui flotte réellement (menus, dialogues, notifications).
2. **Deux couleurs, deux rôles.** Le bleu dit *où l'on est* (sélection, focus, liens,
   progression). L'orange est *le geste* : le bouton **Jouer**, l'état **en jeu**, ce qui est
   vivant (téléchargement actif, succès débloqué). Jouer est orange partout, toujours dessiné de
   la même façon : c'est la signature de Halcyon.
3. **La perche.** Un trait d'accent court (3 px) marque l'élément actif : espace de navigation,
   onglet, entrée d'un index, choix d'une liste, ligne sélectionnée. C'est la branche sur laquelle
   l'oiseau se pose ; il n'y a pas d'autre ornement.
4. **Une voix typographique.** Archivo Expanded pour les titres et les chiffres, Archivo pour le
   texte. La hiérarchie vient de la taille, de la largeur et de la graisse du texte, pas des
   boîtes.

## Couleurs

| Jeton | Valeur | Rôle |
|---|---|---|
| `sidebar` | `#0f1115` | colonne de navigation (la rive) |
| `bg` | `#14171c` | page |
| `surface` / `-hover` / `-active` | `#1b1f25` / `#232830` / `#2b313a` | groupes de réglages, menus, survol, sélection |
| `border` / `border-strong` | blanc 8 % / 15 % | filets |
| `text` / `text-muted` / `text-dim` | `#eceef1` / `#a3abb6` / `#7e8793` | texte principal, secondaire, discret |
| `accent` | `#3d9be9` (bleu du logo) | sélection, focus, liens, progression |
| `warm` | `#f08a24` (orange du logo) | Jouer, en jeu, vivant |
| `danger` / `success` | `#e5534b` / `#3fb27f` | erreur, confirmation |
| `bp-bg` | `#0e1014` | mode console |

Contrastes WCAG mesurés sur `bg` : texte 15,5:1, secondaire 7,7:1, discret 4,9:1, bleu 6,0:1,
orange 7,2:1. Texte sur bouton bleu 6,4:1, sur bouton orange 7,5:1. Sur `surface`, le texte
discret reste à 4,5:1.

## Typographie

Police **Archivo** (SIL OFL), instanciée en fichiers statiques depuis la version variable :

| Famille | Graisses | Usage |
|---|---|---|
| `Archivo Expanded` (`Theme.display`, largeur 125) | 600, 700, 800 | titres de page et de section, chiffres (temps de jeu, compteurs, mesures), libellé « Jouer », mode console |
| `Archivo` (`Theme.font`, largeur 100) | 400, 500, 600, 700 | tout le texte courant |

Échelle (× `text-scale`, 0,9 à 1,3) : 44 / 30 / 20 / 16 / 14 / 13 / 12 / 11 px. Aucune
capitale espacée ; aucune approche négative ; les compteurs sont des chiffres composés dans la
police d'affichage (« Outils 4 »), jamais des pastilles.

## Forme et espacement

- Rayons : 3 px (étiquettes, jaquettes, cases), 5 px (contrôles), 8 px (menus, dialogues). Plus
  de pilules ; l'interrupteur a une piste droite et un curseur carré.
- Grille de 4 px. Marge de page 40 px (28 px en densité compacte). Les réglages et l'apparence
  se lisent dans une colonne bornée à environ 880 px.
- Rangées de liste : 52 px (44 px compacte), séparées par un filet, sans carte.

## Composition

- **Pas de cartes par défaut.** Un sujet s'ouvre par une **rubrique** (`Rubric`) : filet de
  tête, titre, contenu. Les jaquettes reposent sur la page. Les groupes de réglages
  (`Panel`) sont un simple changement de teinte, sans contour.
- **Un index à gauche** quand une page a beaucoup de filtres ou de catégories : bibliothèque
  (sources, affichage, collections, genre), paramètres (catégories).
- **Un chiffre-titre** quand une page a une mesure principale : complétion des succès, temps de
  jeu total, processeur/mémoire/carte graphique.
- **Une planche** pour l'image d'un jeu : bannière nette, coins presque droits, jamais fondue
  dans un dégradé (sauf la vue immersive de la bibliothèque, choisie explicitement).

## Écrans

| Écran | Composition |
|---|---|
| Coque | Colonne de navigation (212 px, 64 px en icônes seules) : logo et nom, six espaces (Accueil, Bibliothèque, Minecraft, Activité, Outils, Système), puis Console et Analyser. L'espace actif prend la teinte de la page et porte la perche. Barre supérieure : historique, recherche (Ctrl K), états réels (en jeu, Performance, hors ligne) et modes. Barre d'espace : titre et onglets soulignés, compteurs en chiffres. |
| Accueil | « Reprendre » : texte à gauche (titre, temps de jeu, dernière partie, source, Jouer), planche à droite. Puis les widgets choisis : étagères de jaquettes sur la page, widgets de données en rubriques. « Personnaliser l'accueil » en pied de page ; en édition, chaque widget montre son cadre. |
| Bibliothèque | Index à gauche (sources, affichage, collections, genre), titre et nombre de jeux, barre d'outils (recherche, tri, taille, vue), puis grille, liste en tableau, carrousel, immersive ou compacte. Le carrousel et l'immersive rendent toute la largeur au jeu. |
| Fiche de jeu | Bande illustrée nette, jaquette qui chevauche son bord, titre et provenance, Jouer et actions, chiffres réels à droite. Onglets soulignés. Corps en deux colonnes : contenu, et fiche technique (suivi, installation, édition, liens). |
| Minecraft | Même structure que Halcyon ; son univers tient dans un bloc d'herbe dessiné au pixel et un filet herbe et terre sous les onglets. |
| Activité | Succès : complétion en chiffre-titre, rubriques par plateforme et par mois. Statistiques : temps total en chiffre-titre, relevé, graphiques en rubriques. Journal : chronologie avec heure, repère de catégorie et fil. Que jouer ? : suggestions avec leur raison. |
| Outils | Téléchargements et mises à jour en rangées à perche (actif) ; sauvegardes, émulateurs, manettes, jeu à distance et extensions en rangées et groupes. |
| Système | Performances en instruments (filet épais, valeur en grand, courbe sur une ligne de base). Apparence : thèmes en miniatures de l'interface. Paramètres : index des catégories, colonne de lecture. |
| Console | Mise à l'échelle de la hauteur d'écran (`u = height / 1080`), rayons réduits, Jouer orange ; le focus télé reste blanc. |

## États

| État | Traitement |
|---|---|
| Survol | teinte `surface` (rangées) ou `surface-hover` (contrôles) |
| Focus clavier / manette | anneau `accent` 2 px à 3 px du contrôle, toujours visible |
| Actif / sélectionné | perche `accent` et texte en graisse 650 |
| Désactivé | texte `text-dim`, fond neutre ; l'infobulle dit pourquoi quand c'est utile |
| Chargement | `Spinner`, `ProgressBar` (indéterminée : segment qui parcourt la piste), `Skeleton` (pulsation de teinte) |
| Erreur | filet ou perche `danger`, message qui dit quoi faire |
| Vide | ce qui manque et l'action qui le remplit (`EmptyState`, notes de rubrique) |
| Confirmation | notification inversée (fond clair, perche de couleur d'état), en bas de la page |

## Mouvement

| Jeton | Complet | Réduit | Aucun |
|---|---|---|---|
| `fast` (survol, focus, pression) | 90 ms | 60 ms | 0 |
| `normal` (perches, onglets, panneaux) | 160 ms | 90 ms | 0 |
| `slow` (graphiques, apparition) | 240 ms | 120 ms | 0 |
| `motion-distance` (glissement) | 6 px | 0 | 0 |

Rien ne flotte ni ne rebondit : les tuiles ne se soulèvent plus au survol (léger éclaircissement
et bouton Jouer). Le mode Performance force « aucun » et coupe les ombres.

Règle d'implémentation : un composant réutilisé garde une racine « propre » (pas d'`animate`,
d'`opacity`, d'ombre ni de `visible` sur la racine) ; les effets vont sur un enfant. Sinon le
compilateur Slint duplique le composant à chaque usage.

## Iconographie

Icônes SVG maison, grille 24 × 24, trait 1,6 px, **extrémités carrées et angles vifs** (accord
avec Archivo), rayons de 1 px au plus. Exposées par le global `Icons`, colorées par `colorize`.
Une icône seule a toujours une infobulle.

## Composants

| Composant | Rôle |
|---|---|
| `PlayButton` | Jouer (orange), `big` pour les en-têtes, `busy` pour « en jeu » |
| `PrimaryButton`, `SecondaryButton`, `TextButton`, `IconButton` | Valider (bleu), action secondaire (contour), lien d'action, icône (infobulle, état actif souligné) |
| `Chip`, `Segmented`, `Tabs`, `Dropdown`, `Toggle`, `ValueSlider` | Choix ; navigables aux flèches |
| `TextField`, `TextArea` | Saisie ; focus : contour et trait d'accent |
| `GameCover`, `GameTile` | Jaquette (planche typographique générée sans image), tuile avec légende titre / source / temps |
| `Rubric`, `SectionHeader`, `SectionTitle`, `Overline`, `SettingRow`, `InfoRow`, `BigStat`, `MeterRow`, `BarChart`, `PageHeader` | Composition des pages |
| `Notice`, `EmptyState`, `Badge`, `KeyHint`, `Skeleton`, `Spinner`, `ProgressBar` | Notes, états, étiquettes |
| `Panel`, `Modal`, `ModalPanel`, `ContextMenu` | Groupes, dialogues, menus |
| `Sidebar`, `TopBar`, `SpaceBar`, `CommandCenter` (`shell.slint`) | Coque |

## Modes d'interface

| Mode | Effet |
|---|---|
| Standard | Colonne de navigation complète |
| Minimal | Colonne en icônes, accueil réduit (recherche, favoris, récents), animations réduites |
| Cinéma | Colonne masquée |
| Performance | Animations et effets coupés, tâches secondaires suspendues ; manuel ou automatique pendant une partie |

## Responsive

Fenêtre minimale 800 × 560. Sous 1080 px de large, la colonne de navigation passe en icônes ;
sous 1000 px, l'index de la bibliothèque laisse place aux filtres en ligne ; sous 940 px, les
paramètres passent en une colonne avec une liste déroulante de catégories. Les grilles calculent
leurs colonnes d'après la largeur disponible ; les textes longs sont élidés.
