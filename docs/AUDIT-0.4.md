# Audit de la 0.4.0 avant la 0.4.1 (identité sonore et installateur)

## 1. Architecture actuelle

Workspace Rust de quatre crates :

| Crate | Rôle | Réutilisable pour la 0.4.1 |
|---|---|---|
| `halcyon-core` | modèle, recherche, règles, statistiques (pur, testé) | — |
| `halcyon-storage` | SQLite (schéma v5), `settings.json` (écriture atomique, valeurs par défaut serde), chemins, journaux | **paramètres audio** : nouveau bloc `sound` avec `#[serde(default)]` (migration implicite) ; chemins de données (pour la désinstallation) |
| `halcyon-integrations` | plateformes, lancement, mises à jour (`update::pick_asset`, `download_verified`, `parse_sums`), archives (`zip`) | **vérification SHA-256** et lecture `SHA256SUMS` ; `zip` pour la charge utile de l'installateur |
| `halcyon-app` | binaire Slint + contrôleurs (`Controller`, modules `ctl_*`) | design system « Nocturne » (`ui/theme.slint`, `ui/components.slint`), polices, icônes ; centre de tâches (`tasks.rs`) ; `desktop.rs` (démarrage auto, raccourcis) |

- **Interface** : Slint 1.18 (rendu femtovg ou logiciel), globals partagés (`AppUi`, `Nav`, `LibraryUi`…), callbacks implémentés en Rust.
- **Fils d'exécution** : boucle Slint ; travaux de fond sur des threads, retour par `controller::post`.
- **Navigation** : `Nav.go` / historique (`nav.rs`) ; changement de page → `Actions.page-shown`.
- **Fenêtres** : fenêtre principale, mode compact (`MiniWindow`), fenêtre compagnon, Big Picture (même fenêtre).
- **Notifications** : bandeaux `Controller::toast` (enregistrés dans le centre de commandes), notifications système `notify.rs` (succès).
- **Évènements** : il n'existe pas de bus d'évènements générique ; les évènements passent par des méthodes du contrôleur (`toast`, `task_*`, `on_navigated`, `play`, `on_session_*`, `toggle_favorite`…) et par des callbacks Slint. **Ces points d'entrée uniques sont les crochets naturels du son** : pas besoin de créer un second système.
- **Lancement des jeux** : `launcher` (URI de plateforme ou exécutable), suivi de session par thread, `active_session`.
- **Mises à jour de Halcyon** : `update.rs` choisit l'actif (`*-setup.exe` sous Windows, `.deb` ou archive `linux*.tar.gz` sous Linux), vérifie l'empreinte, puis lance l'installateur (Windows) ou ouvre le dossier (Linux).
- **Packaging** : Windows = Inno Setup (`packaging/windows/halcyon.iss`, AppId `{6B0E3B4F-…}`, installation par utilisateur ou pour tous, `unins000.exe`) ; Linux = `.deb` + archive avec `install.sh` (fichiers dans `~/.local/bin`, `~/.local/share`) ; CI GitHub (`build.yml`).

## 2. Technologies

Rust 1.95, Slint 1.18, rusqlite (SQLite intégré), serde, ureq (rustls), image, gilrs, sysinfo, zip, sha2, notify-rust, rfd, global-hotkey, ksni (Linux), tray-icon et winreg (Windows).

## 3. Ressources

- Graphiques : logo et icônes SVG (`ui/icons`), `halcyon.png`, `halcyon.ico`, police Inter (OFL).
- **Audio : aucune ressource, aucune dépendance audio.** Tout est à créer.

## 4. Limites techniques constatées

| Point | Conséquence |
|---|---|
| Pas de sortie audio dans Slint | Il faut une bibliothèque de sortie audio : **cpal** (WASAPI sous Windows, ALSA sous Linux ; PipeWire et PulseAudio passent par leur couche ALSA). Aucun décodeur nécessaire si les sons sont en WAV PCM, lus par un petit lecteur maison. |
| Linux : cpal dépend de `libasound` | En-têtes `libasound2-dev` à la compilation, `libasound2` à l'exécution (présent sur toutes les distributions de bureau) ; à ajouter aux dépendances du `.deb` et à la CI. |
| Environnement de développement sans carte son | Lecture réelle testée en dirigeant la sortie ALSA vers un fichier (`pcm.file`) ; aucun test d'écoute humaine possible ici. |
| Pas de machine Windows | Le code Windows de l'installateur (registre, raccourcis `.lnk`, élévation) est compilé et testé par la CI seulement. |
| Pas de certificat de signature de code | Pas de signature Authenticode : SmartScreen avertira au premier lancement. |
| « Media Center » (captures, enregistrements, clips) | **N'existe pas dans Halcyon** : aucune fonction de capture ou d'enregistrement. Les sons correspondants ne sont pas créés (ils seraient inutilisés). |
| Binaire Linux lié à la glibc de compilation (2.39, Ubuntu 24.04) | L'installateur doit vérifier la version de la glibc et le dire clairement. |

## 5. Risques de régression et parades

| Risque | Parade |
|---|---|
| Ouverture du périphérique audio au démarrage (latence, plantage pilote) | Ouverture **paresseuse** au premier son, sur un thread dédié ; fermeture après 20 s de silence ; toute erreur désactive l'audio sans planter. |
| Rappel audio qui bloque l'interface | Communication par canal ; le rappel audio n'attend jamais un verrou (`try_lock`, silence sinon). |
| Bruit pendant une partie | Réglage « pendant une partie » : silencieux par défaut. |
| Sons trop fréquents (sélection rapide à la manette) | Délai minimal par son, limite de voix, remplacement d'une instance du même son. |
| Paramètres 0.4.0 | Nouveau bloc `sound` avec valeurs par défaut : un fichier 0.4.0 se charge tel quel, un fichier 0.4.1 reste lisible par la 0.4.0 (champ inconnu ignoré). |
| Installateur qui écrase des données | L'installateur ne touche qu'au dossier du programme ; les données (`%APPDATA%\Halcyon`, `~/.local/share/Halcyon`) ne sont supprimées que sur demande explicite à la désinstallation. |
| Mise à jour depuis une installation Inno Setup 0.4.0 | Détection de la clé de désinstallation Inno, mise à jour au même endroit, reprise de l'entrée « Applications » et retrait des fichiers propres à Inno. |
| Mise à jour depuis `install.sh` 0.4.0 | Détection de `~/.local/bin/halcyon` sans manifeste ; remplacement par la nouvelle disposition. |
| Mise à jour intégrée de la 0.4.0 | Le nom de l'actif Windows reste `Halcyon-<v>-setup.exe` ; sous Linux sans dpkg, l'actif `*-linux-x86_64-setup` est ajouté au choix. |

## 6. Architecture retenue pour la 0.4.1

- **Nouvelle crate `halcyon-audio`** (indépendante de Slint) : catalogue des sons (identifiants `ui.select`, `game.launch`…), lecteur WAV, mélangeur (catégories, priorités, voix, délais), sortie cpal sur thread dédié, API `Audio::play(Cue)`. Testée hors périphérique par rendu dans un tampon.
- **Sons** : synthétisés par un script reproductible (`tools/sound/make_sounds.py`, NumPy), WAV PCM 16 bits mono 44,1 kHz, intégrés au binaire (`include_bytes!`). Création originale du projet : aucun droit tiers.
- **Intégration** : les méthodes existantes du contrôleur appellent l'audio ; l'interface Slint passe par un global `Sound` (`play(id)`) pour les évènements purement visuels (menus, panneaux, survol). Un seul point d'entrée : `Controller::sound(Cue)`.
- **Paramètres** : `Settings.sound` (activation, volumes général et par catégorie, survol, mode silencieux, comportement pendant une partie) ; section « Son » dans Paramètres, avec prévisualisation.
- **Nouvelle crate `halcyon-installer`** (binaire autonome, Slint, même design system) : charge utile ajoutée à la fin de l'exécutable (archive zip + manifeste SHA-256), détection de l'existant, installation transactionnelle (dossier temporaire puis échange, retour à l'état précédent en cas d'échec), progression calculée sur les octets réellement écrits, vérification d'intégrité, réparation, désinstallation. Moteur testable sans interface (crate en bibliothèque + binaire).
- **Distribution** : Windows = installateur personnalisé `Halcyon-<v>-setup.exe` (remplace Inno Setup) + version portable ; Linux = `.deb` (installation système, mises à jour par apt) + installateur personnalisé `Halcyon-<v>-linux-x86_64-setup` (par utilisateur, toutes distributions à glibc ≥ 2.39). AppImage et Flatpak évalués, non retenus (voir `docs/INSTALLATEUR.md`).

## 7. Plan

1. Audit (ce document) — 2. Identité sonore et script de synthèse — 3. `halcyon-audio` + tests — 4. Paramètres et intégration — 5. Installateur : interface — 6. Moteur d'installation, mise à jour, réparation, désinstallation + tests — 7. Packaging (script Linux, CI Windows) — 8. Validation (Xvfb, sortie ALSA vers fichier, scénarios d'installation) — 9. Documentation, changelog, version 0.4.1.
