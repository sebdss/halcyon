# Audit de la version 0.3.0 (phase 1 de la 0.4.0)

Cet audit précède la refonte. Il établit la référence de performances utilisée pour le
comparatif final (`docs/PERFORMANCES.md`).

## 1. Architecture et technologies

| Élément | 0.3.0 |
|---|---|
| Langage | Rust 2021 (rust-version 1.95) |
| Interface | Slint 1.18 (`.slint` compilé en Rust par `slint-build`), style `fluent-dark` pour les rares widgets standard |
| Rendu | femtovg (OpenGL), repli logiciel |
| Données | SQLite (rusqlite, WAL, migrations v1 → v4), `settings.json` (écriture atomique) |
| Crates | `halcyon-core` (logique pure), `halcyon-storage` (base, paramètres, journal), `halcyon-integrations` (plateformes, émulation, sauvegardes, mods, extensions, jeu à distance), `halcyon-app` (interface, contrôleurs) |
| Taille du code | Rust : environ 21 900 lignes, dont 10 300 dans l'application ; Slint : 7 700 lignes dans 22 fichiers |
| Tests | 97 tests unitaires |

Pour l'interface, des globaux Slint (`LibraryUi`, `DetailUi`, `SettingsUi`…) sont alimentés
par un `Controller` Rust. Les threads de fond renvoient leurs résultats par
`invoke_from_event_loop`. Les jaquettes passent par un `ImageCache` : LRU de 32 Mo, décodage
en threads, miniatures sur disque.

## 2. Fonctionnalités existantes (à préserver)

- **Sources** : Steam (installés et possédés), Epic / Heroic / Legendary, dossiers, ajout
  manuel, ROM.
- **Bibliothèque** :
  - grille virtualisée et liste ;
  - recherche floue avec filtres ;
  - collections manuelles et intelligentes ;
  - tris, favoris, jeux masqués.
- **Fiche** : 7 onglets (Vue d'ensemble, Suivi, Profils, Sauvegardes, Mods, Paramètres,
  Succès).
- **Émulateurs** : définitions JSON, détection, profils.
- **Succès** : Steam, RetroAchievements, succès personnalisés.
- **Mode Big Picture** : carrousels, fiche, recherche, manette.
- **Suivi** : statut, notes, temps de jeu avec provenance, historique des versions.
- **Pages dédiées** : Statistiques, Journal, Mises à jour des jeux.
- **Profils de lancement**.
- **Sauvegardes** : versionnées, multi-PC.
- **Mods** : adaptateurs, profils, déploiement réversible.
- **Configuration** : export et restauration, mode hors ligne.
- **Personnalisation** : apparence (5 thèmes, accent, densité, fond, barre latérale, sections
  de l'accueil) et extensions déclaratives.
- **Manettes** : profils de touches, options par émulateur.
- **Autres outils** : « Que jouer ? », jeu à distance (Sunshine/Moonlight).
- **Intégration au système** :
  - zone de notification, instance unique ;
  - raccourcis personnalisables et lancement rapide (`Ctrl+K`) ;
  - mode compact (fenêtre séparée), fenêtre compagnon ;
  - mise à jour vérifiée, intégrité, désinstallation propre.

## 3. Base de données (schéma v4)

Tables : `games` (44 colonnes, `UNIQUE(source, source_key)`), `play_sessions`
(index `game_id, started_at`), `achievements` (index `unlocked, unlocked_at`),
`achievement_sync`, `collections`, `collection_games`, `version_history`, `activity`
(index `at`), `smart_collections`, `launch_profiles`, `save_config`, `mod_state`,
`mod_profiles`.

La bibliothèque entière est chargée en mémoire au démarrage. Toutes les requêtes sont
servies en mémoire, sans aller-retour SQLite.

## 4. Référence de performances

Banc reproductible : `tools/make_big_library.py` (5 000 jeux Steam, 80 % avec jaquette) et
`tools/bench.py`. Conditions : Linux, Xvfb, rendu logiciel, binaire release.

| Mesure | 0.3.0 |
|---|---|
| Démarrage jusqu'à l'interface, 1er lancement (base vide) | 141 ms |
| Démarrage jusqu'à l'interface, 2e lancement (5 000 jeux en base) | 95–107 ms |
| … dont lecture de la base | 31 ms |
| … dont création de l'interface (`AppWindow::new`) | 42 ms |
| … dont contrôleur (vues, collections, pages secondaires) | 15 ms |
| Analyse Steam de 5 000 jeux (arrière-plan) | 0,9–1,1 s |
| RAM à l'accueil | 70 Mo |
| RAM après défilement de toute la bibliothèque | 101–109 Mo |
| CPU au repos | 0,3–0,4 % |
| Code Rust généré pour l'interface | 28,3 Mo (277 000 lignes) |
| Pic de mémoire de compilation (release) | 5,6 Go |
| Compilation release de l'application | environ 12 min |
| Exécutable | 49,6 Mo |

## 5. Problèmes identifiés

1. **Code d'interface démesuré** : 28 Mo de Rust généré pour 7 700 lignes de Slint. Le
   compilateur Slint *inline* un composant à chaque utilisation dès que sa racine porte une
   animation, une opacité, une ombre, un `clip` ou une visibilité conditionnelle. C'est le cas
   de tous les boutons, interrupteurs et puces. La seule page Paramètres pèse 1,2 Mo. Il en
   découle un pic de compilation à 5,6 Go, une taille d'exécutable passée de 34 à 50 Mo
   depuis la 0.1 et un temps de création de l'interface de 42 ms.
2. **Toutes les pages créent leurs données au démarrage** : succès, émulateurs, mises à jour
   et collections intelligentes sont rafraîchis avant le premier affichage (6,7 ms), même si
   l'utilisateur ne les ouvre jamais.
3. **Réveils périodiques** : le thread manette se réveille toutes les 40 ms, même sans
   manette ni direction maintenue (CPU au repos non nul).
4. **Suggestions de l'accueil** : chaque rafraîchissement de l'accueil relit toutes les
   sessions en base et recalcule les recommandations pour toute la bibliothèque.
5. **Images** : une seule taille de miniature (jaquette). Les vues à grandes illustrations
   (bannières, mode immersif) décodent l'original.
6. **Navigation** : pas d'historique précédent/suivant. Le focus clavier ne parcourt pas les
   boutons (les composants ne sont pas focalisables). La manette n'agit qu'en Big Picture.
7. **Cohérence visuelle** : les composants sont homogènes, mais les pages ont accumulé des
   tailles et marges propres. Il n'existe pas d'échelle typographique, d'état de focus ni de
   chargement (skeleton).

## 6. Risques de régression et parades

| Risque | Parade |
|---|---|
| Réécrire les composants casse les pages | Les noms et l'API publique des composants (`PrimaryButton`, `Toggle`…) restent identiques : les pages existantes en héritent sans modification |
| Les paramètres d'apparence de la 0.3 sont perdus | Les nouveaux champs ont une valeur par défaut. Les sections de l'accueil de la 0.3 sont migrées vers les widgets. Un test vérifie la lecture d'un `settings.json` 0.3 |
| Changer le schéma de la base | Seuls des index sont ajoutés, par une migration v5 idempotente, avec un test de migration depuis v4 |
| Fonction rendue inaccessible par la nouvelle navigation | Toutes les pages restent atteignables : barre latérale, palette `Ctrl+K`, `Ctrl+1…9` |
| Régressions de performances | Le même banc est relancé après chaque phase d'optimisation |
