# Journal des modifications

## 0.5.0 — « Minecraft Integration »

Évolution de la 0.4.1 : aucune fonction retirée, données de la 0.4.1 reprises telles quelles.
Détails : [`docs/NOTES-0.5.0.md`](docs/NOTES-0.5.0.md) · fonctionnement : [`docs/MINECRAFT.md`](docs/MINECRAFT.md).

### Minecraft Java Edition (nouvel onglet)
- Onglet **Minecraft** dans le menu, avec six sections : Jouer, Versions, Mods, Modpacks,
  Loaders, Paramètres. Les autres pages de Halcyon ne changent pas.
- Versions officielles Mojang (releases, snapshots et anciennes versions en option) :
  installation avec progression réelle, réparation, vérification SHA-1, suppression.
- Runtime Java officiel de Mojang installé selon la version, ou Java choisi et vérifié.
- Loaders Fabric, Quilt, Forge et NeoForge par leurs mécanismes officiels, avec liste des
  versions compatibles.
- Mods Modrinth (et CurseForge avec une clé d'API) filtrés par version et loader :
  dépendances requises, incompatibilités, activation, désactivation, suppression, mises à jour.
- Modpacks : recherche, installation, mise à jour (Modrinth), import `.mrpack` et CurseForge,
  export `.mrpack`.
- Lancement par le **launcher officiel** (par défaut) : Halcyon prépare l'installation et
  l'ajoute, avec votre accord et après sauvegarde, comme profil « Halcyon · … » du launcher
  officiel, qui connecte le compte et lance le jeu.
- Lancement **direct** avec connexion Microsoft officielle (code d'appareil, vérification de la
  licence Java Edition) quand un ID d'application Azure approuvé est configuré.
- Paramètres : RAM, Java, arguments JVM, résolution, plein écran.
- « Minecraft: Java Edition » dans la bibliothèque avec les autres jeux (filtre « Minecraft »,
  accueil, Big Picture, collections) ; Jouer lance l'installation sélectionnée.

### Autres corrections (depuis la 0.4.1 publiée)
- Détection des jeux Epic Games corrigée (dossier de manifestes, extensions de moteur ignorées).
- Illustrations SteamGridDB pour les jeux autonomes et ajoutés à la main (clé d'API personnelle).
- Ajout d'émulateurs personnalisés et de plateformes depuis la page Émulateurs.

## 0.4.1 — « Original Sound Identity & Custom Premium Installer »

Évolution de la 0.4.0 : aucune fonction retirée, données de la 0.4.0 reprises telles quelles.
Détails : [`docs/NOTES-0.4.1.md`](docs/NOTES-0.4.1.md).

### Son
- Identité sonore originale : 28 sons synthétisés pour Halcyon (`tools/sound/make_sounds.py`,
  reproductible, licence MIT), intégrés au binaire. Catalogue : [`docs/SON.md`](docs/SON.md).
- Nouveau moteur `halcyon-audio` (cpal) : fil dédié, périphérique ouvert au premier son et
  refermé après 20 s de silence, 6 voix, priorités, anti-répétition par son, aucun blocage
  sans carte son.
- Sons reliés à un seul système d'évènements : navigation, sections, panneaux, menus, retour,
  fiche, favoris, collections, lancement et démarrage d'un jeu, confirmations, erreurs,
  téléchargements, nouveaux jeux, succès, Big Picture.
- **Paramètres › Son** : activation, silencieux, volume général, 4 volumes par catégorie avec
  écoute, survol, comportement pendant une partie (silencieux par défaut), réinitialisation.

### Installateur
- Programme d'installation Halcyon pour Windows et Linux, à l'image de l'application :
  accueil, installation détectée, configuration, vérification du système, progression réelle
  par étape (octets écrits puis relus), finalisation, « Prêt à jouer » (lancement de Halcyon).
- Détection : aucune installation, version plus ancienne, à jour, endommagée, transaction
  interrompue, installation Inno Setup ou `install.sh` de la 0.4.0 (reprise sans perte).
- Transaction avec journal : en cas d'échec ou de coupure, l'installation précédente est
  rétablie. Fichiers inconnus et dossiers jamais déplacés ni supprimés.
- Réparation (seuls les fichiers abîmés sont réécrits), désinstallation (cache et données
  seulement sur demande, données avec confirmation explicite), programme de maintenance dans
  le menu (« Gérer l'installation de Halcyon ») et dans « Applications installées ».
- Charge utile compatible avec la signature Authenticode ; la CI signe si un certificat est
  configuré.
- Mise à jour intégrée sous Linux par le programme d'installation quand Halcyon a été
  installé par lui.

### Paquets
- Windows : programme d'installation Halcyon à la place d'Inno Setup (le script `.iss`
  reste dans le dépôt) ; version portable inchangée.
- Linux : programme d'installation `halcyon-<version>-linux-x86_64-setup` ajouté ; `.deb`
  dépendant désormais d'ALSA ; archive `.tar.gz` de la 0.4.0 conservée.
- Compilation : `libasound2-dev` requis sous Linux.

## 0.4.0 — « Premium Console Experience »

Refonte de l'interface et de l'expérience console, sans perte de fonctionnalité ni de
données. Détails : [`docs/NOTES-0.4.0.md`](docs/NOTES-0.4.0.md).

### Interface
- Design system « Nocturne » : jetons de couleur, échelle typographique, espacements, rayons,
  mouvement et effets centralisés (`docs/DESIGN-SYSTEM.md`) ; composants réécrits (focus
  visible au clavier et à la manette, infobulles, états désactivés expliqués).
- Coquille : barre latérale repliable (automatique sous 1 100 px), barre supérieure avec
  précédent / suivant (Alt+←/→, bouton Retour), recherche universelle, indicateurs
  « En jeu », « Performance », « Hors ligne », centre de commandes.
- Accueil en widgets (14 types) : ordre, largeur (1/3, 2/3, pleine) et affichage
  personnalisables ; un widget masqué n'effectue aucun traitement.
- Bibliothèque : 5 vues (grille, liste, carrousel, immersive, compacte), taille des
  jaquettes S/M/L, sélection clavier/manette conservée entre vues et filtres, menu contextuel
  commun (clic droit, touche Menu, bouton Select).
- Fiche de jeu cinématographique : grand bandeau (ou ambiance floutée tirée de la jaquette),
  chiffres réels (temps de jeu, dernière session, progression des succès), onglets Aperçu,
  Succès, Statistiques, Captures, Sauvegardes, Mods, Paramètres de lancement ; onglets sans
  données masqués.
- Captures Steam locales (F12) dans la fiche et sur l'accueil, en lecture seule.
- Modes d'affichage standard, minimaliste et cinéma ; réglages de mouvement (complet, réduit,
  aucun) et de taille du texte (90 à 130 %).
- Big Picture : écran de réglages entièrement pilotable à la manette.

### Navigation et manette
- Manette sur tout le bureau (hors Big Picture) : flèches dans la bibliothèque et les menus,
  déplacement du focus ailleurs, A / B / X / Y, Select (menu contextuel), LB / RB (vue ou
  onglet précédent / suivant). Trois nouvelles touches configurables.
- Recherche universelle (Ctrl+K) étendue aux réglages, collections et commandes.
- Paramètres : recherche et filtre par section.

### Centre de commandes
- Tâches en cours avec progression (analyse, synchronisation des succès, visuels en ligne,
  session de jeu, mise à jour de Halcyon, export de la configuration), historique, erreurs
  récentes, actions (ouvrir la page, la fiche ou le dossier concerné).

### Performances
- Mode Performance (manuel ou automatique pendant les parties) : animations et effets coupés,
  téléchargements et vérifications reportés, puis rétablis. Aucun programme n'est fermé.
- Pages secondaires calculées à leur ouverture ; badges calculés après la première image.
- Fiche : l'arrivée d'une image ne recharge plus toute la page.
- Thread des manettes : réveil toutes les secondes au repos au lieu de 25 fois par seconde.
- Requête bornée pour les statistiques de la semaine et index `play_sessions(started_at)`.
- Mesures avant/après : [`docs/PERFORMANCES.md`](docs/PERFORMANCES.md).

### Données
- Schéma SQLite v5 : ajout d'un index uniquement (base toujours lisible par la 0.3).
- Paramètres : nouveaux champs d'apparence ; l'accueil de la 0.3 et la vue grille/liste sont
  repris automatiquement. Procédure : [`docs/MISE-A-JOUR-0.4.md`](docs/MISE-A-JOUR-0.4.md).

## 0.3.0
Sauvegardes versionnées, mods réversibles, manettes, collections intelligentes, statistiques,
profils de lancement, mises à jour des jeux, hors ligne, extensions déclaratives,
personnalisation, recherche avancée, lancement rapide, raccourci global, mode compact,
fenêtre compagnon, assistant « Que jouer ? », Sunshine/Moonlight, intégration Windows,
journal d'activité, export de la configuration.

## 0.2.0
Compatibilité Linux complète, émulation, succès Steam et RetroAchievements, Big Picture.

## 0.1.0
Première version : bibliothèque Steam, Epic, standalone et manuelle.
