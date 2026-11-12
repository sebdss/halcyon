# Minecraft dans Halcyon (0.5.0)

L'onglet **Minecraft** (menu latéral, après Big Picture) gère Minecraft Java Edition sans quitter
Halcyon. Il ne remplace aucune page existante. Le code est dans le crate `halcyon-minecraft`
(sans interface) et dans `halcyon-app/src/ctl_minecraft.rs` / `ui/pages/minecraft.slint`.

## Sections

| Section | Ce qu'elle fait |
|---|---|
| **Jouer** | Choix de l'installation, compte, bouton Jouer, temps de jeu, mondes, dossiers, suppression |
| **Versions** | Liste officielle Mojang (releases ; snapshots et anciennes versions en option), installer, réparer, vérifier, supprimer |
| **Mods** | Recherche Modrinth (CurseForge avec clé), filtrée par version et loader de l'installation ; activer, désactiver, supprimer, mettre à jour |
| **Modpacks** | Recherche, installation, mise à jour (Modrinth), import `.mrpack` / zip CurseForge, export `.mrpack` |
| **Loaders** | Fabric, Quilt, Forge, NeoForge : versions compatibles avec la version de Minecraft choisie |
| **Paramètres** | Mode de lancement, RAM, Java, arguments JVM, résolution, plein écran, ID d'application Azure, clé CurseForge |

Une **installation** (profil) = une version de Minecraft + éventuellement un loader et des mods,
avec son propre dossier de jeu. Les installations ne partagent ni mods ni réglages ; les
bibliothèques et assets, eux, sont partagés (téléchargés une seule fois).

## Dans la bibliothèque

« Minecraft: Java Edition » apparaît avec les autres jeux (bibliothèque, accueil, recherche,
Big Picture, collections) dès qu'une installation Halcyon ou le launcher officiel est présent.
Filtre de source : « Minecraft ». **Jouer** depuis la bibliothèque lance l'installation
sélectionnée dans l'onglet Minecraft (ou ouvre l'onglet s'il n'y en a aucune). Favoris, statut,
notes, collections et jaquettes personnalisées fonctionnent comme pour les autres jeux ; avec une
clé SteamGridDB, les visuels sont récupérés automatiquement.

Temps de jeu : mesuré en mode direct. En mode launcher officiel, Halcyon note le lancement mais
ne connaît pas la durée (c'est le launcher qui lance Java).

## Connexion et lancement : deux modes

Halcyon n'utilise jamais de compte « crack », ne contourne aucune authentification, ne lit pas et
ne réutilise pas les jetons du launcher officiel et ne stocke aucun mot de passe.

### Mode « launcher officiel » (par défaut)

Pour qui n'a pas d'application Azure approuvée par Mojang (cas actuel) :

1. Halcyon télécharge et vérifie la version, le loader et les mods dans son propre dossier.
2. Après accord explicite (encadré Compte, une seule fois), Halcyon ajoute un profil
   « Halcyon · *nom* » au fichier `launcher_profiles.json` du launcher officiel. Une copie de
   sauvegarde `launcher_profiles.json.halcyon-backup` est faite avant la première modification.
   Les profils existants ne sont pas modifiés. Le profil pointe vers le dossier de jeu Halcyon.
3. Halcyon ouvre le launcher officiel (version Microsoft Store ou classique). C'est lui qui
   connecte le compte Microsoft, vérifie la licence et lance le jeu.

Halcyon affiche seulement le nom du compte actif du launcher officiel (lu dans
`launcher_accounts*.json`, sans aucun jeton). Supprimer une installation dans Halcyon retire
aussi son profil du launcher officiel.

### Mode « direct »

Halcyon lance lui-même Java, après connexion Microsoft officielle (code d'appareil,
`login.microsoftonline.com`) → Xbox Live → XSTS → Minecraft Services, puis vérification que le
compte possède Java Edition. Il faut un **ID d'application Azure** (application publique, comptes
personnels) **approuvée par Mojang** (formulaire `aka.ms/mce-reviewappid`) ; sans approbation,
Microsoft répond « Invalid app registration » et Halcyon l'explique.

- Le jeton d'actualisation est chiffré par DPAPI (Windows) ou stocké en fichier `0600` (Linux) ;
  le jeton Minecraft reste en mémoire et n'apparaît jamais dans les journaux.
- En cas d'arrêt du jeu dans la première minute, la fin du journal s'affiche dans l'onglet.

## Java

Par défaut, Halcyon installe le runtime Java officiel de Mojang adapté à la version (par exemple
Java 25 pour Minecraft 26.x, Java 8 pour les versions anciennes). Un Java choisi à la main est
vérifié (exécutable, version suffisante). La liste des Java détectés est affichée dans Paramètres.

## Loaders

- Fabric et Quilt : profils officiels de leurs serveurs « meta ».
- Forge et NeoForge : installateur officiel téléchargé depuis leur Maven (somme SHA-1 vérifiée),
  exécuté avec `--installClient` dans le dossier Halcyon. C'est le mécanisme prévu par ces projets.
- NeoForge ne propose pas de version pour Minecraft 1.20.1 dans Halcyon (ancien nom d'artefact).

## Mods et modpacks

- Modrinth est utilisé sans clé. CurseForge demande une clé d'API (console.curseforge.com),
  saisie dans Paramètres et non incluse dans les sauvegardes de configuration.
- Les dépendances requises sont installées automatiquement ; les mods déclarés incompatibles
  bloquent l'installation avec un message.
- Désactiver un mod renomme son fichier en `.jar.disabled`.
- Un modpack CurseForge peut contenir des fichiers dont l'auteur interdit le téléchargement hors
  de CurseForge : Halcyon ne les récupère pas et liste ceux à déposer à la main.
- Les fichiers d'un `.mrpack` ne sont téléchargés que depuis les hôtes autorisés par le format
  (Modrinth, GitHub, GitLab) et ne peuvent pas sortir du dossier de l'installation.

## Emplacements

| Système | Dossier |
|---|---|
| Windows | `%APPDATA%\Halcyon\minecraft` (dossier de données de Halcyon) |
| Linux | `~/.local/share/Halcyon/minecraft` |

Sous-dossiers : `versions`, `libraries`, `assets`, `runtime` (Java Mojang), `profiles/<id>`
(dossiers de jeu : `mods`, `saves`, `config`…), `accounts`, `cache`.

## Limites connues

- Mode direct non testé avec un vrai compte tant qu'aucun ID Azure approuvé n'est disponible.
- La détection de mises à jour des mods peut proposer une version bêta plus récente.
- Pas de système avancé de création d'instances personnalisées dans cette version
  (une installation par version et loader, plus une par modpack).
