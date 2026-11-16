# Halcyon Update System (HUS)

HUS installe les nouvelles versions de Halcyon sans intervention : recherche, téléchargement
en arrière-plan, vérification cryptographique, installation par un updater indépendant,
redémarrage, contrôle du démarrage et retour automatique à la version précédente en cas
d'échec. Windows (ZIP, installation utilisateur) et Linux (AppImage), sans droits
administrateur, sans runtime supplémentaire.

## Sommaire

1. [Architecture](#1-architecture)
2. [Canaux et versions](#2-canaux-et-versions)
3. [Format officiel du manifeste](#3-format-officiel-du-manifeste)
4. [Signature et vérifications](#4-signature-et-vérifications)
5. [Mise en place des clés](#5-mise-en-place-des-clés-une-seule-fois)
6. [Publier une version Stable ou Beta](#6-publier-une-version-stable-ou-beta)
7. [Installation et mise à jour côté utilisateur](#7-installation-et-mise-à-jour-côté-utilisateur)
8. [Transaction, contrôle de santé et récupération](#8-transaction-contrôle-de-santé-et-récupération)
9. [Dépannage](#9-dépannage)
10. [Limites connues](#10-limites-connues)

## 1. Architecture

La cible proposée (huit composants) est adaptée à la structure du dépôt, qui découpe en
crates larges : une seule nouvelle crate, `halcyon-update`, dont les modules reprennent les
rôles demandés. Le lanceur est l'application existante (`halcyon-app`), non renommée.

| Rôle demandé | Emplacement réel |
|---|---|
| halcyon-launcher | `crates/halcyon-app` (`src/ctl_update.rs`, `ui/hus_state.slint`, Paramètres › Mises à jour) |
| halcyon-updater | `crates/halcyon-update/src/bin/halcyon-updater.rs` (exécutable indépendant, sans interface) |
| update-core | `src/version.rs` (SemVer strict), `src/core.rs` (canaux, plateformes, états, sélection) |
| update-network | `src/network.rs` (GitHub Releases, ETag, erreurs d'API, manifeste) |
| update-security | `src/security.rs`, `src/manifest.rs` (Ed25519, SHA-256, anti-retour, structure des paquets) |
| update-download | `src/download.rs` (HTTP Range, reprise, progression, annulation) |
| update-install | `src/install.rs` (ZIP Windows, AppImage, migration de l'installateur Linux) |
| update-recovery | `src/recovery.rs`, `src/runner.rs` (état, journal, verrou, santé, retour arrière, reprise) |
| outil de publication | `src/bin/hus-release.rs` (ZIP reproductible, manifeste, signature, vérification) |

Remplacé : l'ancien module `halcyon-integrations/src/update.rs` (vérification de
`SHA256SUMS`, lancement de l'installateur). Sa vérification de signature est désormais
celle de HUS (`security::verify_signature_b64`), aussi utilisée par les extensions.

**Pas de tokio** : le téléchargement est asynchrone par rapport à l'interface (fil dédié,
progression transmise à Slint par `post`), comme toutes les tâches de fond de Halcyon.
Ajouter un runtime asynchrone n'apporterait rien pour un seul téléchargement à la fois.

### Déroulement

```text
Halcyon (lanceur)                                    halcyon-updater (copie dans .hus/runner)
─────────────────                                    ───────────────────────────────────────
1. GET /releases (If-None-Match)
2. manifeste + signature → vérifiés
3. téléchargement (Range) → SHA-256 → structure
4. .hus/pending/ : paquet + manifeste signé
5. « Installer » ou démarrage suivant :
   copie de l'updater, lancement, fermeture  ──►  6. attend la fin de Halcyon
                                                  7. revérifie tout (signature, SHA-256, structure, version)
                                                  8. extrait dans .hus/staging, journal, remplace
                                                  9. lance la nouvelle version (--hus-health)
10. fenêtre affichée → preuve de démarrage   ──►  11. succès : version précédente → .hus/previous
                                                      échec / délai : retour arrière, relance de l'ancienne
```

## 2. Canaux et versions

- **Stable** : versions finales (`1.0.0`, `1.0.1`). Aucune prépublication n'y est proposée,
  ni une publication GitHub marquée « pre-release ».
- **Beta** : prépublications (`1.1.0-beta.1`) et versions stables plus récentes.
- Versionnement **SemVer 2.0 strict** : une version mal formée est refusée, jamais devinée.
- Le canal se choisit dans *Paramètres › Mises à jour*. Il est enregistré
  (`update_channel` dans `settings.json`) et n'est jamais modifié sans action de
  l'utilisateur. Tant qu'il n'a pas été choisi, le canal par défaut est Beta pour une
  prépublication installée (sinon elle ne recevrait jamais la suivante) et Stable sinon.
- **Beta → Stable** : pas de retour en arrière. Une 1.1.0-beta.2 attend la première version
  stable plus récente (1.1.0). Une prépublication en attente d'installation est abandonnée
  au passage en Stable.

## 3. Format officiel du manifeste

Chaque publication porte `halcyon-manifest.json` et `halcyon-manifest.json.sig`.

```json
{
  "schema": 1,
  "product": "Halcyon",
  "channel": "beta",
  "version": "1.1.0-beta.1",
  "published": "2026-11-16T14:00:00Z",
  "min_updater": "1.0.0-alpha.1",
  "security_floor": "1.0.2",
  "packages": [
    { "platform": "windows-x86_64", "format": "zip", "name": "Halcyon-1.1.0-beta.1-Windows-x64.zip", "size": 9123456, "sha256": "…64 caractères hexadécimaux…" },
    { "platform": "linux-x86_64", "format": "appimage", "name": "Halcyon-1.1.0-beta.1-x86_64.AppImage", "size": 27888120, "sha256": "…" }
  ],
  "notes": "Texte des notes de version (section du CHANGELOG)."
}
```

| Champ | Règle |
|---|---|
| `schema` | 1. Champs inconnus ignorés (ajouts compatibles) ; un changement incompatible augmente `schema` et les versions qui ne le connaissent pas refusent le manifeste |
| `product` | `Halcyon` |
| `channel` | `beta` si la version a un suffixe de prépublication ou si la publication GitHub est « pre-release », `stable` sinon |
| `version` | SemVer, identique au tag de la publication |
| `published` | RFC 3339 UTC |
| `min_updater` | updater minimal capable d'installer ces paquets (défaut : `runner::PROTOCOL_MIN`). Si l'updater installé est plus ancien, Halcyon utilise celui du nouveau paquet |
| `security_floor` | facultatif : version minimale de sécurité. Une installation plus ancienne voit une « mise à jour de sécurité » ; une fois installée, aucune version inférieure n'est plus acceptée |
| `packages[].name` | exactement `Halcyon-{VERSION}-Windows-x64.zip` ou `Halcyon-{VERSION}-x86_64.AppImage` |
| `packages[].size`, `sha256` | taille et empreinte du fichier publié |
| `notes` | 64 Ko au plus |

La signature est l'Ed25519 (base64) des **octets exacts** du fichier manifeste.

## 4. Signature et vérifications

Les informations de GitHub (nom, marque « pre-release », taille affichée) ne sont jamais
une preuve : seule la signature du manifeste par la clé de l'auteur fait foi.

Avant téléchargement (lanceur) :

1. **Authenticité** : signature Ed25519 du manifeste par une clé intégrée au logiciel.
2. **Cohérence** : produit, schéma, version = tag, canal, noms de paquets, empreintes et
   tailles bien formées, plancher de sécurité ≤ version.
3. **Compatibilité** : paquet présent pour ce système ; taille publiée = taille signée.
4. **Version minimale de l'updater** (`min_updater`).
5. **Anti-retour** : version strictement plus récente que l'installée et pas inférieure au
   plancher (plus haute version déjà installée ici, plancher de sécurité accepté).

Après téléchargement : taille et **SHA-256** du manifeste signé (fichier altéré supprimé),
puis **structure du paquet** avant toute extraction :

- ZIP : chemins relatifs sûrs (ni absolu, ni `..`, ni lecteur), ni lien symbolique, ni dossier
  réservé (`data/`, `.hus/`, `portable.txt`, `install.json`), taille décompressée et taux
  de compression bornés (archive piège), `Halcyon.exe` et `halcyon-updater.exe` présents ;
- AppImage : ELF 64 bits x86-64, signature AppImage type 2 (`AI\x02`), image SquashFS
  juste après l'exécutable.

L'updater **refait toutes ces vérifications** avec ses propres clés avant de toucher un
fichier : un paquet en attente modifié sur le disque est refusé. Aucun script fourni par une
publication n'est jamais téléchargé ni exécuté. HTTPS obligatoire, redirections comprises.

Les clés de confiance sont dans `crates/halcyon-update/keys/release.pub` (une par ligne :
deux pendant une rotation) et, en complément, dans la variable `HALCYON_UPDATE_PUBKEY` fixée
à la compilation. **Sans clé, Halcyon signale les nouvelles versions mais ne les installe
jamais automatiquement.**

Authenticode (Windows) : `Halcyon.exe`, `halcyon-updater.exe` et l'installateur sont signés
par la CI si le certificat est configuré (`WINDOWS_CERT_PFX`, `WINDOWS_CERT_PASSWORD`). C'est
une protection complémentaire (SmartScreen) ; la décision d'installer repose sur Ed25519.

## 5. Mise en place des clés (une seule fois)

```sh
# Sur une machine de confiance, hors du dépôt :
cargo run -p halcyon-integrations --example release_sign -- keygen ~/halcyon-release.key
# → affiche la clé publique (base64)
```

1. Ajoutez la clé publique dans `crates/halcyon-update/keys/release.pub` et committez-la.
   Facultatif : la même valeur dans la variable de dépôt `HALCYON_UPDATE_PUBKEY`
   (Settings › Secrets and variables › Actions › Variables).
2. Créez les environnements GitHub **`release-stable`** et **`release-beta`**
   (Settings › Environments). Dans `release-stable`, activez *Required reviewers* : chaque
   version stable attend votre validation avant signature et publication. Limitez les deux
   environnements aux tags `v*` (*Deployment branches and tags*).
3. Dans chacun des deux environnements, ajoutez le secret **`HALCYON_SIGNING_KEY`** (contenu
   du fichier de clé privée). Il n'est visible que de l'étape de signature.
4. Gardez une copie hors ligne de la clé privée ; supprimez-la de la machine de travail.

**Rotation** : générez une nouvelle paire, ajoutez la nouvelle clé publique à
`release.pub` (sans retirer l'ancienne), publiez une version signée par l'**ancienne** clé,
puis remplacez le secret par la nouvelle clé privée ; retirez l'ancienne clé publique une
fois que les utilisateurs ont reçu la version qui connaît la nouvelle.

**Clé compromise** : retirez-la de `release.pub`, publiez avec la nouvelle clé une version
annonçant un `security_floor` égal à elle-même ; les installations qui ne connaissent que
l'ancienne clé devront être mises à jour manuellement une fois.

## 6. Publier une version Stable ou Beta

1. Mettez à jour `version` dans `Cargo.toml` (racine) et la section correspondante de
   `CHANGELOG.md` (`## 1.1.0-beta.1 — …`) : elle devient les notes de version signées.
2. Committez, puis créez le tag : `git tag v1.1.0-beta.1 && git push origin v1.1.0-beta.1`.
3. Le workflow **Release** (`.github/workflows/release.yml`) :
   1. vérifie que le tag est SemVer et égal à la version de `Cargo.toml`, en déduit le canal ;
   2. compile et teste sous Windows et Linux (y compris les tests HUS avec pannes simulées) ;
   3. construit `Halcyon-{V}-Windows-x64.zip` (reproductible), l'installateur et la version
      portable Windows, `Halcyon-{V}-x86_64.AppImage`, l'installateur, le `.deb` et l'archive
      Linux ;
   4. produit le manifeste (SHA-256), `SHA256SUMS` (clients 0.5), et attend la validation
      humaine pour une version Stable ;
   5. signe le manifeste et `SHA256SUMS`, vérifie la signature, publie (pre-release pour Beta) ;
   6. retélécharge les fichiers publiés et les vérifie comme Halcyon (`hus-release verify-remote`).
4. Les installations sur le canal correspondant la reçoivent à leur prochaine recherche
   (au démarrage, puis toutes les 6 heures).

Permissions : lecture seule pour tous les jobs, écriture du dépôt uniquement pour le job de
publication ; la clé privée n'est exposée qu'à l'étape de signature.

Vérification manuelle d'une publication :

```sh
cargo run -p halcyon-update --bin hus-release -- verify-remote --repo sebdss/halycon --tag v1.1.0-beta.1
cargo run -p halcyon-update --bin hus-release -- verify dossier-téléchargé/   # dossier local
```

## 7. Installation et mise à jour côté utilisateur

**Windows 10/11** : le programme d'installation (`…-windows-x86_64-setup.exe`) installe
Halcyon pour l'utilisateur dans `%LOCALAPPDATA%\Programs\Halcyon`, avec `halcyon-updater.exe`.
Les mises à jour suivantes passent par le ZIP, sans droits administrateur ; la fiche de
l'installateur (`install.json`) est tenue à jour (réparation et désinstallation continuent de
fonctionner). La version portable (ZIP + `portable.txt`) est mise à jour sur place, son
dossier `data/` n'est jamais touché. Une installation « pour tous les utilisateurs »
(Program Files) n'est pas modifiable sans droits : HUS la signale et ne l'installe pas.

**Linux** : l'AppImage fonctionne telle quelle. Lancée hors de `~/.local/opt/halcyon`,
*Paramètres › Mises à jour › Installer Halcyon pour mon compte* la copie dans
`~/.local/opt/halcyon/Halcyon.AppImage`, ajoute l'entrée de menu, l'icône et le lien
`~/.local/bin/halcyon`. Les mises à jour remplacent ce fichier de façon atomique (même
chemin : raccourcis et icônes conservés). Une installation faite par l'installateur Linux
(`halcyon-setup`) passe à l'AppImage à sa première mise à jour : `halcyon` devient un lien
vers `Halcyon.AppImage`, les raccourcis restent valides. Le paquet `.deb` reste mis à jour
par apt : HUS signale la version sans l'installer.

**Réglages** (*Paramètres › Mises à jour*) : canal, recherche automatique, mises à jour
automatiques (téléchargement en arrière-plan puis installation au démarrage suivant),
dépôt des versions (vide : officiel). Actions : rechercher, télécharger, annuler (le
téléchargement reprendra où il s'est arrêté), installer et redémarrer, installer au
prochain démarrage. La progression affiche les octets réellement reçus, la vitesse mesurée
et le temps restant calculé sur cette vitesse.

Les données (bibliothèque, paramètres, sauvegardes) sont hors du dossier d'installation,
ou dans `data/` en mode portable : HUS ne les touche jamais.

## 8. Transaction, contrôle de santé et récupération

États : `Idle`, `Checking`, `UpToDate`, `Available`, `Downloading`, `Verifying`, `Staging`,
`Installing`, `HealthCheck`, `Completed`, `Failed`, `Recovery`, `RolledBack`, enregistrés dans
`.hus/state.json` (écriture atomique) et journalisés dans `.hus/hus.log`.

Dossier `.hus/` (à côté de l'installation, même disque : renommages atomiques) :

| Élément | Rôle |
|---|---|
| `state.json` | état, versions, message, plus haute version installée, plancher de sécurité, version bloquée |
| `hus.log` | journal de chaque opération (1 Mo, puis rotation) |
| `pending/` | paquet vérifié et son manifeste signé |
| `staging/` | nouveaux fichiers extraits |
| `backup/` | anciens fichiers pendant la transaction (liens physiques, copie à défaut) |
| `transaction.json` | journal de transaction, synchronisé avant chaque modification |
| `previous/<version>/` | version précédente conservée après succès |
| `health/` | preuve de démarrage de la nouvelle version |
| `runner/` | copie de l'updater qui travaille (l'updater installé peut être remplacé) |
| `downloads/` | téléchargements en cours (`.part`, reprenables) |
| `releases.json` | cache de la liste des versions et son ETag |

Le **contrôle de santé** : l'updater lance la nouvelle version avec `--hus-health <id>` ;
elle écrit sa preuve 3 secondes après l'affichage de sa fenêtre. Sans preuve en
120 secondes, ou si elle s'arrête avant, l'updater l'arrête, restaure l'ancienne version
et la relance (`--hus-rolled-back`) ; la version fautive n'est plus proposée
automatiquement (`blocked`). Halcyon affiche le résultat une seule fois (notification et
*Paramètres › Mises à jour*).

**Reprise** après coupure de courant, arrêt brutal de Halcyon ou de l'updater :

| Interruption | Au démarrage suivant |
|---|---|
| pendant le téléchargement | reprise HTTP Range à l'octet près |
| paquet en attente | revérifié intégralement avant installation |
| pendant l'installation (`Installing`) | Halcyon lance `halcyon-updater recover` et se ferme : le journal est rejoué à l'envers, l'ancienne version est relancée (opération idempotente, elle-même reprenable) |
| après l'installation (`HealthCheck`) | la nouvelle version qui démarre confirme elle-même la transaction |

Retour manuel à la version précédente : elle est dans `.hus/previous/<version>/`.

## 9. Dépannage

| Message | Cause, solution |
|---|---|
| « aucune clé de signature » | compilation sans clé (locale ou fork) : installez depuis GitHub |
| « signature incorrecte » | manifeste non signé par l'auteur : rien n'est installé ; signalez-le |
| « empreinte SHA-256 différente » | fichier altéré en route : supprimé, le téléchargement recommencera |
| « GitHub limite temporairement les requêtes » | quota de l'API dépassé : nouvel essai après le délai affiché |
| « Halcyon ne s'est pas fermé » | fermez Halcyon (zone de notification › Quitter) : rien n'a été modifié |
| « une mise à jour est déjà en cours » | un autre updater travaille sur ce dossier |
| AppImage qui ne démarre pas | FUSE absent : `sudo apt install fuse3`, ou lancer avec `--appimage-extract-and-run` |

Journal détaillé : `.hus/hus.log` dans le dossier d'installation. Évènements de sécurité :
`logs/security.log` (`update.verified`, `update.rejected`, `update.installed`, `update.failed`).

## 10. Limites connues

- Les tests automatiques HUS (serveur local, processus réels, pannes simulées) et le test de
  bout en bout avec de vraies AppImage (`tools/hus_e2e_linux.sh`) ont été exécutés sous
  Linux. Sous Windows, les mêmes tests tournent dans la CI (`windows-latest`) ; le parcours
  complet sur un vrai Windows 10/11 (antivirus, SmartScreen) reste à valider à la main.
- Tant que la paire de clés n'est pas créée (section 5), aucune installation automatique
  n'a lieu : c'est voulu.
- Une installation pour tous les utilisateurs (Windows, Program Files) et le `.deb` ne sont
  pas mis à jour par HUS (droits administrateur, gestionnaire de paquets).
- Un téléchargement bloqué sans coupure TCP n'expire pas de lui-même (l'annulation reste
  possible) : le client HTTP utilisé n'a pas de délai d'inactivité par lecture.
- AppImage : le moteur type 2 utilise FUSE 3 (`fusermount3`), présent par défaut sur les
  distributions récentes ; sinon `--appimage-extract-and-run`.
