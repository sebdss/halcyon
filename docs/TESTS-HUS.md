# Tests du Halcyon Update System

Résultats du 16 novembre 2026, Linux x86-64 (Ubuntu, Rust stable), branche `v1.0.0`.

```sh
cargo test -p halcyon-update --features fault-injection   # 36 tests
tools/hus_e2e_linux.sh …                                   # bout en bout, vraies AppImage
```

La fonctionnalité `fault-injection` compile le serveur HTTP local de test, le faux Halcyon
(`hus-fake-app`) et les pannes simulées (`HUS_FAULT`). Elle n'est jamais activée dans les
binaires publiés.

## 1. Résultat global

| Suite | Tests | Résultat |
|---|---|---|
| Unitaires (`src/`) | 15 | 15 réussis |
| Réseau et téléchargement (`tests/network.rs`, serveur HTTP local) | 8 | 8 réussis |
| Updater en processus réels (`tests/updater.rs`) | 13 | 13 réussis |
| Bout en bout, vraies AppImage de Halcyon (`tools/hus_e2e_linux.sh`) | 2 | 2 réussis |
| Tout le dépôt sans pannes simulées (`cargo test --workspace --exclude halcyon-app`), dont l'installateur (désinstallation de l'état HUS) | 185 | 185 réussis |

Clippy : aucun avertissement dans `halcyon-update` ni dans le code HUS de l'application.

## 2. Scénarios exigés

| Scénario | Test(s) | Vérifié |
|---|---|---|
| Détection d'une nouvelle version | `detects_update_and_respects_channels`, `core::channels_select_expected_versions` | 0.5.1 → 1.0.0 proposée |
| Absence de mise à jour | idem | 1.0.0 à jour sur Stable |
| Passage Stable → Beta | idem | 1.0.0 → 1.1.0-beta.1 proposée |
| Passage Beta → Stable | idem | aucune descente vers 1.0.0, note explicative ; 1.1.0 proposée dès sa sortie |
| Manifeste falsifié refusé | `forged_manifest_is_rejected`, `tampered_or_forged_or_older_packages_change_nothing`, `security::signatures_and_keys`, `manifest::inconsistent_manifests_are_rejected` | autre clé, manifeste modifié après signature, aucune clé, champs incohérents |
| Paquet modifié refusé | `modified_package_is_rejected_and_deleted`, `tampered_or_forged_…` | au téléchargement (fichier supprimé) et dans l'updater (paquet en attente modifié sur le disque) ; aucun fichier installé touché |
| Téléchargement interrompu | `interrupted_download_resumes_with_range`, `resume_after_process_restart_and_server_without_range` | coupure de connexion en plein transfert |
| Reprise du téléchargement | idem | `Range: bytes=5000-` puis fichier identique ; reprise après redémarrage du processus ; serveur sans Range : nouveau téléchargement complet |
| Échec de l'installation | `tampered_or_forged_…` (version plus ancienne), `version_floor_…`, `linux::non_appimage_package_is_refused` | état `Failed`, rien de modifié |
| Interruption pendant l'installation | `interruption_during_install_is_recovered` | arrêt brutal après le premier fichier ; lanceur : `NeedsRecovery` ; retour arrière lui-même interrompu puis repris ; installation 1.0.0 intacte |
| Rollback | `failed_start_rolls_back_and_relaunches_previous`, `silent_new_version_times_out_and_rolls_back`, `linux::appimage_failed_start_is_rolled_back`, `linux::setup_install_migrates_…` | nouvelle version qui plante ou ne confirme jamais : restauration, version bloquée, ancienne relancée (`--hus-rolled-back`) |
| Redémarrage après mise à jour | `update_restarts_new_version_and_keeps_settings`, `interruption_after_install_is_confirmed_by_next_start` | nouvelle version lancée avec `--hus-health`, preuve écrite, transaction conclue ; updater arrêté avant le contrôle : conclu par le démarrage suivant |
| Conservation des paramètres | `update_restarts_…`, et chaque scénario (`assert_user_data_intact`) | `settings.json` hors du dossier, `data/` portable, `portable.txt` et fichier inconnu de l'utilisateur intacts |
| Compatibilité Windows | toute la suite (ZIP, `Halcyon.exe`, `halcyon-updater.exe`) | exécutée sous Linux ; tourne aussi sous `windows-latest` dans la CI (non exécutée ici) |
| Compatibilité Linux AppImage | `linux::appimage_*`, `linux::setup_install_migrates_…`, `security::appimage_format_is_checked`, bout en bout | remplacement atomique, droits 0755, version précédente conservée, migration de l'installateur |
| Erreurs réseau simulées | `api_errors_are_explained` | quota GitHub (403), 503, réponse illisible, serveur injoignable, HTTP refusé hors tests |
| ETag / requête conditionnelle | `etag_makes_requests_conditional` | 2e requête avec `If-None-Match`, réponse 304, liste reprise du cache |
| Interruption de processus | `waits_for_launcher_exit`, `single_updater_at_a_time` | l'updater attend la fermeture du lanceur ; un second updater renonce |
| Annulation | `cancellation_stops_and_keeps_partial` | aucun fichier définitif |
| Détection de l'installation | `client::detects_install_kinds` | AppImage, paquet système, compilation locale, installation par l'installateur |

## 3. Bout en bout avec de vraies AppImage

AppImage construites par `tools/package_appimage.sh` (moteur type 2, SquashFS zstd), clé de
test générée par `release_sign keygen`, updater compilé avec cette clé
(`HALCYON_UPDATE_PUBKEY`), manifeste produit et signé par `hus-release`.

1. **Mise à jour réussie** : installée 1.0.0-alpha.1 → paquet 1.0.0-alpha.2.
   - Durée totale : 3 s (vérification, transaction, démarrage du vrai Halcyon sous Xvfb,
     preuve de santé écrite 3 s après l'affichage de la fenêtre, conclusion).
   - Journal : `Verifying → Installing → transaction (2 opérations) → contrôle de santé →
     santé confirmée → terminée`. État `Completed`, version précédente dans
     `.hus/previous/1.0.0-alpha.1/`.
   - Halcyon relancé a détecté son installation : « AppImage (…/opt) », mises à jour
     automatiques disponibles.
   - Note : le binaire des deux AppImage est le même (seul le manifeste annonce
     1.0.0-alpha.2) ; c'est le mécanisme d'installation qui est testé, pas une nouvelle version
     de l'application.
2. **Nouvelle version qui ne démarre pas** (AppImage dont le programme sort en erreur) : code
   2, `RolledBack`, message « Halcyon 1.0.0-alpha.2 n'a pas démarré correctement (…). La
   version 1.0.0-alpha.1 a été restaurée. », AppImage d'origine identique (SHA-256), ancienne
   version relancée, version 1.0.0-alpha.2 bloquée.

Le même essai a montré qu'un manifeste annonçant un `min_updater` supérieur à l'updater
installé est refusé avec un message clair ; la valeur par défaut de `hus-release` est
désormais `runner::PROTOCOL_MIN`, et Halcyon utilise l'updater du nouveau paquet quand
c'est nécessaire (extraction vérifiée : `--appimage-extract usr/bin/halcyon-updater`).

## 4. Chaîne de publication (simulation locale)

Étapes de `release.yml` rejouées sur les vrais paquets : ZIP reproductible (deux
constructions, même SHA-256), extraction des notes du CHANGELOG, manifeste (2 paquets, canal
déduit), `SHA256SUMS`, signature, `hus-release verify` : signature, SHA-256 et structure
vérifiées pour le ZIP et l'AppImage. `verify-remote` utilise le même code que Halcyon
(`network::inspect`, `download`) et ne peut être exécuté qu'après une vraie publication.

## 5. Non vérifié ici

- Exécution sous Windows 10/11 réel (antivirus, SmartScreen, Authenticode) : prévue dans la
  CI pour les tests automatiques, à faire à la main pour le parcours complet.
- Publication réelle sur GitHub (environnements, validation humaine, `verify-remote`) :
  nécessite la paire de clés et les environnements (`HUS.md`, section 5).
- Le binaire complet de Halcyon ne se compile pas dans l'environnement utilisé (mémoire) :
  captures et bout en bout faits avec le binaire de test sans les pages Minecraft,
  Émulateurs et Jeu à distance ; `cargo check` et `clippy` passent sur l'application complète.
