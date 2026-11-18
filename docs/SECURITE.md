# Sécurité de Halcyon (1.0.0)

Ce document décrit les protections ajoutées en 1.0.0 et celles qui existaient déjà. Le
principe : **aucune optimisation ne désactive une protection**, et une protection qui ne
peut pas être garantie est présentée comme telle.

## 1. Coffre des secrets

| | Avant (≤ 0.5.0) | 1.0.0 |
|---|---|---|
| Clés d'API (Steam, RetroAchievements, Nexus Mods, SteamGridDB, CurseForge) | en clair dans `settings.json` | un fichier par clé dans `<données>/secrets/` |
| Windows | lisible par tout programme du compte | chiffré par **DPAPI** (lié au compte Windows) |
| Linux | `settings.json` en `0644` (lisible par les autres comptes) | fichiers `0600` dans un dossier `0700` ; `settings.json` lui aussi en `0600` |
| Journaux, exports | clés exclues des exports par défaut | idem, et jamais journalisées |

- **Migration automatique** au premier lancement de la 1.0 : chaque clé est écrite dans le
  coffre, *puis* `settings.json` est réécrit sans elle. Une erreur d'écriture du coffre
  laisse la clé en place (rien n'est perdu) et est journalisée.
- Le code applicatif ne change pas : `Settings::load` remplit les champs depuis le coffre,
  `Settings::save` les y renvoie (`crates/halcyon-storage/src/secrets.rs`, `settings.rs`).
- **Pourquoi pas un trousseau sous Linux ?** Secret Service (GNOME Keyring, KWallet) est
  absent de nombreuses sessions de jeu (Gamescope, bureaux légers, Steam Deck en mode jeu).
  Un coffre qui échoue silencieusement serait pire qu'un fichier `0600`, protection
  équivalente à celle de `~/.ssh`.
- **Export** : le coffre n'est jamais copié tel quel (sous Windows il serait illisible sur un
  autre PC). L'option « inclure les clés » les place en clair dans l'archive, seulement si
  vous la cochez ; la restauration les remet dans le coffre.
- Le jeton Microsoft (Minecraft, mode direct) gardait déjà sa propre protection (DPAPI /
  `0600`) ; il n'a pas été déplacé pour ne pas toucher aux sessions existantes.

## 2. Mises à jour signées (Halcyon Update System)

Depuis HUS (1.0), la décision d'installer repose sur un **manifeste signé Ed25519**
(`halcyon-manifest.json` + `.sig`) : version, canal, plateformes, tailles et **SHA-256** des
paquets. Les informations de GitHub ne prouvent rien ; seule la signature fait foi.

- Clés publiques intégrées : `crates/halcyon-update/keys/release.pub` (et, en complément,
  la variable `HALCYON_UPDATE_PUBKEY` fixée à la compilation). Sans clé, aucune installation
  automatique : les versions sont seulement signalées.
- Clé privée : secret `HALCYON_SIGNING_KEY` des environnements GitHub `release-stable` /
  `release-beta`, exposé à la seule étape de signature ; copie hors ligne. Jamais commitée.
- Contrôles : signature, cohérence du manifeste, système, version minimale de l'updater,
  anti-retour (jamais de version inférieure ou égale ni sous le plancher de sécurité),
  SHA-256, structure du paquet avant extraction (ZIP : chemins sûrs, ni lien, ni dossier
  réservé, archive piège refusée ; AppImage : ELF x86-64, type 2, SquashFS). L'updater les
  refait tous avant de modifier un fichier. Aucun script de publication n'est exécuté.
- HTTPS obligatoire, redirections comprises.
- `SHA256SUMS` + `SHA256SUMS.sig` restent publiés et signés par la même clé pour les
  installations 0.5, qui se mettent à jour par l'ancien mécanisme jusqu'à la 1.0.

Procédure complète (clés, rotation, clé compromise, publication Stable avec validation
humaine) : [`HUS.md`](HUS.md), sections 4 à 6.

## 3. Journal de sécurité

`<données>/logs/security.log` (`0600`), une ligne JSON par événement, 512 Kio maximum
(un fichier précédent conservé) :

| Événement | Détail enregistré |
|---|---|
| `secret.set`, `secret.removed`, `secret.migrated` | nom de la clé (jamais sa valeur) |
| `plugin.installed`, `plugin.enabled` | identifiant et autorisations |
| `plugin.disabled`, `plugin.uninstalled` | identifiant |
| `update.verified`, `update.rejected` | version, paquet, niveau de vérification ou motif |
| `minecraft.login`, `minecraft.logout` | nom du profil Minecraft |
| `minecraft.official_profile.write` / `.remove` | profil « Halcyon · … » du launcher officiel |

Un détail contenant un retour à la ligne est échappé : une entrée ne peut pas en imiter une
autre.

## 4. Protections existantes (inchangées)

- Extensions **déclaratives** : aucun code tiers exécuté, installées désactivées,
  autorisations affichées et confirmées, réseau limité aux domaines déclarés (HTTPS).
- Instance unique : canal local protégé par un jeton aléatoire (`instance.json`, `0600`).
- Minecraft : téléchargements HTTPS vérifiés SHA-1, chemins de modpack confinés,
  `launcher_profiles.json` sauvegardé avant toute modification, aucun mot de passe manipulé.
- Sauvegardes et mods : jamais d'écrasement sans copie préalable.
- Aucune injection dans les jeux (anti-triche, stabilité).

## 5. Limites connues

- Linux : les fichiers du coffre ne sont pas chiffrés (protection par les droits du compte).
- Signature : protège à partir de la première version signée installée ; la clé privée est
  un point unique à protéger.
- Les extensions ne sont pas encore signées par leurs auteurs (prévu).

## 6. Audit du 18 novembre 2026 (après la 1.0.0-alpha.4)

Revue du code ajouté depuis (traductions, succès et visuels sans clé) et des points
sensibles de l'ensemble : lancement de processus, archives, extensions, restauration,
téléchargements, site.

| Constat | Correction |
|---|---|
| Un émulateur fourni par une extension pouvait désigner un shell (`sh`, `cmd`, `powershell`…) avec des arguments choisis par l'extension : exécution de commandes malgré « aucun code » | Shells, interpréteurs, couches de compatibilité et outils d'élévation refusés (`emulators::interpreter`) ; programmes lancés nommés dans le résumé d'activation |
| Liste des fichiers exécutables refusés dans une extension incomplète | Ajout de `.jar`, `.lnk`, `.AppImage`, `.desktop`, `.hta`, `.wsf`, `.reg`… |
| Scripts Windows lancés par `cmd /C` : `&`, `|`, `%` des arguments interprétés | Lancement direct, arguments échappés par la bibliothèque standard |
| Restaurer une archive partagée importe des commandes (scripts des profils, émulateurs, extensions) | Avertissement explicite dans la confirmation |
| Lecture locale des succès Steam : un format non reconnu aurait effacé les succès connus | Liste vide ignorée quand des succès existent déjà |
| Visuels absents recherchés à chaque démarrage | Nouvel essai après 7 jours ; requêtes libretro espacées |
| Site : pas de politique de sécurité du contenu ; adresses de l'API GitHub insérées telles quelles | CSP sur toutes les pages (script en ligne autorisé par empreinte) ; HTTPS exigé |

Vérifié sans changement : extraction des archives (chemins confinés), modpacks (noms de
fichiers filtrés, HTTPS, empreintes), raccourcis Windows (valeurs passées par variables
d'environnement), ouverture de liens (`https://` seulement), substitution des traductions
(un seul passage, pas de réinterprétation), aucun secret dans les branches.

Reste à faire : le format local des succès Steam est validé sur des fichiers de test, pas
encore sur de vrais fichiers ; Steam Web API n'accepte la clé que dans l'URL (jamais
journalisée par Halcyon).
