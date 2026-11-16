# Audit avant la 1.0.0

Point de départ : `main`, commit `b25452d` (« Halcyon 0.5.0 - import initial »). Branche de
travail : `v1.0.0`. Audit fait le 16 novembre 2026 sur le code lui-même (et non sur la
documentation), sous Linux (conteneur 2 cœurs, 7 Go, sans GPU, serveur X virtuel).

## 1. État du projet

| Élément | Constat |
|---|---|
| Taille | 34 500 lignes de Rust, 11 400 lignes de Slint, 7 crates (`core`, `storage`, `integrations`, `audio`, `minecraft`, `installer`, `app`) |
| Tests | 167 tests unitaires en 0.5.0 ; CI Windows + Linux (tests, clippy, paquets, `SHA256SUMS`) |
| Stack | Rust + Slint, choix documenté et mesuré (`ARCHITECTURE.md` §2) : 51–69 Mo à l'accueil avec 5 000 jeux, ~90 ms jusqu'à l'interface, 0,1 % de CPU au repos. **Aucune raison technique d'en changer** |
| Dépendances | Minimales et justifiées ; les doublons (`hashbrown`, `getrandom`, `digest`…) viennent tous des dépendances transitives de Slint/winit, pas de Halcyon |
| Architecture | Séparation propre : `core` pur et testé, `storage`/`integrations` sans interface, seul `app` connaît Slint |

### Fonctionnalités existantes vérifiées dans le code

Bibliothèque (Steam, Epic/Heroic/Legendary, standalone, ajout manuel), émulateurs déclaratifs,
succès (Steam, RetroAchievements, personnalisés), Big Picture et manette sur tout le bureau,
recherche universelle, collections intelligentes, statistiques et journal, profils de
lancement, mises à jour des jeux (lecture), sauvegardes multi-PC, mods, extensions
déclaratives, thèmes, jeu à distance, zone de notification, son, installateur, Minecraft
Java Edition (versions, Fabric, Quilt, Forge, NeoForge, Modrinth, CurseForge, launcher
officiel ou connexion Microsoft). Toutes conservées en 1.0.

## 2. Problèmes identifiés

### Sécurité

| # | Problème | Gravité | Preuve |
|---|---|---|---|
| S1 | **Clés d'API en clair dans `settings.json`** (Steam, RetroAchievements, Nexus, SteamGridDB, CurseForge), fichier créé avec les droits par défaut (`0644` sous Linux : lisible par les autres comptes locaux) | Haute | `settings.rs` : `std::fs::write(&tmp, data)` ; champs `*_api_key: String` sérialisés |
| S2 | **Mise à jour vérifiée en intégrité seulement** : `SHA256SUMS` vient de la même version GitHub que le paquet. Un compte GitHub compromis publie paquet *et* empreintes valides | Haute | `update.rs::download_verified` |
| S3 | Pas de journal des événements sensibles (installation d'extension et autorisations, connexion Microsoft, modification du `launcher_profiles.json` officiel, mises à jour) | Moyenne | aucune trace dédiée |
| S4 | Deux implémentations DPAPI (Minecraft seulement) ; aucune protection commune | Faible | `halcyon-minecraft/src/auth.rs` |

Points déjà solides : jeton Microsoft chiffré (DPAPI) ou `0600`, extensions sans code
exécutable et installées désactivées, instance unique protégée par jeton, téléchargements
Minecraft vérifiés SHA-1, chemins de modpack contrôlés.

### Performances

| # | Constat | Décision |
|---|---|---|
| P1 | Code Slint généré : **34,5 Mo** (28,8 Mo en 0.4.0), compilation à ~5 Go | Suivi ; le découpage en fichiers globaux séparés (`perf_state.slint`) évite d'alourdir `state.slint` |
| P2 | Démarrage, mémoire et repos déjà optimisés et mesurés en 0.4 | Pas de « gain » annoncé sans mesure : voir `PERFORMANCES.md` §1.0 |
| P3 | Thread interne de `gilrs` (~20 réveils/s) | Inchangé (bibliothèque tierce), déjà documenté |

### Fonctionnel (cahier des charges V1.0)

| Pilier demandé | Existant | Manque |
|---|---|---|
| Library | ✅ | GOG (via Heroic), empreintes RA disques |
| **Performance Center** | ❌ rien (pas de mesure système) | Tout : monitoring, applications gourmandes, historique, FPS, recommandations |
| Console Mode | ✅ Big Picture | — |
| Customization | ✅ thèmes, widgets, modes d'animation | profils d'interface |
| Extensions | ✅ déclaratives, permissions | signature des extensions |
| Game Hub | ✅ fiche (sauvegardes, mods, succès, profils) | actualités |
| Downloads | ⚠️ tâches dispersées (Minecraft, mise à jour) | centre unifié, pause/reprise, limite de débit |
| Stats | ✅ | export CSV des sessions |

## 3. Demandes recadrées (contraintes réelles)

- **Downloads pour Steam/Epic** : Halcyon ne télécharge pas les jeux des boutiques (respect
  des conditions d'utilisation, DRM). Le futur centre unifié couvrira ce que Halcyon
  télécharge lui-même (Minecraft, mods, modpacks, mises à jour de Halcyon) et pilotera Steam
  par ses URI officielles.
- **FPS et frametimes** : pas d'injection dans les jeux. Lecture des journaux de MangoHud
  (Linux) et PresentMon (Windows, ETW, sans injection).
- **Isolation des extensions** : le modèle déclaratif (aucun code tiers exécuté) est
  conservé ; c'est la meilleure isolation possible. Exécuter du code d'extension serait une
  régression de sécurité.
- **Refonte UI complète** : chantier de plusieurs versions ; la 1.0 ajoute les nouveaux
  écrans dans le design system existant (cohérence) plutôt qu'une refonte non mesurée.

## 4. Plan d'exécution

| Phase | Contenu | Statut |
|---|---|---|
| 1 | Audit (ce document), mesures de référence | ✅ |
| 2 | Sécurité : coffre des secrets (S1, S4), signature Ed25519 des versions (S2), `security.log` (S3) | ✅ |
| 3 | Performance Center : monitoring, applications, recommandations réversibles, historique par partie, FPS par journaux | ✅ |
| 4 | Downloads unifié (file, pause/reprise, débit) | à faire |
| 5 | GOG via Heroic, export des statistiques, signature des extensions | à faire |
| 6 | Refonte visuelle écran par écran, profils d'interface | à faire |
| 7 | Validation Windows sur machine réelle | à faire (par Sebi) |

## 5. Risques

| Risque | Parade |
|---|---|
| Perte des clés d'API à la migration | Migration au chargement, coffre écrit **avant** la réécriture du JSON ; test automatique de la migration |
| Coffre illisible sur un autre PC (DPAPI) | Le coffre n'est jamais copié ; l'export inclut les clés en clair seulement sur demande explicite, la restauration les replace dans le coffre |
| Mises à jour bloquées par la signature | Refus seulement si la clé publique est embarquée ; la CI échoue si la clé publique est définie sans clé privée |
| Coût des mesures pendant le jeu | Échantillon toutes les 10 s sans liste des processus ; désactivable ; coût mesuré (`sysmon_bench`) |
| Base v6 ouverte par la 0.5 | Nouvelle table seulement ; la 0.5 tolère un schéma plus récent (vérifié dans `db.rs`) |
