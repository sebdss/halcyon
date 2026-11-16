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

## 2. Mises à jour signées

`SHA256SUMS` prouve qu'un paquet n'est pas abîmé, pas qu'il vient de l'auteur : un compte
GitHub compromis peut publier un paquet **et** des empreintes valides. Depuis la 1.0 :

1. La CI embarque une **clé publique Ed25519** dans l'application (variable de dépôt
   `HALCYON_UPDATE_PUBKEY`, non secrète).
2. À la publication, la CI signe `SHA256SUMS` avec la **clé privée** (secret de dépôt
   `HALCYON_SIGNING_KEY`) et publie `SHA256SUMS.sig`, puis vérifie la signature.
3. Halcyon refuse une mise à jour si la signature est absente ou fausse, et l'inscrit dans
   `security.log` (`update.rejected`).

Une compilation locale sans clé publique reste limitée à la vérification d'intégrité et
l'affiche dans Paramètres → Mise à jour (« intégrité seule »).

### Mise en place (une seule fois, par l'auteur)

```sh
cargo run -p halcyon-integrations --example release_sign -- keygen halcyon-signing.key
# → affiche la clé publique
```

- GitHub → Settings → Secrets and variables → Actions :
  - **Variables** : `HALCYON_UPDATE_PUBKEY` = la clé publique affichée ;
  - **Secrets** : `HALCYON_SIGNING_KEY` = le contenu de `halcyon-signing.key`.
- Conservez `halcyon-signing.key` hors ligne (clé USB, gestionnaire de mots de passe) et
  supprimez-la du disque. Elle ne doit jamais être commitée.
- Si la variable est définie sans le secret, la publication **échoue** (sinon les clients
  refuseraient la version).

Vérification manuelle d'une version :
`cargo run -p halcyon-integrations --example release_sign -- verify <clé-publique> SHA256SUMS`

> Transition : les installations 0.5.0 n'ont pas de clé embarquée et continuent de vérifier
> l'intégrité seule. La signature protège à partir de la première version installée qui
> contient la clé.

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
