# Halcyon 0.4.1 — « Original Sound Identity & Custom Premium Installer »

La 0.4.1 fait évoluer la 0.4.0 sur deux axes : une identité sonore propre, et un
programme d'installation conçu pour Halcyon. Aucune fonction n'a été retirée ; la
bibliothèque, les paramètres et les sauvegardes de la 0.4.0 sont repris tels quels.

- Audit de départ : [`AUDIT-0.4.md`](AUDIT-0.4.md)
- Son : [`SON.md`](SON.md)
- Installateur : [`INSTALLATEUR.md`](INSTALLATEUR.md)
- Tests : [`TESTS-0.4.1.md`](TESTS-0.4.1.md)
- Journal : [`../CHANGELOG.md`](../CHANGELOG.md)

## Mettre à jour depuis la 0.4.0

| Installation actuelle | Procédure |
|---|---|
| Windows, installateur 0.4.0 (Inno Setup) | Lancez `Halcyon-0.4.1-windows-x86_64-setup.exe`. L'installation est détectée et reprise au même endroit ; l'ancien désinstallateur est retiré |
| Windows, portable | Remplacez `Halcyon.exe` (le dossier `data\` reste) |
| Linux, `.deb` | `sudo apt install ./halcyon_0.4.1_amd64.deb` |
| Linux, archive + `install.sh` | Lancez `halcyon-0.4.1-linux-x86_64-setup` : l'installation est reprise dans `~/.local/opt/halcyon`, l'ancien exécutable de `~/.local/bin` devient un lien |
| Mise à jour intégrée (Paramètres › Mises à jour) | Windows : télécharge et lance le programme d'installation vérifié. Linux : `.deb` comme avant (le programme d'installation sera choisi à partir de la 0.4.1 pour les installations qu'il a faites) |

Fermez Halcyon avant la mise à jour : l'installateur le vérifie et le demande.

## Dépendances ajoutées

| Dépendance | Où | Pourquoi |
|---|---|---|
| `cpal` 0.18 (avec `alsa`, `alsa-sys`, `dasp_sample`, `num-derive`) | `halcyon-audio` | Sortie audio native (WASAPI, ALSA) sans dépendance système autre qu'ALSA ; aucune bibliothèque audio n'existait dans le projet. Les paquets `coreaudio`/`objc2` du verrou ne concernent que macOS et ne sont pas compilés |
| `libasound2` (exécution) / `libasound2-dev` (compilation) | Linux | Requis par cpal ; présent sur toutes les distributions de bureau |
| `halcyon-installer` réutilise `slint`, `sysinfo`, `zip`, `sha2`, `serde_json`, `dirs`, `winreg`, `winresource` | — | Déjà présents dans le projet : aucune nouvelle bibliothèque pour l'installateur |
| NumPy | `tools/sound/make_sounds.py` | Génération des sons, outil de développement seulement (rien n'est livré) |

## État des livrables

Légende : **Terminé et testé** · **Partiel** · **Non disponible**.

| # | Livrable | État | Détail |
|---|---|---|---|
| 1 | Sources | Terminé et testé | Branche `v0.4.1`, archive des sources dans les paquets |
| 2 | Sons originaux | Terminé et testé | 28 sons synthétisés, contrôles automatiques (zéro aux bords, crête, composante continue), sortie réelle vérifiée sous ALSA |
| 3 | Moteur audio | Terminé et testé | `halcyon-audio` : 9 tests (mélangeur, priorités, anti-répétition, gains, décodage) ; aucun blocage sans carte son |
| 4 | Réglages du son | Terminé et testé | Paramètres › Son ; persistance vérifiée, fichier 0.4.0 sans section `sound` accepté |
| 5 | Évènements sonores | Terminé et testé | Vérifiés par le journal de débogage sous Xvfb (navigation, fiche, menus, retour, centre de commandes…) |
| 6 | Installateur | Terminé et testé (Linux) / **Partiel (Windows)** | Linux : parcours complet vérifié. Windows : non compilé ni exécuté ici (voir Limites) |
| 7 | Mise à jour | Terminé et testé (Linux) | Reprise d'une installation `install.sh` 0.4.0 réelle, mise à jour 0.4.1 sur 0.4.1 ; Inno Setup : logique écrite, non exécutée |
| 8 | Réparation | Terminé et testé (Linux) | Fichiers supprimés et modifiés rétablis ; programme de maintenance : intégration recréée, fichier abîmé signalé sans écrasement |
| 9 | Désinstallation | Terminé et testé (Linux) | Données conservées par défaut, confirmation explicite pour les supprimer, fichiers tiers conservés, refus si Halcyon est ouvert |
| 10 | Paquet Windows | **Partiel** | Construit par la CI (`--pack`, contrôle `--info`) ; non construit ici, non testé sur une machine Windows ; signature prête mais sans certificat |
| 11 | Paquets Linux | Terminé et testé | `-setup`, `.deb` (dépendance ALSA ajoutée), `.tar.gz` |
| 12 | Tests fonctionnels | Terminé | [`TESTS-0.4.1.md`](TESTS-0.4.1.md) |
| 13 | Tests de non-régression | Terminé | Tests de la 0.4.0 inchangés et réussis ; données 0.4.0 ouvertes par la 0.4.1 ; mesures comparées |
| 14 | Documentation technique | Terminé | `SON.md`, `INSTALLATEUR.md`, `AUDIT-0.4.md`, commentaires du code |
| 15 | Documentation d'installation | Terminé | `INSTALLATEUR.md`, `GUIDE.md` §1, §23, §25 |
| 16 | Dépendances ajoutées | Terminé | Ci-dessus |
| 17 | Compilation et génération des installateurs | Terminé | `INSTALLATEUR.md` › Construire ; `README.md` › Compiler |
| 18 | Changelog | Terminé | `CHANGELOG.md` |
| 19 | Sons du Media Center | **Non disponible** | Halcyon n'a pas de Media Center ; aucun son créé pour une fonction absente |

## Limites

- **Windows non vérifié** : pas de machine Windows ni de cible Windows de compilation dans
  l'environnement de développement. Le code Windows (registre, raccourcis, relance depuis une
  copie temporaire pour se désinstaller, signature) a été relu mais n'a été ni compilé ni
  exécuté ici. La CI le compilera ; un essai complet sur Windows 10 et 11 est nécessaire
  avant publication.
- **Signature** : prévue dans la CI, inactive sans certificat de signature de code (secrets
  `WINDOWS_CERT_PFX`, `WINDOWS_CERT_PASSWORD`). Sans elle, SmartScreen affiche
  « Éditeur inconnu ».
- **Écoute** : la machine de développement n'a pas de carte son ; les sons ont été écoutés
  sous forme de fichiers et la sortie réelle vérifiée par ALSA vers un fichier.
- **Linux** : seul Ubuntu 24.04 (X11) a été testé. Wayland, Fedora, Arch, etc. : non testés.
- **Archive `.tar.gz`** : conservée pour ne retirer aucun format sans votre accord ; elle fait
  double emploi avec le programme d'installation Linux.
- **Raccourcis de jeux sous Windows** : conservés à la désinstallation (non identifiables
  avec certitude).

## Mesures (bibliothèque de 5 000 jeux, Linux, rendu logiciel, release)

Mesurées avec `tools/bench.py`, la 0.4.0 et la 0.4.1 sur la même machine, le même jour
(`docs/bench-0.4.1.json`). Aucun gain de performance n'était visé ni n'est annoncé.

| | 0.4.0 | 0.4.1 |
|---|---|---|
| Interface prête, 2e lancement | 106 ms | 87 ms |
| Mémoire à l'accueil | 65 Mo | 69 Mo |
| Mémoire après défilement de la grille | 100 Mo | 107 Mo |
| CPU au repos | 0,2 % | 0,1 % |
| Défilement de 150 rangées | 16,2 s | 17,0 s |
| Taille de l'exécutable | 49,7 Mo | 51,1 Mo |

Le coût du son est d'environ 4 Mo de mémoire (sons décodés une fois) et 1,4 Mo d'exécutable.
Les écarts de temps de démarrage et de défilement sont du même ordre que la variation
d'une mesure à l'autre : ils ne sont pas interprétés comme un gain ou une perte.
