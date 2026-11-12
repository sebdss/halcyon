# Mettre à jour de la 0.3.0 vers la 0.4.0

La mise à jour conserve toutes les données : bibliothèque, temps de jeu, succès, collections,
sauvegardes de jeux, mods, profils, thèmes, extensions, raccourcis et réglages.

## Avant de commencer (recommandé)

1. Dans la 0.3 : **Paramètres › Exporter ou restaurer la configuration › Exporter**.
   L'archive `.zip` (sans fichiers de jeux, secrets exclus par défaut) permet de revenir en
   arrière à tout moment.
2. Fermez Halcyon (icône de la zone de notification › Quitter).

## Installer

| Système | Procédure |
|---|---|
| Windows (installateur) | Lancez `Halcyon-0.4.0-setup.exe` : il remplace la 0.3 au même emplacement. Ou, depuis la 0.3 : **Paramètres › Mises à jour › Vérifier**, puis **Télécharger** et **Installer** (empreinte SHA-256 vérifiée). |
| Windows (portable) | Remplacez `halcyon.exe` ; le dossier `data` à côté est conservé. |
| Linux (`.deb`) | `sudo apt install ./halcyon_0.4.0_amd64.deb` |
| Linux (archive) | Extraire `halcyon-0.4.0-linux-x86_64.tar.gz`, puis `./install.sh` (remplace les fichiers de la 0.3 dans `~/.local`). |

## Ce qui se passe au premier lancement

- **Base de données** : migration automatique du schéma v4 vers v5, dans une transaction.
  La v5 ajoute **uniquement un index** (`play_sessions(started_at)`) : aucune table ni colonne
  n'est modifiée, aucune donnée réécrite.
- **Paramètres** : les nouveaux réglages prennent leur valeur par défaut. Sont repris :
  - l'ordre et l'affichage des sections de l'accueil 0.3 → widgets de l'accueil (les nouveaux
    widgets sont ajoutés masqués) ;
  - le choix grille / liste → vue de la bibliothèque ;
  - « Animations désactivées » → mouvement « Aucun ».
  L'ancien format reste écrit à côté du nouveau.
- **Thèmes importés** : compatibles tels quels (mêmes noms de couleurs).
- **Raccourcis de manette** : conservés ; trois actions nouvelles (menu contextuel, onglet
  précédent, onglet suivant) prennent les boutons Select, LB et RB.

## Revenir à la 0.3 (si nécessaire)

La base v5 reste lisible par la 0.3 (elle signale un « schéma plus récent » dans le journal et
continue normalement, l'index supplémentaire étant ignoré). Les réglages propres à la 0.4 sont
ignorés par la 0.3. Pour un retour exact à l'état d'avant la mise à jour, restaurez l'archive
exportée à l'étape 1 dans la 0.3.

## Vérifier après la mise à jour

- **Paramètres › Stockage et diagnostic › Vérifier** : contrôle d'intégrité (base, paramètres,
  dossiers, jeux installés, jaquettes, extensions).
- Le journal (`Paramètres › Journaux › Ouvrir`) contient la ligne
  `migration du schéma vers v5`.
