# Audit avant la 0.5.0 — intégration de Minecraft Java Edition

Point de départ : branche `v0.4.1` (6 crates, 139 tests, installateur Windows/Linux validé sur
un vrai PC Windows). Branche de travail : `v0.5.0`.

## Ce qui existe et sera réutilisé

| Élément | Où | Usage pour Minecraft |
|---|---|---|
| Navigation (`enum Page`, `Nav.go`, historique Alt+← / →) | `ui/state.slint`, `ctl_shell.rs` | Nouvelle page `minecraft`, une entrée de barre latérale ; rien d'autre ne change |
| Composants du design system (boutons, onglets `Tabs`, `Dropdown`, `TextField`, `Toggle`, `ValueSlider`, `ProgressBar`, `Panel`, `Notice`, `EmptyState`) | `ui/components.slint` | Toute l'interface de l'onglet |
| Centre de tâches (progression, fin, erreur) | `tasks.rs` | Installation de versions, loaders, mods, modpacks |
| Fils de travail + `post()` vers l'interface | `controller.rs` | Téléchargements et lancement sans bloquer l'interface |
| Mode hors ligne, `net_allowed()` | `controller.rs` | Fonctions en ligne désactivées hors connexion |
| Session de jeu (mode Performance, son silencieux pendant la partie) | `ctl_profiles.rs`, `halcyon-audio` | Appliqué aussi à une partie de Minecraft |
| `zip`, `sha2`, `ureq` (rustls), `serde_json`, `dirs` | dépendances existantes | Archives (natives, `.mrpack`), empreintes, HTTPS |
| Paramètres JSON atomiques, `SECRET_KEYS` exclus des exports | `halcyon-storage`, `config_archive.rs` | Réglages Minecraft, clé CurseForge |

## Architecture retenue

- **Nouvelle crate `halcyon-minecraft`**, sans interface, testée :
  `meta` (manifeste et JSON des versions, règles, héritage `inheritsFrom`), `download`
  (téléchargements parallèles vérifiés par SHA-1, reprise, annulation), `install` (client,
  bibliothèques, natives, assets), `java` (détection, runtimes officiels de Mojang),
  `launch` (ligne de commande, journal), `loaders` (Fabric, Quilt, Forge, NeoForge),
  `auth` (Microsoft → Xbox Live → XSTS → Minecraft, licence, profil), `mods` (Modrinth,
  CurseForge), `modpack` (`.mrpack`, CurseForge), `profiles` (installations).
- **Dossier séparé** : `<données Halcyon>/minecraft` (`versions`, `libraries`, `assets`,
  `runtime`, `profiles/<nom>` avec `mods`, `config`, `saves`…). Le `.minecraft` du launcher
  officiel n'est jamais lu ni modifié.
- **Une installation par version, loader ou modpack**, créée automatiquement : mods, mondes et
  réglages ne se mélangent pas. Pas de création d'instances personnalisées (hors périmètre).
- **Contrôleur `ctl_minecraft.rs`** et page `ui/pages/minecraft.slint` (Play, Versions, Mods,
  Modpacks, Loaders, Settings).

## Contraintes et limites identifiées

| Sujet | Contrainte | Réponse |
|---|---|---|
| Compte Microsoft | `api.minecraftservices.com` refuse les applications Azure non approuvées par Mojang. Utiliser l'identifiant d'un autre launcher est interdit | Identifiant Azure de Sebi, soumis à Mojang (aka.ms/mce-reviewappid). La connexion reste inactive sans identifiant |
| CurseForge | API sur clé accordée ; certains auteurs interdisent le téléchargement par des tiers | Clé dans les paramètres ; fichier non distribuable signalé avec lien vers la page, jamais contourné |
| Forge / NeoForge | Pas d'API d'installation : seul l'installateur officiel produit les fichiers | L'installateur officiel est lancé en mode client (`--installClient`) avec Java |
| Jetons de session | Aucun mot de passe n'est manipulé (connexion par code sur le site de Microsoft) | Jeton de renouvellement chiffré par DPAPI sous Windows ; fichier `0600` sous Linux (pas de trousseau universel) |
| Réseau de l'environnement de développement | Mojang, Modrinth et Microsoft ne sont pas joignables depuis le conteneur | Tests réseau réels sur le PC Windows de Sebi (accord donné), tests unitaires sur données réelles enregistrées |
| Java | Chaque version demande une version de Java précise (8, 16, 17, 21…) | Runtime officiel de Mojang téléchargé à la demande, ou Java choisi par l'utilisateur |

## Risques

| Risque | Parade |
|---|---|
| Régression de la 0.4.1 | Aucune modification des modules existants hors navigation, paramètres et câblage ; suite de tests complète |
| Téléchargement corrompu ou interrompu | Fichier temporaire, SHA-1 vérifié, renommage atomique ; un fichier valide n'est jamais retéléchargé |
| Mélange des mods entre versions | Dossier de jeu distinct par installation |
| Session volée | Jeton jamais écrit en clair sous Windows, jamais journalisé, jamais exporté |
