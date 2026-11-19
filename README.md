# Halcyon

Halcyon est un launcher universel de jeux vidéo pour **Windows et Linux**. Il réunit dans
une seule bibliothèque vos jeux Steam, Epic Games (launcher Epic, Heroic, Legendary), GOG,
Ubisoft Connect, EA app, Xbox / Game Pass, Battle.net, Riot, Amazon Games, itch.io,
standalone, ajoutés à la main et **émulés**, sans compte à connecter. Il centralise les **succès** et propose un mode
**Big Picture** à la manette. Depuis la 0.5.0, un onglet **Minecraft** gère Minecraft Java
Edition (versions, loaders, mods, modpacks).

Il gère aussi les **sauvegardes** de jeux (versionnées, multi-PC), les **mods** de manière
réversible, les **profils de lancement**, les **statistiques**, des **extensions**
déclaratives et le **jeu à distance** via Sunshine/Moonlight. Il est rapide, léger et
entièrement utilisable hors ligne.

Écrit en **Rust** avec une interface **Slint** (rendu natif accéléré, aucun navigateur embarqué).

- Nouveautés de la 0.5.0 : [`docs/NOTES-0.5.0.md`](docs/NOTES-0.5.0.md) (Minecraft) ·
  [`docs/MINECRAFT.md`](docs/MINECRAFT.md)
- Nouveautés de la 0.4.1 : [`docs/NOTES-0.4.1.md`](docs/NOTES-0.4.1.md) (son, installateur) ·
  [`docs/SON.md`](docs/SON.md) · [`docs/INSTALLATEUR.md`](docs/INSTALLATEUR.md)
- Nouveautés de la 0.4 : [`docs/NOTES-0.4.0.md`](docs/NOTES-0.4.0.md) · mise à jour depuis la 0.3 :
  [`docs/MISE-A-JOUR-0.4.md`](docs/MISE-A-JOUR-0.4.md) · [`CHANGELOG.md`](CHANGELOG.md)
- Utilisation : [`docs/GUIDE.md`](docs/GUIDE.md)
- Design system : [`docs/DESIGN-SYSTEM.md`](docs/DESIGN-SYSTEM.md) · performances :
  [`docs/PERFORMANCES.md`](docs/PERFORMANCES.md) · tests : [`docs/TESTS-0.5.0.md`](docs/TESTS-0.5.0.md)
- Conception et choix techniques : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Créer une extension : [`docs/PLUGINS.md`](docs/PLUGINS.md)

## Fonctionnalités

| Domaine | Contenu |
|---|---|
| **Sources** | Steam (toutes bibliothèques, temps de jeu local, jeux possédés non installés via l'API Web), Epic (launcher Epic sous Windows, Heroic/Legendary sous Windows et Linux), dossiers de jeux (exécutables Windows et Linux natifs), ajout manuel (`.exe`, `.bat`, `.lnk`, `.url`, `.sh`, AppImage) |
| **Interface (0.4)** | Design system « Nocturne », barre latérale repliable, précédent / suivant, centre de commandes, modes standard / minimaliste / cinéma, mode Performance (manuel ou automatique en jeu), mouvement complet / réduit / aucun, texte de 90 à 130 % |
| **Accueil** | Tableau de bord de 14 widgets à déplacer, redimensionner ou masquer (un widget masqué ne calcule rien) |
| **Bibliothèque** | 5 vues (grille, liste, carrousel, immersive, compacte), jaquettes S/M/L, sélection clavier/manette conservée, menu contextuel commun, recherche instantanée (`Ctrl+F`), filtres plateforme / état / collection / genre, 4 tris, favoris, jeux masqués ; **collections** de toutes sources (sélection multiple, création guidée par la recherche, séries détectées, couleur, logo, bannière, ordre manuel) |
| **Fiche jeu** | Bandeau cinématographique, chiffres réels (temps, dernière session, succès), onglets Aperçu, Succès, Statistiques, Captures (Steam F12), Sauvegardes, Mods, Paramètres de lancement ; métadonnées et visuels modifiables (jamais écrasés par une synchronisation) |
| **Émulateurs** | Définitions JSON extensibles sans code ; RetroArch, RPCS3, PCSX2, Dolphin, Cemu, Ryujinx, DuckStation, PPSSPP, melonDS, mGBA ; détection (dossiers, PATH, Flatpak) ; ROM par plateforme, multi-disques, PS3 en dossier ; émulateur par défaut et par jeu ; profils (arguments, plein écran) |
| **Succès** | Steam (déblocages, rareté), RetroAchievements (identification des ROM par empreinte, modes normal/Hardcore, points), succès personnalisés marqués « non vérifiés » ; statistiques, chronologie, plus rares, 100 % ; synchronisation incrémentale ; surveillance pendant la partie et notifications système |
| **Big Picture** | Plein écran façon console, carrousels, fiche avec succès, recherche à clavier virtuel, réglages à la manette ; **étagère** façon coverflow (une par collection, boîtiers épais, halo, reflets, Y pour voir le dos), **musée** (un mur d'étagères par plateforme, tranches vers l'avant), dos et tranches réels en option (base LaunchBox) ; manette (XInput / evdev) ou clavier |
| **Désinstallation** | Toujours par la plateforme (Steam, Legendary, launcher) ; jeux autonomes vers la corbeille avec garde-fous ; ROM jamais supprimées ; confirmation avec « Sauvegarder avant » |
| **Manette sur le bureau** | Toute l'interface hors Big Picture : flèches, focus, A/B/X/Y, Select (menu contextuel), LB/RB (vue ou onglet) |
| **Recherche** | Tolérante aux fautes et aux sigles ; filtres `dev:`, `genre:`, `tag:`, `statut:`, `temps:>10h`, `succes:manquants`, `maj:oui`, négation ; collections intelligentes à règles |
| **Suivi** | Statut (à jouer, en cours, terminé, abandonné), notes, temps de jeu avec provenance (plateforme / mesuré / corrigé), historique des versions, statistiques par semaine et par mois, journal d'activité filtrable |
| **Profils de lancement** | Arguments, environnement, résolution, affichage, limite d'images (Unity, Unreal, Source, Godot), GPU préféré, priorité, scripts avant/après, programmes associés, profils de mods et de manette |
| **Mises à jour des jeux** | État et progression des téléchargements Steam, versions Heroic/Legendary ; téléchargement laissé aux plateformes |
| **Sauvegardes** | Détection des emplacements, copies manuelles et automatiques (avant/après session) dédupliquées, comparaison, restauration atomique précédée d'une copie de sécurité, dépôt partagé (NAS, autre disque) avec détection des conflits entre PC |
| **Mods** | Adaptateurs générique, BepInEx et paks Unreal ; import, ordre, conflits, profils ; déploiement avec copie des originaux et retrait complet ; versions Nexus Mods (clé personnelle) |
| **Personnalisation** | 5 thèmes, import/export de thèmes, couleur d'accent, densité, mouvement, taille du texte, fond (animé en option), barre latérale et widgets de l'accueil réorganisables |
| **Extensions** | Émulateurs, plateformes, emplacements de sauvegarde, thèmes, dossiers, sources de métadonnées et liens, **sans code exécuté** ; installation désactivée et activation après lecture des autorisations |
| **Manettes** | Marque, batterie, dernière utilisée ; profils de touches de navigation et options par émulateur |
| **Que jouer ?** | Suggestions locales justifiées (temps disponible, à reprendre, presque terminés, à découvrir) |
| **Jeu à distance** | Détection de Sunshine et Moonlight, machines, appairage, liste des jeux, diffusion, publication d'un jeu dans Sunshine |
| **Bureau** | Zone de notification, démarrage avec la session, instance unique, raccourcis personnalisables, raccourci global, recherche universelle (`Ctrl+K` : jeux, pages, réglages, collections, commandes), mode compact au premier plan, fenêtre compagnon de session (sans injection), raccourcis de jeux sur le Bureau, associations de fichiers |
| **Données** | Export/restauration de la configuration (sans fichiers de jeux, secrets exclus par défaut), mode hors ligne avec reprise automatique, vérification d'intégrité |
| **Minecraft (0.5.0)** | Versions officielles Mojang et Java adapté, Fabric / Quilt / Forge / NeoForge, mods et modpacks Modrinth (CurseForge avec clé), lancement par le launcher officiel ou connexion Microsoft directe (ID Azure approuvé) |
| **Son (0.4.1)** | Identité sonore originale (28 sons synthétisés), moteur audio dédié, volumes par catégorie, silencieux pendant les parties par défaut |
| **Distribution** | Programme d'installation Halcyon pour Windows et Linux (vérification du système, progression réelle, mise à jour, réparation, désinstallation avec données conservées), version portable Windows, `.deb` ; mise à jour intégrée vérifiée par SHA-256 |

## Mesures (0.4.0, bibliothèque de 5 000 jeux, Linux, rendu logiciel, release)

| | |
|---|---|
| Interface prête (lancement, base remplie) | ~87 ms |
| Mémoire à l'accueil | ~69 Mo |
| Mémoire après défilement complet de la grille | ~104 Mo (cache d'images borné) |
| Analyse de 5 000 manifestes Steam + fusion en base | < 1 s, en arrière-plan |
| CPU au repos | 0,1 % (réveils au repos divisés par deux par rapport à la 0.3) |

Méthode et comparaison avec la 0.3.0 : [`docs/PERFORMANCES.md`](docs/PERFORMANCES.md).

## Compiler

Prérequis : Rust stable 1.95+ (<https://rustup.rs>) et **8 Go de RAM** (le code généré par
Slint demande environ 5 Go par compilation). Sous Linux :
`sudo apt install libxkbcommon-dev libfontconfig1-dev libudev-dev libdbus-1-dev libasound2-dev pkg-config`.

```bash
cargo run --release -p halcyon-app      # lancer
cargo test --workspace -j 1             # 293 tests (Linux)
cargo build --release -p halcyon-app -p halcyon-installer
sh tools/package_linux.sh               # .deb, programme d'installation, archive, SHA256SUMS
```

Programmes d'installation (Windows et Linux), signature : [`docs/INSTALLATEUR.md`](docs/INSTALLATEUR.md).

**Sans rien installer** : poussez ce dépôt sur GitHub. La CI (`.github/workflows/build.yml`)
teste sous Windows et Linux et produit les programmes d'installation, la version portable, le
`.deb` et l'archive Linux ; un tag `v*` publie une version GitHub (utilisée par la vérification de mise à jour).

## Développement

```bash
python3 tools/make_demo_fixture.py /tmp/halcyon-demo    # fausses installations Steam/Epic, dossiers, ROM, émulateur
HALCYON_DATA_DIR=/tmp/halcyon-demo/data cargo run -p halcyon-app
```

## Limites connues (honnêtes)

- **Lancer sans le client Steam/Epic** : impossible proprement (DRM, Steamworks). Halcyon passe
  par le client sans afficher sa bibliothèque ; les arguments ne sont pas transmis via la plateforme.
- **Succès Epic** : Epic ne publie pas d'API — non affichés, jamais estimés.
- **RetroAchievements sur disque** (PS1, PS2, GameCube…) et ROM en `.zip` : pas d'empreinte
  automatique ; l'identifiant se saisit dans la fiche du jeu.
- **Heroic** : lancement par son protocole `heroic://launch` ; si Heroic n'est pas installé,
  la commande `legendary launch` est utilisée.
- **Jeux à lanceur intermédiaire** : la durée mesurée peut être courte (le lanceur se ferme).
- **Métadonnées des jeux non Steam** : visuels trouvés sans clé (boutique Steam pour les jeux
  autonomes, libretro-thumbnails pour les jeux émulés) quand le titre correspond ; pas de
  description sans source tierce. Édition manuelle et jaquettes personnalisées disponibles.
- **Succès Steam sans clé** : lus dans les fichiers du client Steam de ce PC ; un jeu jamais
  lancé sur ce PC n'en a pas encore (une clé Web API facultative les récupère en ligne).
- **Mise à jour de Halcyon sous Linux** : installé par le programme d'installation, Halcyon
  télécharge et lance le nouveau programme vérifié ; installé par le `.deb`, le paquet est
  installé par votre gestionnaire. Halcyon ne se remplace jamais lui-même en cours d'exécution.
- **Minecraft** : la connexion Microsoft directe n'a pas pu être testée sans ID d'application
  Azure approuvé par Mojang ; le mode par défaut passe par le launcher officiel.
- **Windows** : sans certificat, SmartScreen affiche « Éditeur inconnu ».
- **Mises à jour du launcher Epic officiel** : non visibles localement, donc non signalées.
- **Vidéos** (bandes-annonces) : non lues (pas de lecteur vidéo dans Slint) ; captures et
  visuels uniquement.
- **Captures** : celles de Steam (F12) seulement ; pas d'emplacement fiable pour les autres.
- **Manette hors bibliothèque** : parcours dans l'ordre du focus (comme Tab), pas de
  navigation spatiale.
- **Superposition en jeu** : volontairement absente (anti-triche, stabilité). Une fenêtre
  compagnon séparée la remplace ; MangoHud et RTSS restent recommandés pour les performances.
- **Raccourci global sous Wayland** : interdit par le protocole. Liez `halcyon --quick` dans
  les réglages du bureau.
- **Zone de notification sous GNOME** : nécessite l'extension AppIndicator.
- **Extensions** : déclaratives uniquement. Pas de nouvelles sources de jeux ni de succès
  fournis par des tiers.
- **Fonctions propres à Windows** : préférence GPU, raccourcis `.lnk`, clé de démarrage, icône de
  notification `tray-icon`, installateur. Le développement s'est fait sous Linux, sans
  pouvoir compiler ni exécuter ces parties. La CI Windows (`.github/workflows/build.yml`) les
  compilera et lancera les tests au premier envoi sur GitHub : à vérifier avant de diffuser.

## Licences

Code : MIT. Interface : [Slint](https://slint.dev) (licence royalty-free pour applications desktop).
Police : Archivo (SIL Open Font License, `crates/halcyon-app/ui/fonts/LICENSE-Archivo.txt`).
