# Halcyon 0.4.0 — « Premium Console Experience »

Version consacrée à l'interface, à l'expérience console et aux performances. Toutes les
fonctionnalités de la 0.3.0 sont conservées ; les données sont migrées sans perte
(procédure : [`MISE-A-JOUR-0.4.md`](MISE-A-JOUR-0.4.md)).

## Nouveautés

### Une interface repensée
- **Design system « Nocturne »** : fond sombre en couches, verre modéré sur les illustrations,
  typographie Inter sur une échelle unique, focus toujours visible, couleur d'accent
  personnalisable. Toutes les valeurs sont centralisées
  ([`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md)) ; les thèmes 0.3 restent compatibles.
- **Barre latérale repliable** (icônes et infobulles), repliée automatiquement dans une
  fenêtre étroite ; **barre supérieure** avec précédent / suivant, recherche universelle,
  indicateurs « En jeu », « Performance », « Hors ligne » et centre de commandes.
- **Transitions** : apparition des pages, sélection, menus et panneaux animés, sans jamais
  bloquer l'interface. Mouvement **complet, réduit ou aucun**.
- **Taille du texte** : 90 %, 100 %, 115 % ou 130 % pour toute l'interface.

### Accueil en tableau de bord
14 widgets : Continuer à jouer, Derniers jeux lancés, Favoris, Bibliothèque, Succès récents,
Statistiques (7 jours, suivi local), Temps de jeu (avec la provenance : plateforme, suivi
local ou saisi), Activité récente, Stockage (disques réels, taille déclarée des jeux),
Émulateurs, Captures, Recommandations, Ajoutés récemment, En cours. Le bouton crayon permet de
les déplacer, de changer leur largeur (1/3, 2/3, pleine) et de les masquer ; un widget masqué
ne fait aucune requête ni lecture de disque.

### Bibliothèque
- **5 vues** : grille, liste, carrousel, immersive (illustration plein cadre du jeu
  sélectionné), compacte ; taille des jaquettes S / M / L.
- **Sélection** au clavier et à la manette : flèches, Page préc./suiv., Début/Fin, Entrée
  (fiche), Espace (jouer), Menu (actions). Elle est conservée quand on change de vue, de tri
  ou de filtre, et la position de défilement est gardée en quittant la page.
- **Menu contextuel** commun à toutes les vues et à l'accueil : jouer, fiche, favori, succès,
  sauvegardes, dossier du jeu, raccourci sur le bureau, masquer, retirer (jeux ajoutés à la
  main, avec confirmation).

### Fiche de jeu cinématographique
Grand bandeau (bannière, sinon ambiance floutée tirée de la jaquette), logo ou titre, état
d'installation, mise à jour en attente, temps de jeu, dernière session, progression des
succès (uniquement quand elle est connue). Bouton **Jouer** mis en avant avec choix du
profil. Onglets : **Aperçu, Succès, Statistiques, Captures, Sauvegardes, Mods, Paramètres de
lancement** ; un onglet sans données (succès non pris en charge, aucune capture) est masqué.

### Captures
Les captures prises avec l'overlay Steam (F12) sont lues dans le dossier du compte local
(`userdata/…/760/remote`) : onglet Captures de la fiche et widget de l'accueil. Lecture seule ;
un clic ouvre l'image dans la visionneuse du système.

### Modes d'affichage
- **Minimaliste** : barre repliée, accueil réduit à la recherche immédiate, aux derniers jeux et
  aux favoris, mouvement réduit.
- **Cinéma** : sans barre latérale, illustrations plein cadre.
- **Performance** (indépendant) : animations et effets coupés, téléchargements de visuels et
  vérification des mises à jour reportés ; activable à la main ou **automatiquement pendant
  les parties lancées depuis Halcyon**, puis tout est rétabli. Aucun programme ni service
  n'est jamais fermé.

### Manette partout
Hors Big Picture, la manette pilote tout le bureau : flèches dans la bibliothèque et les
menus, déplacement du focus ailleurs, A valider, B retour, Y recherche, X favori,
**Select** menu contextuel, **LB / RB** vue ou onglet précédent / suivant. Les boutons restent
configurables (page Manettes). En Big Picture, **Select** ouvre un écran de **réglages**
(mode Performance, mouvement, taille du texte, thème, mode du bureau, analyse, succès, accès
aux paramètres complets).

### Recherche universelle et paramètres
**Ctrl+K** trouve désormais les jeux, les pages, les **réglages**, les **collections** et les
**commandes** (analyse, modes, vues, Big Picture, centre de commandes…). La page Paramètres
propose une recherche et un filtre par section.

### Centre de commandes
Cloche de la barre supérieure : tâches en cours avec progression (analyse des sources,
synchronisation des succès, visuels en ligne, session de jeu, téléchargement d'une mise à
jour, export de la configuration), notifications et **erreurs récentes**, avec une action
pour ouvrir la page, la fiche ou le dossier concerné.

## Fonctionnalités demandées : réalisées / non réalisées

| Demande | État | Détail |
|---|---|---|
| Design system documenté | Réalisé | `DESIGN-SYSTEM.md`, jetons et composants |
| Bibliothèque : grille, liste, carrousel, immersive, compacte | Réalisé | |
| Taille des jaquettes | Réalisé | S / M / L, enregistrée |
| Sélection claire, menus contextuels, lancement rapide | Réalisé | |
| Navigation complète à la manette | Réalisé | Bureau et Big Picture ; voir limites |
| Chargement intelligent des images | Réalisé (0.3 + 0.4) | Cache borné, miniatures, chargement progressif, fenêtre limitée pour le carrousel |
| Position et filtres conservés | Réalisé | |
| Fiche cinématographique et ses 7 onglets | Réalisé | « Vidéos » : non (voir ci-dessous) |
| Animations, mouvement réduit / aucun | Réalisé | |
| Squelettes de chargement | Partiel | Composant `Skeleton` disponible ; les données locales arrivent en quelques millisecondes, les images utilisent un dégradé aux couleurs du jeu en attendant |
| Thèmes, accent, fonds, import/export, réinitialisation | Réalisé (0.3) | Inchangé, compatible |
| Widgets : activer, déplacer, redimensionner, masquer | Réalisé | Par boutons (pas de glisser-déposer) |
| Modes minimaliste, cinéma, Performance | Réalisé | |
| Aperçu en direct | Réalisé | Chaque réglage s'applique immédiatement |
| Accueil en tableau de bord (12 widgets demandés) | Réalisé | 14 widgets |
| Recherche globale, palette Ctrl+K, réglages et fonctions | Réalisé | |
| Raccourcis personnalisables, précédent / suivant | Réalisé | |
| Big Picture amélioré, accès aux paramètres sans souris | Réalisé | Réglages essentiels en Big Picture + « Paramètres complets » pilotables à la manette sur le bureau |
| Centre de commandes | Réalisé | Téléchargements Steam : visibles dans « Mises à jour » (progression lue chez Steam), pas dupliqués en tâches |
| Optimisations mesurées | Réalisé | `PERFORMANCES.md` |
| Responsive (petites fenêtres, ultra-large, HiDPI) | Réalisé | Vérifié en 800×560, 900×700, 1600×1000, 2560×1080 et 3840×2160 à l'échelle 200 % |
| Accessibilité (contraste, texte, clavier, manette, mouvement) | Réalisé | Lecteur d'écran : rôles et libellés sur les composants principaux seulement |
| Tests | Réalisé | `TESTS-0.4.0.md` |

### Non réalisé, et pourquoi
- **Lecture de vidéos (bandes-annonces)** : Slint ne fournit pas de lecteur vidéo ; en
  intégrer un ajouterait une dépendance lourde (GStreamer / Media Foundation) et un risque de
  stabilité. L'onglet affiche les captures et les visuels officiels.
- **Captures hors Steam** (Epic, GOG, émulateurs) : pas d'emplacement standard fiable. Halcyon
  n'invente pas de dossier ; à ajouter par extension dans une prochaine version.
- **Glisser-déposer des widgets** : remplacé par des boutons déplacer / taille / masquer,
  utilisables aussi au clavier et à la manette.
- **Navigation spatiale à la manette hors bibliothèque** (aller « vers le bas » visuellement) :
  la manette parcourt les éléments dans l'ordre du focus (comme Tab). Les grilles, menus et la
  palette utilisent les directions.
- **Superposition en jeu** : toujours volontairement absente (anti-triche, stabilité).
- **Tests sur une vraie manette et sous Windows** : non effectués dans l'environnement de
  développement (aucun périphérique, compilation Linux seulement). Le code manette passe par
  les mêmes chemins que le clavier, testés ; la CI Windows compile et teste au premier envoi.
