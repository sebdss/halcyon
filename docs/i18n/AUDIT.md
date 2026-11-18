# Audit d'internationalisation — Halcyon 1.0.0-alpha.4

Réalisé avant toute modification, sur la branche `v1.0.0` (commit `df39545`). Inventaire
ligne par ligne : [`inventaire.csv`](inventaire.csv) (fichier, ligne, type, nature, texte).

## 1. Architecture

| Composant | Interface | Logique | Dépend de |
|---|---|---|---|
| Launcher (`halcyon-app`, binaire `halcyon`) | 25 fichiers Slint, 12 246 lignes (`ui/`, `ui/pages/`) | 30 contrôleurs `ctl_*.rs` qui remplissent des modèles et des propriétés Slint | `halcyon-core`, `-storage`, `-integrations`, `-audio`, `-minecraft`, `-update` |
| Installateur (`halcyon-installer`, binaire `halcyon-setup`) | `ui/setup.slint`, 678 lignes | `main.rs` (assistant), `engine.rs` (installation), `platform.rs` | `halcyon-core` |

- **Aucun mécanisme de traduction existant** : tous les textes sont écrits en français,
  directement dans le Slint (`text: "…"`) et dans le Rust (`format!`, `bail!`, tableaux
  statiques). Aucun `@tr`, aucune dépendance gettext/fluent.
- **Slint 1.18.1** : prend en charge `@tr("…")` avec pluriels (`@tr("{n} jeu" | "{n} jeux" % n)`),
  contextes, et les **traductions intégrées au binaire** (`with_bundled_translations`, fichiers
  `.po`, option `bundle-translations` active par défaut dans `slint-build`).
  `slint::select_bundled_translation()` change la langue **à chaud** : toutes les liaisons
  `@tr` sont réévaluées sans recréer la fenêtre ni perdre l'état. Slint détecte aussi la
  langue du système (`sys-locale`).
- Les textes produits par Rust (modèles, notifications, erreurs) ne passent pas par `@tr` :
  il faut un catalogue côté Rust, au même format, et une actualisation des modèles quand la
  langue change.
- Réglages persistants : `halcyon-storage/src/settings.rs` (`settings.json`, serde), conservés
  par les mises à jour (HUS ne touche pas au dossier de données).

## 2. Volume

| Origine | Occurrences | Textes uniques |
|---|---:|---:|
| Slint — textes simples | 1 302 | ≈ 980 |
| Slint — phrases construites par concaténation (`"…" + x + "…"`) | 128 | — |
| Rust — textes | 609 | |
| Rust — phrases avec variables (`format!`) | 572 | |
| Rust — messages d'erreur affichés | 133 | |
| **Total** | **2 744** | **≈ 2 200** |

Les 128 concaténations et une partie des `format!` contiennent des **pluriels écrits
« fichier(s) »** ou des fragments assemblés : ils doivent devenir une seule phrase avec
variables et formes plurielles.

## 3. Éléments à internationaliser

### Launcher — interface Slint

| Domaine | Fichiers | Textes |
|---|---|---:|
| Navigation, coque, barre supérieure, recherche Ctrl K, modes d'interface | `shell.slint`, `app.slint`, `state.slint`, `components.slint` | 128 |
| Accueil, widgets, personnalisation | `home.slint` | 49 |
| Bibliothèque : index, filtres, tri, vues, recherche, états vides, collections intelligentes | `library.slint`, `smart.slint` | 113 |
| Fiche de jeu : en-tête, actions, onglets, succès, statistiques, sauvegardes, mods, profils de lancement, captures, actualités, menus contextuels | `game.slint` | 270 |
| Minecraft : installations, versions, loaders, mods, modpacks, compte | `minecraft.slint` | 124 |
| Activité : succès, statistiques, journal, Que jouer ? | `achievements.slint`, `stats.slint`, `journal.slint`, `assistant.slint` | 120 |
| Outils : téléchargements, mises à jour, sauvegardes, émulateurs, manettes, jeu à distance, extensions | `downloads`, `updates`, `saves`, `emulators`, `controllers`, `remote`, `plugins` | 211 |
| Système : performances, apparence, paramètres (toutes catégories), mises à jour de Halcyon | `performance.slint`, `appearance.slint`, `settings.slint` | 311 |
| Mode console (télévision, manette) | `bigpicture.slint` | 51 |
| Infobulles, libellés d'accessibilité, raccourcis | répartis | inclus ci-dessus |

### Launcher — textes produits par Rust

| Domaine | Fichiers principaux |
|---|---|
| Notifications (203 appels `toast`) : succès, erreurs, avertissements, téléchargements, installation, mises à jour | tous les `ctl_*.rs` |
| Formats : temps de jeu (« 3 h 05 »), dates (« 3 nov. 2026 »), dates relatives (« Il y a 4 jours »), tailles (« 48,2 Go »), vitesses, durées restantes, mois des graphiques | `halcyon-core/util.rs`, `stats.rs`, `halcyon-update/download.rs`, `sysmon.rs`, `minecraft/mods.rs` |
| Descriptions écrites par Halcyon : raisons de « Que jouer ? », recommandations de performances, règles de collections intelligentes, sons | `core/recommend.rs`, `core/rules.rs`, `integrations/sysmon.rs`, `audio/catalog.rs` |
| Erreurs affichées : lancement, sauvegardes, mods, émulateurs, extensions, Minecraft, compte Microsoft, mises à jour, sécurité | `integrations`, `minecraft`, `update`, `storage` |
| Dialogues et confirmations construits dynamiquement | `controller.rs`, `ctl_*.rs` |
| Fenêtre : titre, palette de commandes, profils d'interface | `controller.rs`, `ctl_palette.rs`, `ctl_themes.rs` |

### Installateur

| Étape | Source |
|---|---|
| Bienvenue, présentation, licence | `setup.slint` |
| Répertoire, espace disque, options (raccourcis, menu, association de fichiers) | `setup.slint`, `main.rs` |
| Progression, étapes, téléchargement, extraction, vérification | `main.rs`, `engine.rs` |
| Erreurs (droits, espace, archive, processus en cours) | `engine.rs`, `platform.rs` |
| Fin : confirmation, lancer Halcyon, raccourcis ; désinstallation, mise à jour, réparation | `setup.slint`, `main.rs` |

Pas de sélecteur de langue aujourd'hui.

### Hors périmètre (restent tels quels)

- Journaux internes (`tracing`), `logs/security.log`, messages de l'outil développeur
  `hus-release`, tests.
- Noms de jeux, descriptions, actualités et succès venant de Steam, Epic, GOG, Modrinth,
  RetroAchievements : affichés tels que fournis.
- Noms propres : Halcyon, Steam, Epic Games, GOG, Heroic, Modrinth, noms d'émulateurs.

## 4. Points de vigilance

- **Pluriels et accords** : « fichier(s) », « jeu(x) », « conflit(s) » ; le français met le
  singulier à 0 et 1, l'anglais, l'allemand, l'espagnol, l'italien et le portugais seulement à 1.
- **Formats** : séparateur de milliers (espace fine en français, virgule en anglais, point en
  allemand…), décimales, unités (« Go » / « GB »), ordre des dates, abréviations des mois.
- **Longueurs** : l'allemand et le portugais dépassent souvent le français de 20 à 35 % ;
  la colonne de navigation (212 px), les onglets, les boutons et les badges doivent s'adapter
  sans réduire les polices.
- **Changement de langue à chaud** : Slint réévalue `@tr` seul ; les modèles remplis par Rust
  doivent être recalculés (sans recharger les données ni changer de page).
- **Repli** : une traduction manquante doit afficher l'anglais, pas le français.
- **RTL** : non requis ; l'architecture ne doit pas l'empêcher (pas de texte dans les images,
  pas de positions calculées à partir du sens d'écriture dans le Rust).

## 5. Bilan final (après internationalisation)

État vérifié sur la branche `i18n` (base `df39545`).

### Couverture

| Mesure | Résultat |
|---|---|
| Textes traduisibles | 2 410 textes, 2 435 entrées (pluriels et contextes compris), 12 domaines |
| Langues | fr-FR, en-US, de-DE, es-ES, it-IT, pt-PT : **2 435 / 2 435** chacune, aucune entrée « fuzzy » |
| Textes visibles hors traduction (`i18n.py lint`, Slint + Rust) | **0** (exceptions justifiées dans `tools/i18n/lint-allow.txt`) |
| Concaténations et pseudo-pluriels « (s) » | réécrits en phrases complètes avec `trn!` / `@tr(… \| … % n)` |
| Formats | nombres, décimales, tailles, vitesses, pourcentages, dates et mois selon la langue |
| Tests | 231 tests du workspace passent, dont 6 tests de catalogue sur les fichiers réels |
| Compilation | Linux ; vérification `x86_64-pc-windows-gnu` du launcher et de l'installateur |

Le lint a été durci pendant l'audit final : il signalait mal les mots isolés en minuscules
dans le Slint et ignorait « ou », « et », « par », « dans »… dans le Rust. Les oublis ainsi
trouvés ont été corrigés : libellé « jeux » de l'accueil, champs « nombre » / « valeur »
des collections intelligentes, titres des sources Epic et GOG (« ou »), liaison « et » /
« ou » entre règles de collection, une ligne du journal de mise à jour.

### Vérification visuelle (Xvfb, 1280 × 800, six langues)

Pages capturées dans chaque langue : Accueil, Bibliothèque, Que jouer ?, Statistiques, Mises
à jour, Sauvegardes, Paramètres ; installateur : accueil, sélecteur de langue, options,
vérification, fin d'installation.

Débordements trouvés et corrigés (certains existaient déjà en français) :
- Accueil, carte Bibliothèque : trois chiffres côte à côte dans une colonne étroite (« Nie
  gespi… ») → temps de jeu sur sa propre ligne.
- Statistiques : légendes des chiffres coupées en portugais → retour à la ligne.
- Bibliothèque, collections intelligentes : noms longs coupés avec leur compte (« Succès
  presque complet… », « Achievements nearly com… ») → deux lignes si nécessaire.
- Paramètres, liste des sections : « Accounts and achievem… », « Almacenamiento y diagn… »
  → deux lignes si nécessaire.

Comportements vérifiés :
- détection du système dans l'installateur : `de_AT` → allemand, `es_MX` → espagnol,
  `pt_BR` → portugais, `ja_JP` (indisponible) → anglais ;
- changement de langue dans l'installateur (français → italien, français → portugais) :
  toutes les étapes suivent ; installation complète ; `install-language.txt` écrit ;
- premier lancement du launcher : langue de l'installateur reprise et enregistrée ;
  lancements suivants : préférence conservée même si le fichier change ;
- changement à chaud dans le launcher (français → allemand depuis Paramètres › Langue) :
  la page et la section restent affichées, interface, modèles Rust et collections par
  défaut retraduits, préférence enregistrée.

### Hors périmètre, à dessein

- Contenu externe : noms de jeux, fiches, actualités, succès, notes de version publiées
  sur GitHub.
- Étiquettes Steam : stockées en français dans la base (données), traduites à l'affichage
  par `tc("tag", …)` ; la valeur interne « multijoueur » reste française.
- Journaux techniques (`log::`), descriptions internes des sons (`Cue::description`),
  programme de mise à jour en ligne de commande `halcyon-updater` (sans interface).
- Documentation (`README.md`, `docs/GUIDE.md`…) : en français uniquement.
- Entrées du journal d'activité enregistrées avant cette version : restent en français
  (les nouvelles sont traduites à l'affichage).
- Noms donnés par l'utilisateur (collections, profils, PC).

### Limites connues

- Installateur Windows et launcher Windows : compilés et vérifiés par `cargo check`
  (cible GNU), **pas exécutés** dans cet environnement Linux. À tester sur une vraie machine
  Windows avant publication (sélecteur, détection, écriture de `install-language.txt` dans
  `Program Files` en installation pour tous les utilisateurs).
- Espagnol : « y » / « o » ne deviennent pas « e » / « u » devant un mot commençant par
  « i » / « o » dans les descriptions de collections assemblées (règle d'euphonie).
- Les traductions ont été écrites sans relecture par des locuteurs natifs : une relecture
  humaine de l'allemand, de l'italien et du portugais est recommandée avant publication.
