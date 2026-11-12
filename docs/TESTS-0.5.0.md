# Tests de la 0.5.0

Tests faits le 5 novembre 2026 sur un vrai PC Windows 11 (16 cœurs, 32 Go, launcher Minecraft
officiel installé, compte Java Edition) et sous Linux (sandbox sans accès réseau à Mojang,
Modrinth et Microsoft). « Non testé » veut dire non testé : rien n'est extrapolé.

## Tests automatiques

| Suite | Linux | Windows |
|---|---|---|
| `halcyon-minecraft` (manifeste, règles, natives, fusion des versions, loaders, Maven, modpacks, chemins sûrs, erreurs d'authentification, licence, profils officiels) | 24 / 24 | 24 / 24 |
| `halcyon-storage` (dont réglages Minecraft) | 17 / 17 | 17 / 17 |
| `halcyon-integrations` (non-régression Epic, SteamGridDB, émulateurs…) | 49 / 49 | 49 / 49 |
| `cargo clippy` (`halcyon-minecraft`, `halcyon-app`) | 0 avertissement | — |

## Tests réels sur Windows

### Interface (binaire release 0.5.0)

| Test | Résultat |
|---|---|
| Accueil et autres pages inchangés, nouvel onglet Minecraft après Big Picture | OK |
| Détection du launcher officiel et du compte actif (nom seul) | OK : « Connecté au launcher officiel : Sebi_la_brebis » |
| Versions : liste officielle Mojang (922 versions, dernière 26.3) | OK |
| Loaders : Fabric 26.3, 255 versions compatibles, installation de Fabric 0.20.1 avec Minecraft 26.3 et Java officiel (5 223 fichiers, 586 Mo, progression réelle) | OK, ~45 s |
| Mods : recherche Modrinth filtrée 26.3 / Fabric, installation de Mod Menu avec 2 dépendances (Fabric API, Placeholder API) | OK |
| Mods : désactiver puis réactiver | OK |
| Jouer (launcher officiel) : accord, sauvegarde `launcher_profiles.json.halcyon-backup`, ajout du profil `halcyon-fabric-26.3` (version `fabric-loader-0.20.1-26.3`, dossier de jeu Halcyon, `-Xms1024M -Xmx4096M`), ouverture du launcher | OK ; les 10 profils existants sont identiques avant et après |
| Icônes de mods | Défaut trouvé (format deviné par l'extension), corrigé puis revérifié : icônes affichées |
| Redémarrage de Halcyon : installation et mods conservés | OK |
| Bibliothèque : entrée « Minecraft: Java Edition » ajoutée par l'analyse des sources (« Minecraft : +1 »), visuels SteamGridDB récupérés (3) | OK (journal) ; affichage et Jouer depuis la liste à vérifier à l'écran |

### Moteur (`mc_probe`, même code que l'interface)

| Test | Résultat |
|---|---|
| Vanilla 26.3 : installation, vérification SHA-1, classpath complet, natives, `java -version` avec le runtime Mojang | OK, 25 s |
| Forge 1.20.1-47.4.28 par l'installateur officiel | OK, 42 s |
| NeoForge 26.3.0.101, Quilt 0.29.2 / 1.20.1 | OK |
| Modpack Modrinth Fabulously Optimized (47 mods) | OK |

## Non testé

- **Partie lancée et jouée** depuis le profil « Halcyon · Fabric 26.3 » dans le launcher
  officiel : à confirmer par l'utilisateur (la fenêtre du launcher n'était pas capturable).
- **Mode direct** (connexion Microsoft dans Halcyon) : impossible sans ID d'application Azure
  approuvé par Mojang. Les messages d'erreur (« Invalid app registration », pas de licence,
  compte Xbox absent ou mineur) sont couverts par les tests automatiques.
- **CurseForge** (recherche, modpacks) : aucune clé d'API disponible.
- **Linux avec réseau** : interface vérifiée sous Xvfb (onglet, sections, hors ligne), pas les
  téléchargements.
- Mise à jour d'un modpack et export `.mrpack` depuis l'interface (testés au niveau moteur
  pour l'installation seulement).
