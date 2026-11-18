# Performances — comparaison 0.3.0 / 0.4.0

## Méthode

- Banc reproductible : `python3 tools/bench.py <binaire> <bibliothèque>` sur une bibliothèque
  générée de **5 000 jeux** Steam (`tools/make_big_library.py`). Les données sont copiées à
  chaque mesure ; deux lancements : base vide (analyse complète) puis base remplie
  (miniatures en cache).
- Linux x86_64, serveur X virtuel, **rendu logiciel** (le cas le plus défavorable : sur un vrai
  GPU une partie de la mémoire part en mémoire vidéo), binaires **release**.
- Trois séries par version, alternées ; la **médiane** est retenue. Fichiers bruts :
  `docs/bench-0.3.0.json` (audit), `docs/bench-0.4.0.json`.
- Détail du démarrage : chronométrage intégré (`HALCYON_DEBUG=1`, journal `perf`).
- Réveils au repos : changements de contexte volontaires par thread, mesurés 10 s après 6 s
  d'inactivité (`/proc/<pid>/task/*/status`).

## Résultats (médianes)

| Mesure | 0.3.0 | 0.4.0 | Écart |
|---|---|---|---|
| Interface prête, 1er lancement | 105 ms | 99 ms | −6 % |
| Interface prête, 2e lancement | 87 ms | 87 ms | = |
| Analyse de 5 000 manifestes + fusion en base | 0,91 s | 0,91 s (2e : 0,81 s) | = |
| Mémoire à l'accueil | 69 Mo | 69 Mo | = |
| Mémoire après défilement complet de la grille | 97–109 Mo | 103–104 Mo | ≈ (cache d'images borné) |
| CPU au repos | 0,3 % | 0,1 % | −0,2 point |
| Réveils au repos, thread des manettes | 248 / 10 s | 10 / 10 s | **−96 %** |
| Réveils au repos, tout le processus | 467 / 10 s | 231 / 10 s | **−51 %** |
| Création de l'interface Slint | 42 ms | 31–37 ms | −12 à −26 % |
| Contrôleur au démarrage (5 000 jeux) | 15,4 ms | 10,7–23,7 ms* | ≈ |
| Code d'interface généré | 28,3 Mo | 28,8 Mo | +2 % pour une interface bien plus riche |
| Binaire (Linux, release, sans symboles) | 49,6 Mo | 49,7 Mo | = |

\* Variable selon l'allocation de la bibliothèque en mémoire (3,5 à 14 ms selon les
lancements) ; hors cette étape : 7 à 10 ms, contre 15 ms en 0.3.

## Ce qui a été optimisé, et pourquoi

| Problème mesuré à l'audit | Correction 0.4 | Effet |
|---|---|---|
| Toutes les pages secondaires recalculées au démarrage (6,7 ms sur 5 000 jeux) | Pages calculées à leur ouverture ; badges (mises à jour, collections intelligentes) après la première image | Démarrage du contrôleur ramené à 7–10 ms hors chargement |
| Thread des manettes réveillé toutes les 40 ms, manette branchée ou non | Attente d'1 s au repos, 40 ms seulement quand une direction est maintenue ou le stick incliné ; évaluation immédiate du stick | 25 → 1 réveil par seconde |
| Composants Slint recopiés à chaque usage (animations, opacité, ombre sur la racine) | Racine nue, effets dans un enfant | Création de l'interface plus rapide malgré plus d'écrans |
| Arrivée d'une image sur une fiche = rechargement complet (requêtes en base, profils, sauvegardes…) | Seules les listes d'images sont republiées | Plus de requêtes répétées pendant le téléchargement des visuels |
| Accueil : toutes les étagères calculées, même masquées | Chaque widget n'est calculé que s'il est affiché ; disques et captures lus sur un thread | Accueil complet en 0,7–3,5 ms |
| Statistiques de la semaine : lecture de toutes les sessions | Requête bornée (`sessions_since`) et index `play_sessions(started_at)` (schéma v5) | Coût indépendant de l'historique |
| Carrousel sur une grande bibliothèque | Fenêtre de 21 jeux autour de la sélection, positions animées sans recréer la liste | Coût constant, quelle que soit la taille |
| Fond d'ambiance | Miniature 48×72 floutée une fois, mise en cache disque | Aucun flou calculé à l'affichage |
| Mode Performance | Animations et effets coupés, visuels en ligne et vérification des mises à jour reportés pendant les parties | Rien ne tourne en arrière-plan pendant le jeu, hors suivi de session |

## Non optimisé (mesuré, jugé non prioritaire)

- **Thread interne de `gilrs`** (≈ 20 réveils/s) : propre à la bibliothèque des manettes, non
  configurable sans la modifier.
- **Analyse des sources** : inchangée (déjà en arrière-plan, < 1 s pour 5 000 jeux).
- **Mémoire de compilation** (≈ 5–6 Go pour le code Slint) : non remesurée ; le code généré
  est resté stable.

## 0.4.1 (son et installateur)

Une série par version, mesurées le même jour sur la même machine (moins rigoureux que les
trois séries ci-dessus : les écarts de quelques pour cent ne sont pas significatifs).
Fichier brut : `docs/bench-0.4.1.json`.

| Mesure (2e lancement) | 0.4.0 | 0.4.1 |
|---|---|---|
| Interface prête | 106 ms | 87 ms |
| Mémoire à l'accueil | 65 Mo | 69 Mo |
| Mémoire après défilement complet | 100 Mo | 107 Mo |
| CPU au repos | 0,2 % | 0,1 % |
| Exécutable | 49,7 Mo | 51,1 Mo |

Le son coûte environ 4 Mo de mémoire (28 sons décodés une fois) et 1,4 Mo d'exécutable. Le
fil audio attend sans délai tant que le périphérique est fermé (aucun réveil au repos) ; le
périphérique n'est ouvert qu'au premier son et refermé après 20 s de silence. Aucun gain de
performance n'est annoncé pour la 0.4.1.

## 1.0.0-alpha.1

### Méthode (et ses limites, à lire avant les chiffres)

- Même banc qu'en 0.4 (`tools/bench.py`, 5 000 jeux Steam générés, serveur X virtuel, rendu
  logiciel), **trois séries alternées 0.5.0 / 1.0**, médianes. Fichier brut :
  `docs/bench-1.0.0-alpha.1.json`.
- Machine de mesure : conteneur Linux 2 cœurs, 7,8 Go, **sans GPU**, mémoire de compilation
  plafonnée à ~6 Go. Avec ce plafond, ni la 0.5.0 ni la 1.0 ne se compilent en `release` (le
  code Slint généré dépasse la mémoire disponible). Les deux binaires mesurés sont donc
  compilés **avec le même profil `dev`** (dépendances optimisées en `opt-level 2`, code de
  Halcyon non optimisé) et **sans la page Minecraft** (retirée des deux côtés uniquement pour
  passer sous le plafond). Les valeurs absolues ne sont donc **pas** celles d'une version
  publiée ; seule la comparaison 0.5.0 / 1.0 est significative.
- Aucun gain n'est annoncé : l'objectif était de vérifier que les ajouts n'alourdissent pas
  Halcyon.

### Résultats (médianes, 3 séries)

| Mesure | 0.5.0 | 1.0.0-alpha.1 | Écart |
|---|---|---|---|
| Interface prête, 1er lancement | 65 ms | 45 ms | dans le bruit (45–68 ms des deux côtés) |
| Interface prête, 2e lancement | 124 ms | 134 ms | dans le bruit (122–151 ms) |
| Analyse de 5 000 manifestes + fusion | 0,80 s | 0,70 s | ≈ (la 1.0 analyse aussi GOG) |
| Mémoire à l'accueil (2e lancement) | 96 Mo | 97 Mo | +1 Mo |
| Mémoire après défilement complet | 130 Mo | 131 Mo | +1 Mo |
| CPU au repos | 0,1 % | 0,1 % | = |
| Défilement de 150 crans | 15,75 s | 15,76 s | = |
| Base de données | 3,40 Mo | 3,42 Mo | table `perf_sessions` (vide) |

### Coût du Performance Center (`sysmon_bench`, binaire release)

| Cadence | Durée d'un relevé (médiane / max) | Processeur consommé |
|---|---|---|
| Page affichée (avec la liste des applications), toutes les 1,5 s | 1,95 ms / 3,6 ms | 0,13 % d'un cœur |
| Partie en cours (sans processus), mesuré à 1,5 s, réel toutes les 10 s | 0,10 ms / 0,21 ms | 0,03 % à 1,5 s, soit ~0,005 % à 10 s |

Sans GPU sur la machine de mesure, le coût de la lecture de la carte graphique (compteurs
PDH sous Windows, NVML/sysfs sous Linux) n'est **pas mesuré** : à vérifier sur un vrai PC.
Rien ne tourne quand la page est fermée et qu'aucune partie n'est en cours.

### Compilation

| | 0.5.0 | 1.0.0-alpha.1 |
|---|---|---|
| Code Slint généré | 33,1 Mo | 35,6 Mo (+7,5 %) |
| Pic mémoire de `cargo check -p halcyon-app` | 4,0 Go | 4,27 Go (+6,6 %) |
| Compilation `dev` sur une machine plafonnée à ~6 Go | réussit | **échoue** (mémoire) |

Les runners de la CI (16 Go) et un PC de 16 Go ou plus ne sont pas concernés. Réduire le
code généré par Slint (découpage des pages, composants moins dupliqués) est la prochaine
optimisation à mener pour garder Halcyon compilable sur une machine de 8 Go.
