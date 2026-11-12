# Tests de la 0.4.1

Machine : Ubuntu 24.04 (glibc 2.39), 7,8 Go de RAM, sans carte son, affichage Xvfb (X11),
rendu logiciel. Windows : aucune machine disponible (voir « Non testé »).

## Tests automatiques

`cargo test --workspace -j 1` : **139 tests, tous réussis**.

| Crate | Tests | Nouveautés 0.4.1 |
|---|---|---|
| `halcyon-app` | 24 | Réglages du son → mélangeur |
| `halcyon-audio` | 9 | Décodage WAV, gains (silencieux, pendant une partie, catégories), redémarrage des voix de navigation, limites de voix et priorités, anti-répétition, rééchantillonnage, catalogue complet |
| `halcyon-core` | 25 | Entrées `.desktop` partagées (application, démarrage automatique, guillemets) |
| `halcyon-installer` | 15 | Voir ci-dessous |
| `halcyon-integrations` | 49 | Choix du paquet de mise à jour (installateur Linux) |
| `halcyon-storage` | 17 | Réglages du son : valeurs par défaut, bornes, fichier 0.4.0 sans section `sound` |

`cargo clippy --workspace --all-targets -j 1` : aucun avertissement.

### Installateur (`halcyon-installer`)

| Test | Vérifie |
|---|---|
| `payload_round_trip_and_stub` | Charge utile écrite puis relue, programme seul extrait |
| `rejects_unsafe_paths` | Chemins `..`, absolus, `\`, `:` refusés |
| `fresh_install_then_detect` | Installation, fiche, détection, intégration |
| `update_keeps_user_files_and_drops_obsolete_ones` | Mise à jour : fichiers de l'utilisateur conservés, anciens fichiers retirés |
| `repair_rewrites_only_damaged_files` | Seuls les fichiers abîmés sont réécrits |
| `interrupted_transaction_is_rolled_back` | Coupure simulée en pleine mise en place : état précédent rétabli |
| `failed_placement_restores_previous_version` | Dossier portant le nom d'un fichier : erreur, retour à la version précédente, dossier intact |
| `cancel_before_placement_changes_nothing` | Annulation : rien de modifié |
| `uninstall_keeps_data_unless_asked` | Données conservées, cache supprimé sur demande, fichier inconnu conservé |
| `uninstall_removes_only_our_game_shortcuts` | Raccourcis de jeux de cette installation retirés, ceux d'une autre conservés |
| `maintenance_repair_recreates_integration_only` | Réparation sans charge utile : menu recréé ; fichier modifié signalé, pas écrasé |
| `legacy_archive_install_is_taken_over` | Reprise d'une installation `install.sh` |
| `signed_windows_setup_keeps_payload_readable` | Charge utile lisible après une signature Authenticode, signature retirée de la copie de maintenance |
| `checks_report_blocking_problems` | Espace disque, droits d'écriture, comparaison de versions |
| `system_checks_on_this_machine` | Vérifications réelles sur cette machine |

## Tests manuels (binaires release, Xvfb)

### Installateur Linux (`halcyon-0.4.1-linux-x86_64-setup`, dossier personnel vide)

| Scénario | Résultat |
|---|---|
| Accueil → configuration → vérification → installation → prêt à jouer | Réussi ; 7 vérifications réelles ; progression en octets (ex. « 15 Mo sur 50 Mo ») |
| Chemin relatif saisi | Bloqué à la vérification (« indiquez un chemin complet ») |
| Dossier personnalisé avec espace, contenant déjà un fichier | Avertissement affiché ; fichier conservé après installation **et** après désinstallation |
| Menu, Bureau, démarrage automatique | Entrées créées ; démarrage avec `--minimized` ; lien `~/.local/bin/halcyon` |
| « Lancer Halcyon » | Halcyon démarre, l'installateur se ferme |
| Reprise d'une installation 0.4.0 faite avec `install.sh` (archive 0.4.0 réelle) | Détectée ; mise à jour vers `~/.local/opt/halcyon` ; ancien exécutable remplacé par un lien ; données (`~/.local/share/Halcyon`) intactes |
| 2 fichiers abîmés (un supprimé, un modifié) | Détectés « endommagé » ; réparation : 2 fichiers rétablis |
| Même cas depuis le programme de maintenance | Refus motivé, nommant les fichiers et la version à réinstaller ; rien d'écrasé |
| Désinstallation avec Halcyon ouvert | Refusée, rien retiré |
| Désinstallation, données conservées | Programme, menu, Bureau, démarrage automatique, lien retirés ; données intactes |
| Option « Supprimer mes données » | Bouton désactivé tant que « Je confirme » n'est pas coché |

### Non-régression de l'application

| Scénario | Résultat |
|---|---|
| Données 0.4.0 (bibliothèque de 53 jeux, thème, captures) ouvertes par la 0.4.1 | Accueil, bibliothèque et réglages identiques ; aucun message d'erreur au journal |
| Paramètres › Son | Section présente, réglages enregistrés et relus |
| Sons dans l'interface (journal `HALCYON_DEBUG=1`) | Sections, sélection, menus, fiche, retour, centre de commandes : un son par évènement, sans doublon |
| Sortie audio réelle (ALSA vers fichier) | Signal présent, au gain attendu |
| Mesures 5 000 jeux (`tools/bench.py`, 0.4.0 et 0.4.1 le même jour) | Voir `NOTES-0.4.1.md` : +4 Mo de mémoire, pas d'autre écart significatif |

## Non testé

- **Windows** (installateur, son WASAPI, mise à jour depuis Inno Setup, désinstallation par
  copie temporaire, signature) : aucune machine ni cible de compilation Windows ici. La CI
  compilera et contrôlera la charge utile ; un essai complet sur Windows 10 et 11 reste à
  faire avant toute publication.
- Écoute sur haut-parleurs (pas de carte son).
- Distributions Linux autres qu'Ubuntu 24.04, sessions Wayland.
- Annulation par l'interface pendant la copie : la copie de 50 Mo dure moins d'une seconde
  ici ; le comportement est couvert par le test automatique `cancel_before_placement_changes_nothing`.
