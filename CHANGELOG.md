# Journal des modifications

## 1.0.0-alpha.5 — Le launcher le plus complet

Sept launchers de plus, sauvegardes Ludusavi, récap et story, statut Discord,
configuration requise, et les succès Steam sans clé.

### Sept launchers de plus

- **Ubisoft Connect, EA app, Xbox / Game Pass PC, Battle.net, Riot, Amazon Games** (Windows)
  et **itch.io** (Windows et Linux) : jeux installés lus sans compte ni configuration, lancés
  par leur launcher (connexion, mises à jour et protection anticopie restent les siennes).
  Temps de jeu mesuré par Halcyon (sauf EA app, qui ne dit pas où le jeu est installé).
- Visuels cherchés par nom dans la boutique Steam (puis SteamGridDB) ; jaquette itch.io reprise
  du reçu de l'application itch.
- **Paramètres › Plateformes › Autres launchers** : un interrupteur par launcher, avec son état.
- Bibliothèque : un filtre par launcher, affiché seulement s'il a des jeux ; règle « source »
  des collections intelligentes étendue.
- Un visuel dont l'adresse ne répond plus est cherché ailleurs (boutique Steam, SteamGridDB).

### Récap et story

- **Activité › Récap** : semaine, mois ou année (et les périodes précédentes) : temps de jeu
  comparé à la période d'avant, sessions, jours joués, plus longue série, jeux découverts,
  succès, top 5, moment préféré, plus longue session, succès le plus rare, profil de joueur
  (« le noctambule », « le marathonien »…). Calculé sur les sessions suivies par Halcyon.
- **Story** au format vertical : jusqu'à 9 pages animées aux couleurs du jeu le plus joué,
  clic à gauche ou à droite, flèches, Échap. **Image de cette page** (PNG) et **vidéo**
  1080 × 1920 avec fondu entre les pages (assemblée par ffmpeg s'il est installé ; sinon les
  pages sont enregistrées en images).
- Le récap de la semaine écoulée est annoncé au démarrage, une seule fois.

### Confort

- **Écran de lancement** : bannière et logo du jeu en plein cadre pendant son démarrage,
  puis « Bonne partie ! » quand le jeu apparaît (la réduction de Halcyon vient ensuite).
  Un clic le referme ; désactivable dans Réglages › Comportement.
- **Rappel de pause** toutes les 1, 2 ou 3 heures pendant une partie suivie (désactivé :
  « Jamais » ; 2 h par défaut).
- **Jeux offerts sur l'Epic Games Store** signalés au démarrage, une fois chacun (API
  publique de la boutique, sans compte ; désactivable).
- **Outils › Stockage** : espace libre de chaque disque, part occupée par les jeux de la
  bibliothèque, et jeux installés non lancés depuis six mois, les plus gros d'abord.
- **Alerte de disque presque plein** (moins de 10 % ou de 10 Go), au plus une fois par jour.

### Discord, configuration requise, bannière animée

- **Statut Discord** (« Joue à … », jaquette, durée) pendant une partie, par la connexion
  locale de Discord (tube nommé ou socket, sans compte ni réseau côté Halcyon). Effacé à la
  fin de la session ; désactivable, identifiant d'application personnalisable.
- **Configuration requise** sur l'aperçu des jeux Steam (et des jeux dont les visuels
  viennent de la boutique) : minimale, recommandée et ce PC côte à côte. Mémoire, mémoire
  vidéo, espace disque et 64 bits sont comparés ; processeur et carte graphique sont
  affichés sans verdict. Fiche gardée 30 jours.
- **Bannière animée** : la fiche enchaîne bannière et captures 1080p en fondu, avec un lent
  travelling ; arrêtée hors de l'écran, coupée en mode Performance, sans travelling en
  mouvement « Réduit ».
- Actualités de la fiche : annonces officielles du studio seulement (les flux de presse
  agrégés par Steam mélangeaient les langues).

### Steam hors ligne : fiches, collections, Workshop

- **Fiches lues dans le cache du client** (`appinfo.vdf`, versions 27 à 29) : studio, éditeur,
  date de sortie, solo / multijoueur / coop / manette, sans requête. Genres tirés des
  étiquettes de la boutique (« Rogue-like », « Monde ouvert »…), dans la langue de Halcyon
  (liste publique, sans clé, renouvelée une fois par mois).
- **Collections Steam** reprises sous le nom « Steam · <nom> », tenues à jour à chaque analyse ;
  les autres collections ne sont jamais touchées. Collections dynamiques ignorées.
- **Steam Workshop** : onglet Mods, objets installés (titre, taille, date), lien vers leur
  page ; ils restent gérés par Steam.

### Sauvegardes reconnues par la base Ludusavi

- Emplacements **exacts** de plus de 20 000 jeux (base communautaire Ludusavi, licence MIT),
  proposés en premier et associés automatiquement quand ils existent ; règles Windows des
  jeux Steam cherchées dans leur préfixe Proton sous Linux.
- Base téléchargée au plus une fois par semaine, seulement si elle a changé ; une copie qui
  ne se lit pas entièrement est refusée et l'ancienne garde la main.
- Jamais de dossier trop large : ni racine (dossier du jeu, Documents, AppData), ni dossier
  partagé par plusieurs jeux (une restauration écraserait leurs sauvegardes).

### Succès et visuels sans clé

- **Succès Steam sans clé d'API** : lus dans les fichiers que le client Steam enregistre sur ce
  PC (`appcache/stats`), compte détecté automatiquement, hors ligne compris. Noms et
  descriptions dans la langue de Halcyon quand le jeu les fournit. Rareté mondiale ajoutée en
  ligne, sans clé. Suivi pendant la partie toutes les 20 s depuis le disque.
- **Visuels sans clé** : jeux autonomes et ajoutés à la main cherchés dans la boutique Steam (nom
  identique exigé) ; jeux émulés illustrés par libretro-thumbnails (boîte et capture).
- **Paramètres › Comptes et succès** simplifié : état de Steam affiché, clés d'API (Steam,
  SteamGridDB, Nexus Mods) rangées dans **Réglages avancés**, toutes facultatives.
- Un jeu Steam jamais lancé sur ce PC indique « pas encore enregistré par Steam » au lieu d'une
  erreur.

### Audit de sécurité et corrections

- **Extensions** : un émulateur fourni par une extension ne peut plus lancer un shell ou un
  interpréteur (`sh`, `cmd`, `powershell`, `python`, `java`, `wine`…) ; le résumé d'activation
  nomme les programmes lancés ; plus de types de fichiers exécutables refusés (`.jar`, `.lnk`,
  `.AppImage`, `.desktop`…).
- **Scripts Windows** (`.bat`, `.cmd`) lancés directement : leurs arguments ne sont plus
  interprétés par `cmd /C` (`&`, `|`, `%`).
- **Restauration d'une archive** : la confirmation rappelle qu'elle contient des commandes
  exécutées au lancement des jeux.
- **Succès Steam** : un fichier local sans succès reconnu n'efface plus les succès déjà connus ;
  rareté hors bornes ignorée.
- **Visuels** : un visuel introuvable n'est plus recherché à chaque démarrage (nouvel essai après
  7 jours) ; requêtes vers libretro-thumbnails espacées.
- **Traductions** : trois messages des profils d'interface, restés en français, sont traduits.
- **Qualité** : avertissements de Clippy corrigés sur tout l'espace de travail.

## 1.0.0-alpha.4 — « Berge », refonte de l'interface

Nouvelle direction artistique, appliquée à toute l'application, au mode console et à
l'installateur. Détails : [`docs/DESIGN-SYSTEM.md`](docs/DESIGN-SYSTEM.md). Navigation
inchangée : mêmes six espaces, mêmes onglets, mêmes noms, même ordre, mêmes destinations.

- **Identité** : couleurs du logo avec un rôle chacune (bleu : où l'on est ; orange : jouer et
  ce qui est vivant), ardoise mate sans verre ni dégradé décoratif, police Archivo (Expanded
  pour les titres et les chiffres), icônes redessinées au trait carré, petits rayons,
  « perche » d'accent pour l'élément actif. Le bouton **Jouer** est orange partout.
- **Coque** : colonne de navigation avec libellés (icônes seules sous 1080 px de large),
  compteurs en chiffres plutôt qu'en pastilles, barre supérieure avec états réels (en jeu,
  Performance, hors ligne), centre de commandes en tiroir, notifications inversées.
- **Accueil** : « Reprendre » en deux parties (texte et chiffres à gauche, planche à droite),
  widgets en rubriques sans cartes, « Personnaliser l'accueil » en pied de page.
- **Bibliothèque** : index des filtres à gauche (sources, affichage, collections en un clic,
  genre), barre d'outils dédiée, liste en tableau, légendes des tuiles (titre, source, temps).
- **Fiche de jeu** : bande illustrée nette, jaquette qui chevauche son bord, chiffres réels à
  droite des actions, fiche technique en groupes (suivi, installation, édition, liens).
- **Activité, Outils, Système** : chiffres-titres (complétion, temps total), rubriques à
  filet, chronologie du journal, rangées à perche pour les transferts, instruments du
  Performance Center, miniatures des thèmes, colonne de lecture des paramètres.
- **Minecraft** : bloc d'herbe dessiné au pixel et filet herbe et terre sous les onglets.
- **Thème conservé** : l'apparence des préversions reste disponible (« Alcyon (1.0
  préversion) »), comme « Nocturne (0.5) ».
- La ligne de provenance des jeux ne répète plus le temps de jeu (affiché à part).

## 1.0.0-alpha.3 — Alignements

- Compteurs des onglets (Mises à jour, Émulateurs…) qui débordaient de leur bouton, notamment
  avec la mise à l'échelle de Windows : position calculée, plus de débordement.
- Icônes centrées verticalement sur le texte voisin dans toutes les rangées (paramètres,
  barre de recherche, téléchargements, succès, notifications, Minecraft…).
- Titres de section alignés sur les boutons qui les accompagnent.
- Bouton à icône seule (collections intelligentes, fenêtre étroite) sans marge vide.
- Première version distribuée par la mise à jour automatique (HUS) depuis la 1.0.0-alpha.2.

## 1.0.0-alpha.2 — Nouveau logo

- Nouveau logo de Halcyon : application, programme d'installation, icônes Windows et Linux.
- Première version signée : les mises à jour automatiques (HUS) peuvent l'installer, et elle
  pourra recevoir automatiquement les versions suivantes. Une installation en
  1.0.0-alpha.1 (non signée) la signale sans l'installer : installez-la une fois à la main.

## 1.0.0-alpha.1 — « Performance First » (première étape de la 1.0)

Évolution de la 0.5.0 : aucune fonction retirée, données de la 0.5.0 reprises (schéma v6 :
une table ajoutée). Détails : [`docs/NOTES-1.0.0.md`](docs/NOTES-1.0.0.md) · audit :
[`docs/AUDIT-1.0.md`](docs/AUDIT-1.0.md) · sécurité : [`docs/SECURITE.md`](docs/SECURITE.md).

### Halcyon Update System (nouveau)
- **Mises à jour automatiques** : recherche au démarrage puis toutes les 6 heures (requêtes
  conditionnelles ETag), téléchargement en arrière-plan reprenable (HTTP Range), installation
  au démarrage suivant ou tout de suite, redémarrage automatique.
- **Canaux Stable et Beta** (SemVer strict), choisis dans *Paramètres › Mises à jour*, jamais
  modifiés sans vous ; pas de retour vers une version plus ancienne.
- **Sécurité** : manifeste signé Ed25519 (version, canal, paquets, SHA-256), vérifié avant le
  téléchargement puis à nouveau par l'updater ; contrôle de structure des paquets avant
  extraction ; anti-retour et plancher de sécurité ; HTTPS obligatoire ; aucun script exécuté.
- **Updater indépendant** (`halcyon-updater`, sans interface) : attend la fermeture de
  Halcyon, installe de façon transactionnelle, relance Halcyon et attend la preuve qu'il
  démarre ; sinon restaure l'ancienne version et la relance. Coupure de courant ou arrêt
  brutal : reprise au démarrage suivant. Version précédente conservée.
- **Windows** : paquet `Halcyon-<version>-Windows-x64.zip`, installation utilisateur sans
  droits administrateur (installateur et version portable mis à jour sur place).
- **Linux** : `Halcyon-<version>-x86_64.AppImage` (moteur type 2, sans FUSE 2), installation
  pour le compte dans `~/.local/opt/halcyon` depuis les paramètres ; une installation par le
  programme d'installation Linux passe à l'AppImage à sa première mise à jour, raccourcis
  conservés. Le `.deb` reste mis à jour par apt.
- **Publication** : workflow `release.yml` (tag `vX.Y.Z` → Stable avec validation humaine,
  `vX.Y.Z-beta.N` → Beta), permissions minimales, manifeste signé, vérification des fichiers
  publiés ; outil `hus-release`. Les anciens fichiers restent publiés pour les clients 0.5.
- Remplace l'ancienne mise à jour (vérification de `SHA256SUMS`, lancement de
  l'installateur). Détails : [`docs/HUS.md`](docs/HUS.md) · tests : [`docs/TESTS-HUS.md`](docs/TESTS-HUS.md).

### Nouvelle expérience (UI/UX 1.0)
- **Bureau, nouvelle navigation** : le menu de 19 entrées devient un **rail de six espaces**
  (Accueil, Bibliothèque, Minecraft, Activité, Outils, Système), avec pastilles réelles
  (téléchargements actifs, mises à jour en attente, nouvelle version, partie Minecraft en cours).
  Les espaces Activité (Que jouer ?, Succès, Statistiques, Journal), Outils (Téléchargements,
  Mises à jour, Sauvegardes, Émulateurs, Manettes, Jeu à distance, Extensions) et Système
  (Performances, Apparence, Paramètres) ont une **barre d'espace à onglets** commune, avec
  compteurs ; chaque espace rouvre le dernier onglet consulté. Console et analyse des sources en
  bas du rail. Le réglage « barre latérale repliée » devient « Menu : icônes seules ».
- **Mode console** (Big Picture) repensé pour la télévision : sections Jouer / Bibliothèque /
  Recherche, illustration plein écran du jeu sélectionné, rangée de jaquettes, fiche plein
  écran, menu d'options du jeu, centre de contrôle (mode Performance, pause des
  téléchargements, son, mouvement, taille du texte, thème…), barre d'aide contextuelle.
- **Accueil** plein cadre (illustration du jeu à reprendre, texte à gauche), étagères de jeux
  sans cadre sur le fond ; seules les données (stats, stockage, succès…) restent en panneaux.
- **Fiche de jeu** : bandeau sans raccord, titre en Sora, chiffres clés lisibles.
- **Paramètres en deux colonnes** : catégories avec icônes et recherche à gauche, une catégorie
  à la fois à droite (« Tous les réglages » reste disponible).
- **Bibliothèque** : en-tête et filtres regroupés sur deux rangées (recherche dans l'en-tête,
  source, filtres et tri sur la seconde).
- **Typographie** : police d'affichage **Sora** (SIL OFL) pour les titres, sections et grands
  chiffres ; Inter pour le reste. Notifications aux couleurs Alcyon avec ligne d'horizon.
- Mesures (binaire de test, 3 lancements) : mémoire au repos 84–87 Mo (96–97 Mo avant la
  refonte), processeur au repos 0,2–0,3 %.

### Refonte graphique « Alcyon »
- Nouvelle identité tirée du martin-pêcheur : fonds bleu d'eau profonde, accent cobalt,
  orange réservé à ce qui est vivant (en jeu, hors ligne, alertes, mises à jour).
- Signature unique, la **ligne d'horizon** (dégradé cobalt → orange) : page active du menu,
  onglet actif, barres de progression, marque.
- Plus aucune étiquette en capitales espacées (badges, groupes du menu, cartes, installateur) ;
  titres de page plus grands ; panneaux plus arrondis, bordures teintées plus légères ;
  menu en pilules ; contrastes vérifiés (texte discret porté à 5,1:1).
- Logo et icônes Windows/Linux aux nouvelles couleurs.
- L'ancienne apparence reste disponible : thème « Nocturne (0.5) » ; accent « Martin-pêcheur »
  ajouté aux couleurs proposées.
- Aucun coût : ni ombre, ni flou, ni animation ajoutés ; code d'interface généré légèrement
  réduit. Détails : [`docs/DESIGN-SYSTEM.md`](docs/DESIGN-SYSTEM.md).

### Performance Center (nouveau)
- Page **Performances** : processeur (total, par unité de calcul, courbe), mémoire et
  mémoire d'échange, cartes graphiques (utilisation, mémoire vidéo), stockage, mesurés par
  `sysinfo` (fonctionnalité `gpu` activée) uniquement pendant l'affichage de la page.
- Applications les plus gourmandes (processus regroupés), coût de Halcyon affiché.
- Recommandations à partir des mesures (mémoire, mémoire d'échange, processeur hors partie,
  carte graphique occupée, disque presque plein) ; action réversible : mode Performance.
- Historique des performances par partie (schéma v6, table `perf_sessions`, 50 parties par
  jeu, désactivable) : un relevé toutes les 10 s pendant le jeu.
- FPS, 1 % low, 0,1 % low, 99ᵉ centile, saccades et courbe des temps d'image depuis les
  journaux CSV de MangoHud et PresentMon (`halcyon-integrations::frametimes`).
- Outil de mesure du coût : `cargo run --release -p halcyon-integrations --example sysmon_bench`.

### Sécurité
- Coffre des secrets (`halcyon-storage::secrets`) : clés d'API hors de `settings.json`,
  DPAPI sous Windows, `0600`/`0700` sous Linux, migration automatique, export inchangé
  (clés incluses seulement sur demande).
- `settings.json` écrit en `0600` sous Unix (et synchronisé sur disque avant renommage).
- Signature Ed25519 de `SHA256SUMS` (`SHA256SUMS.sig`) vérifiée avant toute mise à jour
  quand la clé publique est embarquée (`HALCYON_UPDATE_PUBKEY`) ; outil `release_sign`
  (génération des clés, signature, vérification) ; étape de signature dans la CI.
- Journal `logs/security.log` (`halcyon-storage::audit`).

### Bibliothèque : GOG (nouveau)
- Source **GOG** : jeux installés par **Heroic** (natif, Flatpak ; `gog_store/installed.json`
  et cache de bibliothèque pour titres et visuels), lancés par `heroic://launch?…&runner=gog`,
  et par **GOG Galaxy** sous Windows (registre), lancés par leur exécutable (jeux sans DRM).
- Paramètres › Sources (activation, dossier Heroic forcé), filtre « GOG » de la bibliothèque,
  règle « Source : GOG » des collections intelligentes.

### Téléchargements (nouveau)
- Page **Téléchargements** : vitesse, volume reçu, file, tâches en cours et terminées,
  annulation, état des mises à jour de jeux des plateformes.
- Contrôle commun `halcyon-core::transfer` : **pause et reprise** globales, **limite de
  débit** (seau à jetons), **téléchargements simultanés** (1 à 4, les suivants attendent).
  Appliqué à Minecraft (versions, Java, bibliothèques, loaders, mods, modpacks) et à la mise
  à jour de Halcyon.
- **Reprise après coupure** : un fichier partiel reprend par requête `Range` (repart de
  zéro si le serveur ne le permet pas) ; l'empreinte porte toujours sur le fichier entier.

### Statistiques
- Export **CSV** (temps par jeu, sessions ; BOM UTF-8, formules de tableur neutralisées)
  et **JSON** (tout), depuis la page Statistiques.

### Game Hub
- **Actualités** des jeux Steam dans la fiche (API publique `ISteamNews`, sans clé ;
  en ligne seulement, une requête par jeu et par heure ; liens HTTPS uniquement).

### Extensions
- **Empreinte du contenu** et **signature Ed25519 facultative de l'auteur** (`plugin.sig`,
  outil `release_sign sign-plugin`) ; contenu signé modifié après coup : extension inutilisable.
- **Épinglage** : une extension signée ne peut être remplacée que par une version signée par
  la même clé. État affiché dans la page Extensions et dans la confirmation d'installation.

### Personnalisation
- **Profils d'interface** : enregistrer l'apparence actuelle sous un nom et l'appliquer d'un
  clic (20 au plus) ; le mode Performance en cours est conservé.

### Corrections
- Comparaison de versions conforme à semver pour les pré-versions (`1.0.0-alpha.1` < `1.0.0`),
  dans l'application **et dans l'installateur** (qui proposait « Réinstaller » au lieu de
  « Mettre à jour » pour une pré-version).
- Minecraft, Performances et Téléchargements ajoutés au lancement rapide.
- Libellés des sources des collections intelligentes générés depuis le modèle.

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
