# Tests de la 1.0.0-alpha.7

Linux (conteneur 2 cœurs, sans GPU), serveur X virtuel, binaire release. Bibliothèques
générées par `tools/make_shelf_fixture.py` : 53 jeux, 513 jeux, vide ; scans de test (tranches
et dos dessinés) placés dans `cache/case-art/`, puis la vraie base LaunchBox.

## Automatiques

| Domaine | Tests |
|---|---|
| `halcyon-core::launchbox` | titres normalisés (accents, `&`, ™ / ®, articles, chiffres romains), plateformes, rang des images (scans avant reconstructions, régions), noms de fichiers refusés (`..`, chemin absolu, sous-dossiers, extensions), signature PNG / JPEG |
| `halcyon-storage::lbindex` | construction puis recherche (homonymes, meilleure image, plateforme), index interrompu ignoré |
| `halcyon-integrations::launchbox` | lecture en flux d'un XML coupé tous les 7 octets (noms alternatifs, plateformes ignorées, nom dangereux écarté), annulation, entités XML ; mesure sur une archive de test (ignorée par défaut) |
| `ctl_museum` | regroupement des rangées par plateforme, ordre des jeux conservé |

`cargo test --workspace -j 2`, `cargo clippy --workspace --all-targets -- -D warnings`,
`i18n.py status` (6 langues complètes) et `i18n.py lint` (aucun texte hors traduction).

## À l'écran

| Scénario | Vérifié |
|---|---|
| Étagère coverflow, 53 jeux (femtovg) | sélection au centre, halo, pile de gauche avec tranches (scans de test), pile de droite avec chants, reflets, voile des voisins |
| Retourner (F) sans scan | dos composé, tranche à droite, aide « Face avant » |
| Retourner (F) avec un dos de test | scan affiché, tranche réelle à droite |
| Mode Performance (rendu logiciel) | jaquettes à plat ; retourner montre le dos d'un coup |
| Musée, 53 jeux | rangées Epic, GBA, PS1…, plaques, boîtier choisi de face, carte, projecteur (galerie) |
| Musée, boîtier retourné | dos composé à la taille du boîtier |
| Musée, 513 jeux, rendu logiciel | rangée Steam de 500 jeux (fenêtre de 56 boîtiers), éclairage « jour » ; rangées hors cadre masquées (corrigé : elles débordaient sur la barre des sections) |
| Musée vide | message « Le musée est vide… » |
| Musée en 800 × 560 | trois rangées lisibles, aides tronquées à droite comme ailleurs en petite fenêtre |
| Repos | processeur à 0,1 % (étagère) et 0,4 % (musée) 12 s après la dernière touche |
| **LaunchBox, vraie base** | Paramètres › Comportement › *Télécharger la base* : 110 Mo téléchargés et indexés en 21 s en tout (35 357 jeux avec dos ou tranche, index de 5,8 Mo), `launchbox.download` et `launchbox.index` dans `security.log` ; *Récupérer pour toute la bibliothèque* : vrais dos et tranches pour les 8 jeux de la bibliothèque de test qui portent un vrai titre (Resident Evil 2, 3, 4, Village, Sonic Mania, Frontiers, Generations, Celeste) ; les 45 titres inventés notés introuvables |
| Étagère avec vrais scans | tranche réelle (Sonic Mania) dans la pile, dos réel de Sonic Frontiers au retournement |

Vérifié aussi sur la vraie base : 497 Mo de XML, 190 844 jeux, 1 361 862 images ; types
« Box - Back » et « Box - Spine » présents ; 158 218 noms de dos et tranches acceptés par le
contrôle, 1 refusé (un `.webp`, format non pris en charge).

## Non testé

- Windows, carte graphique réelle, manette réelle (bouton Y) : chemins vérifiés au clavier.
