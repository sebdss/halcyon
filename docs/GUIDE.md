# Halcyon — Guide d'utilisation

Halcyon réunit vos jeux Steam, Epic Games, standalone et émulés dans une seule
bibliothèque. Il centralise les succès, propose un mode Big Picture pour la télévision et gère
les sauvegardes, les mods, les profils de lancement et les statistiques. Il fonctionne sous
**Windows** et **Linux**.

Sommaire : 1 Installation · 2 Premier lancement · 3 Bibliothèque · 4 Émulateurs · 5 Succès ·
6 Big Picture · 7 Recherche avancée et collections intelligentes · 8 Suivi, statistiques et
journal · 9 Profils de lancement · 10 Mises à jour des jeux · 11 Sauvegardes · 12 Mods ·
13 Manettes · 14 Apparence · 15 Extensions · 16 Que jouer ? · 17 Jeu à distance ·
18 Raccourcis, lancement rapide et mode compact · 19 Fenêtre compagnon · 20 Système ·
21 Hors ligne · 22 Exporter et restaurer · 23 Mise à jour de Halcyon · 24 Données,
confidentialité, dépannage · 25 Désinstallation

## 1. Installation

### Windows
- **Programme d'installation** : `Halcyon-<version>-windows-x86_64-setup.exe`. Installation
  pour votre compte (sans droits administrateur) ou, lancé en administrateur, pour tous les
  utilisateurs. Il vérifie le système, affiche la progression réelle et propose de lancer
  Halcyon à la fin. Détails : [`INSTALLATEUR.md`](INSTALLATEUR.md).
- **Portable** : `Halcyon-<version>-windows-portable.zip`. Décompressez où vous voulez ; le
  fichier `portable.txt` fourni place les données dans le dossier `data\` voisin.

### Linux
- **AppImage** (recommandé, mises à jour automatiques) : `Halcyon-<version>-x86_64.AppImage`.
  `chmod +x`, puis lancez-la ; *Paramètres › Mises à jour › Installer Halcyon pour mon compte*
  la place dans `~/.local/opt/halcyon` avec son entrée de menu.
- **Programme d'installation** (toutes distributions) : `chmod +x halcyon-<version>-linux-x86_64-setup`
  puis lancez-le. Installation dans `~/.local/opt/halcyon`, sans droits administrateur, avec
  entrée de menu, raccourci et « Gérer l'installation de Halcyon » (réparer, désinstaller).
- **Debian / Ubuntu / Mint** : `sudo apt install ./halcyon_<version>_amd64.deb`
- **Archive** (format de la 0.4.0) : décompressez `halcyon-<version>-linux-x86_64.tar.gz` puis
  lancez `./install.sh` (`./uninstall.sh` pour retirer). Une installation faite ainsi est
  reprise par le programme d'installation.

Dépendances système : une session X11 ou Wayland, OpenGL (repli logiciel automatique),
`libudev` (manettes), ALSA (son ; PipeWire et PulseAudio passent par lui) — présentes sur
toutes les distributions de bureau. Le programme d'installation les vérifie.

## 2. Premier lancement

Halcyon analyse automatiquement :

| Source | Windows | Linux |
|---|---|---|
| Steam | registre + dossier d'installation | `~/.steam/steam`, `~/.local/share/Steam`, Flatpak |
| Epic Games | launcher Epic, ou Heroic / Legendary | Heroic (natif ou Flatpak), Legendary |
| Dossiers de jeux | à ajouter dans **Paramètres › Dossiers de jeux** | idem |
| ROM | à ajouter dans **Émulateurs › Bibliothèques de ROM** | idem |

Les jeux disparus d'une source sont marqués « Non installé » — jamais supprimés : favoris,
temps de jeu et personnalisations sont conservés.

## 2 bis. Se repérer dans l'interface (0.4)

- **Menu (1.0)** : six espaces — Accueil, Bibliothèque, Minecraft, **Activité** (Que jouer ?,
  Succès, Statistiques, Journal), **Outils** (Téléchargements, Mises à jour, Sauvegardes,
  Émulateurs, Manettes, Jeu à distance, Extensions) et **Système** (Performances, Apparence,
  Paramètres). Les trois derniers ont des onglets en haut de page ; chaque espace rouvre le
  dernier onglet consulté. En bas du menu : **Console** (mode Big Picture) et **Analyser**.
  *Apparence › Menu : icônes seules* masque les libellés.
- **Barre supérieure** : précédent / suivant (aussi `Alt+←` / `Alt+→`, touche Retour du
  clavier multimédia), recherche universelle (`Ctrl+K`), indicateurs *En jeu*, *Performance*, *Hors ligne*,
  mode Performance, bascule de mode d'affichage, **centre de commandes** (cloche).
- **Centre de commandes** : tâches en cours (analyse, succès, visuels en ligne, session de jeu,
  mise à jour, export) avec leur progression, notifications et erreurs récentes ; chaque
  entrée peut ouvrir la page, la fiche ou le dossier concerné.
- **Accueil** : widgets. Le bouton crayon (en haut à droite) permet de déplacer (‹ ›), de
  changer la largeur (1/3, 2/3, pleine) et de masquer chaque widget ; les widgets masqués
  apparaissent en haut pour les rajouter. *Réinitialiser* revient à la disposition d'origine.
- **Modes** : *standard*, *minimaliste* (accueil réduit à la recherche, aux derniers jeux et aux
  favoris), *cinéma* (sans menu). Le **mode Performance** coupe animations et effets et
  reporte les tâches secondaires ; il peut s'activer tout seul pendant les parties
  (*Apparence › Automatique pendant les parties*). Aucun programme n'est jamais fermé.

## 3. Bibliothèque

- **Vues** : grille, liste, carrousel, immersive (illustration plein cadre du jeu sélectionné),
  compacte. Taille des jaquettes **S / M / L** en grille et en vue compacte.
- **Sélection au clavier** : flèches, `Page↑` / `Page↓`, `Début` / `Fin` ; `Entrée` ouvre la fiche,
  `Espace` (ou `Ctrl+Entrée`) lance le jeu, la touche `Menu` ouvre le menu du jeu. La sélection
  et la position de défilement sont conservées.
- **Menu d'un jeu** : clic droit sur une jaquette ou une ligne, bouton `…`, touche `Menu` ou
  bouton Select de la manette : jouer, fiche, favori, succès, sauvegardes, dossier, raccourci
  sur le bureau, masquer, retirer (jeux ajoutés à la main, après confirmation).
- **Recherche** : `Ctrl+F`, insensible aux accents et à l'ordre des mots.
- **Filtres** : plateforme (Steam, Epic, Émulation, Autres), installés, favoris, masqués,
  collection, genre. **Tri** : nom, dernier lancement, ajout récent, temps de jeu.
- Double-clic sur une jaquette pour jouer.
- **Collections** : depuis la fiche d'un jeu (onglet *Aperçu*), créez une collection
  ou ajoutez le jeu à une collection existante.

### Fiche d'un jeu
Le bandeau affiche l'état d'installation, le temps de jeu, la dernière session et la
progression des succès quand elle est connue. Onglets : **Aperçu**, **Succès**,
**Statistiques**, **Captures** (captures Steam F12 du compte local, en lecture seule ; un clic
ouvre l'image), **Sauvegardes**, **Mods**, **Paramètres de lancement** (lancement, profils,
informations et visuels). Un onglet sans données est masqué. À la manette, LB / RB changent
d'onglet.

- **Paramètres de lancement** : nom, exécutable, arguments, répertoire de travail ; métadonnées
  (description, développeur, éditeur, date, genres) ; jaquette et bannière personnalisées.
  Vos modifications ne sont jamais écrasées par une synchronisation.
- **Jeux Steam** : sans exécutable, le jeu passe par Steam (recommandé : succès, cloud, DRM).
  Un jeu possédé mais non installé propose **Installer via Steam**.
- **Steam, sans réseau** : studio, éditeur, date de sortie, catégories (solo, coop, manette) et
  genres (étiquettes de la boutique) viennent du cache du client Steam. Vos **collections Steam**
  apparaissent dans la bibliothèque (« Steam · <nom> ») et suivent Steam. L'onglet **Mods** liste
  les objets du **Steam Workshop** installés (gérés par Steam).
- **Linux, jeux Windows** : les `.exe` passent par la commande définie dans
  **Paramètres › Linux** (`wine` par défaut ; `umu-run`, Proton…).

## 4. Émulateurs

1. **Émulateurs** : Halcyon détecte RetroArch, RPCS3, PCSX2, Dolphin, Cemu, Ryujinx,
   DuckStation, PPSSPP, melonDS et mGBA (dossiers usuels, `PATH`, Flatpak). Sinon, ouvrez
   l'émulateur (icône réglages) et indiquez son exécutable.
2. **Bibliothèques de ROM** : choisissez la plateforme, le dossier, puis *Ajouter*.
3. Pour chaque plateforme, choisissez l'**émulateur par défaut** ; un jeu peut en imposer un
   autre dans sa fiche.
4. **Profil d'émulateur** : arguments supplémentaires et lancement en plein écran.

Jeux multi-disques : un `.m3u` regroupe les disques ; un `.cue` masque ses pistes `.bin`.
Jeux PS3 : dossier contenant `PS3_GAME/USRDIR/EBOOT.BIN`.

**RetroArch** : les cœurs (ex. `snes9x`) se téléchargent depuis RetroArch
(*Menu principal › Charger un cœur › Télécharger un cœur*). Halcyon signale clairement un cœur manquant.

Halcyon ne fournit et ne télécharge aucune ROM ni aucun BIOS.

### Ajouter un émulateur
Déposez un fichier JSON dans le dossier `emulators` des données de Halcyon
(bouton dans la page Émulateurs), puis *Redétecter* :

```json
{
  "id": "xemu",
  "name": "xemu",
  "executables": { "windows": ["xemu.exe"], "linux": ["xemu"] },
  "search_paths": { "windows": ["%PROGRAMFILES%\\xemu"], "linux": ["/usr/bin"] },
  "flatpak": "app.xemu.xemu",
  "fullscreen_args": ["-full-screen"],
  "platforms": { "xbox": { "args": ["-dvd_path", "{rom}"] } }
}
```

Variables : `{rom}`, `{rom_dir}`, `{rom_name}`, `{emu_dir}`, `{core_dir}`, `{lib_ext}`.
Une nouvelle plateforme se déclare de la même façon dans le dossier `platforms`
(`{"id": "xbox", "name": "Xbox", "extensions": ["iso"]}`).

## 5. Succès

Les succès Steam et les visuels ne demandent **aucune configuration**. Les comptes se règlent
dans **Paramètres › Comptes et succès** :

| Plateforme | Ce qu'il faut | Remarques |
|---|---|---|
| Steam | **Rien** : le client Steam de ce PC suffit | Succès lus dans les fichiers que Steam enregistre sur ce PC (`appcache/stats`), compte détecté automatiquement, fonctionne hors ligne. Un jeu apparaît après sa première session sur ce PC. Rareté mondiale ajoutée quand la connexion est disponible. |
| Steam (facultatif) | Clé Web API (steamcommunity.com/dev/apikey), dans **Réglages avancés** | Récupère en ligne les succès des jeux jamais lancés sur ce PC et ajoute vos jeux possédés non installés. Les « détails de jeu » de votre profil doivent être publics. |
| RetroAchievements | Pseudo + clé Web API (paramètres du site) | ROM reconnues automatiquement par empreinte pour les consoles à cartouche (NES, SNES, N64, GB/GBC/GBA, Mega Drive, Master System, Game Gear, PC Engine, Atari 2600/7800, Lynx). Pour les jeux sur disque, saisissez l'identifiant du jeu dans sa fiche. Modes normal et Hardcore distingués. |
| Epic Games | — | Aucune API publique : non disponibles, jamais estimés. |
| Jeux standalone | — | Succès **personnalisés**, créés par vous et marqués « non vérifiés ». |

- **Synchronisation** incrémentale au démarrage (jeux non vérifiés depuis 12 h, ou dont Steam a
  enregistré de nouveaux succès depuis).
- **Pendant une partie**, le jeu lancé est vérifié régulièrement (Steam : toutes les 20 s sur le
  disque, ou toutes les 2 min en ligne avec une clé ; RetroAchievements : toutes les 90 s) ; un
  succès débloqué déclenche
  une **notification système** (elle ne prend pas le focus et n'interrompt pas le jeu).
- Page **Succès** : complétion globale, progression par plateforme, chronologie, succès
  récents et les plus rares, jeux terminés à 100 %.

**Visuels** (jaquette, bannière, logo), sans clé :

| Jeux | Source |
|---|---|
| Steam, Epic Games, GOG | Le launcher lui-même (cache local, sinon son CDN) |
| Jeux autonomes et ajoutés à la main | Recherche du titre dans la boutique Steam (correspondance exacte du nom), puis CDN Steam |
| Jeux émulés | [libretro-thumbnails](https://github.com/libretro-thumbnails) (projet RetroArch), d'après le nom de la ROM : boîte et capture |

Une clé SteamGridDB (facultative, **Réglages avancés**) complète ce qui manque encore. Les
visuels personnalisés restent prioritaires.

## 6. Mode Big Picture

Entrée : bouton **Console** du menu, touche **F11**, ou bouton **Start / Guide**
de la manette. Option « Démarrer en mode Big Picture » dans les paramètres.

| Action | Manette | Clavier |
|---|---|---|
| Naviguer | Croix / stick gauche | Flèches |
| Valider | A | Entrée |
| Retour | B | Échap |
| Rechercher | Y | Tab (puis taper directement) |
| Favori | X | — |
| Réglages | Select | Menu ou Ctrl+, |
| Quitter Big Picture | Start | F11 |

**Réglages en Big Picture** : mode Performance, mouvement, taille du texte, thème, mode du
bureau, analyse des sources, synchronisation des succès, accès aux paramètres complets (sur le
bureau, eux aussi pilotables à la manette).

La manette est ignorée quand Halcyon n'a pas le focus ou pendant une partie lancée par Halcyon.
Les touches se personnalisent dans la page **Manettes** (section 13).

## 6 bis. Minecraft (0.5.0)

Espace **Minecraft** du menu. Détails : [`MINECRAFT.md`](MINECRAFT.md).

1. **Loaders** (ou **Versions** pour du vanilla) : choisissez la version de Minecraft, le loader
   et sa version, puis **Installer**. La progression affiche fichiers et mégaoctets réels.
2. **Mods** : choisissez l'installation, cherchez un mod ; seuls les mods compatibles avec sa
   version et son loader sont proposés. Les dépendances sont ajoutées automatiquement.
   L'interrupteur active ou désactive un mod, la corbeille le supprime.
3. **Modpacks** : installez depuis Modrinth, ou importez un `.mrpack` / zip CurseForge.
4. **Jouer** : la première fois, **Autoriser** Halcyon à ajouter ses installations au launcher
   officiel (une sauvegarde de `launcher_profiles.json` est faite). **Jouer** prépare
   l'installation et ouvre le launcher officiel : choisissez le profil « Halcyon · *nom* »,
   puis Jouer. C'est le launcher officiel qui connecte votre compte Microsoft.
5. **Paramètres** : RAM, Java, arguments JVM, résolution ; mode direct (connexion Microsoft dans
   Halcyon) si vous avez un ID d'application Azure approuvé par Mojang.

## 6 bis+. GOG (1.0.0)

**Paramètres › Sources › GOG** : Halcyon lit les jeux GOG installés par **Heroic**
(Linux et Windows, natif ou Flatpak) et par **GOG Galaxy** (Windows). Les jeux Heroic se
lancent par Heroic (qui applique Wine/Proton) ; ceux de Galaxy par leur exécutable (jeux
sans DRM). Si Heroic est installé ailleurs, indiquez son dossier de configuration (celui qui
contient `gog_store`). Filtre « GOG » dans la bibliothèque. Les succès GOG ne sont pas
disponibles (pas d'API publique sans connexion au compte) : utilisez les succès
personnalisés.

## 6 bis++. Autres launchers (1.1)

**Paramètres › Plateformes › Autres launchers** : Halcyon lit les jeux **installés** de ces
launchers, sans compte à connecter ni clé. Chaque jeu se lance par son launcher, qui garde la
connexion, les mises à jour et la protection anticopie.

| Launcher | Système | Lancement | Temps de jeu |
|---|---|---|---|
| Ubisoft Connect | Windows | `uplay://` | mesuré par Halcyon |
| EA app | Windows | `origin2://` | non mesuré (l'EA app ne dit pas où le jeu est installé) |
| Xbox / Game Pass PC | Windows | comme le menu Démarrer | mesuré par Halcyon |
| Battle.net | Windows | client Battle.net | mesuré par Halcyon |
| Riot (League of Legends, VALORANT, Legends of Runeterra, 2XKO) | Windows | Riot Client | mesuré par Halcyon |
| Amazon Games | Windows | `amazon-games://` | mesuré par Halcyon |
| itch.io | Windows, Linux | exécutable du jeu | mesuré par Halcyon |

Un interrupteur par launcher. Dans la bibliothèque, un filtre apparaît pour chaque launcher qui
a des jeux. Les visuels sont cherchés par nom dans la boutique Steam (puis SteamGridDB si vous
avez une clé). Les succès de ces plateformes ne sont pas publics : utilisez les succès
personnalisés.

## 6 quater. Téléchargements (1.0.0)

Menu **Téléchargements** : tout ce que Halcyon télécharge lui-même (versions et Java de
Minecraft, loaders, mods, modpacks, mises à jour de Halcyon).

- **Tout mettre en pause / Reprendre** : les transferts attendent sans rien perdre.
- **Limite de débit** (illimitée à 50 Mio/s) et **téléchargements simultanés** (1 à 4) :
  appliqués immédiatement ; au-delà, les opérations attendent dans la file.
- Une coupure réseau ne fait pas tout recommencer : le fichier reprend où il s'était
  arrêté quand le serveur le permet, et reste vérifié par son empreinte.
- Les jeux Steam, Epic et GOG restent téléchargés par leur client : la page indique combien
  de mises à jour sont en attente et renvoie vers **Mises à jour**.

## 6 ter. Performances (1.0.0)

Menu **Performances** (icône jauge), sous Minecraft.

- **En direct** : processeur (total, courbe des 90 dernières secondes, une barre par unité de
  calcul), mémoire et mémoire d'échange, carte(s) graphique(s) avec mémoire vidéo, stockage.
  Les mesures ne tournent **que pendant que la page est ouverte** (toutes les 1,5 s) et
  s'arrêtent dès que vous la quittez.
- **Carte graphique** : utilisation et mémoire vidéo lues par le système (Windows : compteurs
  de performance ; Linux : pilote AMD, NVIDIA par NVML, autres par Vulkan). Si le pilote ne
  fournit pas une valeur, Halcyon affiche « non fourni par le pilote » : rien n'est estimé.
- **Applications les plus gourmandes** : processus regroupés par application (un navigateur
  en lance des dizaines), avec la consommation de Halcyon lui-même.
- **Recommandations** : constats tirés des mesures (mémoire presque pleine, mémoire
  d'échange utilisée, application qui occupe le processeur hors partie, carte graphique
  occupée sans jeu, disque presque plein). La seule action proposée est réversible :
  activer le mode Performance de Halcyon. Halcyon ne ferme jamais un programme et ne change
  aucun réglage de Windows ou de Linux.
- **Historique des parties** : pendant chaque partie lancée depuis Halcyon, un relevé toutes
  les 10 s (processeur, mémoire, carte graphique) ; seul le résumé est gardé (50 parties par
  jeu). Interrupteur « Mesurer pendant les parties » pour le désactiver.
- **Images par seconde et temps d'image** : Halcyon n'injecte rien dans les jeux. Il lit les
  journaux des outils dédiés :
  - **Linux — MangoHud** : dans `~/.config/MangoHud/MangoHud.conf`, ajoutez
    `output_folder=/home/vous/mangohud-logs` ; en jeu, Maj+F2 démarre et arrête
    l'enregistrement (ou `autostart_log=1`). Lancez le jeu avec MangoHud (profil de
    lancement, option « MangoHud » ou `mangohud %command%` dans Steam).
  - **Windows — PresentMon** (Intel, gratuit) : enregistrez une capture en CSV.
  - Bouton **Ouvrir un journal…** puis **Dernier journal** (le plus récent du même dossier).
    Résultat : moyenne, « 1 % low » et « 0,1 % low » (1000 / 99ᵉ et 99,9ᵉ centiles du temps
    d'image), pire image, nombre de saccades et courbe des temps d'image.

## 6 quinquies. Autres nouveautés 1.0

- **Export des statistiques** (page Statistiques) : temps par jeu (CSV), sessions (CSV)
  ou tout (JSON). Les CSV s'ouvrent dans Excel, LibreOffice ou Google Sheets.
- **Actualités** (fiche d'un jeu Steam, onglet Aperçu) : dernières annonces officielles ;
  un clic ouvre l'article. En ligne uniquement.
- **Profils d'interface** (Apparence) : enregistrez l'apparence actuelle sous un nom
  (« Bureau », « Salon »…) et basculez d'un clic.
- **Extensions** : chaque extension affiche son empreinte, et « Signée par l'auteur » si
  elle est signée ; voir [`PLUGINS.md`](PLUGINS.md#signer-une-extension-100).

## 7. Recherche avancée et collections intelligentes

La recherche tolère les fautes de frappe : « nigth owl » trouve *Night Owl*. Les résultats
approximatifs s'affichent après les correspondances exactes. Les initiales fonctionnent aussi :
« gow » trouve *God of War*. Des **filtres** se combinent aux mots libres. Le bouton **i**
à côté du champ affiche l'aide.

| Filtre | Exemple |
|---|---|
| `dev:` / `editeur:` | `dev:fromsoftware` |
| `genre:` / `tag:` | `genre:rpg tag:coop` |
| `plateforme:` / `source:` | `plateforme:snes`, `source:epic` |
| `installe:` / `favori:` / `maj:` | `installe:oui`, `favori:non`, `maj:oui` |
| `statut:` | `statut:encours`, `statut:termine`, `statut:ajouer`, `statut:abandonne` |
| `temps:` | `temps:>10h`, `temps:<30min` |
| `succes:` | `succes:manquants`, `succes:complet`, `succes:>=80` |
| négation | `-genre:sport` |

**Collections intelligentes** (bouton *Collections intelligentes* de la bibliothèque) : des
règles combinées par « toutes » ou « au moins une », avec négation possible. Exemples de
règles : jamais lancé, pas joué depuis N jours, installé récemment, statut, succès manquants,
étiquette, genre, source, plateforme, dossier, temps de jeu ou une recherche avancée. Neuf
collections sont proposées par défaut ; elles restent modifiables et supprimables. Elles se
mettent à jour seules et apparaissent dans le filtre *Collections*.

## 8. Suivi, statistiques et journal

- **Onglet Suivi** d'une fiche : statut (*À jouer, En cours, Terminé, Abandonné*), notes,
  temps de jeu, sessions et historique des versions installées. Le temps de jeu indique sa
  provenance : plateforme (Steam), mesuré par Halcyon ou corrigé à la main. Une correction
  manuelle remplace l'affichage sans effacer les mesures.
- **Statistiques** : temps par semaine et par mois, jeux les plus joués, répartition par
  source, succès débloqués, jeux terminés et sessions récentes. Chaque chiffre indique sa
  provenance : données des plateformes, sessions suivies par Halcyon ou corrections manuelles.
- **Journal** : lancements, sessions, succès, ajouts, installations, mises à jour,
  sauvegardes, mods, profils, extensions et synchronisations. Il est filtrable par catégorie
  et consultable par recherche.

### Récap et story (1.1)

**Activité › Récap** : votre semaine, votre mois ou votre année (flèches : périodes
précédentes). Temps de jeu comparé à la période d'avant, sessions, jours joués, plus longue
série, jeux découverts, succès débloqués, top 5, moment préféré, plus longue session, succès le
plus rare et **profil de joueur**. Le récap est calculé sur les sessions suivies par Halcyon
(les plateformes ne datent pas leur temps de jeu).

**Voir ma story** ouvre les mêmes chiffres en pages verticales : clic à droite ou flèche →
page suivante, clic à gauche ou ← page précédente, Échap pour fermer. **Image de cette page**
enregistre la page dans *Images/Halcyon* ; **Exporter en vidéo** produit une vidéo 1080 × 1920
dans *Vidéos/Halcyon* si [ffmpeg](https://ffmpeg.org) est installé (sinon, les pages sont
enregistrées en images).

### Confort (1.1)

- **Écran de lancement** : au clic sur *Jouer*, la bannière et le logo du jeu occupent la
  fenêtre pendant son démarrage, puis « Bonne partie ! » quand le jeu est détecté. Un clic le
  referme. *Réglages › Comportement › Écran de lancement* pour le désactiver.
- **Rappel de pause** : *Réglages › Comportement* (Jamais, 1 h, 2 h ou 3 h). Une notification
  pendant les parties suivies par Halcyon.
- **Jeux offerts** : les jeux gratuits de la semaine sur l'Epic Games Store sont signalés au
  démarrage, une fois chacun, et listés dans *Outils › Stockage*.
- **Outils › Stockage** : espace libre de chaque disque, place prise par les jeux de la
  bibliothèque, et jeux installés non lancés depuis six mois (les plus gros d'abord). Ouvrez un
  jeu pour le désinstaller depuis sa plateforme ; vos sauvegardes Halcyon et votre temps de
  jeu restent.
- **Disque presque plein** : moins de 10 % ou de 10 Go libres → une alerte, au plus une fois
  par jour (désactivable).

### Discord, configuration requise, bannière animée (1.1)

- **Statut Discord** : *Réglages › Comptes et succès › Statut Discord*. Pendant une partie,
  votre profil Discord affiche « Joue à … » avec la jaquette et la durée. Halcyon parle au
  Discord ouvert sur le PC (rien ne passe par Internet de son côté) ; le réglage « Partager
  mon activité » de Discord reste maître.
- **Configuration requise** : sur l'aperçu d'un jeu Steam, la configuration minimale et
  recommandée de la boutique à côté de votre PC. La mémoire, la mémoire vidéo, l'espace
  disque et le 64 bits sont comparés (marge de 10 % pour la mémoire réservée par le
  système) ; le processeur et la carte graphique sont affichés côte à côte, sans verdict.
- **Bannière animée** : *Réglages › Comportement*. La bannière de la fiche enchaîne les
  captures d'écran en fondu avec un lent travelling.

## 9. Profils de lancement

Onglet **Profils** de la fiche : plusieurs façons de lancer un même jeu, choisies dans le menu
à côté de *Jouer*.

| Réglage | Remarque |
|---|---|
| Arguments (ajoutés ou remplaçants), variables d'environnement, répertoire de travail | jeux lancés directement |
| Mode d'affichage, résolution, limite d'images | traduits en arguments pour Unity, Unreal, Source et Godot (moteur détecté) ; sinon signalés comme non appliqués |
| GPU préféré | Windows : préférence graphique du système (avec autorisation explicite, restaurée après la session) |
| Priorité du processus | appliquée au lancement |
| Commandes avant / après | la commande « avant » est attendue (60 s maximum) |
| Programmes associés | démarrés avec le jeu ; « fermer après » arrête seulement ceux lancés par Halcyon |
| Profil de mods, profil de manette | appliqués avant le lancement |

Un jeu Steam ou Epic se lance par sa plateforme : seuls les arguments sont transmis
(`steam://run`). Les réglages qui ne peuvent pas s'appliquer sont listés après le lancement.
Le profil **par défaut** est utilisé par le bouton *Jouer*, le mode Big Picture et le lancement
rapide. Les fichiers du jeu ne sont jamais modifiés.

## 10. Mises à jour des jeux

La page **Mises à jour** liste les jeux dont la plateforme signale une mise à jour (Steam :
disponible, en attente, en cours de téléchargement avec progression ; Heroic/Legendary :
nouvelle version). Le bouton ouvre la page du jeu dans sa plateforme, qui télécharge elle-même.
Halcyon ne remplace jamais le système de distribution des plateformes. Le launcher Epic
Games n'indique pas localement les mises à jour en attente : seule la version installée est
affichée. L'historique des versions figure sur cette page et dans l'onglet *Suivi*. Une
notification système peut signaler les nouvelles mises à jour (option de la page).

## 11. Sauvegardes

Onglet **Sauvegardes** d'une fiche, et page **Sauvegardes** pour la vue d'ensemble.

1. **Emplacements** : Halcyon propose d'abord les dossiers exacts de la **base Ludusavi**
   (communautaire, plus de 20 000 jeux, aussi dans les préfixes Proton sous Linux), puis ceux
   qu'il devine (Documents/My Games, AppData, LocalLow, Steam Cloud local, préfixes Proton,
   dossier du jeu, Unreal `Saved/SaveGames`, ROM et RetroArch). Associez-les, ou ajoutez un
   dossier ou un fichier. La base (~18 Mo) est téléchargée au plus une fois par semaine, et
   seulement si elle a changé ; un dossier partagé par plusieurs jeux n'est jamais proposé.
2. **Copies** : manuelles (*Sauvegarder maintenant*) ou automatiques **avant** et **après**
   chaque session. Une copie identique à la précédente n'est pas dupliquée. Les copies
   manuelles ne sont jamais purgées. Le nombre de copies automatiques conservées est réglable.
3. **Restaurer** demande confirmation. L'état actuel est d'abord copié (« avant restauration »).
   La copie choisie est vérifiée par empreinte, puis mise en place d'un bloc, avec retour
   arrière en cas d'erreur. Rien n'est jamais écrasé sans possibilité de récupération.
4. **Comparer** une copie à l'état actuel : fichiers ajoutés, modifiés, supprimés.
5. **Plusieurs PC** : choisissez un dépôt commun (disque secondaire, NAS, dossier synchronisé)
   et des copies miroir dans la page Sauvegardes. Chaque copie porte le nom du PC. Halcyon
   indique *à jour*, *modifié ici*, *plus récent sur un autre PC* ou *conflit*. En cas de
   conflit, vous choisissez : garder la version locale ou restaurer l'autre. Si le dépôt est
   inaccessible, les copies restent locales puis sont synchronisées à son retour. Les données
   locales ne sont jamais supprimées parce qu'une synchronisation a échoué.

## 12. Mods

Onglet **Mods** des jeux PC. Halcyon propose un **adaptateur** selon le jeu : dossier
générique, BepInEx (Unity) ou paks Unreal (`~mods`). Vous pouvez le changer.

- **Importer** une archive `.zip` ou un dossier. Le mod est copié dans la bibliothèque de mods
  de Halcyon, pas encore dans le jeu.
- **Activer, ordonner** (le dernier l'emporte en cas de conflit, conflits signalés) et
  enregistrer des **profils** de mods.
- **Déployer** copie les fichiers dans le jeu. Chaque fichier original remplacé est d'abord
  sauvegardé. **Retirer** restaure les originaux et supprime les fichiers ajoutés. Les
  fichiers modifiés depuis le déploiement sont conservés et signalés.
- **Versions** : avec une clé d'API personnelle Nexus Mods (**Paramètres**), Halcyon compare
  la version installée à la dernière publiée. Rien n'est téléchargé automatiquement.

Pour les jeux à anti-triche en ligne, n'utilisez des mods que s'ils sont autorisés par
l'éditeur.

## 13. Manettes

**Sur le bureau** (hors Big Picture), la manette pilote tout Halcyon : croix et stick
déplacent la sélection dans la bibliothèque et les menus, et le focus ailleurs (comme `Tab`) ;
A valide, B revient en arrière ou ferme, Y ouvre la recherche universelle, X ajoute aux
favoris, **Select** ouvre le menu contextuel, **LB / RB** changent de vue (bibliothèque) ou
d'onglet (fiche). Start entre en Big Picture.

Page **Manettes** : manettes connectées (marque Xbox, PlayStation, Nintendo, Valve…,
batterie, dernière utilisée). Un **profil de manette** modifie les touches de navigation de
Halcyon (Valider, Retour, Recherche, Favori, Big Picture). Pour en changer une : *Changer*,
puis appuyez sur le bouton voulu. Le profil ajoute aussi des **options par émulateur** à la
ligne de commande, par exemple `--appendconfig=…` pour RetroArch. Le profil actif s'applique
partout. Un profil de lancement d'un jeu émulé peut en imposer un autre. Pendant un jeu PC,
Halcyon n'intercepte jamais la manette : la configuration reste celle du jeu, de Steam Input
ou de votre pilote.

## 14. Apparence

Thèmes intégrés (Halcyon, Minuit, Braise, Forêt, Graphite), importés (`.halcyontheme` /
`.json`) ou fournis par une extension. Vous pouvez aussi régler :

- la couleur d'accent ;
- la densité, confortable ou compacte ;
- le **mouvement** : complet, réduit (fondus courts, sans glissement) ou aucun ;
- la **taille du texte** : 90 %, 100 %, 115 % ou 130 % ;
- le **mode d'affichage** (standard, minimaliste, cinéma), la barre latérale repliée, le mode
  Performance et son activation automatique pendant les parties ;
- une image de fond avec son intensité et un fond animé facultatif ;
- les entrées affichées (onglets d'Activité et d'Outils, bouton Console) ;
- l'ordre et l'affichage des widgets de l'accueil (largeur : bouton crayon de l'accueil).

*Exporter le thème actuel* produit un fichier partageable. *Rétablir* revient aux réglages
d'origine.

### Son (0.4.1)

**Paramètres › Son** : sons de l'interface (activés par défaut, discrets), mode silencieux,
volume général et volumes par catégorie — navigation, interaction, notifications,
important (lancement d'un jeu, succès) — avec un bouton *Écouter* par curseur. Le son au
survol est désactivé par défaut. *Pendant une partie* : silencieux (défaut), réduit ou
normal. *Rétablir* revient aux valeurs d'origine. Sans carte son, Halcyon reste muet sans
erreur. Les sons sont originaux, créés pour Halcyon : [`SON.md`](SON.md).

## 15. Extensions

Page **Extensions** : installer depuis un dossier, un `plugin.json` ou une archive `.zip` /
`.halcyonplugin`. Une extension ne contient **que des données** : émulateurs, plateformes,
emplacements de sauvegarde, thèmes, dossiers de jeux, sources de métadonnées et liens. Les
fichiers exécutables sont refusés. Elle est installée **désactivée** et s'active après
lecture de ses autorisations. Pour en créer une, voir [`PLUGINS.md`](PLUGINS.md).

## 16. Que jouer ?

Suggestions calculées **sur votre PC** à partir de vos données réelles. Choisissez le temps
disponible (15 min à toute la soirée) : Halcyon propose des jeux dont vos sessions ont une
durée compatible, puis des parties à reprendre, des jeux presque terminés (succès), à
découvrir (genres que vous jouez le plus) et à redécouvrir. Chaque suggestion indique sa
raison. *Surprends-moi* ouvre la fiche d'un jeu au hasard parmi ces suggestions.

## 17. Jeu à distance

Halcyon pilote **Sunshine** (sur le PC qui exécute les jeux) et **Moonlight** (sur l'appareil
qui affiche). Il ne réimplémente aucun protocole de diffusion.

1. Installez Sunshine sur le PC de jeu, Moonlight sur l'autre appareil. Halcyon les détecte
   (installation, `PATH`, Flatpak) ou accepte le chemin de Moonlight.
2. **Machines** : ajoutez l'adresse du PC de jeu. Halcyon indique s'il est joignable.
3. **Appairer** : Moonlight affiche un code PIN, à saisir dans l'interface web de Sunshine
   (*Ouvrir la configuration* sur le PC de jeu).
4. **Jeux** liste les applications publiées. **Diffuser** démarre Moonlight, et *Arrêter la
   session* ferme le jeu distant.
5. Depuis une fiche, **Publier dans Sunshine** ajoute le jeu aux applications de Sunshine
   après confirmation, avec une copie de sauvegarde de `apps.json`. Sunshine lance alors
   Halcyon (`--launch`), qui applique profil, mods et sauvegardes comme pour un lancement local.

## 18. Raccourcis, lancement rapide et mode compact

La **recherche universelle** (`Ctrl+K`, ou la barre de recherche en haut) trouve les jeux
(`Entrée` : jouer, `Ctrl+Entrée` : fiche), les pages, les **sections de paramètres**, les
**collections** et des **commandes** : analyser les sources, mode Performance, modes
d'affichage, vues de la bibliothèque, Big Picture, mode compact, centre de commandes,
personnaliser l'accueil, ajouter un jeu. La page **Paramètres** a sa propre recherche et un
filtre par section.

| Action | Par défaut |
|---|---|
| Recherche universelle (jeux, pages, réglages, collections, commandes) | `Ctrl+K` |
| Rechercher dans la bibliothèque | `Ctrl+F` |
| Big Picture | `F11` |
| Mode compact | `Ctrl+M` |
| Analyser les sources | `F5` |
| Surprends-moi | `Ctrl+J` |
| Paramètres | `Ctrl+,` |
| Pages (Accueil … Paramètres) | `Ctrl+1` … `Ctrl+9` |

Les raccourcis se modifient dans **Paramètres › Raccourcis clavier** : cliquez sur la
combinaison puis appuyez sur les nouvelles touches.

- **Lancement rapide** : tapez quelques lettres. `↑` et `↓` choisissent, `Entrée` lance le jeu,
  `Ctrl+Entrée` ouvre sa fiche. Les pages de Halcyon sont aussi proposées.
- **Raccourci global**, désactivé par défaut (par exemple `Ctrl+Alt+H`) : ouvre le lancement
  rapide même quand Halcyon est en arrière-plan. Sous Wayland, les applications ne peuvent pas
  enregistrer de raccourci global. Associez plutôt la commande `halcyon --quick` à un raccourci
  dans les réglages de votre bureau.
- **Mode compact** (icône à côté du logo, `Ctrl+M` ou l'icône de notification) : petite fenêtre
  avec recherche, favoris et jeux récents. Un clic lance le jeu. L'épingle la garde au premier
  plan.

## 19. Fenêtre compagnon

Option de **Paramètres › Système**, désactivée par défaut : pendant une partie lancée par
Halcyon, une petite fenêtre au premier plan affiche le temps de session, la progression des
succès et vos notes sur le jeu, modifiables.

Halcyon **n'injecte rien dans les jeux** : pas de superposition dans l'image du jeu, aucun
risque pour la stabilité ni pour les anti-triche. La fenêtre est visible en mode fenêtré sans
bordure ou sur un second écran. Pour une superposition des performances dans le jeu (images
par seconde, températures), utilisez des outils dédiés : **MangoHud** sous Linux, **RTSS** ou
la **Xbox Game Bar** sous Windows. L'overlay Steam reste disponible pour les jeux Steam.

## 20. Système

**Paramètres › Système** :

- **Démarrer avec la session** : Halcyon s'ouvre réduit dans la zone de notification. Sous
  Windows, une clé du registre utilisateur est créée. Sous Linux, un fichier est ajouté dans
  `~/.config/autostart`.
- **Fermer / réduire dans la zone de notification** : l'icône propose *Afficher*, *Lancement
  rapide*, *Mode compact*, *Big Picture* et *Quitter*. Sous GNOME, l'icône nécessite
  l'extension « AppIndicator ». Sans zone de notification, la fenêtre se réduit normalement.
- **Raccourci sur le Bureau** : bouton dans la fiche d'un jeu. Le raccourci lance le jeu via
  Halcyon, avec son profil par défaut.
- **Instance unique** : relancer Halcyon, ouvrir un raccourci de jeu ou un fichier associé
  réutilise la fenêtre déjà ouverte.
- **Associations de fichiers** : les fichiers `.halcyontheme` et `.halcyonplugin` s'ouvrent dans
  Halcyon, qui demande confirmation avant toute installation.

Ligne de commande : `halcyon --launch <id>`, `--quick`, `--big-picture`, `--minimized`,
`halcyon <fichier.halcyontheme|.halcyonplugin>`.

## 21. Hors ligne

Halcyon détecte la connexion. Hors ligne, un badge **HORS LIGNE** apparaît et tout reste
utilisable : bibliothèque, lancement, sauvegardes locales, mods, statistiques. Les fonctions en
ligne attendent le retour de la connexion : visuels, métadonnées, succès, mises à jour et
dépôt de sauvegardes distant. Au retour, elles se synchronisent seules. *Mode hors ligne
forcé* (**Paramètres › Connexion**) coupe toute requête réseau.

## 22. Exporter et restaurer

**Paramètres › Exporter ou restaurer la configuration** : une archive `.zip` contient la
bibliothèque, les métadonnées et visuels personnalisés, les collections, les profils, les
réglages des émulateurs, les thèmes, les extensions, les statistiques et les fiches des mods.
**Les fichiers des jeux n'y sont jamais inclus.** Les clés d'API en sont exclues par défaut,
tout comme les fichiers des mods ; deux options permettent de les ajouter.

La restauration affiche le contenu de l'archive et demande confirmation. Elle crée d'abord une
archive de sécurité de l'état actuel, puis redémarre Halcyon.

## 23. Mise à jour de Halcyon

Depuis la 1.0, Halcyon se met à jour tout seul (**Halcyon Update System**, détails :
[`HUS.md`](HUS.md)). Dans **Paramètres › Mises à jour** :

- **Canal** : *Stable* (versions finales) ou *Beta* (prépublications et versions stables plus
  récentes). Le choix est conservé ; passer de Beta à Stable ne fait jamais redescendre de
  version : la prochaine version stable plus récente sera proposée.
- **Rechercher automatiquement** : au démarrage puis toutes les 6 heures.
- **Mises à jour automatiques** : la nouvelle version est téléchargée et vérifiée en
  arrière-plan, puis installée au démarrage suivant de Halcyon.
- **Rechercher des mises à jour**, **Télécharger**, **Annuler** (le téléchargement reprendra
  où il s'est arrêté), **Installer et redémarrer**, **Installer au prochain démarrage**.
  La progression montre les octets reçus, la vitesse et le temps restant réels.

Une version n'est installée que si son manifeste porte la **signature Ed25519** de l'auteur
et si le paquet a la bonne **empreinte SHA-256** ; jamais une version plus ancienne. Pendant
l'installation, Halcyon se ferme, un programme séparé (`halcyon-updater`) remplace les
fichiers, relance Halcyon et vérifie qu'il démarre ; sinon l'ancienne version est restaurée
et relancée, et vous en êtes averti. Une coupure de courant pendant l'installation est
réparée au démarrage suivant. Vos jeux, paramètres et sauvegardes ne sont jamais touchés.

Linux : l'AppImage peut être installée pour votre compte depuis cette page (*Installer
Halcyon pour mon compte*). Une installation faite par le programme d'installation Linux passe
à l'AppImage à sa première mise à jour, raccourcis conservés. Le paquet `.deb` reste mis à
jour par votre gestionnaire de paquets.

**Clés d'API** (Steam, RetroAchievements, Nexus Mods, SteamGridDB, CurseForge) : depuis la
1.0, elles sont rangées dans un coffre (`<données>/secrets`, chiffré par Windows ; réservé à
votre compte sous Linux) et non plus en clair dans `settings.json`. Le déplacement est
automatique au premier lancement. Les événements sensibles sont inscrits dans
`logs/security.log` (jamais la valeur d'une clé).

**Vérification d'intégrité** (**Paramètres › Stockage et diagnostic**) : elle contrôle la base
de données, les paramètres, l'accès au dossier des données, les dossiers des jeux installés,
les jaquettes et les extensions.

## 24. Données, confidentialité, dépannage

| | Windows | Linux |
|---|---|---|
| Bibliothèque et paramètres | `%APPDATA%\Halcyon` | `~/.local/share/Halcyon` |
| Cache (miniatures, images) | `%LOCALAPPDATA%\Halcyon\cache` | `~/.cache/Halcyon` |
| Journaux (7 jours) | `%LOCALAPPDATA%\Halcyon\logs` | `~/.local/share/Halcyon/logs` |

- Tout fonctionne hors ligne ; les fonctions en ligne (visuels, métadonnées, succès, mises à
  jour) sont marquées **EN LIGNE** et désactivables.
- Services publics contactés, sans compte : boutique et API publique de Steam (visuels,
  recherche par nom, rareté des succès), libretro-thumbnails et la base Ludusavi (dépôts
  GitHub), GitHub (mises à jour de Halcyon), la liste des jeux offerts de l'Epic Games Store
  (désactivable), la configuration requise et les captures en grand de la boutique Steam.
  Avec une clé facultative : Steam Web API,
  RetroAchievements, SteamGridDB, Nexus Mods, CurseForge.
- Les clés d'API restent sur votre ordinateur, dans `settings.json`.
- `HALCYON_DEBUG=1` active des journaux détaillés ; `HALCYON_DATA_DIR` change le dossier de données.
- Base de données illisible : elle est mise de côté (`library.corrupt-…`) et reconstruite à la
  prochaine analyse.

## 25. Désinstallation

- **Programme d'installation (Windows et Linux)** : *Paramètres Windows › Applications ›
  Halcyon › Désinstaller*, ou sous Linux le menu *Gérer l'installation de Halcyon*. Il retire
  le programme, ses raccourcis, les associations de fichiers et le démarrage automatique.
  Cache et données ne sont supprimés que si vous le demandez ; les données demandent en plus
  une confirmation explicite. Ces données comprennent les copies de sauvegardes de jeux
  conservées par Halcyon. Vos jeux et leurs sauvegardes d'origine ne sont jamais touchés.
  Le même menu propose de **réparer** une installation.
- **Linux (.deb)** : `sudo apt remove halcyon`. Les données de `~/.local/share/Halcyon` sont
  conservées.
- **Linux (archive)** : `./uninstall.sh` retire le programme, les raccourcis de jeux et le
  démarrage automatique. `./uninstall.sh --purge` propose aussi de supprimer les données.
