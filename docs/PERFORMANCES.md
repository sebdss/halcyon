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
