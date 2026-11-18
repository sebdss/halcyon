# Halcyon — Analyse technique et architecture

> Launcher universel de jeux vidéo pour **Windows et Linux**.
> Ce document présente l'analyse du besoin, le comparatif des technologies, l'architecture
> et l'état d'avancement : étapes 1 à 7 du cahier des charges, l'extension V1 (section 5), la
> version 0.4.0 « Premium Console Experience » (section 5 bis) et la 0.4.1 « Original Sound
> Identity & Custom Premium Installer » (section 5 ter).

## 1. Analyse du besoin

Les huit priorités du cahier des charges se ramènent à quatre contraintes techniques :

| Contrainte | Conséquence technique |
|---|---|
| Légèreté (priorité n°1) : démarrage rapide, peu de RAM/CPU, des milliers de jeux | Rendu natif ou GPU, pas de moteur web ; listes **virtualisées** ; images décodées **à la taille d'affichage** et mises en cache ; aucun service en arrière-plan quand l'appli est fermée |
| Interface haut de gamme + mode Big Picture manette | Moteur UI déclaratif avec animations, dégradés, ombres, polices embarquées ; navigation au focus clavier (réutilisée par la manette) |
| Nombreuses sources hétérogènes (Steam, Epic, standalone, émulateurs, succès) | **Adaptateurs** indépendants derrière des contrats communs ; format de données unique ; synchronisation asynchrone qui ne bloque jamais l'UI |
| Fiabilité hors ligne | Base locale SQLite (WAL) comme source de vérité ; réseau optionnel et clairement signalé ; écritures atomiques de la config |

## 2. Comparatif des technologies desktop

Mesures indicatives pour une fenêtre affichant une grille de jaquettes (Windows 11) :

| Technologie | RAM au repos | Binaire | Qualité graphique | Maintenance / typage | Verdict |
|---|---|---|---|---|---|
| **Electron** (Chromium + Node) | 250–400 Mo, 4+ processus | 150 Mo+ | Excellente (CSS) | TypeScript | ❌ Exactement le « web encapsulé gourmand » à éviter |
| **Tauri** (Rust + WebView2) | 120–200 Mo (processus WebView2) | 5–10 Mo | Excellente (CSS) | Rust + TS | ⚠️ Bon compromis, mais la RAM reste celle d'un navigateur ; deux langages |
| **WPF / WinUI 3** (.NET) | 100–180 Mo | 60 Mo+ (self-contained) | Bonne (Playnite utilise WPF) | C# | ⚠️ Windows uniquement, WinUI 3 encore instable, WPF vieillissant |
| **Avalonia** (.NET + Skia) | 80–130 Mo | 60 Mo+ | Très bonne | C# | ⚠️ Solide, mais runtime .NET + GC ; démarrage à froid plus lent |
| **Qt / QML** (C++) | 60–100 Mo | 30 Mo+ (DLL Qt) | Excellente | C++ | ⚠️ Performant, mais C++ plus coûteux à maintenir ; déploiement Qt lourd |
| **Rust + Slint** | **40–60 Mo** (mesuré : 39 Mo vide, 51 Mo avec 5 000 jeux) | **un seul exécutable** (34 Mo en 0.1, 50 Mo en 0.3 sous Linux) | Très bonne (GPU, animations, polices embarquées) | Rust, typage fort, UI compilée | ✅ **Retenu** |
| Rust + egui / iced | 30–60 Mo | ~10 Mo | egui : look « outil » ; iced : moins de contrôle visuel | Rust | ❌ Pas adapté à une UI premium |

### Choix : **Rust + Slint** (rendu OpenGL via femtovg, repli logiciel)

- **Performances** : pas de navigateur, pas de GC, UI compilée en code natif. Démarrage mesuré
  à ~50 ms avec 5 000 jeux, RAM dominée par les jaquettes visibles (décodées à la bonne taille).
- **Qualité visuelle** : langage déclaratif `.slint` (proche de QML) avec animations
  d'état, dégradés, ombres portées, coins arrondis, clipping, police Archivo embarquée.
- **Maintenabilité** : un seul langage typé pour la logique, l'UI est vérifiée à la compilation
  (une propriété mal nommée = erreur de build, pas un bug silencieux).
- **Distribution** : un exécutable unique (50 Mo en 0.3 avec police, icônes et définitions), sans runtime à installer.
- **Licence Slint** : licence royalty-free gratuite pour les applications desktop (mention
  « Made with Slint » dans l'écran À propos), ou GPLv3.

Bibliothèques retenues (minimales) : `rusqlite` (SQLite embarqué), `image` (JPEG/PNG/WebP
uniquement), `ureq` (HTTP synchrone dans les threads de travail, TLS rustls), `serde_json`,
`base64`, `log`, `dirs`, `md-5` (empreintes RetroAchievements), `gilrs` (manettes),
`notify-rust` (notifications système), `rfd` (boîtes de dialogue natives / portail XDG),
`sha2` et `zip` (sauvegardes, mods, exports, mises à jour vérifiées), `sysinfo` (détection des
jeux en cours d'exécution), `global-hotkey` (raccourci global), `cpal` (sortie audio, 0.4.1). Sous Windows uniquement :
`winreg` (registre) et `tray-icon` (zone de notification). Sous Linux uniquement : `ksni`
(StatusNotifierItem par D-Bus) et `libc` (priorité des processus). Aucun runtime async : quelques threads dédiés suffisent et
coûtent moins cher qu'un exécuteur.

## 3. Architecture

```
┌──────────────────────────────── halcyon-app (binaire) ────────────────────────────────┐
│  UI Slint (ui/*.slint)      Contrôleurs Rust          Cache d'images (LRU + disque)   │
│  Accueil · Bibliothèque  ◄─ Controller ─────────────► ImageCache (threads de décodage)│
│  Fiche · Paramètres         │  invoke_from_event_loop                                  │
└─────────────────────────────┼──────────────────────────────────────────────────────────┘
                              │ threads de fond → invoke_from_event_loop
┌──────────────────────────────▼─────────────┐   ┌──────────────────────────────────────┐
│ halcyon-integrations                        │   │ halcyon-storage                      │
│  steam · epic · standalone (GameScanner)    │   │  SQLite (WAL, migrations versionnées)│
│  launcher : lancement (URI / exécutable)    │──►│  repo : fusion des scans sans doublon│
│  artwork : visuels manquants (en ligne)     │   │  Settings JSON (écriture atomique)   │
│  emulators · roms · rahash (émulation)      │   │  Logger fichier rotatif              │
│  steam_web · retro (succès) · update        │   │  achievements : succès, statistiques │
└──────────────────────────────┬─────────────┘   └──────────────────┬───────────────────┘
                               └──────────────┬─────────────────────┘
                              ┌───────────────▼────────────────┐
                              │ halcyon-core                    │
                              │  Modèle (Game, GameSource…)     │
                              │  Contrats (trait GameScanner…)  │
                              │  Requête bibliothèque (pure)    │
                              │  Erreurs, utilitaires           │
                              └─────────────────────────────────┘
```

Règles de dépendance : `core` ne dépend de rien (hors serde) ; `storage` et `integrations`
ne connaissent pas l'UI ; seul `app` connaît Slint. Toute la logique testable (filtres,
parseurs, fusion) vit hors de l'UI et est couverte par des tests unitaires.

### 3.1 Contrats d'extension

- `GameScanner` : `fn scan(&self, ctx) -> Result<ScanReport>` — produit des `DiscoveredGame`
  (format commun). Ajouter une plateforme = un module qui implémente ce trait.
- `LaunchTarget` : chaque `Game` produit sa cible de lancement (URI de plateforme **ou** exécutable +
  arguments + répertoire de travail). Le lanceur ne connaît pas les plateformes.
- **Émulateurs** : définitions **déclaratives** en JSON (`data/emulators.json`,
  `data/platforms.json` intégrés, complétés par `<données>/emulators/*.json` et
  `<données>/platforms/*.json`) : exécutables par système, dossiers de recherche, identifiant
  Flatpak, dossiers des cœurs, gabarits d'arguments par plateforme (`{rom}`, `{core_dir}`,
  `{lib_ext}`…). Ajouter un émulateur ne demande **aucune** modification du code. Les ROM sont
  une source comme une autre (`RomScanner` implémente `GameScanner`).
- **Succès** : format commun `AchievementData` (identifiant plateforme, nom, description, icône,
  rareté, points, déblocage normal/Hardcore, progression). Chaque fournisseur (Steam Web API,
  RetroAchievements) produit ce format ; la fusion (`merge_achievements`) est transactionnelle,
  conserve les icônes en cache et renvoie les **nouveaux** déblocages (notifications). Les succès
  **personnalisés** ont leur propre fournisseur (`custom`) et ne se synchronisent jamais.
  Aucune donnée n'est inventée : un champ non fourni reste vide.

### 3.2 Fusion des sources (prévention des doublons)

Chaque jeu a une clé naturelle unique `(source, source_key)` :
Steam → `appid`, Epic → `AppName`, standalone → chemin normalisé de l'exécutable.
Un scan produit la liste complète d'une source ; la fusion :

1. insère les nouveaux jeux, met à jour les existants **sans écraser** les champs que
   l'utilisateur a modifiés (masque `overrides`) ;
2. marque comme **désinstallés** (sans les supprimer) les jeux absents du scan — favoris,
   temps de jeu et personnalisations sont conservés ;
3. le tout dans une transaction : une coupure en plein scan ne corrompt rien.

### 3.3 Performances

| Problème | Solution |
|---|---|
| Milliers de jaquettes | Grille **virtualisée** (seules les lignes visibles existent) ; le modèle Slint est implémenté en Rust et déclenche le chargement d'une image **uniquement quand sa ligne est demandée** |
| Décodage d'images | Threads dédiés, redimensionnement à la taille de la carte, miniature JPEG mise en cache sur disque (le 2ᵉ lancement ne redécode pas les originaux), LRU en mémoire borné |
| Recherche / filtres | Bibliothèque en mémoire (quelques Mo pour 10 000 jeux), filtrage pur en Rust : < 5 ms pour 10 000 jeux (mesuré par test) |
| Scan | Thread de travail ; l'UI reste interactive et affiche la progression |
| Base de données | SQLite WAL, index sur `(source, source_key)`, écritures groupées en transaction |
| Arrière-plan | Aucun processus résident ; un thread par jeu lancé pour mesurer la session ; la surveillance des succès s'arrête avec la partie (8 h max) |
| Succès | Synchronisation incrémentale (12 h ou jeu relancé), requêtes espacées, table d'empreintes RetroAchievements en cache 7 jours |
| Big Picture | Seules ~9 jaquettes par rangée existent (fenêtre glissante autour de l'élément actif) |
| Manette | Thread `gilrs` bloquant (aucune scrutation active), actions ignorées si la fenêtre n'a pas le focus ou pendant une partie |

### 3.4 Fiabilité

- **Config** : `settings.json` écrit en fichier temporaire puis renommé (jamais à moitié écrit) ;
  un fichier illisible est sauvegardé en `.bak` et remplacé par les valeurs par défaut.
- **Base** : WAL + transactions ; migrations numérotées (`PRAGMA user_version`).
- **Logs** : `logs/halcyon-AAAA-MM-JJ.log`, 7 jours conservés ; les paniques sont journalisées
  avec leur emplacement.
- **Lancement** : vérification de l'exécutable et du répertoire avant lancement, message
  clair en cas d'échec ; jeux désinstallés signalés.
- **Mode portable** : un fichier `portable.txt` à côté de l'exécutable place données et
  cache dans le dossier `data/` voisin.

Emplacements : Windows `%APPDATA%\Halcyon` (données) et `%LOCALAPPDATA%\Halcyon\cache` ;
Linux `~/.local/share/Halcyon` et `~/.cache/Halcyon` (XDG).

### 3.5 Compatibilité Windows / Linux

| Domaine | Windows | Linux |
|---|---|---|
| Steam | registre, `steamapps` | `~/.steam/steam`, `~/.local/share/Steam`, Flatpak ; jeux Proton via Steam |
| Epic | launcher Epic (manifestes, catalogue) ; Heroic/Legendary | Heroic (natif, Flatpak) ou Legendary (`installed.json`, cache Heroic) |
| Exécutables | `.exe`, `.bat`, `.lnk`, `.url` | ELF, `.sh`, `.x86_64`, AppImage ; `.exe` via Wine/Proton (commande configurable) |
| Émulateurs | dossiers usuels, `%APPDATA%`, Scoop, PATH | PATH, `~/.local/bin`, `~/Applications`, Flatpak |
| Ouverture d'URI / dossiers | gestionnaire d'URL Windows, Explorateur | `xdg-open` |
| Boîtes de dialogue | API Windows | portail XDG (GNOME, KDE…) |
| Manette | XInput | evdev (`libudev`) |
| Notifications | notifications Windows (AppUserModelID déclaré par l'installateur) | D-Bus (freedesktop) |
| Son (0.4.1) | WASAPI (cpal) | ALSA (cpal), PipeWire et PulseAudio par leur couche ALSA |
| Zone de notification | `tray-icon` | StatusNotifierItem (`ksni`) ; GNOME : extension AppIndicator |
| Démarrage avec la session | `HKCU\…\Run` | `~/.config/autostart/halcyon.desktop` |
| Raccourci global | `RegisterHotKey` | X11 ; impossible sous Wayland (commande `--quick`) |
| Raccourcis de jeux | `.lnk` (WScript.Shell) | `.desktop` (menu et Bureau) |
| Associations | registre (installateur Halcyon) | type MIME + `MimeType=` |
| Paquets | installateur Halcyon (0.4.1 ; Inno Setup jusqu'à la 0.4.0), zip portable | installateur Halcyon (par utilisateur), `.deb`, archive + `install.sh` (0.4.0) |

### 3.6 Mesures

Bibliothèque de 5 000 jeux générée (80 % avec jaquette), Linux, renderer logiciel de Slint :

| Mesure | Résultat |
|---|---|
| Mémoire, application vide | 39 Mo |
| Mémoire, accueil avec 5 000 jeux | 51 Mo (release), 69 Mo (debug) |
| Démarrage jusqu'à l'interface, 5 000 jeux en base | ~50 ms (release) |
| Mémoire après défilement complet de la grille et retour | plafond ~106 Mo, stable (pas de fuite) |
| Analyse de 5 000 manifestes Steam + fusion SQLite | 0,8 s (thread de fond) |
| Requête bibliothèque sur 10 000 jeux | < 5 ms (test automatisé, build debug) |
| CPU au repos | 0 % (aucun rendu tant que rien ne change) |

Ces chiffres excluent le pilote graphique : avec OpenGL, la mémoire du pilote s'ajoute
(sur un vrai GPU, l'essentiel des textures vit en mémoire vidéo). Contrôle en 0.3.0 (release,
rendu logiciel) : 51 Mo à l'accueil (40 jeux), démarrage en 92 ms (53 jeux). Avec OpenGL émulé par le processeur
(llvmpipe, sans GPU), la mémoire du pilote porte le total à environ 195 Mo.

## 4. Ce qui est possible — et ce qui ne l'est pas

| Fonctionnalité | Faisabilité | Approche |
|---|---|---|
| Jeux Steam **installés** | ✅ hors ligne | `libraryfolders.vdf` + `appmanifest_*.acf` |
| Jaquettes Steam | ✅ hors ligne si déjà affichées dans Steam | `appcache/librarycache` ; sinon CDN Steam (en ligne) |
| Temps de jeu / dernier lancement Steam | ✅ hors ligne | `userdata/<id>/config/localconfig.vdf` |
| Jeux Steam **possédés non installés** | ✅ en ligne + clé | Steam Web API `GetOwnedGames` ; bouton « Installer via Steam » (`steam://install`) |
| Lancer un jeu Steam sans l'interface Steam | ⚠️ partiel | `steam://rungameid/<id>` : le **client Steam doit tourner** (DRM, Steamworks). Il est démarré en arrière-plan sans afficher la bibliothèque. Lancer l'exe directement contournerait Steam : non fait. |
| Jeux Epic installés | ✅ hors ligne | manifestes `*.item` |
| Jaquettes / descriptions Epic | ✅ hors ligne (URL) + en ligne (images) | `catcache.bin` du launcher Epic |
| Lancer un jeu Epic | ⚠️ partiel | URI `com.epicgames.launcher://…?action=launch&silent=true` : le launcher Epic gère l'authentification, en mode silencieux |
| Succès Steam (sans clé) | ✅ hors ligne | `appcache/stats/UserGameStatsSchema_<appid>.bin` (définitions, noms localisés, icônes) + `UserGameStats_<compte>_<appid>.bin` (bits débloqués, dates), KeyValues binaires ; compte = SteamID64 − 76561197960265728, détecté dans `loginusers.vdf` ; rareté par `GetGlobalAchievementPercentagesForApp` (sans clé, en ligne) |
| Succès Steam (clé facultative) | ✅ en ligne + clé API | `GetSchemaForGame` + `GetPlayerAchievements` ; jeux jamais lancés sur ce PC ; repli sur la lecture locale en cas d'erreur ; profil public requis |
| Visuels des jeux autonomes | ✅ en ligne, sans clé | `store.steampowered.com/api/storesearch` (nom identique exigé, type `app`) puis CDN Steam ; SteamGridDB (clé facultative) pour le reste |
| Fiches Steam hors ligne | ✅ hors ligne | `appcache/appinfo.vdf` v27–v29 (table des noms en v29) : `associations`, `steam_release_date`, `category`, `store_tags`, `controller_support` ; noms des étiquettes par `IStoreService/GetTagList` (public), cache mensuel |
| Collections Steam | ✅ hors ligne | `userdata/<compte>/config/cloudstorage/cloud-storage-namespace-1.json` (`user-collections.*`, sans `filterSpec`) |
| Steam Workshop | ✅ hors ligne + titres en ligne | `steamapps/workshop/appworkshop_<appid>.acf` ; titres par `ISteamRemoteStorage/GetPublishedFileDetails` (public) |
| Emplacements de sauvegarde | ✅ base locale, mise à jour en ligne | Manifeste Ludusavi (`raw.githubusercontent.com/mtkennerly/ludusavi-manifest`), `ETag`, hebdomadaire ; analyseur YAML strict du sous-ensemble utilisé ; correspondance par numéro Steam/GOG, nom, alias, dossier ; motif coupé au premier joker, dossier refusé s'il est partagé par plusieurs jeux du manifeste |
| Visuels des jeux émulés | ✅ en ligne, sans clé | `raw.githubusercontent.com/libretro-thumbnails/<système>/master/Named_{Boxarts,Snaps}/<titre>.png` ; système déclaré par `libretro` dans `platforms.json`, caractères `&*/:`<>?\|"` remplacés par `_` |
| Succès Epic | ❌ pas d'API publique | Affiché comme « non disponible », jamais inventé |
| RetroAchievements (cartouches) | ✅ en ligne (clé Web API) | Empreinte MD5 normalisée (en-têtes NES/SNES/PCE/Lynx/7800, ordre d'octets N64) comparée à `API_GetGameList` |
| RetroAchievements (disques, `.zip`) | ⚠️ manuel | Empreintes de disque non implémentées : identifiant saisi dans la fiche |
| Notifications pendant le jeu | ✅ | Interrogation périodique du jeu lancé + notification système (ne prend pas le focus) |
| Jeux Epic sous Linux | ✅ via Heroic / Legendary | `heroic://launch?appName=…&runner=legendary` ou `legendary launch` |
| Mise à jour de Halcyon | ✅ en ligne | Version GitHub, paquet adapté au système, empreinte vérifiée par `SHA256SUMS` ; programme d'installation lancé (Windows, et Linux s'il a fait l'installation), sinon gestionnaire de paquets |
| Mises à jour des jeux Steam | ✅ hors ligne | `StateFlags`, `TargetBuildID`, `BytesDownloaded/ToDownload` des manifestes ; téléchargement par Steam |
| Mises à jour Epic (launcher officiel) | ❌ pas d'information locale | version installée seule ; Heroic/Legendary : comparaison avec le cache `assets.json` |
| Superposition dans le jeu | ❌ volontairement | injection = risque anti-triche et stabilité ; fenêtre compagnon séparée, MangoHud / RTSS recommandés |
| Raccourci global sous Wayland | ❌ protocole | Wayland interdit l'écoute globale du clavier ; commande `halcyon --quick` à lier dans le bureau |
| Jeu à distance | ✅ via Sunshine/Moonlight | pilotage du client `moonlight` (list, pair, stream, quit), publication dans `apps.json` de Sunshine |
| Succès fournis par extension | ❌ volontairement | seules les plateformes officielles et les succès créés par le joueur sont affichés |

## 5. Extension V1 (0.3.0)

### 5.1 Modules ajoutés

| Domaine | Logique pure (testée) | Intégration / stockage | Interface |
|---|---|---|---|
| Recherche floue, filtres | `core::search` (Damerau-Levenshtein borné, sigles, filtres `champ:valeur`, négation) | — | bibliothèque, lancement rapide, mode compact |
| Collections intelligentes | `core::rules` (20 règles + négation, JSON) | table `smart_collections` | `pages/smart.slint` |
| Suivi, statistiques, journal | `core::stats` (semaines/mois en heure locale) | tables `version_history`, `activity` (purge au-delà de 20 000) | `stats`, `journal`, onglet Suivi |
| Profils de lancement | `core::profiles` (plan : arguments moteur, environnement, priorité) | `integrations::{engines, gpu, launcher}`, table `launch_profiles` | onglet Profils |
| Mises à jour des jeux | `UpdateState` | `steam::update_info`, `epic` (Legendary), sondage 8 s pendant un téléchargement | `updates` |
| Sauvegardes | — | `integrations::saves` : détection, copies dédupliquées par empreinte, restauration atomique (copie de sécurité + vérification + échange de dossiers), état multi-PC | onglet et page Sauvegardes |
| Mods | — | `integrations::mods` : adaptateurs (générique, BepInEx, paks Unreal), plan avec conflits, déploiement avec copie des originaux, retrait | onglet Mods |
| Export / restauration | — | `config_archive` (base copiée par `VACUUM INTO`, secrets exclus, aucun fichier de jeu) | Paramètres |
| Hors ligne | — | sonde TCP toutes les 45 s ; reprise des tâches au retour | badge, Paramètres |
| Extensions | — | `integrations::plugins` (manifeste, permissions, refus des exécutables) | `plugins` |
| Apparence | — | `ctl_themes` (thèmes JSON, accent, densité, fond) | `appearance` |
| Manettes | — | `gamepad` (marque, batterie, association des touches, capture) | `controllers` |
| Que jouer ? | `core::recommend` (raisons explicites, données réelles) | — | `assistant`, accueil |
| Jeu à distance | — | `integrations::remote_play` | `remote` |
| Système | — | `desktop` (démarrage, raccourcis), `instance` (instance unique), `tray`, `global-hotkey`, `update::download_verified` | Paramètres, mode compact, fenêtre compagnon |

### 5.2 Décisions

- **Aucune injection dans les jeux** : la « superposition » demandée par le cahier des charges
  a été étudiée. Une vraie superposition en jeu suppose de détourner le rendu (DirectX, Vulkan
  ou OpenGL). Ce procédé est détecté par les anti-triche (EAC, BattlEye, Vanguard) et peut
  provoquer des plantages. La solution retenue est une fenêtre compagnon au premier plan,
  désactivée par défaut, avec les outils dédiés en complément (MangoHud, RTSS, overlay Steam).
- **Extensions déclaratives** : pas d'exécution de code tiers. Les apports sont des données
  validées : JSON, pointeurs JSON, gabarits d'URL limités aux domaines déclarés. Une extension
  est installée désactivée, avec une confirmation explicite qui détaille les autorisations.
- **Sauvegardes** : jamais d'écrasement sans copie préalable. Une restauration se déroule en
  quatre temps : copie de l'état actuel, extraction dans un dossier temporaire, vérification
  des empreintes, puis échange de dossiers avec retour arrière. En cas de conflit entre PC,
  aucun choix automatique n'est fait.
- **Mods** : réversibles. Les originaux remplacés sont copiés et les dossiers créés sont
  enregistrés. Un fichier modifié par le joueur depuis le déploiement n'est jamais supprimé.
- **Distribution des plateformes respectée** : les mises à jour de jeux sont lues, jamais
  téléchargées par Halcyon.
- **Instance unique** : canal TCP local (`127.0.0.1`, port choisi par le système) protégé par
  un jeton aléatoire, stocké dans `instance.json` (droits `0600` sous Unix).
- **Mise à jour vérifiée** : sans fichier `SHA256SUMS` publié, rien n'est installé. La CI le
  génère à chaque version.
- **Mémoire de compilation** : le code Rust généré par Slint pour l'interface est volumineux.
  La compilation incrémentale est désactivée en `dev` pour rester sous 5 Go.

## 5 bis. Version 0.4.0 — interface et expérience console

**Audit préalable** : `docs/AUDIT-0.3.md` (fonctions à préserver, schéma, mesures de départ,
risques). Constat principal : le compilateur Slint recopiait (« inline ») les composants dont
la racine porte une animation, une opacité, une ombre ou une visibilité conditionnelle, d'où
28 Mo de code généré, 5 Go de mémoire de compilation et un coût de création de l'interface.
Les composants 0.4 gardent une racine nue et placent les effets dans un enfant : à interface
égale, le code généré est passé de 28,3 à 22,5 Mo ; avec tous les nouveaux écrans de la 0.4, il
revient à 28,8 Mo (mesures : `docs/PERFORMANCES.md`).

**Coquille** (`ui/shell.slint`, `src/ctl_shell.rs`) : barre latérale et barre supérieure en
positionnement absolu (une mise en page dépendant de la largeur de la fenêtre créait une
boucle de liaison), historique de navigation générique (`src/nav.rs`), centre de tâches
(`src/tasks.rs`) alimenté par les opérations longues, modes d'interface et mode Performance
appliqués au thème (`Theme.motion`, `Theme.effects`, `Theme.text-scale`).

**Accueil** (`src/ctl_home.rs`) : la disposition des widgets est calculée en Rust (rangées de
trois colonnes, pas de trou) ; chaque widget n'est calculé que s'il est affiché ; disques et
captures sont lus sur un thread. Le même module construit le **menu contextuel** des jeux,
une superposition pilotable au clavier plutôt qu'une fenêtre surgissante.

**Bibliothèque** (`src/ctl_library.rs`) : sélection mémorisée par identifiant (elle survit
aux filtres et aux vues) ; la grille et la liste restent virtualisées ; le carrousel et la
vue immersive ne reçoivent qu'une fenêtre de 21 jeux autour de la sélection ; l'ambiance
floutée est une miniature de 48×72 décodée et floutée une fois puis mise en cache
(`images::Kind::Ambient`).

**Manette sur le bureau** : le thread des manettes envoie des touches à la fenêtre
(`Window::dispatch_event`) ; toute l'interface est donc pilotable par les chemins déjà testés
au clavier. Le thread ne se réveille plus qu'une fois par seconde au repos.

**Recherche universelle** (`src/ctl_palette.rs`) : jeux, pages, sections de paramètres,
collections et commandes, avec les mêmes règles de correspondance (sans accents, tous les
mots).

**Données** : schéma v5 (un index), nouveaux champs d'apparence avec reprise des réglages 0.3
(`Appearance::widgets()`, `motion()`), aucune donnée réécrite au premier lancement.

## 5 ter. Version 0.4.1 — identité sonore et installateur

**Audit préalable** : `docs/AUDIT-0.4.md`. Aucun son n'existait ; l'installation reposait
sur Inno Setup (Windows) et un script shell (Linux), sans vérification ni réparation.

**Son** (`crates/halcyon-audio`, `docs/SON.md`) : catalogue typé (`Cue` : identifiant,
catégorie, priorité, délai minimal), mélangeur pur et testé (voix limitées par catégorie,
redémarrage des voix de navigation, éviction par priorité, rééchantillonnage linéaire), sortie
`cpal` sur un fil dédié. Le périphérique s'ouvre au premier son et se referme après 20 s : au
repos, le fil attend sur son canal sans se réveiller. Le rappel audio prend le mélangeur par
`try_lock` (silence plutôt qu'attente). Un seul point d'entrée : `Controller::sound`, et le
global Slint `Sound.play(id)` qui y aboutit. Les sons sont synthétisés par
`tools/sound/make_sounds.py` et intégrés au binaire.

**Installateur** (`crates/halcyon-installer`, `docs/INSTALLATEUR.md`) : bibliothèque testée
(`payload` : archive ajoutée à l'exécutable, compatible Authenticode ; `engine` : détection,
vérifications, transaction avec journal et retour arrière, réparation, désinstallation ;
`platform` : chemins et intégration Windows/Linux, isolés par `Env` pour les tests) et
binaire Slint `halcyon-setup` qui réutilise le thème et les composants de l'application.
Le même exécutable, copié sans sa charge utile dans le dossier d'installation, sert de
programme de maintenance. Les entrées `.desktop` sont désormais produites par
`halcyon_core::desktop`, partagé par l'application et l'installateur.

## 5 quater. Version 1.0.0-alpha.1 — Performance Center et sécurité

**Audit préalable** : `docs/AUDIT-1.0.md`. Constats principaux : clés d'API en clair dans
`settings.json`, mises à jour vérifiées en intégrité mais pas en authenticité, aucune mesure
système (le Performance Center demandé n'existait pas).

**Sécurité** (`docs/SECURITE.md`) :
- `halcyon-storage::secrets` : `SecretStore`, un fichier par clé (DPAPI sous Windows,
  `0600`/`0700` sous Unix), écriture atomique synchronisée. `Settings::load` / `save`
  l'utilisent de façon transparente (`SECRET_FIELDS`) : le reste du code ne change pas.
- `halcyon-storage::audit` : `security.log`, lignes JSON, 512 Kio + un fichier précédent,
  initialisé par `main`. Événements émis par les contrôleurs (extensions, Minecraft) et la
  mise à jour.
- `halcyon-integrations::update` : signature Ed25519 de `SHA256SUMS` (`ed25519-compact`,
  aucune dépendance, sans génération de clés dans l'application). Clé publique fixée à la
  compilation (`option_env!("HALCYON_UPDATE_PUBKEY")`).

**Performance Center** :
- `halcyon-integrations::sysmon` : `Monitor` (sysinfo : processeur, mémoire, processus,
  disques, cartes graphiques via la fonctionnalité `gpu` — PDH/D3DKMT sous Windows, sysfs,
  NVML ou Vulkan sous Linux) ; logique pure testée : regroupement des processus,
  `History` (fenêtre glissante), `recommend`, `SessionAccumulator`.
- `halcyon-integrations::frametimes` : lecture des CSV MangoHud / PresentMon, statistiques
  (centiles, saccades), réduction de la courbe en gardant les pics.
- `halcyon-storage::perf` + migration v6 (`perf_sessions`, purge à 50 parties par jeu).
- `src/ctl_perf.rs` : deux fils de mesure (`perf-live` : 1,5 s, seulement page ouverte ;
  `perf-session` : 10 s pendant une partie), arrêtés par drapeau + `unpark` (aucune attente).
  Interface : `ui/perf_state.slint` (global `PerfUi` dans un fichier séparé, pour ne pas
  alourdir `state.slint`) et `ui/pages/performance.slint`.

**Version** : `version_newer` suit semver pour les pré-versions (`1.0.0-alpha.1` < `1.0.0`).

## 5 quinquies. Halcyon Update System (1.0)

Crate `halcyon-update` (bibliothèque sans interface, plus deux exécutables) : `version`
(SemVer strict), `core` (canaux, états, sélection sans retour arrière), `manifest`,
`security` (Ed25519, SHA-256, anti-retour, structure des paquets), `network` (GitHub
Releases, ETag), `download` (HTTP Range), `install` (transaction ZIP / AppImage / migration),
`recovery` (état, journal, verrou, santé), `runner` (déroulement de l'updater, décisions au
démarrage), `client` (nature de l'installation, lancement de l'updater). Exécutables :
`halcyon-updater` (installe Halcyon fermé, contrôle de santé, retour arrière, reprise) et
`hus-release` (publication). Lanceur : `src/ctl_update.rs`, `ui/hus_state.slint`. Pas de
runtime asynchrone : un fil de fond par opération, comme le reste de l'application.
Détails : [`HUS.md`](HUS.md).

## 6. Arborescence

```
halcyon/
├── Cargo.toml                    # workspace
├── docs/                         # ARCHITECTURE, GUIDE, PLUGINS, DESIGN-SYSTEM, PERFORMANCES, SON, INSTALLATEUR, notes et tests 0.4.x
├── examples/plugins/             # extension d'exemple (liens, thème, sauvegardes)
├── crates/
│   ├── halcyon-core/             # modèle, contrats, recherche, règles, profils, stats, suggestions (pur, testé)
│   │   └── src/{achievements,launch_args,lib,model,profiles,query,recommend,rules,scanner,search,stats,util}.rs
│   ├── halcyon-storage/          # SQLite (migrations v1→v6), settings, journal, suivi, logs, chemins
│   │   └── src/{achievements,activity,audit,db,lib,logging,migrations,paths,perf,repo,saves_mods,secrets,settings,tracking}.rs
│   ├── halcyon-integrations/     # plateformes, émulation, succès, lancement, sauvegardes, mods, extensions
│   │   ├── data/{platforms,emulators}.json
│   │   └── src/{archive,artwork,config_archive,emulators,engines,epic,frametimes,gpu,launcher,lib,mods,plugins,process,rahash,remote_play,retro,roms,saves,standalone,steam,steam_web,steamgriddb,sysmon,vdf}.rs
│   ├── halcyon-audio/            # 0.4.1 : catalogue des sons, mélangeur, sortie cpal ; sounds/*.wav
│   ├── halcyon-installer/        # 0.4.1 : programme d'installation (payload, engine, platform, ui/setup.slint)
│   ├── halcyon-update/           # 1.0 : HUS (bibliothèque, halcyon-updater, hus-release, keys/release.pub, tests/)
│   └── halcyon-app/              # binaire : UI Slint + contrôleurs
│       ├── build.rs              # compilation de l'UI, icône Windows
│       ├── ui/                   # app, theme, types, state, components, shell, pages/, icons/, fonts/
│       └── src/{controller,ctl_achievements,ctl_assistant,ctl_backup,ctl_bigpicture,ctl_emulators,ctl_home,ctl_library,ctl_mods,ctl_palette,ctl_pads,ctl_perf,ctl_plugins,ctl_profiles,ctl_remote,ctl_saves,ctl_shell,ctl_system,ctl_themes,ctl_tracking,desktop,dialogs,gamepad,images,instance,main,models,nav,notify,perf,remote,tasks,tray}.rs
├── packaging/                    # windows/ (icône ; halcyon.iss, Inno Setup, hors CI), linux/ (.desktop, MIME, install/uninstall de l'archive)
├── tools/                        # make_demo_fixture.py, make_big_library.py, bench.py, package_linux.sh, sound/make_sounds.py
└── .github/workflows/            # build.yml (tests, compilation), release.yml (publication signée sur tag v*)
```

## 7. Feuille de route

| Étape | Contenu | État |
|---|---|---|
| 1 | Conception (ce document) | ✅ |
| 2 | Socle : workspace, SQLite, config, logs, navigation | ✅ |
| 3 | Bibliothèque : Steam, Epic, standalone, ajout manuel, recherche/filtres/tri, lancement, temps de jeu | ✅ |
| 4 | Émulateurs : définitions JSON, détection, ROM, lancement | ✅ |
| 5 | Succès : Steam, RetroAchievements, personnalisés, stats, notifications | ✅ |
| 6 | Big Picture (manette via `gilrs`), optimisation, tests grande bibliothèque | ✅ |
| 7 | Installeurs Windows/Linux, documentation, vérification de mise à jour | ✅ |
| + | Compatibilité Linux complète | ✅ |
| V1+ lot 1 | Suivi, statistiques, journal, collections intelligentes, recherche avancée, profils de lancement, mises à jour des jeux | ✅ |
| V1+ lot 2 | Sauvegardes (multi-PC), mods, export/restauration, hors ligne avancé | ✅ |
| V1+ lot 3 | Apparence, extensions, manettes avancées, assistant local, jeu à distance | ✅ |
| V1+ lot 4 | Zone de notification, démarrage, instance unique, raccourcis, lancement rapide, mode compact, fenêtre compagnon, mise à jour vérifiée, intégrité, désinstallation propre | ✅ |
| 0.4.0 | Design system, coquille, accueil en widgets, bibliothèque 5 vues, fiche cinématographique, manette sur tout le bureau, centre de commandes, modes et mode Performance, recherche universelle étendue, optimisations mesurées | ✅ |
| 0.4.1 | Identité sonore originale et moteur audio, programme d'installation Windows/Linux (vérification, progression réelle, mise à jour, réparation, désinstallation) | ✅ Linux · Windows à valider sur machine réelle |
| 0.5.0 | Minecraft Java Edition : versions, loaders, mods, modpacks, launcher officiel, connexion Microsoft | ✅ Windows · Linux sans réseau |
| 1.0.0-alpha.1 | Audit 1.0, coffre des secrets, mises à jour signées, journal de sécurité, Performance Center (monitoring, applications, recommandations, historique par partie, FPS par journaux) | ✅ Linux · Windows à valider sur machine réelle |

Pistes suivantes : gestionnaire de téléchargements unifié, empreintes RetroAchievements des jeux sur disque, GOG via Heroic,
validation de bout en bout sous Windows (préférence GPU, `.lnk`, icône de notification,
installateur) sur un vrai PC, puis profils de manette appliqués aux jeux via Steam Input.
