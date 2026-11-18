# Tests de la 1.0.0-alpha.1

Tests faits le 16 novembre 2026 sous Linux (conteneur 2 cœurs, 7,8 Go, sans GPU ni accès au
réseau des plateformes, serveur X virtuel). « Non testé » veut dire non testé : rien n'est
extrapolé. **Aucun test sur un vrai PC Windows** pour cette version.

## Tests automatiques

| Suite | Résultat |
|---|---|
| `halcyon-core` (dont `transfer` : débit, pause, file ; `export` : CSV/JSON, injection de formules ; versions semver) | 32 / 32 |
| `halcyon-storage` (dont coffre des secrets, migration des clés, droits `0600`, `security.log`, historique des performances, profils d'interface) | 24 / 24 |
| `halcyon-integrations` (dont `sysmon`, `frametimes` MangoHud/PresentMon, signature des mises à jour, GOG Heroic/Galaxy, signature et épinglage des extensions, actualités Steam) | 69 / 69 |
| `halcyon-minecraft` | 24 / 24 |
| `halcyon-installer` (dont comparaison des pré-versions) | 15 / 15 |
| `halcyon-audio` | 9 / 9 |
| `cargo clippy` (toutes les crates, application comprise) | 0 avertissement |
| `cargo check -p halcyon-app` | OK |
| Tests de `halcyon-app` (24) | **non exécutés** : le binaire de test ne se compile pas sous le plafond mémoire de la machine (voir `PERFORMANCES.md`) ; exécutés par la CI |

## Tests réels de l'interface (binaire de test, voir la méthode dans `PERFORMANCES.md`)

Données : `tools/make_demo_fixture.py` (24 jeux Steam, 4 Epic, 3 standalone, 5 ROM), plus un
faux Heroic avec 3 jeux GOG, un `settings.json` 0.5 contenant deux clés d'API en clair
(droits `0644`) et un journal MangoHud de 6 000 images avec 9 saccades.

| Test | Résultat |
|---|---|
| Migration des clés au premier lancement | OK : `settings.json` réécrit sans clé, en `0600` ; `secrets/` en `0700`, fichiers `0600` ; 3 entrées dans `security.log` |
| Clés relues depuis le coffre (Paramètres › Comptes) | OK |
| Niveau de vérification des mises à jour affiché | OK (« intégrité seule » : compilation sans clé) |
| GOG : détection (Paramètres › Sources), analyse, filtre « GOG » de la bibliothèque | OK : 3 jeux, « 3 sur 40 » |
| Performance Center : mesures en direct (CPU, cœurs, mémoire, disques, applications, coût de Halcyon) | OK, valeurs réelles de la machine |
| Carte graphique absente | OK : « Aucune carte graphique mesurable », aucune valeur inventée |
| Recommandations | Défaut trouvé (disques ignorés entre deux relectures), corrigé puis revérifié : « Peu d'espace libre sur / » affiché |
| Action « Activer » (mode Performance) | OK : la recommandation disparaît |
| Journal MangoHud (« Dernier journal ») | OK : 119,7 i/s, 1 % low 102,5, 9 saccades détectées et visibles |
| Téléchargements : page, pause / reprise, état des mises à jour Steam | OK (« 4 mises à jour en attente, dont 1 en cours ») |
| Profils d'interface : enregistrement, liste | OK |
| Arrêt des mesures en quittant la page | OK (badge « EN PAUSE » géré par le contrôleur ; fil arrêté par drapeau et réveil) |
| Défaut trouvé : jeux GOG affichés « windows » (champ réservé à l'émulation) | corrigé dans le scanner |

## Non testé

- **Windows** : DPAPI du coffre, GOG Galaxy (registre), compteurs GPU PDH, PresentMon.
- **Réseau** : téléchargements Minecraft avec limite de débit, pause et reprise `Range`
  (logique de débit et de pause testée automatiquement, pas le transfert HTTP réel) ;
  actualités Steam (analyse testée sur une réponse enregistrée) ; mise à jour signée de bout
  en bout (signature testée automatiquement, pas avec une vraie version publiée).
- Historique des performances d'une vraie partie (accumulation testée automatiquement).
- Export des statistiques depuis l'interface (sélecteur de fichier indisponible sous le
  serveur X virtuel) ; génération CSV/JSON testée automatiquement.
- Installation d'une extension signée depuis l'interface (même raison) ; vérification et
  épinglage testés automatiquement.
- Binaire `release` (non compilable sur cette machine) : à valider par la CI et sur PC.
