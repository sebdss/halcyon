# Installateur de Halcyon (0.4.1)

La 0.4.1 remplace l'installateur Inno Setup (Windows) et le script `install.sh` (Linux) par
un programme d'installation écrit pour Halcyon, avec la même interface que l'application.
Il installe, met à jour, répare et désinstalle, sous Windows et sous Linux.

## Fichiers publiés

| Système | Fichier | Usage |
|---|---|---|
| Windows | `Halcyon-<version>-windows-x86_64-setup.exe` | **Recommandé.** Installation pour votre compte (sans droits d'administrateur) ou pour tous (lancé en administrateur) |
| Windows | `Halcyon-<version>-windows-portable.zip` | Sans installation (données à côté de l'exécutable) |
| Linux | `halcyon-<version>-linux-x86_64-setup` | **Recommandé.** Installation pour votre compte, toutes distributions avec glibc suffisante |
| Linux | `halcyon_<version>_amd64.deb` | Installation système (Debian, Ubuntu et dérivées), mise à jour par `apt` |
| Linux | `halcyon-<version>-linux-x86_64.tar.gz` | Archive de la 0.4.0 (scripts `install.sh` / `uninstall.sh`), conservée en attendant votre validation de son retrait |
| Tous | `SHA256SUMS` | Empreintes, vérifiées par la mise à jour intégrée |

Sous Linux, après téléchargement : `chmod +x halcyon-*-setup && ./halcyon-*-setup`.

### Pourquoi pas AppImage ni Flatpak

- **AppImage** : demande FUSE 2, absent par défaut des distributions récentes (Ubuntu 22.04
  et suivantes) ; n'apporte ni entrée de menu, ni désinstallation, ni réparation sans un
  outil supplémentaire. Le programme d'installation fait déjà tout cela, avec un seul
  fichier lui aussi.
- **Flatpak** : le bac à sable va à l'encontre du rôle de Halcyon (lancer n'importe quel jeu
  ou émulateur, lire les dossiers de Steam, Heroic et des sauvegardes, suivre les processus,
  lire les manettes par evdev). Il faudrait ouvrir tout le système
  (`--filesystem=host`, `flatpak-spawn --host`), ce qui retire l'intérêt du format et
  complique chaque lancement. Non retenu.
- Formats : un programme d'installation par système, plus le `.deb` pour qui préfère le
  gestionnaire de paquets. L'archive de la 0.4.0 est le seul format en double ; son retrait
  attend votre décision.

## Parcours

1. **Accueil** — présentation et version.
2. **Installation détectée** (si Halcyon est déjà là) — opérations possibles selon l'état
   réel : mettre à jour, réparer, réinstaller, désinstaller. L'opération recommandée est
   signalée.
3. **Configuration** — dossier (sélecteur du système : PowerShell sous Windows, zenity ou
   kdialog sous Linux ; sinon saisie), entrée de menu, raccourci Bureau, démarrage avec la
   session, pour tous les utilisateurs (Windows lancé en administrateur seulement ; sous
   Linux, le `.deb` s'en charge). Espace nécessaire et espace libre affichés.
4. **Vérification du système** — système, architecture, glibc, bibliothèques (ALSA,
   Fontconfig, udev), espace disque, droits d'écriture, installation existante, version plus
   récente déjà installée, Halcyon en cours d'exécution, conflit avec le paquet système.
   Une ligne rouge bloque l'installation ; « Relancer la vérification » après correction.
5. **Installation** — six étapes réelles du moteur, octets copiés et fichier en cours ;
   annulation possible tant que rien n'est en place.
6. **Finalisation** — raccourcis, menu, registre, enregistrement.
7. **Prêt à jouer** — « Lancer Halcyon » démarre l'application et ferme l'installateur ;
   « Guide d'utilisation » ouvre `GUIDE.md`.

Aucune durée ni aucun pourcentage n'est estimé : la barre suit les octets écrits puis
relus ; une étape sans mesure (mise en place, intégration) affiche une barre indéterminée et
le nom du fichier traité.

## Fonctionnement

### Charge utile

Le programme d'installation est un exécutable unique : le programme lui-même, suivi d'une
archive zip (fichiers + `manifest.json` avec taille et empreinte SHA-256 de chaque fichier)
et d'un pied de 24 octets. `--info` affiche le manifeste. Sous Windows, une signature
Authenticode ajoutée après la charge utile est prise en compte.

### Transaction

1. Les fichiers sont écrits dans `.halcyon-staging`, empreinte vérifiée pendant l'écriture,
   puis relus entièrement.
2. La mise en place remplace les fichiers un par un ; chaque ancienne version est mise de
   côté dans `.halcyon-backup`, avec un journal `.halcyon-transaction.json`.
3. Si une étape échoue, ou si l'ordinateur s'arrête, l'installation précédente est
   rétablie (au lancement suivant, la transaction interrompue est détectée et réparée).
4. Un dossier portant le nom d'un fichier à installer n'est jamais déplacé ni supprimé :
   l'installation s'arrête et rétablit l'état précédent.
5. Les fichiers inconnus du dossier ne sont jamais modifiés.

L'installation est décrite dans `install.json` (dans le dossier) et son emplacement dans
`~/.local/state/halcyon/installation.json` (Linux) ou `%LOCALAPPDATA%\halcyon\installation.json`
(Windows).

### Emplacements par défaut

| | Windows | Linux |
|---|---|---|
| Programme | `%LOCALAPPDATA%\Programs\Halcyon` (pour tous : `%ProgramFiles%\Halcyon`) | `~/.local/opt/halcyon` |
| Lien de commande | — | `~/.local/bin/halcyon` |
| Menu | Menu Démarrer › `Halcyon.lnk` | `~/.local/share/applications/halcyon.desktop` |
| Maintenance | « Applications installées » (Modifier = réparer) | Menu › « Gérer l'installation de Halcyon » |
| Données (jamais touchées sans demande) | `%APPDATA%\Halcyon`, cache `%LOCALAPPDATA%\Halcyon` | `~/.local/share/Halcyon`, cache `~/.cache/Halcyon` |

### Mise à jour

- Depuis la 0.4.0 : l'installation Inno Setup (Windows) ou `install.sh` (Linux) est
  détectée et reprise. Sous Windows, le programme de désinstallation d'Inno Setup et sa clé
  sont retirés ; sous Linux, l'exécutable copié dans `~/.local/bin` est remplacé par un lien.
- Bibliothèque, paramètres, statistiques, profils, mods et copies de sauvegardes ne sont
  jamais touchés : ils ne sont pas dans le dossier du programme.
- Les fichiers de l'ancienne version absents de la nouvelle sont retirés en fin de
  transaction ; les autres fichiers du dossier sont conservés.
- Mise à jour intégrée (Paramètres › À propos) : Windows lance l'installateur vérifié ;
  Linux le lance aussi quand Halcyon a été installé par lui (sinon, le `.deb`).

### Réparation

- Avec le programme d'installation : seuls les fichiers absents ou modifiés sont réécrits,
  puis raccourcis et entrées sont recréés.
- Avec le programme de maintenance (copie sans charge utile conservée dans le dossier) :
  chaque fichier est vérifié, raccourcis et entrées sont recréés. Un fichier endommagé est
  signalé par son nom, avec la version à réinstaller : rien n'est écrasé.

### Désinstallation

- Retire les fichiers listés dans `install.json`, les raccourcis et entrées créés, la clé
  de registre (Windows), et les raccourcis de jeux créés par Halcyon (Linux :
  `halcyon-jeu-*.desktop`, seulement s'ils lancent cette installation).
- Le dossier n'est supprimé que s'il est vide.
- Cache : supprimé sur demande. Données personnelles : sur demande **et** après la case
  « Je confirme ». Vos jeux et leurs sauvegardes d'origine ne sont jamais touchés.
- Refusée tant que Halcyon est ouvert.
- Windows : le programme se relance depuis une copie temporaire (un exécutable ne peut pas
  se supprimer lui-même), qui s'efface après fermeture.
- Windows : les raccourcis de jeux créés sur le Bureau portent le nom du jeu et ne sont pas
  identifiables avec certitude ; ils sont conservés.

## Ligne de commande

```text
halcyon-setup                      interface
halcyon-setup --repair             ouvre la réparation
halcyon-setup --uninstall          ouvre la désinstallation
halcyon-setup --info               affiche le manifeste de la charge utile
halcyon-setup --pack <dossier> <version> <sortie> [--min-glibc X.Y]
```

## Construire les programmes d'installation

Linux (Ubuntu 22.04 ou plus récent) :

```sh
sudo apt install libxkbcommon-dev libfontconfig1-dev libudev-dev libdbus-1-dev libasound2-dev pkg-config
cargo build --release -p halcyon-app -p halcyon-installer
sh tools/package_linux.sh       # dist/<version>/ : .deb, -setup, .tar.gz, sources, SHA256SUMS
```

La glibc de la machine de compilation devient la version minimale vérifiée par
l'installateur et déclarée par le `.deb` : compilez sur la distribution la plus ancienne à
prendre en charge (la CI utilise Ubuntu 22.04, glibc 2.35).

Windows (PowerShell) :

```powershell
cargo build --release -p halcyon-app -p halcyon-installer
New-Item -ItemType Directory dist\payload
Copy-Item target\release\halcyon.exe dist\payload\Halcyon.exe
Copy-Item README.md, CHANGELOG.md, docs\GUIDE.md, docs\PLUGINS.md, docs\SON.md, docs\INSTALLATEUR.md dist\payload\
target\release\halcyon-setup.exe --pack dist\payload 0.4.1 dist\Halcyon-0.4.1-windows-x86_64-setup.exe
```

La CI (`.github/workflows/build.yml`) fait la même chose à chaque tag `v*`.

### Signature (Windows)

Sans signature, Windows SmartScreen affiche « Éditeur inconnu ». La CI signe
`Halcyon.exe` puis le programme d'installation si les secrets `WINDOWS_CERT_PFX` (certificat
PFX en base64) et `WINDOWS_CERT_PASSWORD` sont définis dans le dépôt, puis vérifie que la
charge utile reste lisible. Aucun certificat n'est fourni : la signature dépend de vous.

`packaging/windows/halcyon.iss` (Inno Setup) reste dans le dépôt pour compiler l'ancien
installateur au besoin ; la CI ne l'utilise plus.

## Distributions testées

| Système | Résultat |
|---|---|
| Ubuntu 24.04 (glibc 2.39), X11 (Xvfb) | Installation, reprise d'une installation `install.sh` 0.4.0, réparation (fichiers supprimés et modifiés), réparation par le programme de maintenance, désinstallation avec données conservées, lancement de Halcyon : vérifiés |
| Windows 10 / 11 | **Non testé** : aucune machine Windows disponible. Le code Windows n'a pas pu être compilé ici (cible absente) ; la CI le compile et contrôle la charge utile, mais le parcours complet reste à vérifier sur une vraie machine |
| Autres distributions Linux | Non testées. Prérequis : x86_64, glibc ≥ celle de la compilation, ALSA, Fontconfig, udev (vérifiés par l'installateur) |
