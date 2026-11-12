# Rapport de tests — Halcyon 0.4.0

Environnement : Linux x86_64, Rust 1.95, Slint 1.18, serveur X virtuel (Xvfb), rendu
logiciel. Pas de manette physique ni de machine Windows disponibles (voir « Non couvert »).

## 1. Tests automatisés

`cargo test --workspace` : **112 tests, 0 échec**. `cargo clippy --workspace --all-targets` :
aucun avertissement.

| Crate | Tests | Nouveaux en 0.4 |
|---|---|---|
| `halcyon-app` | 24 | historique précédent/suivant et limite (`nav`) ; cycle de vie des tâches, dédoublonnage des évènements (`tasks`) ; disposition des widgets en rangées de 3, widgets masqués, mode minimaliste, format des tailles (`ctl_home`) ; déplacement de la sélection en grille, liste et bandes, bornes, sélection devenue invalide (`ctl_library`) ; recherche dans les paramètres sans accents, filtre par section, correspondance de tous les mots (`ctl_palette`) |
| `halcyon-core` | 23 | — |
| `halcyon-integrations` | 49 | lecture des captures Steam (tri, filtre par jeu, vignettes, fichiers ignorés) |
| `halcyon-storage` | 16 | migration v4 → v5 sur une base 0.3 remplie (données conservées, index créé, pas de rejeu) ; reprise de l'accueil 0.3 dans les widgets |

## 2. Scénarios d'interface (Xvfb, `xdotool`)

| Scénario | Résultat |
|---|---|
| Démarrage sur une bibliothèque de démonstration (53 jeux) puis de 5 000 jeux | OK, aucune erreur dans le journal |
| Barre latérale : clic, repli manuel, repli automatique sous 1 100 px, défilement en fenêtre basse | OK |
| Historique : bibliothèque → fiche → Alt+← → Alt+← (accueil) → Alt+→ ×2 (fiche) | OK |
| Centre de commandes : ouverture, tâche « Analyse des sources » terminée avec action « Ouvrir », fermeture par clic extérieur et Échap | OK |
| Accueil : 14 widgets, édition (déplacer, taille, masquer, ajouter), « Réinitialiser » | OK ; disposition enregistrée |
| Accueil : stockage (disque réel), activité, statistiques 7 jours, captures | OK |
| Bibliothèque : 5 vues, taille S/M/L, sélection aux flèches, défilement automatique vers la sélection, Entrée (fiche), Menu (menu contextuel), Échap (fermeture du menu) | OK |
| Vue immersive : fond d'ambiance flouté pour un jeu sans bannière | OK |
| Fiche : bandeau, chiffres réels, onglets ; onglet Captures avec 3 captures Steam de test | OK |
| Paramètres : recherche « retro » → section Comptes seule ; chips de section ; liste déroulante en fenêtre étroite | OK |
| Ctrl+K : « mode » → commandes de mode ; réglages et collections proposés | OK |
| Apparence : mouvement, taille du texte 130 % (texte agrandi partout), modes, Performance | OK |
| Big Picture : touche Menu → réglages, navigation haut/bas, retour | OK |
| Focus au clavier (Tab) visible sur la barre latérale, les boutons et les jaquettes | OK |
| Persistance : vue carrousel et mouvement réduit → redémarrage → restaurés | OK |
| Tailles : 800×560, 900×700, 1600×1000, 2560×1080, 3840×2160 à l'échelle 200 % | OK, pas de chevauchement ; en 800×560 la barre latérale défile |

## 3. Migration et compatibilité

| Scénario | Résultat |
|---|---|
| Données 0.3 réelles (schéma v4, 53 jeux) ouvertes par la 0.4 | Migration v5 journalisée, 53 jeux, index présent ; `settings.json` non réécrit tant qu'aucun réglage ne change |
| Accueil 0.3 personnalisé → widgets | Ordre et visibilité repris (test automatisé) |
| Même dossier rouvert ensuite par la **0.3.0** | Démarre normalement (avertissement « schéma plus récent » dans le journal), bibliothèque intacte |
| Thèmes importés en 0.3 | Appliqués à l'identique |

## 4. Performances

Voir [`PERFORMANCES.md`](PERFORMANCES.md) (banc reproductible `tools/bench.py`).

## 5. Non couvert, à vérifier avant diffusion

- **Manette physique** (Xbox, PlayStation) : la manette envoie les mêmes touches que le
  clavier, chemin testé ; la détection et la réactivité doivent être confirmées sur matériel
  réel, ainsi que le nouveau rythme de réveil du thread des manettes au repos (1 s).
- **Windows** : compilation et tests confiés à la CI (`.github/workflows/build.yml`) ; contrôle
  visuel à faire sur Windows 10/11 (échelle 125/150 %).
- **Wayland natif** : tests faits sous X11.
- **Lecteurs d'écran** : rôles et libellés présents sur les composants principaux, non
  vérifiés avec Orca / Narrateur.
